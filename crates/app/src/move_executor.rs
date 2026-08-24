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
    JobCommand, JobFailure, JobFailureKind, JobId, MoveCancellation, MoveError, MoveRequest,
    OperationId, RenameRequest, execute_move, execute_rename,
};
use thiserror::Error;

use crate::job_manager::{JobManagerError, SharedJobManager};

pub const DEFAULT_MOVE_QUEUE_CAPACITY: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MoveSubmission {
    operation_id: OperationId,
    job_id: JobId,
}

impl MoveSubmission {
    pub const fn operation_id(self) -> OperationId {
        self.operation_id
    }

    pub const fn job_id(self) -> JobId {
        self.job_id
    }
}

#[derive(Debug, Error)]
pub enum MoveExecutorSpawnError {
    #[error("move executor queue capacity must be greater than zero")]
    ZeroCapacity,
    #[error("could not start move executor: {0}")]
    Thread(#[source] io::Error),
}

#[derive(Debug, Error)]
pub enum MoveSubmitError {
    #[error(transparent)]
    JobManager(#[from] JobManagerError),
    #[error("move queue is at capacity; job {job_id:?} was failed")]
    QueueFull { job_id: JobId },
    #[error("move executor has stopped; job {job_id:?} was failed")]
    ExecutorStopped { job_id: JobId },
}

impl MoveSubmitError {
    pub const fn job_id(&self) -> Option<JobId> {
        match self {
            Self::QueueFull { job_id } | Self::ExecutorStopped { job_id } => Some(*job_id),
            Self::JobManager(_) => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum MoveCancelError {
    #[error("job {0:?} is not an active move or rename job")]
    NotActive(JobId),
}

#[derive(Debug)]
enum MoveOperation {
    Move(MoveRequest),
    Rename(RenameRequest),
}

#[derive(Debug)]
struct MoveTask {
    job_id: JobId,
    operation: MoveOperation,
    cancellation: MoveCancellation,
}

#[derive(Debug)]
enum MoveCommand {
    Execute(MoveTask),
    Shutdown,
}

/// Fixed-capacity, single-worker executor for atomic move and rename jobs.
///
/// GTK never calls the filesystem engine through this type. Application
/// commands may submit requests, while the worker reports structured lifecycle
/// events through the shared job manager.
#[derive(Debug)]
pub struct MoveExecutor {
    sender: Option<SyncSender<MoveCommand>>,
    cancellations: Arc<Mutex<HashMap<JobId, MoveCancellation>>>,
    jobs: SharedJobManager,
    worker: Option<JoinHandle<()>>,
}

impl MoveExecutor {
    pub fn spawn(jobs: SharedJobManager) -> Result<Self, MoveExecutorSpawnError> {
        Self::spawn_with_capacity(jobs, DEFAULT_MOVE_QUEUE_CAPACITY, None)
    }

    fn spawn_with_capacity(
        jobs: SharedJobManager,
        capacity: usize,
        start_gate: Option<Receiver<()>>,
    ) -> Result<Self, MoveExecutorSpawnError> {
        if capacity == 0 {
            return Err(MoveExecutorSpawnError::ZeroCapacity);
        }
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let cancellations = Arc::new(Mutex::new(HashMap::new()));
        let worker_jobs = Arc::clone(&jobs);
        let worker_cancellations = Arc::clone(&cancellations);
        let worker = thread::Builder::new()
            .name("floe-move-worker".to_owned())
            .spawn(move || {
                if let Some(gate) = start_gate {
                    let _ = gate.recv();
                }
                run_worker(receiver, worker_jobs, worker_cancellations);
            })
            .map_err(MoveExecutorSpawnError::Thread)?;
        Ok(Self {
            sender: Some(sender),
            cancellations,
            jobs,
            worker: Some(worker),
        })
    }

    pub fn submit_move(&self, request: MoveRequest) -> Result<MoveSubmission, MoveSubmitError> {
        self.submit(MoveOperation::Move(request), MoveCancellation::new())
    }

    pub fn submit_rename(&self, request: RenameRequest) -> Result<MoveSubmission, MoveSubmitError> {
        self.submit(MoveOperation::Rename(request), MoveCancellation::new())
    }

    pub fn cancel(&self, job_id: JobId) -> Result<(), MoveCancelError> {
        let cancellation = lock(&self.cancellations)
            .get(&job_id)
            .cloned()
            .ok_or(MoveCancelError::NotActive(job_id))?;
        cancellation.cancel();
        Ok(())
    }

    pub fn shutdown(mut self) {
        self.stop();
    }

    fn submit(
        &self,
        operation: MoveOperation,
        cancellation: MoveCancellation,
    ) -> Result<MoveSubmission, MoveSubmitError> {
        let queued = lock(&self.jobs).queue_operation()?;
        let operation_id = queued.operation_id();
        let job_id = queued.job_id();
        lock(&self.cancellations).insert(job_id, cancellation.clone());
        let task = MoveCommand::Execute(MoveTask {
            job_id,
            operation,
            cancellation,
        });

        match &self.sender {
            Some(sender) => match sender.try_send(task) {
                Ok(()) => Ok(MoveSubmission {
                    operation_id,
                    job_id,
                }),
                Err(TrySendError::Full(_)) => {
                    lock(&self.cancellations).remove(&job_id);
                    fail_submission(&self.jobs, job_id, "move queue is at capacity");
                    Err(MoveSubmitError::QueueFull { job_id })
                }
                Err(TrySendError::Disconnected(_)) => {
                    lock(&self.cancellations).remove(&job_id);
                    fail_submission(&self.jobs, job_id, "move executor has stopped");
                    Err(MoveSubmitError::ExecutorStopped { job_id })
                }
            },
            None => {
                lock(&self.cancellations).remove(&job_id);
                fail_submission(&self.jobs, job_id, "move executor has stopped");
                Err(MoveSubmitError::ExecutorStopped { job_id })
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn submit_move_with_cancellation(
        &self,
        request: MoveRequest,
        cancellation: MoveCancellation,
    ) -> Result<MoveSubmission, MoveSubmitError> {
        self.submit(MoveOperation::Move(request), cancellation)
    }

    #[cfg(test)]
    fn spawn_blocked(
        jobs: SharedJobManager,
        capacity: usize,
        start_gate: Receiver<()>,
    ) -> Result<Self, MoveExecutorSpawnError> {
        Self::spawn_with_capacity(jobs, capacity, Some(start_gate))
    }

    fn stop(&mut self) {
        for cancellation in lock(&self.cancellations).values() {
            cancellation.cancel();
        }
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(MoveCommand::Shutdown);
        }
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::error!("move executor worker panicked during shutdown");
        }
    }
}

impl Drop for MoveExecutor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_worker(
    receiver: Receiver<MoveCommand>,
    jobs: SharedJobManager,
    cancellations: Arc<Mutex<HashMap<JobId, MoveCancellation>>>,
) {
    while let Ok(command) = receiver.recv() {
        match command {
            MoveCommand::Execute(task) => execute_task(task, &jobs, &cancellations),
            MoveCommand::Shutdown => break,
        }
    }
}

fn execute_task(
    task: MoveTask,
    jobs: &SharedJobManager,
    cancellations: &Arc<Mutex<HashMap<JobId, MoveCancellation>>>,
) {
    if transition(jobs, task.job_id, JobCommand::Start).is_err() {
        lock(cancellations).remove(&task.job_id);
        return;
    }

    let result = match &task.operation {
        MoveOperation::Move(request) => execute_move(request, &task.cancellation),
        MoveOperation::Rename(request) => execute_rename(request, &task.cancellation),
    };
    let command = match result {
        Ok(_) => JobCommand::Complete,
        Err(MoveError::Cancelled) => JobCommand::Cancel,
        Err(error) => JobCommand::Fail(move_failure(&error)),
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
        tracing::error!(job_id = job_id.get(), %error, "move job transition failed");
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

fn move_failure(error: &MoveError) -> JobFailure {
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

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

    use floe_core::{ConflictPolicy, JobEventKind, JobState};
    use tempfile::tempdir;

    use crate::job_manager::ApplicationJobManager;

    use super::*;

    fn jobs() -> SharedJobManager {
        Arc::new(Mutex::new(ApplicationJobManager::new()))
    }

    fn move_request(source: &std::path::Path, destination: &std::path::Path) -> MoveRequest {
        MoveRequest::new(source, destination, ConflictPolicy::FailIfExists)
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
                "move job did not become terminal"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn move_executor_completes_move_lifecycle() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"move").expect("source fixture should be writable");
        let jobs = jobs();
        let executor = MoveExecutor::spawn(Arc::clone(&jobs)).expect("move executor should start");

        let submission = executor
            .submit_move(move_request(&source, &destination))
            .expect("move should be submitted");

        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Completed
        );
        assert_eq!(fs::read(destination).expect("move should finish"), b"move");
        let events = lock(&jobs).drain_events();
        assert!(events.iter().any(|event| {
            event.job_id() == submission.job_id() && event.kind() == &JobEventKind::Started
        }));
        assert!(events.iter().any(|event| {
            event.job_id() == submission.job_id() && event.kind() == &JobEventKind::Completed
        }));
    }

    #[test]
    fn move_executor_completes_rename_lifecycle() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("before");
        fs::write(&source, b"rename").expect("source fixture should be writable");
        let jobs = jobs();
        let executor = MoveExecutor::spawn(Arc::clone(&jobs)).expect("move executor should start");

        let submission = executor
            .submit_rename(RenameRequest::new(
                &source,
                "after",
                ConflictPolicy::FailIfExists,
            ))
            .expect("rename should be submitted");

        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Completed
        );
        assert_eq!(
            fs::read(fixture.path().join("after")).expect("rename should finish"),
            b"rename"
        );
    }

    #[test]
    fn move_executor_maps_pre_cancelled_request_to_cancelled_job() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"move").expect("source fixture should be writable");
        let jobs = jobs();
        let executor = MoveExecutor::spawn(Arc::clone(&jobs)).expect("move executor should start");
        let cancellation = MoveCancellation::new();
        cancellation.cancel();

        let submission = executor
            .submit_move_with_cancellation(move_request(&source, &destination), cancellation)
            .expect("cancelled move should be submitted");

        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Cancelled
        );
        assert!(source.exists());
        assert!(!destination.exists());
    }

    #[test]
    fn move_executor_maps_conflict_without_overwriting() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"source").expect("source fixture should be writable");
        fs::write(&destination, b"keep").expect("destination fixture should be writable");
        let jobs = jobs();
        let executor = MoveExecutor::spawn(Arc::clone(&jobs)).expect("move executor should start");

        let submission = executor
            .submit_move(move_request(&source, &destination))
            .expect("conflicting move should be submitted");

        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Failed
        );
        let guard = lock(&jobs);
        let failure = guard
            .record(submission.job_id())
            .expect("failed move should remain registered")
            .failure()
            .expect("failed move should retain failure details");
        assert_eq!(failure.kind(), JobFailureKind::Conflict);
        assert_eq!(fs::read(source).expect("source should remain"), b"source");
        assert_eq!(
            fs::read(destination).expect("destination should remain"),
            b"keep"
        );
    }

    #[test]
    fn move_executor_enforces_queue_capacity() {
        let fixture = tempdir().expect("temporary directory should be available");
        let first_source = fixture.path().join("first-source");
        let first_destination = fixture.path().join("first-destination");
        let second_source = fixture.path().join("second-source");
        let second_destination = fixture.path().join("second-destination");
        fs::write(&first_source, b"first").expect("first source should be writable");
        fs::write(&second_source, b"second").expect("second source should be writable");
        let jobs = jobs();
        let (gate_sender, gate_receiver) = mpsc::sync_channel(1);
        let executor = MoveExecutor::spawn_blocked(Arc::clone(&jobs), 1, gate_receiver)
            .expect("blocked move executor should start");
        let first = executor
            .submit_move(move_request(&first_source, &first_destination))
            .expect("first move should fill queue");

        let error = executor
            .submit_move(move_request(&second_source, &second_destination))
            .expect_err("second move must be rejected at capacity");
        let rejected_job = match error {
            MoveSubmitError::QueueFull { job_id } => job_id,
            other => panic!("unexpected submission error: {other}"),
        };
        assert_eq!(
            lock(&jobs)
                .record(rejected_job)
                .map(|record| record.state()),
            Some(JobState::Failed)
        );

        gate_sender
            .send(())
            .expect("worker gate should be releasable");
        assert_eq!(
            wait_for_terminal(&jobs, first.job_id()),
            JobState::Completed
        );
        assert!(second_source.exists());
        assert!(!second_destination.exists());
    }

    #[test]
    fn move_executor_shutdown_cancels_queued_work() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"move").expect("source fixture should be writable");
        let jobs = jobs();
        let (gate_sender, gate_receiver) = mpsc::sync_channel(1);
        let executor = MoveExecutor::spawn_blocked(Arc::clone(&jobs), 1, gate_receiver)
            .expect("blocked move executor should start");
        let submission = executor
            .submit_move(move_request(&source, &destination))
            .expect("move should be queued");
        let cancellation = lock(&executor.cancellations)
            .get(&submission.job_id())
            .cloned()
            .expect("queued move should retain cancellation state");
        let shutdown = thread::spawn(move || executor.shutdown());
        let deadline = Instant::now() + Duration::from_secs(3);
        while !cancellation.is_cancelled() {
            assert!(
                Instant::now() < deadline,
                "shutdown did not cancel queued move"
            );
            thread::yield_now();
        }
        gate_sender
            .send(())
            .expect("worker gate should be releasable");
        shutdown.join().expect("shutdown should join cleanly");

        assert_eq!(
            lock(&jobs)
                .record(submission.job_id())
                .map(|record| record.state()),
            Some(JobState::Cancelled)
        );
        assert!(source.exists());
        assert!(!destination.exists());
    }
}
