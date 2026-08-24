use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

use floe_core::{JobCommand, JobFailure, JobFailureKind, JobId, OperationId};
use gio::prelude::*;
use gtk::{gio, glib};
use thiserror::Error;

use crate::job_manager::{JobManagerError, SharedJobManager};

pub const DEFAULT_TRASH_QUEUE_CAPACITY: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrashRequest {
    source: PathBuf,
}

impl TrashRequest {
    pub fn new(source: PathBuf) -> Result<Self, TrashRequestError> {
        if source.file_name().is_none() {
            return Err(TrashRequestError::InvalidSource(source));
        }
        Ok(Self { source })
    }

    pub fn source(&self) -> &Path {
        &self.source
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TrashRequestError {
    #[error("this path cannot be moved to Trash: {}", .0.display())]
    InvalidSource(PathBuf),
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TrashError {
    #[error("trash operation was cancelled")]
    Cancelled,
    #[error("trash source was not found: {message}")]
    NotFound { message: String },
    #[error("permission was denied while moving item to Trash: {message}")]
    PermissionDenied { message: String },
    #[error("this location does not support Trash: {message}")]
    NotSupported { message: String },
    #[error("could not move item to Trash: {message}")]
    Io { message: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrashSubmission {
    operation_id: OperationId,
    job_id: JobId,
}

impl TrashSubmission {
    pub const fn operation_id(self) -> OperationId {
        self.operation_id
    }

    pub const fn job_id(self) -> JobId {
        self.job_id
    }
}

#[derive(Debug, Error)]
pub enum TrashExecutorSpawnError {
    #[error("trash executor queue capacity must be greater than zero")]
    ZeroCapacity,
    #[error("could not start trash executor: {0}")]
    Thread(#[source] io::Error),
}

#[derive(Debug, Error)]
pub enum TrashSubmitError {
    #[error(transparent)]
    JobManager(#[from] JobManagerError),
    #[error("trash queue is at capacity; job {job_id:?} was failed")]
    QueueFull { job_id: JobId },
    #[error("trash executor has stopped; job {job_id:?} was failed")]
    ExecutorStopped { job_id: JobId },
}

impl TrashSubmitError {
    pub const fn job_id(&self) -> Option<JobId> {
        match self {
            Self::QueueFull { job_id } | Self::ExecutorStopped { job_id } => Some(*job_id),
            Self::JobManager(_) => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum TrashCancelError {
    #[error("job {0:?} is not an active trash job")]
    NotActive(JobId),
}

pub(crate) trait TrashBackend: Send + Sync + 'static {
    fn trash(
        &self,
        request: &TrashRequest,
        cancellable: &gio::Cancellable,
    ) -> Result<(), TrashError>;
}

#[derive(Debug, Default)]
pub(crate) struct GioTrashBackend;

impl TrashBackend for GioTrashBackend {
    fn trash(
        &self,
        request: &TrashRequest,
        cancellable: &gio::Cancellable,
    ) -> Result<(), TrashError> {
        if cancellable.is_cancelled() {
            return Err(TrashError::Cancelled);
        }

        gio::File::for_path(request.source())
            .trash(Some(cancellable))
            .map_err(map_gio_error)
    }
}

struct TrashTask {
    job_id: JobId,
    request: TrashRequest,
    cancellable: gio::Cancellable,
}

enum TrashCommand {
    Execute(TrashTask),
    Shutdown,
}

/// Fixed-capacity, single-worker GIO trash executor.
///
/// Synchronous GIO work runs on the named worker, never in a GTK callback.
#[derive(Debug)]
pub struct TrashExecutor {
    sender: Option<SyncSender<TrashCommand>>,
    cancellations: Arc<Mutex<HashMap<JobId, gio::Cancellable>>>,
    jobs: SharedJobManager,
    worker: Option<JoinHandle<()>>,
}

impl TrashExecutor {
    pub fn spawn(jobs: SharedJobManager) -> Result<Self, TrashExecutorSpawnError> {
        Self::spawn_with_backend_and_gate(
            jobs,
            DEFAULT_TRASH_QUEUE_CAPACITY,
            Arc::new(GioTrashBackend),
            None,
        )
    }

    fn spawn_with_backend_and_gate(
        jobs: SharedJobManager,
        capacity: usize,
        backend: Arc<dyn TrashBackend>,
        start_gate: Option<Receiver<()>>,
    ) -> Result<Self, TrashExecutorSpawnError> {
        if capacity == 0 {
            return Err(TrashExecutorSpawnError::ZeroCapacity);
        }

        let (sender, receiver) = mpsc::sync_channel(capacity);
        let cancellations = Arc::new(Mutex::new(HashMap::new()));
        let worker_jobs = Arc::clone(&jobs);
        let worker_cancellations = Arc::clone(&cancellations);
        let worker = thread::Builder::new()
            .name("floe-trash-worker".to_owned())
            .spawn(move || {
                if let Some(gate) = start_gate {
                    let _ = gate.recv();
                }
                run_worker(receiver, worker_jobs, worker_cancellations, backend);
            })
            .map_err(TrashExecutorSpawnError::Thread)?;

        Ok(Self {
            sender: Some(sender),
            cancellations,
            jobs,
            worker: Some(worker),
        })
    }

    pub fn submit_trash(&self, request: TrashRequest) -> Result<TrashSubmission, TrashSubmitError> {
        self.submit_with_cancellable(request, gio::Cancellable::new())
    }

    pub fn submit_trash_retry(
        &self,
        failed_job_id: JobId,
        request: TrashRequest,
    ) -> Result<TrashSubmission, TrashSubmitError> {
        let queued = lock(&self.jobs).retry(failed_job_id)?;
        self.enqueue(
            queued.operation_id(),
            queued.job_id(),
            request,
            gio::Cancellable::new(),
        )
    }

    pub fn cancel(&self, job_id: JobId) -> Result<(), TrashCancelError> {
        let cancellable = lock(&self.cancellations)
            .get(&job_id)
            .cloned()
            .ok_or(TrashCancelError::NotActive(job_id))?;
        cancellable.cancel();
        Ok(())
    }

    pub fn shutdown(mut self) {
        self.stop();
    }

    fn submit_with_cancellable(
        &self,
        request: TrashRequest,
        cancellable: gio::Cancellable,
    ) -> Result<TrashSubmission, TrashSubmitError> {
        let queued = lock(&self.jobs).queue_operation()?;
        self.enqueue(queued.operation_id(), queued.job_id(), request, cancellable)
    }

    fn enqueue(
        &self,
        operation_id: OperationId,
        job_id: JobId,
        request: TrashRequest,
        cancellable: gio::Cancellable,
    ) -> Result<TrashSubmission, TrashSubmitError> {
        lock(&self.cancellations).insert(job_id, cancellable.clone());
        let command = TrashCommand::Execute(TrashTask {
            job_id,
            request,
            cancellable,
        });

        match &self.sender {
            Some(sender) => match sender.try_send(command) {
                Ok(()) => Ok(TrashSubmission {
                    operation_id,
                    job_id,
                }),
                Err(TrySendError::Full(_)) => {
                    lock(&self.cancellations).remove(&job_id);
                    fail_submission(&self.jobs, job_id, "trash queue is at capacity");
                    Err(TrashSubmitError::QueueFull { job_id })
                }
                Err(TrySendError::Disconnected(_)) => {
                    lock(&self.cancellations).remove(&job_id);
                    fail_submission(&self.jobs, job_id, "trash executor has stopped");
                    Err(TrashSubmitError::ExecutorStopped { job_id })
                }
            },
            None => {
                lock(&self.cancellations).remove(&job_id);
                fail_submission(&self.jobs, job_id, "trash executor has stopped");
                Err(TrashSubmitError::ExecutorStopped { job_id })
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn spawn_with_backend(
        jobs: SharedJobManager,
        capacity: usize,
        backend: Arc<dyn TrashBackend>,
    ) -> Result<Self, TrashExecutorSpawnError> {
        Self::spawn_with_backend_and_gate(jobs, capacity, backend, None)
    }

    #[cfg(test)]
    fn spawn_blocked(
        jobs: SharedJobManager,
        capacity: usize,
        backend: Arc<dyn TrashBackend>,
        start_gate: Receiver<()>,
    ) -> Result<Self, TrashExecutorSpawnError> {
        Self::spawn_with_backend_and_gate(jobs, capacity, backend, Some(start_gate))
    }

    fn stop(&mut self) {
        for cancellable in lock(&self.cancellations).values() {
            cancellable.cancel();
        }
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(TrashCommand::Shutdown);
        }
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("trash executor worker panicked during shutdown");
        }
    }
}

impl Drop for TrashExecutor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_worker(
    receiver: Receiver<TrashCommand>,
    jobs: SharedJobManager,
    cancellations: Arc<Mutex<HashMap<JobId, gio::Cancellable>>>,
    backend: Arc<dyn TrashBackend>,
) {
    while let Ok(command) = receiver.recv() {
        match command {
            TrashCommand::Execute(task) => {
                execute_task(task, &jobs, &cancellations, backend.as_ref());
            }
            TrashCommand::Shutdown => break,
        }
    }
}

fn execute_task(
    task: TrashTask,
    jobs: &SharedJobManager,
    cancellations: &Arc<Mutex<HashMap<JobId, gio::Cancellable>>>,
    backend: &dyn TrashBackend,
) {
    if transition(jobs, task.job_id, JobCommand::Start).is_err() {
        lock(cancellations).remove(&task.job_id);
        return;
    }

    let command = match backend.trash(&task.request, &task.cancellable) {
        Ok(()) => JobCommand::Complete,
        Err(TrashError::Cancelled) => JobCommand::Cancel,
        Err(error) => JobCommand::Fail(trash_failure(&error)),
    };
    let _ = transition(jobs, task.job_id, command);
    lock(cancellations).remove(&task.job_id);
}

fn transition(
    jobs: &SharedJobManager,
    job_id: JobId,
    command: JobCommand,
) -> Result<(), JobManagerError> {
    if let Err(error) = lock(jobs).transition(job_id, command) {
        tracing::error!(job_id = job_id.get(), %error, "trash job transition failed");
        Err(error)
    } else {
        Ok(())
    }
}

fn fail_submission(jobs: &SharedJobManager, job_id: JobId, message: &'static str) {
    let _ = transition(
        jobs,
        job_id,
        JobCommand::Fail(JobFailure::new(JobFailureKind::Internal, message)),
    );
}

fn map_gio_error(error: glib::Error) -> TrashError {
    let message = error.message().to_owned();
    if error.matches(gio::IOErrorEnum::Cancelled) {
        TrashError::Cancelled
    } else if error.matches(gio::IOErrorEnum::NotFound) {
        TrashError::NotFound { message }
    } else if error.matches(gio::IOErrorEnum::PermissionDenied) {
        TrashError::PermissionDenied { message }
    } else if error.matches(gio::IOErrorEnum::NotSupported) {
        TrashError::NotSupported { message }
    } else {
        TrashError::Io { message }
    }
}

fn trash_failure(error: &TrashError) -> JobFailure {
    let kind = match error {
        TrashError::PermissionDenied { .. } => JobFailureKind::PermissionDenied,
        TrashError::NotSupported { .. } => JobFailureKind::Unsupported,
        TrashError::NotFound { .. } | TrashError::Io { .. } => JobFailureKind::Io,
        TrashError::Cancelled => JobFailureKind::Internal,
    };
    JobFailure::new(kind, error.to_string())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        os::unix::ffi::{OsStrExt, OsStringExt},
        sync::atomic::{AtomicUsize, Ordering},
        time::{Duration, Instant},
    };

    use floe_core::{JobEventKind, JobState};

    use super::*;
    use crate::job_manager::ApplicationJobManager;

    #[derive(Clone, Debug)]
    enum TestBehavior {
        Success,
        WaitForCancellation,
        Fail(TrashError),
    }

    #[derive(Debug)]
    struct TestBackend {
        behavior: TestBehavior,
        calls: AtomicUsize,
        paths: Mutex<Vec<PathBuf>>,
    }

    impl TestBackend {
        fn new(behavior: TestBehavior) -> Self {
            Self {
                behavior,
                calls: AtomicUsize::new(0),
                paths: Mutex::new(Vec::new()),
            }
        }
    }

    impl TrashBackend for TestBackend {
        fn trash(
            &self,
            request: &TrashRequest,
            cancellable: &gio::Cancellable,
        ) -> Result<(), TrashError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            lock(&self.paths).push(request.source().to_path_buf());
            match &self.behavior {
                TestBehavior::Success => Ok(()),
                TestBehavior::WaitForCancellation => {
                    let deadline = Instant::now() + Duration::from_secs(3);
                    while !cancellable.is_cancelled() {
                        assert!(
                            Instant::now() < deadline,
                            "trash cancellation was not observed"
                        );
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(TrashError::Cancelled)
                }
                TestBehavior::Fail(error) => Err(error.clone()),
            }
        }
    }

    fn jobs() -> SharedJobManager {
        Arc::new(Mutex::new(ApplicationJobManager::new()))
    }

    fn wait_for_terminal(jobs: &SharedJobManager, job_id: JobId) -> JobState {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(state) = lock(jobs).record(job_id).map(|record| record.state())
                && state.is_terminal()
            {
                return state;
            }
            assert!(
                Instant::now() < deadline,
                "trash job did not become terminal"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn phase_4e_request_preserves_non_utf8_source() {
        let name = OsString::from_vec(b"trash-\xff".to_vec());
        let source = PathBuf::from("/tmp").join(&name);
        let request = TrashRequest::new(source.clone()).expect("source should be accepted");

        assert_eq!(request.source(), source);
        assert_eq!(
            request.source().file_name().expect("name").as_bytes(),
            name.as_bytes()
        );
        assert!(matches!(
            TrashRequest::new(PathBuf::from("/")),
            Err(TrashRequestError::InvalidSource(_))
        ));
        assert!(matches!(
            TrashRequest::new(PathBuf::new()),
            Err(TrashRequestError::InvalidSource(_))
        ));
    }

    #[test]
    fn phase_4e_gio_errors_map_without_touching_real_trash() {
        assert_eq!(
            map_gio_error(glib::Error::new(gio::IOErrorEnum::Cancelled, "cancelled")),
            TrashError::Cancelled
        );
        assert_eq!(
            map_gio_error(glib::Error::new(gio::IOErrorEnum::NotFound, "missing")),
            TrashError::NotFound {
                message: "missing".to_owned()
            }
        );
        assert_eq!(
            map_gio_error(glib::Error::new(
                gio::IOErrorEnum::PermissionDenied,
                "denied"
            )),
            TrashError::PermissionDenied {
                message: "denied".to_owned()
            }
        );
        assert_eq!(
            map_gio_error(glib::Error::new(
                gio::IOErrorEnum::NotSupported,
                "unsupported"
            )),
            TrashError::NotSupported {
                message: "unsupported".to_owned()
            }
        );
    }

    #[test]
    fn phase_4e_executor_completes_without_touching_real_trash() {
        let jobs = jobs();
        let backend = Arc::new(TestBackend::new(TestBehavior::Success));
        let executor = TrashExecutor::spawn_with_backend(jobs.clone(), 2, backend.clone())
            .expect("trash executor should start");
        let source = PathBuf::from("/virtual/original");
        let submission = executor
            .submit_trash(TrashRequest::new(source.clone()).expect("valid request"))
            .expect("trash should be submitted");

        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Completed
        );
        assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
        assert_eq!(lock(&backend.paths).as_slice(), &[source]);
        assert!(lock(&jobs).drain_events().iter().any(|event| {
            event.job_id() == submission.job_id() && event.kind() == &JobEventKind::Completed
        }));
    }

    #[test]
    fn phase_4e_executor_cancels_through_gio_cancellable() {
        let jobs = jobs();
        let backend = Arc::new(TestBackend::new(TestBehavior::WaitForCancellation));
        let executor = TrashExecutor::spawn_with_backend(jobs.clone(), 2, backend)
            .expect("trash executor should start");
        let submission = executor
            .submit_trash(TrashRequest::new(PathBuf::from("/virtual/item")).expect("valid request"))
            .expect("trash should be submitted");

        executor
            .cancel(submission.job_id())
            .expect("active job should cancel");
        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Cancelled
        );
    }

    #[test]
    fn phase_4e_executor_maps_structured_failures() {
        for (error, expected) in [
            (
                TrashError::PermissionDenied {
                    message: "denied".to_owned(),
                },
                JobFailureKind::PermissionDenied,
            ),
            (
                TrashError::NotSupported {
                    message: "unsupported".to_owned(),
                },
                JobFailureKind::Unsupported,
            ),
            (
                TrashError::NotFound {
                    message: "missing".to_owned(),
                },
                JobFailureKind::Io,
            ),
        ] {
            assert_eq!(trash_failure(&error).kind(), expected);
        }
    }

    #[test]
    fn phase_4e_executor_rejects_work_beyond_capacity() {
        let jobs = jobs();
        let backend = Arc::new(TestBackend::new(TestBehavior::Success));
        let (gate_sender, gate_receiver) = mpsc::sync_channel(1);
        let executor = TrashExecutor::spawn_blocked(jobs.clone(), 1, backend, gate_receiver)
            .expect("blocked executor should start");
        let first = executor
            .submit_trash(TrashRequest::new(PathBuf::from("/virtual/first")).expect("request"))
            .expect("first request should fill queue");
        let error = executor
            .submit_trash(TrashRequest::new(PathBuf::from("/virtual/second")).expect("request"))
            .expect_err("second request should exceed capacity");
        let rejected = error.job_id().expect("capacity error should retain job id");

        assert_eq!(
            lock(&jobs).record(rejected).map(|record| record.state()),
            Some(JobState::Failed)
        );
        gate_sender.send(()).expect("worker gate should release");
        assert_eq!(
            wait_for_terminal(&jobs, first.job_id()),
            JobState::Completed
        );
    }

    #[test]
    fn phase_4e_shutdown_cancels_queued_work() {
        let jobs = jobs();
        let backend = Arc::new(TestBackend::new(TestBehavior::WaitForCancellation));
        let (gate_sender, gate_receiver) = mpsc::sync_channel(1);
        let executor = TrashExecutor::spawn_blocked(jobs.clone(), 1, backend, gate_receiver)
            .expect("blocked executor should start");
        let submission = executor
            .submit_trash(TrashRequest::new(PathBuf::from("/virtual/item")).expect("request"))
            .expect("request should queue");
        let shutdown = thread::spawn(move || executor.shutdown());
        gate_sender.send(()).expect("worker gate should release");
        shutdown.join().expect("shutdown should finish");

        assert_eq!(
            lock(&jobs)
                .record(submission.job_id())
                .map(|record| record.state()),
            Some(JobState::Cancelled)
        );
    }

    #[test]
    fn phase_4e_failure_backend_never_touches_filesystem() {
        let jobs = jobs();
        let backend = Arc::new(TestBackend::new(TestBehavior::Fail(TrashError::Io {
            message: "fixture failure".to_owned(),
        })));
        let executor = TrashExecutor::spawn_with_backend(jobs.clone(), 1, backend)
            .expect("executor should start");
        let submission = executor
            .submit_trash(TrashRequest::new(PathBuf::from("/virtual/item")).expect("request"))
            .expect("request should queue");

        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Failed
        );
    }
}
