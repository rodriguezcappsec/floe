//! Bounded executor for the explicit Copy and Verify workflow.
//!
//! Ordinary Copy continues to use `copy_executor`; this module first records a
//! no-follow source-tree identity and SHA-256 baseline, delegates publication
//! to the ordinary safe copy engine, syncs the published tree, and only then
//! re-hashes both trees. Any post-publication failure retains the destination
//! and reports it as `CopiedUnverified`.

use std::{
    collections::{HashMap, VecDeque},
    ffi::OsString,
    fs::{self, File},
    io,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

use floe_core::{
    ChecksumAlgorithm, ChecksumRequest, CopyCancellation, CopyError, CopyOutcome, CopyProgress,
    CopyRequest, JobCommand, JobFailure, JobFailureKind, JobId, JobProgress, OperationId,
    VERIFIED_COPY_DEPTH_CAPACITY, VERIFIED_COPY_ENTRY_CAPACITY, VERIFIED_COPY_PATH_BYTES,
    VerifiedCopyOutcome, VerifiedCopyProgress, VerifiedCopyRequest, VerifiedCopyRetry,
    VerifiedCopyStage, VerifiedDestinationState, execute_copy,
};
use rustix::fs::{Mode, OFlags};
use thiserror::Error;

use crate::{
    checksum_executor::{ChecksumError, execute_checksum},
    job_manager::{JobManagerError, SharedJobManager},
};

pub const VERIFIED_COPY_QUEUE_CAPACITY: usize = 4;
pub const VERIFIED_COPY_RESULT_CAPACITY: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

impl FileIdentity {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum BaselineKind {
    Regular {
        identity: FileIdentity,
        digest: String,
    },
    Directory {
        identity: FileIdentity,
    },
    Symlink {
        target: PathBuf,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BaselineEntry {
    relative: PathBuf,
    kind: BaselineKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TreeBaseline {
    entries: Vec<BaselineEntry>,
    regular_files: u64,
    links: u64,
}

#[derive(Debug, Error)]
pub enum VerifiedCopyError {
    #[error("copy and verify was cancelled")]
    Cancelled,
    #[error(transparent)]
    Copy(#[from] CopyError),
    #[error("source changed before verification completed: {}", .0.display())]
    SourceChanged(PathBuf),
    #[error("destination content did not match source: {}", .0.display())]
    VerificationMismatch(PathBuf),
    #[error("unsupported source entry: {}", .0.display())]
    UnsupportedEntry(PathBuf),
    #[error("source tree exceeds the supported entry limit of {VERIFIED_COPY_ENTRY_CAPACITY}")]
    TooManyEntries,
    #[error("source tree exceeds the supported depth limit of {VERIFIED_COPY_DEPTH_CAPACITY}")]
    TooDeep,
    #[error("path exceeds the supported raw-byte limit: {}", .0.display())]
    PathTooLong(PathBuf),
    #[error("integrity hash failed for {}: {message}", path.display())]
    Hash { path: PathBuf, message: String },
    #[error("could not {action} {}: {source}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl VerifiedCopyError {
    fn job_failure(&self) -> JobFailure {
        let kind = match self {
            Self::Copy(error) if error.is_conflict() => JobFailureKind::Conflict,
            Self::Copy(error) if error.is_unsupported() => JobFailureKind::Unsupported,
            Self::UnsupportedEntry(_) => JobFailureKind::Unsupported,
            Self::Io { source, .. } if source.kind() == io::ErrorKind::PermissionDenied => {
                JobFailureKind::PermissionDenied
            }
            _ => JobFailureKind::Io,
        };
        JobFailure::new(kind, self.to_string())
    }
}

#[derive(Debug)]
pub struct VerifiedCopyFailure {
    error: Box<VerifiedCopyError>,
    stage: VerifiedCopyStage,
    destination_state: VerifiedDestinationState,
    retry: VerifiedCopyRetry,
}

impl VerifiedCopyFailure {
    pub fn error(&self) -> &VerifiedCopyError {
        &self.error
    }

    pub const fn stage(&self) -> VerifiedCopyStage {
        self.stage
    }

    pub const fn destination_state(&self) -> VerifiedDestinationState {
        self.destination_state
    }

    pub fn retry(&self) -> &VerifiedCopyRetry {
        &self.retry
    }
}

pub type VerifiedCopyResult = Result<VerifiedCopyOutcome, VerifiedCopyFailure>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedCopyPresentation {
    pub title: String,
    pub detail: String,
    pub notice: &'static str,
    pub retry_enabled: bool,
}

pub fn present_verified_copy(result: &VerifiedCopyResult) -> VerifiedCopyPresentation {
    const NOTICE: &str = "SHA-256 equality is byte-integrity evidence. It does not prove authenticity, authorship, freshness, or safety.";
    match result {
        Ok(outcome) => VerifiedCopyPresentation {
            title: "Copy verified".to_owned(),
            detail: format!(
                "Verified {} regular file{} and {} symbolic link{} after the destination was synced.",
                outcome.regular_files_verified(),
                if outcome.regular_files_verified() == 1 {
                    ""
                } else {
                    "s"
                },
                outcome.links_verified(),
                if outcome.links_verified() == 1 {
                    ""
                } else {
                    "s"
                },
            ),
            notice: NOTICE,
            retry_enabled: false,
        },
        Err(failure) => {
            let (title, retained) = match failure.destination_state() {
                VerifiedDestinationState::NotCreated => (
                    "Copy and Verify did not create a destination",
                    "No destination was published. You can retry the explicit operation.",
                ),
                VerifiedDestinationState::CopiedUnverified => (
                    "Copy retained without verification",
                    "The destination remains in place but is not verified. Review or remove it before retrying to the same path.",
                ),
                VerifiedDestinationState::Verified => (
                    "Copy verification failed",
                    "The destination state is uncertain; Floe does not claim it is verified.",
                ),
            };
            VerifiedCopyPresentation {
                title: title.to_owned(),
                detail: format!("{retained}\n\n{}", failure.error()),
                notice: NOTICE,
                retry_enabled: failure.destination_state() == VerifiedDestinationState::NotCreated,
            }
        }
    }
}

type CopyRunner<'a> = dyn FnMut(
        &CopyRequest,
        &CopyCancellation,
        &mut dyn FnMut(CopyProgress),
    ) -> Result<CopyOutcome, CopyError>
    + 'a;

struct ExecutionHooks<'a> {
    copy: &'a mut CopyRunner<'a>,
    after_copy: &'a mut dyn FnMut(&VerifiedCopyRequest) -> io::Result<()>,
    sync_destination: &'a mut dyn FnMut(&Path, &CopyCancellation) -> Result<(), VerifiedCopyError>,
    before_destination_hash:
        &'a mut dyn FnMut(&VerifiedCopyRequest) -> Result<(), VerifiedCopyError>,
}

pub fn execute_verified_copy(
    request: &VerifiedCopyRequest,
    cancellation: &CopyCancellation,
    report_progress: impl FnMut(VerifiedCopyProgress),
) -> VerifiedCopyResult {
    let mut after_copy = no_post_copy;
    let mut sync_destination = sync_tree;
    let mut copy = ordinary_copy;
    let mut before_destination_hash = no_verification_fault;
    execute_verified_copy_with(
        request,
        cancellation,
        report_progress,
        &mut ExecutionHooks {
            copy: &mut copy,
            after_copy: &mut after_copy,
            sync_destination: &mut sync_destination,
            before_destination_hash: &mut before_destination_hash,
        },
    )
}

fn no_post_copy(_: &VerifiedCopyRequest) -> io::Result<()> {
    Ok(())
}

fn no_verification_fault(_: &VerifiedCopyRequest) -> Result<(), VerifiedCopyError> {
    Ok(())
}

fn ordinary_copy(
    request: &CopyRequest,
    cancellation: &CopyCancellation,
    report_progress: &mut dyn FnMut(CopyProgress),
) -> Result<CopyOutcome, CopyError> {
    execute_copy(request, cancellation, report_progress)
}

fn execute_verified_copy_with(
    request: &VerifiedCopyRequest,
    cancellation: &CopyCancellation,
    mut report_progress: impl FnMut(VerifiedCopyProgress),
    hooks: &mut ExecutionHooks<'_>,
) -> VerifiedCopyResult {
    let mut stage = VerifiedCopyStage::Planning;
    let mut destination_state = VerifiedDestinationState::NotCreated;
    report_progress(VerifiedCopyProgress::new(stage, 0, None));

    let result = (|| {
        check_cancelled(cancellation)?;
        stage = VerifiedCopyStage::HashingSource;
        let baseline = capture_tree(request.source(), cancellation, stage, &mut report_progress)?;

        check_cancelled(cancellation)?;
        stage = VerifiedCopyStage::Copying;
        let mut copy_progress = |progress| {
            report_progress(VerifiedCopyProgress::from_copy(progress));
        };
        let copy = match (hooks.copy)(request.copy_request(), cancellation, &mut copy_progress) {
            Ok(copy) => copy,
            Err(error @ CopyError::CleanupFailed { .. }) => {
                destination_state = VerifiedDestinationState::CopiedUnverified;
                return Err(error.into());
            }
            Err(error) => return Err(error.into()),
        };
        destination_state = VerifiedDestinationState::CopiedUnverified;

        (hooks.after_copy)(request).map_err(|source| VerifiedCopyError::Io {
            action: "run post-copy verification step for",
            path: request.destination().to_path_buf(),
            source,
        })?;
        check_cancelled(cancellation)?;

        stage = VerifiedCopyStage::SyncingDestination;
        report_progress(VerifiedCopyProgress::new(stage, 0, None));
        (hooks.sync_destination)(request.destination(), cancellation)?;

        stage = VerifiedCopyStage::RevalidatingSource;
        let current_source =
            capture_tree(request.source(), cancellation, stage, &mut report_progress)?;
        compare_source_baselines(request.source(), &baseline, &current_source)?;

        stage = VerifiedCopyStage::HashingDestination;
        (hooks.before_destination_hash)(request)?;
        let destination = capture_tree(
            request.destination(),
            cancellation,
            stage,
            &mut report_progress,
        )?;
        compare_destination_baseline(request.destination(), &baseline, &destination)?;
        revalidate_source_identity(request.source(), &baseline, cancellation)?;

        destination_state = VerifiedDestinationState::Verified;
        stage = VerifiedCopyStage::Verified;
        report_progress(VerifiedCopyProgress::new(
            stage,
            baseline.entries.len() as u64,
            Some(baseline.entries.len() as u64),
        ));
        Ok(VerifiedCopyOutcome::new(
            copy,
            baseline.regular_files,
            baseline.links,
        ))
    })();

    result.map_err(|error| VerifiedCopyFailure {
        error: Box::new(error),
        stage,
        destination_state,
        retry: VerifiedCopyRetry::new(request.clone(), destination_state),
    })
}

fn capture_tree(
    root: &Path,
    cancellation: &CopyCancellation,
    stage: VerifiedCopyStage,
    report_progress: &mut impl FnMut(VerifiedCopyProgress),
) -> Result<TreeBaseline, VerifiedCopyError> {
    validate_path_bytes(root)?;
    let mut pending = vec![(root.to_path_buf(), PathBuf::new(), 0usize)];
    let mut entries = Vec::new();
    let mut regular_files = 0u64;
    let mut links = 0u64;

    while let Some((path, relative, depth)) = pending.pop() {
        check_cancelled(cancellation)?;
        if depth > VERIFIED_COPY_DEPTH_CAPACITY {
            return Err(VerifiedCopyError::TooDeep);
        }
        if entries.len() >= VERIFIED_COPY_ENTRY_CAPACITY {
            return Err(VerifiedCopyError::TooManyEntries);
        }
        validate_path_bytes(&relative)?;
        let metadata = fs::symlink_metadata(&path).map_err(|source| VerifiedCopyError::Io {
            action: "inspect",
            path: path.clone(),
            source,
        })?;
        let file_type = metadata.file_type();
        let kind = if file_type.is_file() {
            let identity = FileIdentity::from_metadata(&metadata);
            let digest = sha256_file(&path, cancellation, stage, report_progress)?;
            let current = fs::symlink_metadata(&path)
                .map_err(|_| VerifiedCopyError::SourceChanged(path.clone()))?;
            if current.file_type().is_symlink() || FileIdentity::from_metadata(&current) != identity
            {
                return Err(VerifiedCopyError::SourceChanged(path));
            }
            regular_files = regular_files.saturating_add(1);
            BaselineKind::Regular { identity, digest }
        } else if file_type.is_dir() {
            let mut children = fs::read_dir(&path)
                .map_err(|source| VerifiedCopyError::Io {
                    action: "enumerate",
                    path: path.clone(),
                    source,
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|source| VerifiedCopyError::Io {
                    action: "enumerate",
                    path: path.clone(),
                    source,
                })?;
            children.sort_by(|left, right| {
                left.file_name()
                    .as_os_str()
                    .as_bytes()
                    .cmp(right.file_name().as_os_str().as_bytes())
            });
            for child in children.into_iter().rev() {
                let name: OsString = child.file_name();
                pending.push((child.path(), relative.join(name), depth + 1));
            }
            BaselineKind::Directory {
                identity: FileIdentity::from_metadata(&metadata),
            }
        } else if file_type.is_symlink() {
            let target = fs::read_link(&path).map_err(|source| VerifiedCopyError::Io {
                action: "read symbolic link",
                path: path.clone(),
                source,
            })?;
            links = links.saturating_add(1);
            BaselineKind::Symlink { target }
        } else {
            return Err(VerifiedCopyError::UnsupportedEntry(path));
        };
        entries.push(BaselineEntry { relative, kind });
        report_progress(VerifiedCopyProgress::new(stage, entries.len() as u64, None));
    }

    entries.sort_by(|left, right| {
        left.relative
            .as_os_str()
            .as_bytes()
            .cmp(right.relative.as_os_str().as_bytes())
    });
    Ok(TreeBaseline {
        entries,
        regular_files,
        links,
    })
}

fn sha256_file(
    path: &Path,
    cancellation: &CopyCancellation,
    stage: VerifiedCopyStage,
    report_progress: &mut impl FnMut(VerifiedCopyProgress),
) -> Result<String, VerifiedCopyError> {
    let request = ChecksumRequest::new(vec![path.to_path_buf()], ChecksumAlgorithm::Sha256, None)
        .map_err(|error| VerifiedCopyError::Hash {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let outcome = execute_checksum(
        &request,
        || cancellation.is_cancelled(),
        |completed, total| {
            report_progress(VerifiedCopyProgress::new(stage, completed, Some(total)));
        },
    )
    .map_err(|error| map_checksum_error(path, error))?;
    outcome
        .items
        .first()
        .map(|item| item.digest.clone())
        .ok_or_else(|| VerifiedCopyError::Hash {
            path: path.to_path_buf(),
            message: "hash engine returned no result".to_owned(),
        })
}

fn map_checksum_error(path: &Path, error: ChecksumError) -> VerifiedCopyError {
    match error {
        ChecksumError::Cancelled => VerifiedCopyError::Cancelled,
        ChecksumError::SourceChanged(changed) => VerifiedCopyError::SourceChanged(changed),
        other => VerifiedCopyError::Hash {
            path: path.to_path_buf(),
            message: other.to_string(),
        },
    }
}

fn compare_source_baselines(
    root: &Path,
    expected: &TreeBaseline,
    actual: &TreeBaseline,
) -> Result<(), VerifiedCopyError> {
    if expected.entries.len() != actual.entries.len() {
        return Err(VerifiedCopyError::SourceChanged(root.to_path_buf()));
    }
    for (expected, actual) in expected.entries.iter().zip(&actual.entries) {
        if expected != actual {
            return Err(VerifiedCopyError::SourceChanged(baseline_path(
                root,
                &expected.relative,
            )));
        }
    }
    Ok(())
}

fn compare_destination_baseline(
    root: &Path,
    source: &TreeBaseline,
    destination: &TreeBaseline,
) -> Result<(), VerifiedCopyError> {
    if source.entries.len() != destination.entries.len() {
        return Err(VerifiedCopyError::VerificationMismatch(root.to_path_buf()));
    }
    for (source, destination) in source.entries.iter().zip(&destination.entries) {
        let matches = source.relative == destination.relative
            && match (&source.kind, &destination.kind) {
                (
                    BaselineKind::Regular { digest: source, .. },
                    BaselineKind::Regular {
                        digest: destination,
                        ..
                    },
                ) => source == destination,
                (BaselineKind::Directory { .. }, BaselineKind::Directory { .. }) => true,
                (
                    BaselineKind::Symlink { target: source },
                    BaselineKind::Symlink {
                        target: destination,
                    },
                ) => source == destination,
                _ => false,
            };
        if !matches {
            return Err(VerifiedCopyError::VerificationMismatch(baseline_path(
                root,
                &source.relative,
            )));
        }
    }
    Ok(())
}

fn revalidate_source_identity(
    root: &Path,
    baseline: &TreeBaseline,
    cancellation: &CopyCancellation,
) -> Result<(), VerifiedCopyError> {
    for entry in &baseline.entries {
        check_cancelled(cancellation)?;
        let path = baseline_path(root, &entry.relative);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| VerifiedCopyError::SourceChanged(path.clone()))?;
        let matches = match &entry.kind {
            BaselineKind::Regular { identity, .. } => {
                metadata.file_type().is_file()
                    && !metadata.file_type().is_symlink()
                    && FileIdentity::from_metadata(&metadata) == *identity
            }
            BaselineKind::Directory { identity } => {
                metadata.file_type().is_dir()
                    && !metadata.file_type().is_symlink()
                    && FileIdentity::from_metadata(&metadata) == *identity
            }
            BaselineKind::Symlink { target } => {
                metadata.file_type().is_symlink()
                    && fs::read_link(&path).is_ok_and(|current| current == *target)
            }
        };
        if !matches {
            return Err(VerifiedCopyError::SourceChanged(path));
        }
    }
    Ok(())
}

fn baseline_path(root: &Path, relative: &Path) -> PathBuf {
    if relative.as_os_str().is_empty() {
        root.to_path_buf()
    } else {
        root.join(relative)
    }
}

fn sync_tree(root: &Path, cancellation: &CopyCancellation) -> Result<(), VerifiedCopyError> {
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    let mut directories = Vec::new();
    let mut entries = 0usize;
    while let Some((path, depth)) = pending.pop() {
        check_cancelled(cancellation)?;
        if depth > VERIFIED_COPY_DEPTH_CAPACITY {
            return Err(VerifiedCopyError::TooDeep);
        }
        entries = entries.saturating_add(1);
        if entries > VERIFIED_COPY_ENTRY_CAPACITY {
            return Err(VerifiedCopyError::TooManyEntries);
        }
        let metadata = fs::symlink_metadata(&path).map_err(|source| VerifiedCopyError::Io {
            action: "inspect copied destination",
            path: path.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            directories.push(path.clone());
            let children = fs::read_dir(&path).map_err(|source| VerifiedCopyError::Io {
                action: "enumerate copied destination",
                path: path.clone(),
                source,
            })?;
            for child in children {
                let child = child.map_err(|source| VerifiedCopyError::Io {
                    action: "enumerate copied destination",
                    path: path.clone(),
                    source,
                })?;
                pending.push((child.path(), depth + 1));
            }
        } else if metadata.is_file() {
            sync_path(&path, false)?;
        } else {
            return Err(VerifiedCopyError::UnsupportedEntry(path));
        }
    }
    for directory in directories.into_iter().rev() {
        check_cancelled(cancellation)?;
        sync_path(&directory, true)?;
    }
    if let Some(parent) = root.parent() {
        check_cancelled(cancellation)?;
        sync_path(parent, true)?;
    }
    Ok(())
}

fn sync_path(path: &Path, directory: bool) -> Result<(), VerifiedCopyError> {
    let mut flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW;
    if directory {
        flags |= OFlags::DIRECTORY;
    }
    let descriptor =
        rustix::fs::open(path, flags, Mode::empty()).map_err(|source| VerifiedCopyError::Io {
            action: "open copied destination for sync",
            path: path.to_path_buf(),
            source: io::Error::from(source),
        })?;
    File::from(descriptor)
        .sync_all()
        .map_err(|source| VerifiedCopyError::Io {
            action: "sync copied destination",
            path: path.to_path_buf(),
            source,
        })
}

fn validate_path_bytes(path: &Path) -> Result<(), VerifiedCopyError> {
    if path.as_os_str().as_bytes().len() > VERIFIED_COPY_PATH_BYTES {
        Err(VerifiedCopyError::PathTooLong(path.to_path_buf()))
    } else {
        Ok(())
    }
}

fn check_cancelled(cancellation: &CopyCancellation) -> Result<(), VerifiedCopyError> {
    if cancellation.is_cancelled() {
        Err(VerifiedCopyError::Cancelled)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedCopySubmission {
    operation_id: OperationId,
    job_id: JobId,
}

impl VerifiedCopySubmission {
    pub const fn operation_id(self) -> OperationId {
        self.operation_id
    }

    pub const fn job_id(self) -> JobId {
        self.job_id
    }
}

#[derive(Debug, Error)]
pub enum VerifiedCopyExecutorSpawnError {
    #[error("verified copy queue capacity must be greater than zero")]
    ZeroCapacity,
    #[error("could not start verified copy executor: {0}")]
    Thread(#[source] io::Error),
}

#[derive(Debug, Error)]
pub enum VerifiedCopySubmitError {
    #[error(transparent)]
    JobManager(#[from] JobManagerError),
    #[error("verified copy queue is full for {job_id:?}")]
    QueueFull { job_id: JobId },
    #[error("verified copy executor stopped for {job_id:?}")]
    ExecutorStopped { job_id: JobId },
}

impl VerifiedCopySubmitError {
    pub const fn job_id(&self) -> Option<JobId> {
        match self {
            Self::QueueFull { job_id } | Self::ExecutorStopped { job_id } => Some(*job_id),
            Self::JobManager(_) => None,
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum VerifiedCopyCancelError {
    #[error("verified copy job is not active: {0:?}")]
    NotActive(JobId),
}

struct Task {
    job_id: JobId,
    request: VerifiedCopyRequest,
    cancellation: CopyCancellation,
}

enum Command {
    Execute(Task),
    Shutdown,
}

#[derive(Debug)]
pub struct VerifiedCopyExecutor {
    sender: Option<SyncSender<Command>>,
    cancellations: Arc<Mutex<HashMap<JobId, CopyCancellation>>>,
    results: Arc<Mutex<VecDeque<(JobId, VerifiedCopyResult)>>>,
    jobs: SharedJobManager,
    worker: Option<JoinHandle<()>>,
}

impl VerifiedCopyExecutor {
    pub fn spawn(jobs: SharedJobManager) -> Result<Self, VerifiedCopyExecutorSpawnError> {
        Self::spawn_with_capacity(jobs, VERIFIED_COPY_QUEUE_CAPACITY)
    }

    pub fn spawn_with_capacity(
        jobs: SharedJobManager,
        capacity: usize,
    ) -> Result<Self, VerifiedCopyExecutorSpawnError> {
        if capacity == 0 {
            return Err(VerifiedCopyExecutorSpawnError::ZeroCapacity);
        }
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let cancellations = Arc::new(Mutex::new(HashMap::new()));
        let results = Arc::new(Mutex::new(VecDeque::with_capacity(
            VERIFIED_COPY_RESULT_CAPACITY,
        )));
        let worker_jobs = Arc::clone(&jobs);
        let worker_cancellations = Arc::clone(&cancellations);
        let worker_results = Arc::clone(&results);
        let worker = thread::Builder::new()
            .name("floe-verified-copy-worker".to_owned())
            .spawn(move || {
                run_worker(receiver, worker_jobs, worker_cancellations, worker_results);
            })
            .map_err(VerifiedCopyExecutorSpawnError::Thread)?;
        Ok(Self {
            sender: Some(sender),
            cancellations,
            results,
            jobs,
            worker: Some(worker),
        })
    }

    pub fn submit(
        &self,
        request: VerifiedCopyRequest,
    ) -> Result<VerifiedCopySubmission, VerifiedCopySubmitError> {
        let queued = lock(&self.jobs).queue_operation()?;
        self.enqueue(queued.operation_id(), queued.job_id(), request)
    }

    pub fn submit_retry(
        &self,
        failed_job_id: JobId,
        request: VerifiedCopyRequest,
    ) -> Result<VerifiedCopySubmission, VerifiedCopySubmitError> {
        let queued = lock(&self.jobs).retry(failed_job_id)?;
        self.enqueue(queued.operation_id(), queued.job_id(), request)
    }

    fn enqueue(
        &self,
        operation_id: OperationId,
        job_id: JobId,
        request: VerifiedCopyRequest,
    ) -> Result<VerifiedCopySubmission, VerifiedCopySubmitError> {
        let cancellation = CopyCancellation::new();
        lock(&self.cancellations).insert(job_id, cancellation.clone());
        let task = Command::Execute(Task {
            job_id,
            request,
            cancellation,
        });
        let sent = match &self.sender {
            Some(sender) => sender.try_send(task),
            None => Err(TrySendError::Disconnected(task)),
        };
        match sent {
            Ok(()) => Ok(VerifiedCopySubmission {
                operation_id,
                job_id,
            }),
            Err(TrySendError::Full(_)) => {
                lock(&self.cancellations).remove(&job_id);
                fail_submission(&self.jobs, job_id, "verified copy queue is at capacity");
                Err(VerifiedCopySubmitError::QueueFull { job_id })
            }
            Err(TrySendError::Disconnected(_)) => {
                lock(&self.cancellations).remove(&job_id);
                fail_submission(&self.jobs, job_id, "verified copy executor has stopped");
                Err(VerifiedCopySubmitError::ExecutorStopped { job_id })
            }
        }
    }

    pub fn cancel(&self, job_id: JobId) -> Result<(), VerifiedCopyCancelError> {
        let cancellation = lock(&self.cancellations)
            .get(&job_id)
            .cloned()
            .ok_or(VerifiedCopyCancelError::NotActive(job_id))?;
        cancellation.cancel();
        Ok(())
    }

    pub fn take_result(&self, job_id: JobId) -> Option<VerifiedCopyResult> {
        let mut results = lock(&self.results);
        let index = results.iter().position(|(id, _)| *id == job_id)?;
        results.remove(index).map(|(_, result)| result)
    }
}

impl Drop for VerifiedCopyExecutor {
    fn drop(&mut self) {
        for cancellation in lock(&self.cancellations).values() {
            cancellation.cancel();
        }
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(Command::Shutdown);
        }
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("verified copy executor panicked during shutdown");
        }
    }
}

fn run_worker(
    receiver: Receiver<Command>,
    jobs: SharedJobManager,
    cancellations: Arc<Mutex<HashMap<JobId, CopyCancellation>>>,
    results: Arc<Mutex<VecDeque<(JobId, VerifiedCopyResult)>>>,
) {
    while let Ok(command) = receiver.recv() {
        match command {
            Command::Execute(task) => run_task(task, &jobs, &cancellations, &results),
            Command::Shutdown => return,
        }
    }
}

fn run_task(
    task: Task,
    jobs: &SharedJobManager,
    cancellations: &Mutex<HashMap<JobId, CopyCancellation>>,
    results: &Mutex<VecDeque<(JobId, VerifiedCopyResult)>>,
) {
    if transition(jobs, task.job_id, JobCommand::Start).is_err() {
        lock(cancellations).remove(&task.job_id);
        return;
    }
    let result = execute_verified_copy(&task.request, &task.cancellation, |progress| {
        let progress = JobProgress::items(progress.completed(), progress.total());
        if let Ok(progress) = progress {
            let _ = transition(jobs, task.job_id, JobCommand::SetProgress(progress));
        }
    });
    let command = match &result {
        Ok(_) => JobCommand::Complete,
        Err(failure) if matches!(failure.error(), VerifiedCopyError::Cancelled) => {
            JobCommand::Cancel
        }
        Err(failure) => JobCommand::Fail(failure.error().job_failure()),
    };
    let mut queue = lock(results);
    if queue.len() == VERIFIED_COPY_RESULT_CAPACITY {
        queue.pop_front();
    }
    queue.push_back((task.job_id, result));
    drop(queue);
    let _ = transition(jobs, task.job_id, command);
    lock(cancellations).remove(&task.job_id);
}

fn transition(
    jobs: &SharedJobManager,
    job_id: JobId,
    command: JobCommand,
) -> Result<(), JobManagerError> {
    lock(jobs).transition(job_id, command).map(|_| ())
}

fn fail_submission(jobs: &SharedJobManager, job_id: JobId, message: &'static str) {
    let _ = transition(
        jobs,
        job_id,
        JobCommand::Fail(JobFailure::new(JobFailureKind::Internal, message)),
    );
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        fs,
        os::unix::{ffi::OsStringExt, fs::symlink},
        sync::{Arc, Mutex},
        thread,
        time::{Duration, Instant},
    };

    use floe_core::{JobState, VerifiedCopyStage};
    use tempfile::tempdir;

    use super::*;
    use crate::job_manager::ApplicationJobManager;

    #[test]
    fn phase_18v_copy_verifies_exact_tree_and_preserves_link_target_bytes() {
        let fixture = tempdir().expect("temporary directory");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::create_dir(&source).expect("source directory");
        fs::write(source.join("plain"), b"plain bytes").expect("plain fixture");
        let raw_name = OsString::from_vec(b"raw-\xff".to_vec());
        fs::write(source.join(&raw_name), b"raw bytes").expect("raw fixture");
        let raw_target = PathBuf::from(OsString::from_vec(b"../outside-\xfe".to_vec()));
        symlink(&raw_target, source.join("link")).expect("link fixture");

        let mut stages = Vec::new();
        let outcome = execute_verified_copy(
            &VerifiedCopyRequest::new(source, destination.clone()),
            &CopyCancellation::new(),
            |progress| stages.push(progress.stage()),
        )
        .expect("copy and verify");

        assert_eq!(
            outcome.destination_state(),
            VerifiedDestinationState::Verified
        );
        assert_eq!(outcome.regular_files_verified(), 2);
        assert_eq!(outcome.links_verified(), 1);
        assert_eq!(
            fs::read(destination.join(raw_name)).expect("raw copy"),
            b"raw bytes"
        );
        assert_eq!(
            fs::read_link(destination.join("link")).expect("copied link"),
            raw_target
        );
        assert!(stages.contains(&VerifiedCopyStage::HashingSource));
        assert!(stages.contains(&VerifiedCopyStage::Copying));
        assert!(stages.contains(&VerifiedCopyStage::SyncingDestination));
        assert!(stages.contains(&VerifiedCopyStage::RevalidatingSource));
        assert!(stages.contains(&VerifiedCopyStage::HashingDestination));
        assert_eq!(stages.last(), Some(&VerifiedCopyStage::Verified));
    }

    #[test]
    fn phase_18v_copy_rejects_conflict_without_touching_destination() {
        let fixture = tempdir().expect("temporary directory");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"new").expect("source fixture");
        fs::write(&destination, b"keep").expect("destination fixture");

        let failure = execute_verified_copy(
            &VerifiedCopyRequest::new(source, destination.clone()),
            &CopyCancellation::new(),
            |_| {},
        )
        .expect_err("conflict must fail");
        assert_eq!(
            failure.destination_state(),
            VerifiedDestinationState::NotCreated
        );
        assert!(matches!(
            failure.error(),
            VerifiedCopyError::Copy(CopyError::DestinationExists(_))
        ));
        assert_eq!(
            fs::read(destination).expect("existing destination"),
            b"keep"
        );
    }

    #[test]
    fn phase_18v_copy_failure_changed_source_and_mismatch_keep_unverified_output() {
        let fixture = tempdir().expect("temporary directory");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"before").expect("source fixture");
        let request = VerifiedCopyRequest::new(source.clone(), destination.clone());
        let cancellation = CopyCancellation::new();
        let mut change_source = |_: &VerifiedCopyRequest| {
            fs::write(&source, b"after")?;
            Ok(())
        };
        let mut sync = sync_tree;
        let mut copy = ordinary_copy;
        let mut no_fault = no_verification_fault;
        let failure = execute_verified_copy_with(
            &request,
            &cancellation,
            |_| {},
            &mut ExecutionHooks {
                copy: &mut copy,
                after_copy: &mut change_source,
                sync_destination: &mut sync,
                before_destination_hash: &mut no_fault,
            },
        )
        .expect_err("changed source must fail");
        assert_eq!(
            failure.destination_state(),
            VerifiedDestinationState::CopiedUnverified
        );
        assert!(matches!(
            failure.error(),
            VerifiedCopyError::SourceChanged(_)
        ));
        assert_eq!(
            fs::read(&destination).expect("retained destination"),
            b"before"
        );

        fs::remove_file(&destination).expect("reset destination");
        fs::write(&source, b"stable").expect("reset source");
        let request = VerifiedCopyRequest::new(source, destination.clone());
        let mut corrupt_destination = |request: &VerifiedCopyRequest| {
            fs::write(request.destination(), b"corrupt")?;
            Ok(())
        };
        let failure = execute_verified_copy_with(
            &request,
            &CopyCancellation::new(),
            |_| {},
            &mut ExecutionHooks {
                copy: &mut copy,
                after_copy: &mut corrupt_destination,
                sync_destination: &mut sync,
                before_destination_hash: &mut no_fault,
            },
        )
        .expect_err("mismatch must fail");
        assert_eq!(
            failure.destination_state(),
            VerifiedDestinationState::CopiedUnverified
        );
        assert!(matches!(
            failure.error(),
            VerifiedCopyError::VerificationMismatch(_)
        ));
        assert_eq!(
            fs::read(destination).expect("retained corrupt output"),
            b"corrupt"
        );
    }

    #[test]
    fn phase_18v_failure_cancel_and_sync_failure_keep_copied_output() {
        let fixture = tempdir().expect("temporary directory");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("cancelled");
        fs::write(&source, b"content").expect("source fixture");
        let cancellation = CopyCancellation::new();
        let cancel_for_hook = cancellation.clone();
        let mut cancel_after_copy = move |_: &VerifiedCopyRequest| {
            cancel_for_hook.cancel();
            Ok(())
        };
        let mut sync = sync_tree;
        let mut copy = ordinary_copy;
        let mut no_fault = no_verification_fault;
        let failure = execute_verified_copy_with(
            &VerifiedCopyRequest::new(source.clone(), destination.clone()),
            &cancellation,
            |_| {},
            &mut ExecutionHooks {
                copy: &mut copy,
                after_copy: &mut cancel_after_copy,
                sync_destination: &mut sync,
                before_destination_hash: &mut no_fault,
            },
        )
        .expect_err("post-copy cancellation must remain explicit");
        assert!(matches!(failure.error(), VerifiedCopyError::Cancelled));
        assert_eq!(
            failure.destination_state(),
            VerifiedDestinationState::CopiedUnverified
        );
        assert_eq!(fs::read(&destination).expect("retained copy"), b"content");

        let destination = fixture.path().join("sync-failed");
        let mut no_change = no_post_copy;
        let mut fail_sync = |path: &Path, _: &CopyCancellation| {
            Err(VerifiedCopyError::Io {
                action: "sync injected destination",
                path: path.to_path_buf(),
                source: io::Error::other("injected sync failure"),
            })
        };
        let failure = execute_verified_copy_with(
            &VerifiedCopyRequest::new(source, destination.clone()),
            &CopyCancellation::new(),
            |_| {},
            &mut ExecutionHooks {
                copy: &mut copy,
                after_copy: &mut no_change,
                sync_destination: &mut fail_sync,
                before_destination_hash: &mut no_fault,
            },
        )
        .expect_err("sync failure must remain explicit");
        assert_eq!(failure.stage(), VerifiedCopyStage::SyncingDestination);
        assert_eq!(
            failure.destination_state(),
            VerifiedDestinationState::CopiedUnverified
        );
        assert_eq!(fs::read(destination).expect("retained copy"), b"content");
    }

    #[test]
    fn phase_18v_failure_missing_source_is_not_created_and_retry_is_typed() {
        let fixture = tempdir().expect("temporary directory");
        let request = VerifiedCopyRequest::new(
            fixture.path().join("missing"),
            fixture.path().join("destination"),
        );
        let failure = execute_verified_copy(&request, &CopyCancellation::new(), |_| {})
            .expect_err("missing source must fail");
        assert_eq!(
            failure.destination_state(),
            VerifiedDestinationState::NotCreated
        );
        assert_eq!(failure.retry().request(), &request);
        assert_eq!(
            failure.retry().destination_state(),
            VerifiedDestinationState::NotCreated
        );
        assert!(!request.destination().exists());
    }

    #[test]
    fn phase_18v_failure_injected_full_disk_and_hash_errors_keep_truthful_states() {
        let fixture = tempdir().expect("temporary directory");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"content").expect("source fixture");
        let request = VerifiedCopyRequest::new(source.clone(), destination.clone());
        let mut full_disk =
            |_: &CopyRequest, _: &CopyCancellation, _: &mut dyn FnMut(CopyProgress)| {
                Err(CopyError::Io {
                    action: "write injected destination",
                    path: destination.clone(),
                    source: io::Error::from_raw_os_error(28),
                })
            };
        let mut no_change = no_post_copy;
        let mut sync = sync_tree;
        let mut no_hash_fault = no_verification_fault;
        let failure = execute_verified_copy_with(
            &request,
            &CopyCancellation::new(),
            |_| {},
            &mut ExecutionHooks {
                copy: &mut full_disk,
                after_copy: &mut no_change,
                sync_destination: &mut sync,
                before_destination_hash: &mut no_hash_fault,
            },
        )
        .expect_err("full disk must fail before publication");
        assert_eq!(failure.stage(), VerifiedCopyStage::Copying);
        assert_eq!(
            failure.destination_state(),
            VerifiedDestinationState::NotCreated
        );
        assert!(!destination.exists());

        let partial_destination = destination.clone();
        let mut cleanup_failure =
            move |_: &CopyRequest, _: &CopyCancellation, _: &mut dyn FnMut(CopyProgress)| {
                fs::write(&partial_destination, b"partial").expect("inject partial destination");
                Err(CopyError::CleanupFailed {
                    original: Box::new(CopyError::Cancelled),
                    path: partial_destination.clone(),
                    cleanup: io::Error::other("injected cleanup failure"),
                })
            };
        let failure = execute_verified_copy_with(
            &request,
            &CopyCancellation::new(),
            |_| {},
            &mut ExecutionHooks {
                copy: &mut cleanup_failure,
                after_copy: &mut no_change,
                sync_destination: &mut sync,
                before_destination_hash: &mut no_hash_fault,
            },
        )
        .expect_err("cleanup failure must retain partial output state");
        assert_eq!(failure.stage(), VerifiedCopyStage::Copying);
        assert_eq!(
            failure.destination_state(),
            VerifiedDestinationState::CopiedUnverified
        );
        assert_eq!(fs::read(&destination).expect("partial output"), b"partial");
        fs::remove_file(&destination).expect("clear injected partial output");

        let mut copy = ordinary_copy;
        let mut hash_failure = |request: &VerifiedCopyRequest| {
            Err(VerifiedCopyError::Hash {
                path: request.destination().to_path_buf(),
                message: "injected read failure".to_owned(),
            })
        };
        let failure = execute_verified_copy_with(
            &request,
            &CopyCancellation::new(),
            |_| {},
            &mut ExecutionHooks {
                copy: &mut copy,
                after_copy: &mut no_change,
                sync_destination: &mut sync,
                before_destination_hash: &mut hash_failure,
            },
        )
        .expect_err("hash read failure must retain copied output");
        assert_eq!(failure.stage(), VerifiedCopyStage::HashingDestination);
        assert_eq!(
            failure.destination_state(),
            VerifiedDestinationState::CopiedUnverified
        );
        assert_eq!(fs::read(destination).expect("retained output"), b"content");
    }

    #[test]
    fn phase_18v_copy_executor_is_bounded_and_retains_terminal_result() {
        let fixture = tempdir().expect("temporary directory");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"executor").expect("source fixture");
        let jobs = Arc::new(Mutex::new(ApplicationJobManager::new()));
        let executor = VerifiedCopyExecutor::spawn(Arc::clone(&jobs)).expect("executor");
        let submission = executor
            .submit(VerifiedCopyRequest::new(source, destination))
            .expect("submission");
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if lock(&jobs)
                .record(submission.job_id())
                .is_some_and(|record| record.state().is_terminal())
            {
                break;
            }
            assert!(Instant::now() < deadline, "verified copy worker timed out");
            thread::sleep(Duration::from_millis(5));
        }
        let state = lock(&jobs)
            .record(submission.job_id())
            .map(|record| record.state());
        let result = executor
            .take_result(submission.job_id())
            .expect("terminal result");
        assert!(result.is_ok(), "terminal result was {result:?}");
        assert_eq!(state, Some(JobState::Completed));
        assert_eq!(
            result.expect("verified outcome").destination_state(),
            VerifiedDestinationState::Verified
        );
    }

    #[test]
    fn phase_18v_failure_progress_never_reports_verified_before_final_stage() {
        let fixture = tempdir().expect("temporary directory");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"content").expect("source fixture");
        let saw_verified = Cell::new(false);
        let mut corrupt = |request: &VerifiedCopyRequest| {
            fs::write(request.destination(), b"changed")?;
            Ok(())
        };
        let mut sync = sync_tree;
        let mut copy = ordinary_copy;
        let mut no_fault = no_verification_fault;
        let _ = execute_verified_copy_with(
            &VerifiedCopyRequest::new(source, destination),
            &CopyCancellation::new(),
            |progress| {
                if progress.stage() == VerifiedCopyStage::Verified {
                    saw_verified.set(true);
                }
            },
            &mut ExecutionHooks {
                copy: &mut copy,
                after_copy: &mut corrupt,
                sync_destination: &mut sync,
                before_destination_hash: &mut no_fault,
            },
        )
        .expect_err("corrupt destination must fail");
        assert!(!saw_verified.get());
    }

    #[test]
    fn phase_18v_ui_presentation_is_truthful_accessible_and_retry_safe() {
        let fixture = tempdir().expect("temporary directory");
        let missing = VerifiedCopyRequest::new(
            fixture.path().join("missing"),
            fixture.path().join("destination"),
        );
        let failed = execute_verified_copy(&missing, &CopyCancellation::new(), |_| {})
            .expect_err("missing source");
        let presentation = present_verified_copy(&Err(failed));
        assert!(presentation.title.contains("did not create"));
        assert!(presentation.retry_enabled);
        assert!(presentation.notice.contains("does not prove authenticity"));

        let source = fixture.path().join("source");
        let destination = fixture.path().join("verified");
        fs::write(&source, b"content").expect("source fixture");
        let verified = execute_verified_copy(
            &VerifiedCopyRequest::new(source, destination),
            &CopyCancellation::new(),
            |_| {},
        )
        .expect("verified copy");
        let presentation = present_verified_copy(&Ok(verified));
        assert_eq!(presentation.title, "Copy verified");
        assert!(!presentation.retry_enabled);
        assert!(presentation.detail.contains("destination was synced"));
        assert!(presentation.notice.contains("byte-integrity evidence"));
    }
}
