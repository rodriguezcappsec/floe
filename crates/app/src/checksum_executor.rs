use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File},
    io::Read,
    os::unix::fs::MetadataExt,
    path::PathBuf,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

use floe_core::{
    ChecksumAlgorithm, ChecksumRequest, JobCommand, JobFailure, JobFailureKind, JobId, JobProgress,
    OperationId,
};
use glib::{Checksum, ChecksumType};
use rustix::fs::{FileType, Mode, OFlags};
use thiserror::Error;

use crate::job_manager::{JobManagerError, SharedJobManager};

pub const CHECKSUM_QUEUE_CAPACITY: usize = 4;
pub const CHECKSUM_RESULT_CAPACITY: usize = 16;
const CHECKSUM_CHUNK_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChecksumVerification {
    NotRequested,
    Match,
    Mismatch { expected: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChecksumItemResult {
    pub path: PathBuf,
    pub algorithm: ChecksumAlgorithm,
    pub digest: String,
    pub bytes: u64,
    pub verification: ChecksumVerification,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChecksumOutcome {
    pub items: Arc<[ChecksumItemResult]>,
    pub total_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SourceIdentity {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

impl SourceIdentity {
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

#[derive(Clone, Debug)]
struct PlannedSource {
    path: PathBuf,
    identity: SourceIdentity,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ChecksumError {
    #[error("checksum calculation cancelled")]
    Cancelled,
    #[error("checksum target is not a regular file and was not followed: {}", .0.display())]
    NotRegular(PathBuf),
    #[error("checksum target changed while it was being read: {}", .0.display())]
    SourceChanged(PathBuf),
    #[error("checksum byte total exceeds supported limits")]
    SizeOverflow,
    #[error("{algorithm} is unavailable on this GLib build", algorithm = .0.display_name())]
    AlgorithmUnavailable(ChecksumAlgorithm),
    #[error("checksum I/O failed at {}: {message}", path.display())]
    Io { path: PathBuf, message: String },
}

pub fn execute_checksum(
    request: &ChecksumRequest,
    cancelled: impl Fn() -> bool,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<ChecksumOutcome, ChecksumError> {
    let mut plan = Vec::with_capacity(request.targets().len());
    let mut total_bytes = 0u64;
    for target in request.targets() {
        if cancelled() {
            return Err(ChecksumError::Cancelled);
        }
        let metadata = fs::symlink_metadata(target).map_err(|error| ChecksumError::Io {
            path: target.clone(),
            message: error.to_string(),
        })?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(ChecksumError::NotRegular(target.clone()));
        }
        total_bytes = total_bytes
            .checked_add(metadata.len())
            .ok_or(ChecksumError::SizeOverflow)?;
        plan.push(PlannedSource {
            path: target.clone(),
            identity: SourceIdentity::from_metadata(&metadata),
        });
    }

    let mut completed_bytes = 0u64;
    let mut results = Vec::with_capacity(plan.len());
    for source in plan {
        if cancelled() {
            return Err(ChecksumError::Cancelled);
        }
        let descriptor = rustix::fs::open(
            &source.path,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::empty(),
        )
        .map_err(|error| ChecksumError::Io {
            path: source.path.clone(),
            message: error.to_string(),
        })?;
        let stat = rustix::fs::fstat(&descriptor).map_err(|error| ChecksumError::Io {
            path: source.path.clone(),
            message: error.to_string(),
        })?;
        if FileType::from_raw_mode(stat.st_mode).is_symlink()
            || stat.st_dev != source.identity.device
            || stat.st_ino != source.identity.inode
            || u64::try_from(stat.st_size).unwrap_or(u64::MAX) != source.identity.size
        {
            return Err(ChecksumError::SourceChanged(source.path));
        }
        let mut checksum = Checksum::new(checksum_type(request.algorithm()))
            .ok_or(ChecksumError::AlgorithmUnavailable(request.algorithm()))?;
        let mut file = File::from(descriptor);
        let mut buffer = vec![0u8; CHECKSUM_CHUNK_BYTES];
        loop {
            if cancelled() {
                return Err(ChecksumError::Cancelled);
            }
            let read = file.read(&mut buffer).map_err(|error| ChecksumError::Io {
                path: source.path.clone(),
                message: error.to_string(),
            })?;
            if read == 0 {
                break;
            }
            checksum.update(&buffer[..read]);
            completed_bytes = completed_bytes
                .checked_add(u64::try_from(read).unwrap_or(u64::MAX))
                .ok_or(ChecksumError::SizeOverflow)?;
            on_progress(completed_bytes, total_bytes);
        }
        let end_metadata = fs::symlink_metadata(&source.path)
            .map_err(|_| ChecksumError::SourceChanged(source.path.clone()))?;
        let end_identity = SourceIdentity::from_metadata(&end_metadata);
        if end_metadata.file_type().is_symlink() || end_identity != source.identity {
            return Err(ChecksumError::SourceChanged(source.path));
        }
        let digest = checksum
            .string()
            .ok_or(ChecksumError::AlgorithmUnavailable(request.algorithm()))?
            .to_string();
        let verification = match request.expected() {
            Some(expected) if digest == expected.canonical_hex() => ChecksumVerification::Match,
            Some(expected) => ChecksumVerification::Mismatch {
                expected: expected.canonical_hex(),
            },
            None => ChecksumVerification::NotRequested,
        };
        results.push(ChecksumItemResult {
            path: source.path,
            algorithm: request.algorithm(),
            digest,
            bytes: source.identity.size,
            verification,
        });
    }
    if total_bytes == 0 {
        on_progress(0, 0);
    }
    Ok(ChecksumOutcome {
        items: results.into(),
        total_bytes,
    })
}

fn checksum_type(algorithm: ChecksumAlgorithm) -> ChecksumType {
    match algorithm {
        ChecksumAlgorithm::Sha256 => ChecksumType::Sha256,
        ChecksumAlgorithm::Sha512 => ChecksumType::Sha512,
        ChecksumAlgorithm::Md5Legacy => ChecksumType::Md5,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChecksumSubmission {
    operation_id: OperationId,
    job_id: JobId,
}

impl ChecksumSubmission {
    pub const fn operation_id(self) -> OperationId {
        self.operation_id
    }
    pub const fn job_id(self) -> JobId {
        self.job_id
    }
}

#[derive(Debug, Error)]
pub enum ChecksumExecutorSpawnError {
    #[error("checksum queue capacity cannot be zero")]
    ZeroCapacity,
    #[error("could not spawn checksum worker: {0}")]
    Thread(#[source] std::io::Error),
}

#[derive(Debug, Error)]
pub enum ChecksumSubmitError {
    #[error(transparent)]
    JobManager(#[from] JobManagerError),
    #[error("checksum queue is full for {0:?}")]
    QueueFull(JobId),
    #[error("checksum executor stopped for {0:?}")]
    ExecutorStopped(JobId),
}

impl ChecksumSubmitError {
    pub const fn job_id(&self) -> Option<JobId> {
        match self {
            Self::QueueFull(id) | Self::ExecutorStopped(id) => Some(*id),
            Self::JobManager(_) => None,
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ChecksumCancelError {
    #[error("checksum job is not active: {0:?}")]
    NotActive(JobId),
}

struct Task {
    job_id: JobId,
    request: ChecksumRequest,
    cancellation: Arc<AtomicBool>,
}

enum Command {
    Execute(Task),
    Shutdown,
}

#[derive(Debug)]
pub struct ChecksumExecutor {
    sender: Option<SyncSender<Command>>,
    cancellations: Arc<Mutex<HashMap<JobId, Arc<AtomicBool>>>>,
    results: Arc<Mutex<VecDeque<(JobId, ChecksumOutcome)>>>,
    jobs: SharedJobManager,
    worker: Option<JoinHandle<()>>,
}

impl ChecksumExecutor {
    pub fn spawn(jobs: SharedJobManager) -> Result<Self, ChecksumExecutorSpawnError> {
        let (sender, receiver) = mpsc::sync_channel(CHECKSUM_QUEUE_CAPACITY);
        let cancellations = Arc::new(Mutex::new(HashMap::new()));
        let results = Arc::new(Mutex::new(VecDeque::with_capacity(
            CHECKSUM_RESULT_CAPACITY,
        )));
        let worker_cancellations = Arc::clone(&cancellations);
        let worker_results = Arc::clone(&results);
        let worker_jobs = Arc::clone(&jobs);
        let worker = thread::Builder::new()
            .name("floe-checksum-worker".to_owned())
            .spawn(move || run_worker(receiver, worker_jobs, worker_cancellations, worker_results))
            .map_err(ChecksumExecutorSpawnError::Thread)?;
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
        request: ChecksumRequest,
    ) -> Result<ChecksumSubmission, ChecksumSubmitError> {
        let queued = lock(&self.jobs).queue_operation()?;
        let operation_id = queued.operation_id();
        let job_id = queued.job_id();
        let cancellation = Arc::new(AtomicBool::new(false));
        lock(&self.cancellations).insert(job_id, Arc::clone(&cancellation));
        let Some(sender) = &self.sender else {
            let _ = transition(
                &self.jobs,
                job_id,
                JobCommand::Fail(JobFailure::new(
                    JobFailureKind::Internal,
                    "checksum executor stopped",
                )),
            );
            return Err(ChecksumSubmitError::ExecutorStopped(job_id));
        };
        match sender.try_send(Command::Execute(Task {
            job_id,
            request,
            cancellation,
        })) {
            Ok(()) => Ok(ChecksumSubmission {
                operation_id,
                job_id,
            }),
            Err(TrySendError::Full(_)) => {
                lock(&self.cancellations).remove(&job_id);
                let _ = transition(
                    &self.jobs,
                    job_id,
                    JobCommand::Fail(JobFailure::new(
                        JobFailureKind::Internal,
                        "checksum queue is full",
                    )),
                );
                Err(ChecksumSubmitError::QueueFull(job_id))
            }
            Err(TrySendError::Disconnected(_)) => {
                lock(&self.cancellations).remove(&job_id);
                let _ = transition(
                    &self.jobs,
                    job_id,
                    JobCommand::Fail(JobFailure::new(
                        JobFailureKind::Internal,
                        "checksum executor stopped",
                    )),
                );
                Err(ChecksumSubmitError::ExecutorStopped(job_id))
            }
        }
    }

    pub fn cancel(&self, job_id: JobId) -> Result<(), ChecksumCancelError> {
        let cancellation = lock(&self.cancellations)
            .get(&job_id)
            .cloned()
            .ok_or(ChecksumCancelError::NotActive(job_id))?;
        cancellation.store(true, Ordering::Release);
        Ok(())
    }

    pub fn take_result(&self, job_id: JobId) -> Option<ChecksumOutcome> {
        let mut results = lock(&self.results);
        let index = results.iter().position(|(id, _)| *id == job_id)?;
        results.remove(index).map(|(_, outcome)| outcome)
    }
}

impl Drop for ChecksumExecutor {
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
            tracing::error!("checksum worker panicked during shutdown");
        }
    }
}

fn run_worker(
    receiver: Receiver<Command>,
    jobs: SharedJobManager,
    cancellations: Arc<Mutex<HashMap<JobId, Arc<AtomicBool>>>>,
    results: Arc<Mutex<VecDeque<(JobId, ChecksumOutcome)>>>,
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
    results: &Mutex<VecDeque<(JobId, ChecksumOutcome)>>,
) {
    if transition(jobs, task.job_id, JobCommand::Start).is_err() {
        lock(cancellations).remove(&task.job_id);
        return;
    }
    let target_count = task.request.targets().len() as u64;
    let result = execute_checksum(
        &task.request,
        || task.cancellation.load(Ordering::Acquire),
        |completed, total| {
            let progress = if total == 0 {
                JobProgress::items(target_count, Some(target_count))
            } else {
                JobProgress::bytes(completed, Some(total))
            };
            if let Ok(progress) = progress {
                let _ = transition(jobs, task.job_id, JobCommand::SetProgress(progress));
            }
        },
    );
    let command = match result {
        Ok(outcome) => {
            let mut queue = lock(results);
            if queue.len() == CHECKSUM_RESULT_CAPACITY {
                queue.pop_front();
            }
            queue.push_back((task.job_id, outcome));
            JobCommand::Complete
        }
        Err(ChecksumError::Cancelled) => JobCommand::Cancel,
        Err(error) => JobCommand::Fail(checksum_failure(&error)),
    };
    let _ = transition(jobs, task.job_id, command);
    lock(cancellations).remove(&task.job_id);
}

fn checksum_failure(error: &ChecksumError) -> JobFailure {
    let kind = if matches!(
        error,
        ChecksumError::NotRegular(_) | ChecksumError::AlgorithmUnavailable(_)
    ) {
        JobFailureKind::Unsupported
    } else {
        JobFailureKind::Io
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
        cell::Cell,
        os::unix::fs::symlink,
        sync::{Arc, Mutex},
        thread,
        time::{Duration, Instant},
    };

    use floe_core::{ChecksumRequest, ExpectedDigest, JobEventKind, JobState};
    use tempfile::tempdir;

    use super::*;
    use crate::job_manager::ApplicationJobManager;

    #[test]
    fn phase_10e_checksum_vectors_cover_sha256_sha512_legacy_md5_and_verification() {
        let root = tempdir().expect("root");
        let path = root.path().join("abc");
        fs::write(&path, b"abc").expect("fixture");
        let vectors = [
            (
                ChecksumAlgorithm::Sha256,
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                ChecksumAlgorithm::Sha512,
                "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
            ),
            (
                ChecksumAlgorithm::Md5Legacy,
                "900150983cd24fb0d6963f7d28e17f72",
            ),
        ];
        for (algorithm, expected) in vectors {
            let request = ChecksumRequest::new(
                vec![path.clone()],
                algorithm,
                Some(ExpectedDigest::parse(algorithm, expected).expect("expected")),
            )
            .expect("request");
            let outcome = execute_checksum(&request, || false, |_, _| {}).expect("checksum");
            assert_eq!(outcome.items[0].digest, expected);
            assert_eq!(outcome.items[0].verification, ChecksumVerification::Match);
        }
        let mismatch = ExpectedDigest::parse(
            ChecksumAlgorithm::Sha256,
            "aa7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .expect("expected");
        let request = ChecksumRequest::new(vec![path], ChecksumAlgorithm::Sha256, Some(mismatch))
            .expect("request");
        assert!(matches!(
            execute_checksum(&request, || false, |_, _| {})
                .expect("checksum")
                .items[0]
                .verification,
            ChecksumVerification::Mismatch { .. }
        ));

        let worker_path = root.path().join("worker");
        fs::write(&worker_path, b"abc").expect("worker fixture");
        let jobs = Arc::new(Mutex::new(ApplicationJobManager::new()));
        let executor = ChecksumExecutor::spawn(Arc::clone(&jobs)).expect("executor");
        let submission = executor
            .submit(
                ChecksumRequest::new(vec![worker_path], ChecksumAlgorithm::Sha256, None)
                    .expect("request"),
            )
            .expect("submission");
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let terminal = lock(&jobs)
                .record(submission.job_id())
                .is_some_and(|record| record.state().is_terminal());
            if terminal {
                break;
            }
            assert!(Instant::now() < deadline, "checksum worker timed out");
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            lock(&jobs)
                .record(submission.job_id())
                .map(|record| record.state()),
            Some(JobState::Completed)
        );
        let events = lock(&jobs).drain_events();
        assert!(events.iter().any(|event| {
            event.job_id() == submission.job_id()
                && matches!(event.kind(), JobEventKind::Progressed(progress) if progress.completed() == 3)
        }));
        assert_eq!(
            executor
                .take_result(submission.job_id())
                .expect("worker result")
                .items[0]
                .digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn phase_10e_checksum_streaming_is_cancellable_no_follow_and_change_safe() {
        let root = tempdir().expect("root");
        let target = root.path().join("target");
        fs::write(&target, vec![7u8; CHECKSUM_CHUNK_BYTES * 2]).expect("target");
        let link = root.path().join("link");
        symlink(&target, &link).expect("link");
        let link_request =
            ChecksumRequest::new(vec![link.clone()], ChecksumAlgorithm::Sha256, None)
                .expect("request");
        assert_eq!(
            execute_checksum(&link_request, || false, |_, _| {}),
            Err(ChecksumError::NotRegular(link))
        );

        let request = ChecksumRequest::new(vec![target.clone()], ChecksumAlgorithm::Sha256, None)
            .expect("request");
        let cancel = Cell::new(false);
        assert_eq!(
            execute_checksum(&request, || cancel.get(), |_, _| cancel.set(true)),
            Err(ChecksumError::Cancelled)
        );

        let displaced = root.path().join("displaced");
        let changed = Cell::new(false);
        let error = execute_checksum(
            &request,
            || false,
            |_, _| {
                if !changed.replace(true) {
                    fs::rename(&target, &displaced).expect("displace");
                    fs::write(&target, b"replacement").expect("replace");
                }
            },
        )
        .expect_err("path replacement must be rejected");
        assert_eq!(error, ChecksumError::SourceChanged(target));
    }
}
