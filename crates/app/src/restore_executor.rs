use std::{
    collections::HashMap,
    io,
    sync::{
        Arc, Mutex, MutexGuard,
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

use floe_core::{
    JobCommand, JobFailure, JobFailureKind, JobId, MoveCancellation, MoveError, OperationId,
    RestoreError, RestoreRequest, execute_restore,
};
use thiserror::Error;

use crate::job_manager::{JobManagerError, SharedJobManager};

pub const DEFAULT_RESTORE_QUEUE_CAPACITY: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RestoreSubmission {
    operation_id: OperationId,
    job_id: JobId,
}

impl RestoreSubmission {
    pub const fn operation_id(self) -> OperationId {
        self.operation_id
    }

    pub const fn job_id(self) -> JobId {
        self.job_id
    }
}

#[derive(Debug, Error)]
pub enum RestoreExecutorSpawnError {
    #[error("restore executor queue capacity must be greater than zero")]
    ZeroCapacity,
    #[error("could not start restore executor: {0}")]
    Thread(#[source] io::Error),
}

#[derive(Debug, Error)]
pub enum RestoreSubmitError {
    #[error(transparent)]
    JobManager(#[from] JobManagerError),
    #[error("restore queue is at capacity; job {job_id:?} was failed")]
    QueueFull { job_id: JobId },
    #[error("restore executor has stopped; job {job_id:?} was failed")]
    ExecutorStopped { job_id: JobId },
}

impl RestoreSubmitError {
    pub const fn job_id(&self) -> Option<JobId> {
        match self {
            Self::QueueFull { job_id } | Self::ExecutorStopped { job_id } => Some(*job_id),
            Self::JobManager(_) => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum RestoreCancelError {
    #[error("job {0:?} is not an active restore job")]
    NotActive(JobId),
}

pub(crate) trait RestoreBackend: Send + Sync + 'static {
    fn restore(
        &self,
        request: &RestoreRequest,
        cancellation: &MoveCancellation,
    ) -> Result<(), RestoreError>;
}

#[derive(Debug, Default)]
struct FilesystemRestoreBackend;

impl RestoreBackend for FilesystemRestoreBackend {
    fn restore(
        &self,
        request: &RestoreRequest,
        cancellation: &MoveCancellation,
    ) -> Result<(), RestoreError> {
        execute_restore(request, cancellation).map(|_| ())
    }
}

struct RestoreTask {
    job_id: JobId,
    request: RestoreRequest,
    cancellation: MoveCancellation,
}

enum RestoreCommand {
    Execute(RestoreTask),
    Shutdown,
}

#[derive(Debug)]
pub struct RestoreExecutor {
    sender: Option<SyncSender<RestoreCommand>>,
    cancellations: Arc<Mutex<HashMap<JobId, MoveCancellation>>>,
    jobs: SharedJobManager,
    worker: Option<JoinHandle<()>>,
}

impl RestoreExecutor {
    pub fn spawn(jobs: SharedJobManager) -> Result<Self, RestoreExecutorSpawnError> {
        Self::spawn_with_backend(
            jobs,
            DEFAULT_RESTORE_QUEUE_CAPACITY,
            Arc::new(FilesystemRestoreBackend),
        )
    }

    fn spawn_with_backend(
        jobs: SharedJobManager,
        capacity: usize,
        backend: Arc<dyn RestoreBackend>,
    ) -> Result<Self, RestoreExecutorSpawnError> {
        if capacity == 0 {
            return Err(RestoreExecutorSpawnError::ZeroCapacity);
        }
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let cancellations = Arc::new(Mutex::new(HashMap::new()));
        let worker_jobs = Arc::clone(&jobs);
        let worker_cancellations = Arc::clone(&cancellations);
        let worker = thread::Builder::new()
            .name("floe-restore-worker".to_owned())
            .spawn(move || run_worker(receiver, worker_jobs, worker_cancellations, backend))
            .map_err(RestoreExecutorSpawnError::Thread)?;
        Ok(Self {
            sender: Some(sender),
            cancellations,
            jobs,
            worker: Some(worker),
        })
    }

    pub fn submit(&self, request: RestoreRequest) -> Result<RestoreSubmission, RestoreSubmitError> {
        let queued = lock(&self.jobs).queue_operation()?;
        self.enqueue(queued.operation_id(), queued.job_id(), request)
    }

    pub fn submit_retry(
        &self,
        failed_job_id: JobId,
        request: RestoreRequest,
    ) -> Result<RestoreSubmission, RestoreSubmitError> {
        let queued = lock(&self.jobs).retry(failed_job_id)?;
        self.enqueue(queued.operation_id(), queued.job_id(), request)
    }

    fn enqueue(
        &self,
        operation_id: OperationId,
        job_id: JobId,
        request: RestoreRequest,
    ) -> Result<RestoreSubmission, RestoreSubmitError> {
        let cancellation = MoveCancellation::new();
        lock(&self.cancellations).insert(job_id, cancellation.clone());
        let command = RestoreCommand::Execute(RestoreTask {
            job_id,
            request,
            cancellation,
        });
        match &self.sender {
            Some(sender) => match sender.try_send(command) {
                Ok(()) => Ok(RestoreSubmission {
                    operation_id,
                    job_id,
                }),
                Err(TrySendError::Full(_)) => {
                    lock(&self.cancellations).remove(&job_id);
                    fail_submission(&self.jobs, job_id, "restore queue is at capacity");
                    Err(RestoreSubmitError::QueueFull { job_id })
                }
                Err(TrySendError::Disconnected(_)) => {
                    lock(&self.cancellations).remove(&job_id);
                    fail_submission(&self.jobs, job_id, "restore executor has stopped");
                    Err(RestoreSubmitError::ExecutorStopped { job_id })
                }
            },
            None => {
                lock(&self.cancellations).remove(&job_id);
                fail_submission(&self.jobs, job_id, "restore executor has stopped");
                Err(RestoreSubmitError::ExecutorStopped { job_id })
            }
        }
    }

    pub fn cancel(&self, job_id: JobId) -> Result<(), RestoreCancelError> {
        let cancellation = lock(&self.cancellations)
            .get(&job_id)
            .cloned()
            .ok_or(RestoreCancelError::NotActive(job_id))?;
        cancellation.cancel();
        Ok(())
    }

    fn stop(&mut self) {
        for cancellation in lock(&self.cancellations).values() {
            cancellation.cancel();
        }
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(RestoreCommand::Shutdown);
        }
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::error!("restore worker panicked during shutdown");
        }
    }
}

impl Drop for RestoreExecutor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_worker(
    receiver: Receiver<RestoreCommand>,
    jobs: SharedJobManager,
    cancellations: Arc<Mutex<HashMap<JobId, MoveCancellation>>>,
    backend: Arc<dyn RestoreBackend>,
) {
    while let Ok(command) = receiver.recv() {
        match command {
            RestoreCommand::Execute(task) => {
                execute_task(task, &jobs, &cancellations, backend.as_ref())
            }
            RestoreCommand::Shutdown => break,
        }
    }
}

fn execute_task(
    task: RestoreTask,
    jobs: &SharedJobManager,
    cancellations: &Arc<Mutex<HashMap<JobId, MoveCancellation>>>,
    backend: &dyn RestoreBackend,
) {
    if transition(jobs, task.job_id, JobCommand::Start).is_err() {
        lock(cancellations).remove(&task.job_id);
        return;
    }
    let command = match backend.restore(&task.request, &task.cancellation) {
        Ok(()) => JobCommand::Complete,
        Err(RestoreError::Move(MoveError::Cancelled)) => JobCommand::Cancel,
        Err(error) => JobCommand::Fail(restore_failure(&error)),
    };
    let _ = transition(jobs, task.job_id, command);
    lock(cancellations).remove(&task.job_id);
}

fn restore_failure(error: &RestoreError) -> JobFailure {
    let kind = if error.is_partial() {
        JobFailureKind::Partial
    } else if error.is_conflict() {
        JobFailureKind::Conflict
    } else if matches!(error, RestoreError::Move(move_error) if move_error.is_unsupported()) {
        JobFailureKind::Unsupported
    } else if error.io_kind() == Some(io::ErrorKind::PermissionDenied) {
        JobFailureKind::PermissionDenied
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
    lock(jobs)
        .transition(job_id, command)
        .map(|_| ())
        .map_err(|error| {
            tracing::error!(job_id = job_id.get(), %error, "restore job transition failed");
            error
        })
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
        fs, thread,
        time::{Duration, Instant},
    };

    use tempfile::tempdir;

    use super::*;
    use crate::job_manager::ApplicationJobManager;
    use floe_core::JobState;

    fn wait_for_terminal(jobs: &SharedJobManager, job_id: JobId) -> JobState {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(state) = lock(jobs).record(job_id).map(|record| record.state())
                && state.is_terminal()
            {
                return state;
            }
            assert!(Instant::now() < deadline, "restore job did not finish");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn phase_6n_worker_restores_on_bounded_worker_and_completes_shared_job() {
        let fixture = tempdir().expect("fixture");
        let trash = fixture.path().join("Trash/files/item");
        let info = fixture.path().join("Trash/info/item.trashinfo");
        let destination = fixture.path().join("restored");
        fs::create_dir_all(trash.parent().expect("files parent")).expect("files directory");
        fs::create_dir_all(info.parent().expect("info parent")).expect("info directory");
        fs::write(&trash, b"payload").expect("payload");
        fs::write(&info, b"metadata").expect("metadata");
        let jobs = Arc::new(Mutex::new(ApplicationJobManager::new()));
        let executor = RestoreExecutor::spawn(Arc::clone(&jobs)).expect("executor");
        let submission = executor
            .submit(RestoreRequest::new(&trash, &info, &destination).expect("request"))
            .expect("submission");

        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Completed
        );
        assert_eq!(fs::read(destination).expect("restored"), b"payload");
    }

    #[test]
    fn phase_6n_worker_maps_no_overwrite_restore_to_conflict() {
        let fixture = tempdir().expect("fixture");
        let trash = fixture.path().join("Trash/files/item");
        let info = fixture.path().join("Trash/info/item.trashinfo");
        let destination = fixture.path().join("restored");
        fs::create_dir_all(trash.parent().expect("files parent")).expect("files directory");
        fs::create_dir_all(info.parent().expect("info parent")).expect("info directory");
        fs::write(&trash, b"payload").expect("payload");
        fs::write(&info, b"metadata").expect("metadata");
        fs::write(&destination, b"existing").expect("existing");
        let jobs = Arc::new(Mutex::new(ApplicationJobManager::new()));
        let executor = RestoreExecutor::spawn(Arc::clone(&jobs)).expect("executor");
        let submission = executor
            .submit(RestoreRequest::new(&trash, &info, &destination).expect("request"))
            .expect("submission");

        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Failed
        );
        let failure_kind = lock(&jobs)
            .record(submission.job_id())
            .and_then(|record| record.failure())
            .map(JobFailure::kind)
            .expect("failure");
        assert_eq!(failure_kind, JobFailureKind::Conflict);
        assert_eq!(fs::read(destination).expect("existing"), b"existing");
    }
}
