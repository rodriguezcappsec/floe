//! Bounded application-owned worker for integrity actions.
//!
//! It deliberately has no GTK dependency.  Every filesystem read and store write happens on
//! this single worker thread; GTK observes the shared job manager and later retrieves outcomes.

use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

use floe_core::{JobCommand, JobFailure, JobFailureKind, JobId, JobProgress, OperationId};
use rustix::{
    fs::{CWD, Mode, OFlags, RenameFlags, open, renameat_with},
    io::Errno,
};
use thiserror::Error;

use crate::{
    fingerprint_store::{FingerprintStore, FingerprintStoreError},
    integrity::{
        FingerprintVerification, IntegrityError, ManifestVerification, SavedFingerprint,
        generate_sha256sums, parse_sha256sums, save_fingerprint, verify_fingerprint,
        verify_sha256sums,
    },
    job_manager::{JobManagerError, SharedJobManager},
};

pub const INTEGRITY_QUEUE_CAPACITY: usize = 4;
pub const INTEGRITY_RESULT_CAPACITY: usize = 16;
const MAX_MANIFEST_READ_BYTES: u64 = 4 * 1024 * 1024;
static MANIFEST_TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IntegrityRequest {
    SaveFingerprint {
        target: PathBuf,
        store_path: PathBuf,
    },
    VerifyFingerprint {
        target: PathBuf,
        store_path: PathBuf,
    },
    GenerateSha256Sums {
        root: PathBuf,
        targets: Vec<PathBuf>,
        output_path: PathBuf,
    },
    VerifySha256Sums {
        root: PathBuf,
        manifest_path: PathBuf,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IntegrityOutcome {
    FingerprintSaved(SavedFingerprint),
    FingerprintVerified(FingerprintVerification),
    ManifestGenerated {
        output_path: PathBuf,
        entries: usize,
    },
    ManifestVerified(ManifestVerification),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntegritySubmission {
    operation_id: OperationId,
    job_id: JobId,
}
impl IntegritySubmission {
    pub const fn operation_id(self) -> OperationId {
        self.operation_id
    }
    pub const fn job_id(self) -> JobId {
        self.job_id
    }
}

#[derive(Debug, Error)]
pub enum IntegrityExecutorSpawnError {
    #[error("could not spawn integrity worker: {0}")]
    Thread(#[source] std::io::Error),
}

#[derive(Debug, Error)]
pub enum IntegritySubmitError {
    #[error(transparent)]
    JobManager(#[from] JobManagerError),
    #[error("integrity queue is full for {0:?}")]
    QueueFull(JobId),
    #[error("integrity executor stopped for {0:?}")]
    ExecutorStopped(JobId),
}
impl IntegritySubmitError {
    pub const fn job_id(&self) -> Option<JobId> {
        match self {
            Self::QueueFull(id) | Self::ExecutorStopped(id) => Some(*id),
            Self::JobManager(_) => None,
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum IntegrityCancelError {
    #[error("integrity job is not active: {0:?}")]
    NotActive(JobId),
}

struct Task {
    job_id: JobId,
    request: IntegrityRequest,
    cancellation: Arc<AtomicBool>,
}
enum Command {
    Execute(Task),
    Shutdown,
}

#[derive(Debug)]
pub struct IntegrityExecutor {
    sender: Option<SyncSender<Command>>,
    cancellations: Arc<Mutex<HashMap<JobId, Arc<AtomicBool>>>>,
    results: Arc<Mutex<VecDeque<(JobId, IntegrityOutcome)>>>,
    jobs: SharedJobManager,
    worker: Option<JoinHandle<()>>,
}

impl IntegrityExecutor {
    pub fn spawn(jobs: SharedJobManager) -> Result<Self, IntegrityExecutorSpawnError> {
        let (sender, receiver) = mpsc::sync_channel(INTEGRITY_QUEUE_CAPACITY);
        let cancellations = Arc::new(Mutex::new(HashMap::new()));
        let results = Arc::new(Mutex::new(VecDeque::with_capacity(
            INTEGRITY_RESULT_CAPACITY,
        )));
        let worker = thread::Builder::new()
            .name("floe-integrity-worker".to_owned())
            .spawn({
                let jobs = Arc::clone(&jobs);
                let cancellations = Arc::clone(&cancellations);
                let results = Arc::clone(&results);
                move || run_worker(receiver, jobs, cancellations, results)
            })
            .map_err(IntegrityExecutorSpawnError::Thread)?;
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
        request: IntegrityRequest,
    ) -> Result<IntegritySubmission, IntegritySubmitError> {
        let queued = lock(&self.jobs).queue_operation()?;
        let submission = IntegritySubmission {
            operation_id: queued.operation_id(),
            job_id: queued.job_id(),
        };
        let cancellation = Arc::new(AtomicBool::new(false));
        lock(&self.cancellations).insert(submission.job_id, Arc::clone(&cancellation));
        let Some(sender) = &self.sender else {
            return self.fail_submit(
                submission.job_id,
                "integrity executor stopped",
                IntegritySubmitError::ExecutorStopped(submission.job_id),
            );
        };
        match sender.try_send(Command::Execute(Task {
            job_id: submission.job_id,
            request,
            cancellation,
        })) {
            Ok(()) => Ok(submission),
            Err(TrySendError::Full(_)) => self.fail_submit(
                submission.job_id,
                "integrity queue is full",
                IntegritySubmitError::QueueFull(submission.job_id),
            ),
            Err(TrySendError::Disconnected(_)) => self.fail_submit(
                submission.job_id,
                "integrity executor stopped",
                IntegritySubmitError::ExecutorStopped(submission.job_id),
            ),
        }
    }

    fn fail_submit<T>(
        &self,
        job_id: JobId,
        message: &'static str,
        error: IntegritySubmitError,
    ) -> Result<T, IntegritySubmitError> {
        lock(&self.cancellations).remove(&job_id);
        let _ = transition(
            &self.jobs,
            job_id,
            JobCommand::Fail(JobFailure::new(JobFailureKind::Internal, message)),
        );
        Err(error)
    }

    pub fn cancel(&self, job_id: JobId) -> Result<(), IntegrityCancelError> {
        let cancellation = lock(&self.cancellations)
            .get(&job_id)
            .cloned()
            .ok_or(IntegrityCancelError::NotActive(job_id))?;
        cancellation.store(true, Ordering::Release);
        Ok(())
    }

    pub fn take_result(&self, job_id: JobId) -> Option<IntegrityOutcome> {
        let mut results = lock(&self.results);
        results
            .iter()
            .position(|(id, _)| *id == job_id)
            .and_then(|index| results.remove(index).map(|(_, result)| result))
    }
}

impl Drop for IntegrityExecutor {
    fn drop(&mut self) {
        for cancellation in lock(&self.cancellations).values() {
            cancellation.store(true, Ordering::Release);
        }
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(Command::Shutdown);
        }
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("integrity worker panicked during shutdown");
        }
    }
}

fn run_worker(
    receiver: Receiver<Command>,
    jobs: SharedJobManager,
    cancellations: Arc<Mutex<HashMap<JobId, Arc<AtomicBool>>>>,
    results: Arc<Mutex<VecDeque<(JobId, IntegrityOutcome)>>>,
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
    cancellations: &Mutex<HashMap<JobId, Arc<AtomicBool>>>,
    results: &Mutex<VecDeque<(JobId, IntegrityOutcome)>>,
) {
    if transition(jobs, task.job_id, JobCommand::Start).is_err() {
        lock(cancellations).remove(&task.job_id);
        return;
    }
    let result = execute_request(
        &task.request,
        || task.cancellation.load(Ordering::Acquire),
        |complete, total| {
            let progress = if total == 0 {
                JobProgress::items(0, Some(0))
            } else {
                JobProgress::bytes(complete, Some(total))
            };
            if let Ok(progress) = progress {
                let _ = transition(jobs, task.job_id, JobCommand::SetProgress(progress));
            }
        },
    );
    match result {
        Ok(outcome) => {
            let mut queue = lock(results);
            if queue.len() == INTEGRITY_RESULT_CAPACITY {
                queue.pop_front();
            }
            queue.push_back((task.job_id, outcome));
            let _ = transition(jobs, task.job_id, JobCommand::Complete);
        }
        Err(IntegrityTaskError::Integrity(IntegrityError::Cancelled)) => {
            let _ = transition(jobs, task.job_id, JobCommand::Cancel);
        }
        Err(error) => {
            let _ = transition(jobs, task.job_id, JobCommand::Fail(task_failure(&error)));
        }
    }
    lock(cancellations).remove(&task.job_id);
}

fn execute_request(
    request: &IntegrityRequest,
    cancelled: impl Fn() -> bool,
    on_progress: impl FnMut(u64, u64),
) -> Result<IntegrityOutcome, IntegrityTaskError> {
    match request {
        IntegrityRequest::SaveFingerprint { target, store_path } => {
            let fingerprint = save_fingerprint(target.clone(), &cancelled, on_progress)?;
            if cancelled() {
                return Err(IntegrityError::Cancelled.into());
            }
            let mut store = FingerprintStore::load(store_path)?;
            store.insert(fingerprint.clone())?;
            store.persist(store_path)?;
            Ok(IntegrityOutcome::FingerprintSaved(fingerprint))
        }
        IntegrityRequest::VerifyFingerprint { target, store_path } => {
            let store = FingerprintStore::load(store_path)?;
            let fingerprint = store
                .get(target)
                .cloned()
                .ok_or_else(|| IntegrityTaskError::FingerprintNotSaved(target.clone()))?;
            Ok(IntegrityOutcome::FingerprintVerified(verify_fingerprint(
                &fingerprint,
                cancelled,
                on_progress,
            )?))
        }
        IntegrityRequest::GenerateSha256Sums {
            root,
            targets,
            output_path,
        } => {
            let manifest = generate_sha256sums(root, targets, &cancelled, on_progress)?;
            if cancelled() {
                return Err(IntegrityError::Cancelled.into());
            }
            write_manifest(output_path, &manifest.encode())?;
            Ok(IntegrityOutcome::ManifestGenerated {
                output_path: output_path.clone(),
                entries: manifest.entries().len(),
            })
        }
        IntegrityRequest::VerifySha256Sums {
            root,
            manifest_path,
        } => {
            let input = read_manifest_nofollow(manifest_path)?;
            let manifest = parse_sha256sums(&input)?;
            Ok(IntegrityOutcome::ManifestVerified(verify_sha256sums(
                root,
                &manifest,
                cancelled,
                on_progress,
            )?))
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct ManifestIdentity {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

impl ManifestIdentity {
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

fn read_manifest_nofollow(path: &Path) -> Result<Vec<u8>, IntegrityTaskError> {
    read_manifest_nofollow_after_read(path, || {})
}

fn read_manifest_nofollow_after_read(
    path: &Path,
    after_read: impl FnOnce(),
) -> Result<Vec<u8>, IntegrityTaskError> {
    let descriptor = open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|error| match error {
        Errno::NOENT => IntegrityTaskError::Integrity(IntegrityError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
            not_found: true,
        }),
        Errno::LOOP => IntegrityTaskError::UnsafeManifest(path.to_path_buf()),
        _ => IntegrityTaskError::Io {
            operation: "open manifest",
            source: std::io::Error::from_raw_os_error(error.raw_os_error()),
        },
    })?;
    let mut file = File::from(descriptor);
    let start_metadata = file.metadata().map_err(|source| IntegrityTaskError::Io {
        operation: "inspect manifest",
        source,
    })?;
    if !start_metadata.is_file() {
        return Err(IntegrityTaskError::UnsafeManifest(path.to_path_buf()));
    }
    let identity = ManifestIdentity::from_metadata(&start_metadata);
    if identity.size > MAX_MANIFEST_READ_BYTES {
        return Err(IntegrityError::ManifestTooLarge.into());
    }
    let mut input = Vec::with_capacity(usize::try_from(identity.size).unwrap_or(0));
    (&mut file)
        .take(MAX_MANIFEST_READ_BYTES.saturating_add(1))
        .read_to_end(&mut input)
        .map_err(|source| IntegrityTaskError::Io {
            operation: "read manifest",
            source,
        })?;
    if u64::try_from(input.len()).unwrap_or(u64::MAX) > MAX_MANIFEST_READ_BYTES {
        return Err(IntegrityError::ManifestTooLarge.into());
    }
    after_read();
    let end_metadata = file.metadata().map_err(|source| IntegrityTaskError::Io {
        operation: "revalidate manifest",
        source,
    })?;
    if ManifestIdentity::from_metadata(&end_metadata) != identity {
        return Err(IntegrityError::SourceChanged(path.to_path_buf()).into());
    }
    Ok(input)
}

fn write_manifest(path: &Path, content: &[u8]) -> Result<(), IntegrityTaskError> {
    let parent = path
        .parent()
        .ok_or_else(|| IntegrityTaskError::UnsafeManifest(path.to_path_buf()))?;
    let name = path
        .file_name()
        .ok_or_else(|| IntegrityTaskError::UnsafeManifest(path.to_path_buf()))?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        name.to_string_lossy(),
        MANIFEST_TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(&temporary)
        .map_err(|source| IntegrityTaskError::Io {
            operation: "create temporary manifest",
            source,
        })?;
    let result = (|| {
        file.write_all(content)
            .map_err(|source| IntegrityTaskError::Io {
                operation: "write manifest",
                source,
            })?;
        file.sync_all().map_err(|source| IntegrityTaskError::Io {
            operation: "sync manifest",
            source,
        })?;
        renameat_with(CWD, &temporary, CWD, path, RenameFlags::NOREPLACE).map_err(|error| {
            if error == Errno::EXIST {
                IntegrityTaskError::ManifestConflict(path.to_path_buf())
            } else {
                IntegrityTaskError::Io {
                    operation: "publish manifest",
                    source: std::io::Error::from_raw_os_error(error.raw_os_error()),
                }
            }
        })?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|source| IntegrityTaskError::Io {
                operation: "sync manifest directory",
                source,
            })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[derive(Debug, Error)]
enum IntegrityTaskError {
    #[error(transparent)]
    Integrity(#[from] IntegrityError),
    #[error(transparent)]
    Store(#[from] FingerprintStoreError),
    #[error("no saved fingerprint exists for {0:?}")]
    FingerprintNotSaved(PathBuf),
    #[error("manifest output is not a regular non-symlink file: {0:?}")]
    UnsafeManifest(PathBuf),
    #[error("manifest output already exists: {0:?}")]
    ManifestConflict(PathBuf),
    #[error("could not {operation}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
}

fn task_failure(error: &IntegrityTaskError) -> JobFailure {
    let kind = match error {
        IntegrityTaskError::Integrity(
            IntegrityError::InvalidRoot(_)
            | IntegrityError::InvalidPath(_)
            | IntegrityError::Symlink(_)
            | IntegrityError::MalformedManifest
            | IntegrityError::TooManyEntries
            | IntegrityError::ManifestTooLarge
            | IntegrityError::TooManyDirectories
            | IntegrityError::DiscoveryTooDeep
            | IntegrityError::DiscoveryTooLarge
            | IntegrityError::MalformedFingerprintRecord
            | IntegrityError::CrossDevice(_)
            | IntegrityError::SourceChanged(_)
            | IntegrityError::DuplicateManifestPath(_)
            | IntegrityError::Request(_)
            | IntegrityError::Checksum(
                crate::checksum_executor::ChecksumError::NotRegular(_)
                | crate::checksum_executor::ChecksumError::AlgorithmUnavailable(_),
            ),
        )
        | IntegrityTaskError::FingerprintNotSaved(_)
        | IntegrityTaskError::UnsafeManifest(_) => JobFailureKind::Unsupported,
        IntegrityTaskError::ManifestConflict(_) => JobFailureKind::Conflict,
        IntegrityTaskError::Integrity(IntegrityError::Io { .. } | IntegrityError::Checksum(_))
        | IntegrityTaskError::Store(FingerprintStoreError::Io { .. })
        | IntegrityTaskError::Io { .. } => JobFailureKind::Io,
        IntegrityTaskError::Store(_) => JobFailureKind::Internal,
        IntegrityTaskError::Integrity(IntegrityError::Cancelled) => JobFailureKind::Internal,
    };
    JobFailure::new(kind, error.to_string())
}

fn transition(
    jobs: &SharedJobManager,
    job_id: JobId,
    command: JobCommand,
) -> Result<(), JobManagerError> {
    lock(jobs).transition(job_id, command).map(|_| ())
}
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::fs::symlink,
        sync::{Arc, Mutex},
        thread,
        time::{Duration, Instant},
    };

    use floe_core::JobState;
    use tempfile::tempdir;

    use super::*;
    use crate::job_manager::ApplicationJobManager;

    fn wait_for_terminal(jobs: &SharedJobManager, job_id: JobId) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if lock(jobs)
                .record(job_id)
                .is_some_and(|record| record.state().is_terminal())
            {
                return;
            }
            thread::sleep(Duration::from_millis(2));
        }
        panic!("integrity job did not reach a terminal state");
    }

    #[test]
    fn phase_18t_integrity_executor_runs_saved_fingerprint_jobs_off_caller_thread() {
        let fixture = tempdir().expect("temporary root");
        let target = fixture.path().join("document");
        fs::write(&target, b"integrity executor fixture").expect("fixture");
        let store_path = fixture.path().join("private").join("fingerprints.bin");
        let jobs = Arc::new(Mutex::new(ApplicationJobManager::new()));
        let executor = IntegrityExecutor::spawn(Arc::clone(&jobs)).expect("executor");

        let save = executor
            .submit(IntegrityRequest::SaveFingerprint {
                target: target.clone(),
                store_path: store_path.clone(),
            })
            .expect("save submission");
        wait_for_terminal(&jobs, save.job_id());
        assert!(matches!(
            executor.take_result(save.job_id()),
            Some(IntegrityOutcome::FingerprintSaved(_))
        ));

        let verify = executor
            .submit(IntegrityRequest::VerifyFingerprint { target, store_path })
            .expect("verify submission");
        wait_for_terminal(&jobs, verify.job_id());
        assert_eq!(
            executor.take_result(verify.job_id()),
            Some(IntegrityOutcome::FingerprintVerified(
                FingerprintVerification::Match
            ))
        );
        assert_eq!(
            lock(&jobs)
                .record(verify.job_id())
                .map(|record| record.state()),
            Some(JobState::Completed)
        );
    }

    #[test]
    fn phase_18t_integrity_executor_publishes_then_verifies_manifest() {
        let fixture = tempdir().expect("temporary root");
        let target = fixture.path().join("document");
        fs::write(&target, b"manifest fixture").expect("fixture");
        let output = fixture.path().join("SHA256SUMS");
        let jobs = Arc::new(Mutex::new(ApplicationJobManager::new()));
        let executor = IntegrityExecutor::spawn(Arc::clone(&jobs)).expect("executor");
        let generated = executor
            .submit(IntegrityRequest::GenerateSha256Sums {
                root: fixture.path().to_path_buf(),
                targets: vec![target],
                output_path: output.clone(),
            })
            .expect("generation submission");
        wait_for_terminal(&jobs, generated.job_id());
        assert!(matches!(
            executor.take_result(generated.job_id()),
            Some(IntegrityOutcome::ManifestGenerated { entries: 1, .. })
        ));

        let verified = executor
            .submit(IntegrityRequest::VerifySha256Sums {
                root: fixture.path().to_path_buf(),
                manifest_path: output,
            })
            .expect("verification submission");
        wait_for_terminal(&jobs, verified.job_id());
        assert!(matches!(
            executor.take_result(verified.job_id()),
            Some(IntegrityOutcome::ManifestVerified(_))
        ));
    }

    #[test]
    fn phase_18t_manifest_publish_never_replaces_existing_output() {
        let fixture = tempdir().expect("temporary root");
        let target = fixture.path().join("document");
        fs::write(&target, b"manifest fixture").expect("fixture");
        let output = fixture.path().join("SHA256SUMS");
        fs::write(&output, b"keep this existing manifest").expect("existing output");
        let jobs = Arc::new(Mutex::new(ApplicationJobManager::new()));
        let executor = IntegrityExecutor::spawn(Arc::clone(&jobs)).expect("executor");

        let submission = executor
            .submit(IntegrityRequest::GenerateSha256Sums {
                root: fixture.path().to_path_buf(),
                targets: vec![target],
                output_path: output.clone(),
            })
            .expect("submission");
        wait_for_terminal(&jobs, submission.job_id());

        assert_eq!(
            fs::read(&output).expect("preserved output"),
            b"keep this existing manifest"
        );
        assert!(executor.take_result(submission.job_id()).is_none());
        assert_eq!(
            lock(&jobs)
                .record(submission.job_id())
                .map(|record| record.state()),
            Some(JobState::Failed)
        );
    }

    #[test]
    fn phase_18t_manifest_read_refuses_symlinks_and_revalidates_open_descriptor() {
        let fixture = tempdir().expect("temporary root");
        let manifest = fixture.path().join("SHA256SUMS");
        let target = fixture.path().join("target");
        fs::write(&target, b"not a manifest").expect("target");
        symlink(&target, &manifest).expect("manifest symlink");
        assert!(matches!(
            read_manifest_nofollow(&manifest),
            Err(IntegrityTaskError::UnsafeManifest(path)) if path == manifest
        ));

        fs::remove_file(&manifest).expect("remove symlink");
        fs::write(&manifest, b"a".repeat(64)).expect("manifest");
        assert!(matches!(
            read_manifest_nofollow_after_read(&manifest, || {
                fs::write(&manifest, b"changed").expect("replace during read");
            }),
            Err(IntegrityTaskError::Integrity(IntegrityError::SourceChanged(path))) if path == manifest
        ));
    }
}
