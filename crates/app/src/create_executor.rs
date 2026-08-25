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
    CreateCancellation, CreateError, CreateOutcome, CreateProgress, CreateRequest, JobCommand,
    JobFailure, JobFailureKind, JobId, JobProgress, OperationId, execute_create,
};
use thiserror::Error;

use crate::job_manager::{JobManagerError, SharedJobManager};

pub const DEFAULT_CREATE_QUEUE_CAPACITY: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreateSubmission {
    operation_id: OperationId,
    job_id: JobId,
}

impl CreateSubmission {
    pub const fn operation_id(self) -> OperationId {
        self.operation_id
    }

    pub const fn job_id(self) -> JobId {
        self.job_id
    }
}

#[derive(Debug, Error)]
pub enum CreateExecutorSpawnError {
    #[error("create executor queue capacity must be greater than zero")]
    ZeroCapacity,
    #[error("could not start create executor: {0}")]
    Thread(#[source] io::Error),
}

#[derive(Debug, Error)]
pub enum CreateSubmitError {
    #[error(transparent)]
    JobManager(#[from] JobManagerError),
    #[error("create queue is at capacity; job {job_id:?} was failed")]
    QueueFull { job_id: JobId },
    #[error("create executor has stopped; job {job_id:?} was failed")]
    ExecutorStopped { job_id: JobId },
}

impl CreateSubmitError {
    pub const fn job_id(&self) -> Option<JobId> {
        match self {
            Self::QueueFull { job_id } | Self::ExecutorStopped { job_id } => Some(*job_id),
            Self::JobManager(_) => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum CreateCancelError {
    #[error("job {0:?} is not an active create job")]
    NotActive(JobId),
}

#[derive(Debug)]
enum CreateCommand {
    Execute(CreateTask),
    Shutdown,
}

#[derive(Debug)]
struct CreateTask {
    job_id: JobId,
    request: CreateRequest,
    cancellation: CreateCancellation,
}

#[derive(Debug)]
pub struct CreateExecutor {
    sender: Option<SyncSender<CreateCommand>>,
    cancellations: Arc<Mutex<HashMap<JobId, CreateCancellation>>>,
    outcomes: Arc<Mutex<HashMap<JobId, CreateOutcome>>>,
    jobs: SharedJobManager,
    worker: Option<JoinHandle<()>>,
}

impl CreateExecutor {
    pub fn spawn(jobs: SharedJobManager) -> Result<Self, CreateExecutorSpawnError> {
        Self::spawn_with_capacity(jobs, DEFAULT_CREATE_QUEUE_CAPACITY)
    }

    pub fn spawn_with_capacity(
        jobs: SharedJobManager,
        capacity: usize,
    ) -> Result<Self, CreateExecutorSpawnError> {
        if capacity == 0 {
            return Err(CreateExecutorSpawnError::ZeroCapacity);
        }
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let cancellations = Arc::new(Mutex::new(HashMap::new()));
        let outcomes = Arc::new(Mutex::new(HashMap::new()));
        let worker_jobs = Arc::clone(&jobs);
        let worker_cancellations = Arc::clone(&cancellations);
        let worker_outcomes = Arc::clone(&outcomes);
        let worker = thread::Builder::new()
            .name("floe-create-worker".to_owned())
            .spawn(move || {
                run_worker(receiver, worker_jobs, worker_cancellations, worker_outcomes);
            })
            .map_err(CreateExecutorSpawnError::Thread)?;
        Ok(Self {
            sender: Some(sender),
            cancellations,
            outcomes,
            jobs,
            worker: Some(worker),
        })
    }

    pub fn submit(&self, request: CreateRequest) -> Result<CreateSubmission, CreateSubmitError> {
        let queued = lock(&self.jobs).queue_operation()?;
        self.enqueue(
            queued.operation_id(),
            queued.job_id(),
            request,
            CreateCancellation::new(),
        )
    }

    pub fn submit_retry(
        &self,
        failed_job_id: JobId,
        request: CreateRequest,
    ) -> Result<CreateSubmission, CreateSubmitError> {
        let queued = lock(&self.jobs).retry(failed_job_id)?;
        self.enqueue(
            queued.operation_id(),
            queued.job_id(),
            request,
            CreateCancellation::new(),
        )
    }

    #[cfg(test)]
    pub fn submit_with_cancellation(
        &self,
        request: CreateRequest,
        cancellation: CreateCancellation,
    ) -> Result<CreateSubmission, CreateSubmitError> {
        let queued = lock(&self.jobs).queue_operation()?;
        self.enqueue(
            queued.operation_id(),
            queued.job_id(),
            request,
            cancellation,
        )
    }

    pub fn cancel(&self, job_id: JobId) -> Result<(), CreateCancelError> {
        let cancellation = lock(&self.cancellations)
            .get(&job_id)
            .cloned()
            .ok_or(CreateCancelError::NotActive(job_id))?;
        cancellation.cancel();
        Ok(())
    }

    pub fn take_outcome(&self, job_id: JobId) -> Option<CreateOutcome> {
        lock(&self.outcomes).remove(&job_id)
    }

    fn enqueue(
        &self,
        operation_id: OperationId,
        job_id: JobId,
        request: CreateRequest,
        cancellation: CreateCancellation,
    ) -> Result<CreateSubmission, CreateSubmitError> {
        lock(&self.cancellations).insert(job_id, cancellation.clone());
        let command = CreateCommand::Execute(CreateTask {
            job_id,
            request,
            cancellation,
        });
        let result = match &self.sender {
            Some(sender) => sender.try_send(command),
            None => Err(TrySendError::Disconnected(command)),
        };
        match result {
            Ok(()) => Ok(CreateSubmission {
                operation_id,
                job_id,
            }),
            Err(TrySendError::Full(_)) => {
                lock(&self.cancellations).remove(&job_id);
                fail_submission(&self.jobs, job_id, "create executor queue is full");
                Err(CreateSubmitError::QueueFull { job_id })
            }
            Err(TrySendError::Disconnected(_)) => {
                lock(&self.cancellations).remove(&job_id);
                fail_submission(&self.jobs, job_id, "create executor stopped");
                Err(CreateSubmitError::ExecutorStopped { job_id })
            }
        }
    }

    fn stop(&mut self) {
        for cancellation in lock(&self.cancellations).values() {
            cancellation.cancel();
        }
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(CreateCommand::Shutdown);
        }
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::error!("create worker panicked during shutdown");
        }
    }
}

impl Drop for CreateExecutor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_worker(
    receiver: Receiver<CreateCommand>,
    jobs: SharedJobManager,
    cancellations: Arc<Mutex<HashMap<JobId, CreateCancellation>>>,
    outcomes: Arc<Mutex<HashMap<JobId, CreateOutcome>>>,
) {
    while let Ok(command) = receiver.recv() {
        match command {
            CreateCommand::Execute(task) => {
                execute_task(task, &jobs, &cancellations, &outcomes);
            }
            CreateCommand::Shutdown => break,
        }
    }
}

fn execute_task(
    task: CreateTask,
    jobs: &SharedJobManager,
    cancellations: &Arc<Mutex<HashMap<JobId, CreateCancellation>>>,
    outcomes: &Arc<Mutex<HashMap<JobId, CreateOutcome>>>,
) {
    if transition(jobs, task.job_id, JobCommand::Start).is_err() {
        lock(cancellations).remove(&task.job_id);
        return;
    }
    let result = execute_create(&task.request, &task.cancellation, |progress| {
        if let Some(progress) = create_job_progress(progress) {
            let _ = transition(jobs, task.job_id, JobCommand::SetProgress(progress));
        }
    });
    let command = match result {
        Ok(outcome) => {
            lock(outcomes).insert(task.job_id, outcome);
            JobCommand::Complete
        }
        Err(CreateError::Cancelled) => JobCommand::Cancel,
        Err(error) => JobCommand::Fail(create_failure(&error)),
    };
    let _ = transition(jobs, task.job_id, command);
    lock(cancellations).remove(&task.job_id);
}

fn create_job_progress(progress: CreateProgress) -> Option<JobProgress> {
    match progress {
        CreateProgress::Item { completed, total } => {
            JobProgress::items(completed, Some(total)).ok()
        }
        CreateProgress::Copy(progress) if progress.total_bytes() > 0 => {
            JobProgress::bytes(progress.bytes_copied(), Some(progress.total_bytes())).ok()
        }
        CreateProgress::Copy(progress) => {
            JobProgress::items(progress.entries_copied(), Some(progress.total_entries())).ok()
        }
    }
}

fn create_failure(error: &CreateError) -> JobFailure {
    let kind = if error.is_conflict() {
        JobFailureKind::Conflict
    } else if error.is_unsupported() {
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
            tracing::error!(job_id = job_id.get(), %error, "create job transition failed");
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
        fs,
        time::{Duration, Instant},
    };

    use crate::job_manager::ApplicationJobManager;
    use floe_core::JobState;
    use tempfile::tempdir;

    use super::*;

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
            assert!(Instant::now() < deadline, "create job timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn phase_6q_state_executor_completes_conflicts_and_cancels() {
        let fixture = tempdir().expect("temporary fixture");
        let jobs = jobs();
        let executor = CreateExecutor::spawn(Arc::clone(&jobs)).expect("executor");
        let destination = fixture.path().join("created");
        let completed = executor
            .submit(CreateRequest::directory(&destination).expect("request"))
            .expect("submission");
        assert_eq!(
            wait_for_terminal(&jobs, completed.job_id()),
            JobState::Completed
        );
        assert_eq!(
            executor
                .take_outcome(completed.job_id())
                .expect("outcome")
                .destination(),
            destination
        );

        let conflict = executor
            .submit(CreateRequest::directory(&destination).expect("request"))
            .expect("submission");
        assert_eq!(
            wait_for_terminal(&jobs, conflict.job_id()),
            JobState::Failed
        );

        let cancelled_path = fixture.path().join("cancelled");
        let cancellation = CreateCancellation::new();
        cancellation.cancel();
        let cancelled = executor
            .submit_with_cancellation(
                CreateRequest::empty_file(&cancelled_path).expect("request"),
                cancellation,
            )
            .expect("submission");
        assert_eq!(
            wait_for_terminal(&jobs, cancelled.job_id()),
            JobState::Cancelled
        );
        assert!(!cancelled_path.exists());
        assert!(
            fs::metadata(destination)
                .expect("created directory")
                .is_dir()
        );
    }
}
