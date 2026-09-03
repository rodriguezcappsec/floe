use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex, MutexGuard,
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

use floe_core::{
    ArchiveCancellation, ArchiveError, ArchiveOutcome, ArchiveProgress, ArchiveRequest, JobCommand,
    JobFailure, JobFailureKind, JobId, JobProgress, OperationId, execute_archive,
};
use thiserror::Error;

use crate::job_manager::{JobManagerError, SharedJobManager};

pub const ARCHIVE_QUEUE_CAPACITY: usize = 4;
pub const ARCHIVE_RESULT_CAPACITY: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArchiveSubmission {
    operation_id: OperationId,
    job_id: JobId,
}

impl ArchiveSubmission {
    pub const fn operation_id(self) -> OperationId {
        self.operation_id
    }

    pub const fn job_id(self) -> JobId {
        self.job_id
    }
}

#[derive(Debug, Error)]
pub enum ArchiveExecutorSpawnError {
    #[error("archive queue capacity cannot be zero")]
    ZeroCapacity,
    #[error("could not spawn archive worker: {0}")]
    Thread(#[source] std::io::Error),
}

#[derive(Debug, Error)]
pub enum ArchiveSubmitError {
    #[error(transparent)]
    JobManager(#[from] JobManagerError),
    #[error("archive queue is full for {0:?}")]
    QueueFull(JobId),
    #[error("archive executor stopped for {0:?}")]
    ExecutorStopped(JobId),
}

impl ArchiveSubmitError {
    pub const fn job_id(&self) -> Option<JobId> {
        match self {
            Self::QueueFull(job_id) | Self::ExecutorStopped(job_id) => Some(*job_id),
            Self::JobManager(_) => None,
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ArchiveCancelError {
    #[error("archive job is not active: {0:?}")]
    NotActive(JobId),
}

#[derive(Debug)]
struct Task {
    job_id: JobId,
    request: ArchiveRequest,
    cancellation: ArchiveCancellation,
}

#[derive(Debug)]
enum Command {
    Execute(Task),
    Shutdown,
}

#[derive(Debug)]
pub struct ArchiveExecutor {
    sender: Option<SyncSender<Command>>,
    cancellations: Arc<Mutex<HashMap<JobId, ArchiveCancellation>>>,
    results: Arc<Mutex<VecDeque<(JobId, ArchiveOutcome)>>>,
    jobs: SharedJobManager,
    worker: Option<JoinHandle<()>>,
}

impl ArchiveExecutor {
    pub fn spawn(jobs: SharedJobManager) -> Result<Self, ArchiveExecutorSpawnError> {
        Self::spawn_with_capacity(jobs, ARCHIVE_QUEUE_CAPACITY)
    }

    fn spawn_with_capacity(
        jobs: SharedJobManager,
        capacity: usize,
    ) -> Result<Self, ArchiveExecutorSpawnError> {
        if capacity == 0 {
            return Err(ArchiveExecutorSpawnError::ZeroCapacity);
        }
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let cancellations = Arc::new(Mutex::new(HashMap::new()));
        let results = Arc::new(Mutex::new(VecDeque::with_capacity(ARCHIVE_RESULT_CAPACITY)));
        let worker_jobs = Arc::clone(&jobs);
        let worker_cancellations = Arc::clone(&cancellations);
        let worker_results = Arc::clone(&results);
        let worker = thread::Builder::new()
            .name("floe-archive-worker".to_owned())
            .spawn(move || {
                run_worker(receiver, worker_jobs, worker_cancellations, worker_results);
            })
            .map_err(ArchiveExecutorSpawnError::Thread)?;

        Ok(Self {
            sender: Some(sender),
            cancellations,
            results,
            jobs,
            worker: Some(worker),
        })
    }

    pub fn submit(&self, request: ArchiveRequest) -> Result<ArchiveSubmission, ArchiveSubmitError> {
        let queued = lock(&self.jobs).queue_operation()?;
        let submission = ArchiveSubmission {
            operation_id: queued.operation_id(),
            job_id: queued.job_id(),
        };
        let cancellation = ArchiveCancellation::new();
        lock(&self.cancellations).insert(submission.job_id, cancellation.clone());

        let Some(sender) = &self.sender else {
            fail_submission(&self.jobs, submission.job_id, "archive executor stopped");
            lock(&self.cancellations).remove(&submission.job_id);
            return Err(ArchiveSubmitError::ExecutorStopped(submission.job_id));
        };
        let task = Task {
            job_id: submission.job_id,
            request,
            cancellation,
        };
        match sender.try_send(Command::Execute(task)) {
            Ok(()) => Ok(submission),
            Err(TrySendError::Full(_)) => {
                lock(&self.cancellations).remove(&submission.job_id);
                fail_submission(&self.jobs, submission.job_id, "archive queue is full");
                Err(ArchiveSubmitError::QueueFull(submission.job_id))
            }
            Err(TrySendError::Disconnected(_)) => {
                lock(&self.cancellations).remove(&submission.job_id);
                fail_submission(&self.jobs, submission.job_id, "archive executor stopped");
                Err(ArchiveSubmitError::ExecutorStopped(submission.job_id))
            }
        }
    }

    pub fn cancel(&self, job_id: JobId) -> Result<(), ArchiveCancelError> {
        let cancellation = lock(&self.cancellations)
            .get(&job_id)
            .cloned()
            .ok_or(ArchiveCancelError::NotActive(job_id))?;
        cancellation.cancel();
        Ok(())
    }

    pub fn take_result(&self, job_id: JobId) -> Option<ArchiveOutcome> {
        let mut results = lock(&self.results);
        let index = results
            .iter()
            .position(|(candidate, _)| *candidate == job_id)?;
        results.remove(index).map(|(_, outcome)| outcome)
    }
}

impl Drop for ArchiveExecutor {
    fn drop(&mut self) {
        for cancellation in lock(&self.cancellations).values() {
            cancellation.cancel();
        }
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(Command::Shutdown);
            drop(sender);
        }
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("archive worker panicked during shutdown");
        }
    }
}

fn run_worker(
    receiver: Receiver<Command>,
    jobs: SharedJobManager,
    cancellations: Arc<Mutex<HashMap<JobId, ArchiveCancellation>>>,
    results: Arc<Mutex<VecDeque<(JobId, ArchiveOutcome)>>>,
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
    cancellations: &Mutex<HashMap<JobId, ArchiveCancellation>>,
    results: &Mutex<VecDeque<(JobId, ArchiveOutcome)>>,
) {
    if transition(jobs, task.job_id, JobCommand::Start).is_err() {
        lock(cancellations).remove(&task.job_id);
        return;
    }
    let outcome = execute_archive(&task.request, &task.cancellation, |progress| {
        let progress = match progress {
            ArchiveProgress::Items { completed, total } => {
                JobProgress::items(completed, (total > 0).then_some(total))
            }
            ArchiveProgress::Bytes { completed, total } => {
                JobProgress::bytes(completed, (total > 0).then_some(total))
            }
        };
        if let Ok(progress) = progress {
            let _ = transition(jobs, task.job_id, JobCommand::SetProgress(progress));
        }
    });
    let terminal = match outcome {
        Ok(outcome) => {
            let mut queue = lock(results);
            if queue.len() == ARCHIVE_RESULT_CAPACITY {
                queue.pop_front();
            }
            queue.push_back((task.job_id, outcome));
            JobCommand::Complete
        }
        Err(ArchiveError::Cancelled) => JobCommand::Cancel,
        Err(error) => JobCommand::Fail(archive_failure(&error)),
    };
    let _ = transition(jobs, task.job_id, terminal);
    lock(cancellations).remove(&task.job_id);
}

fn archive_failure(error: &ArchiveError) -> JobFailure {
    let kind = match error {
        ArchiveError::DestinationExists(_) | ArchiveError::MemberConflict(_) => {
            JobFailureKind::Conflict
        }
        ArchiveError::UnsupportedEntry(_)
        | ArchiveError::UnsupportedSource(_)
        | ArchiveError::NonUtf8MemberName { .. }
        | ArchiveError::PasswordRequired => JobFailureKind::Unsupported,
        ArchiveError::Io { source, .. }
            if source.kind() == std::io::ErrorKind::PermissionDenied =>
        {
            JobFailureKind::PermissionDenied
        }
        _ => JobFailureKind::Io,
    };
    JobFailure::new(kind, error.to_string())
}

fn fail_submission(jobs: &SharedJobManager, job_id: JobId, message: &'static str) {
    let _ = transition(
        jobs,
        job_id,
        JobCommand::Fail(JobFailure::new(JobFailureKind::Internal, message)),
    );
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
        sync::{Arc, Mutex},
        thread,
        time::{Duration, Instant},
    };

    use floe_core::{ArchiveRequest, JobEventKind, JobState};
    use tempfile::tempdir;

    use super::*;
    use crate::job_manager::ApplicationJobManager;

    fn wait_for_terminal(jobs: &SharedJobManager, job_id: JobId) -> JobState {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(state) = lock(jobs).record(job_id).map(|record| record.state()) {
                if state.is_terminal() {
                    return state;
                }
            }
            assert!(Instant::now() < deadline, "archive worker timed out");
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn phase_12a_archive_executor_reports_progress_results_and_clean_shutdown() {
        let root = tempdir().expect("root");
        let source = root.path().join("source.txt");
        fs::write(&source, b"archive executor").expect("source");
        let destination = root.path().join("bundle.zip");
        let request = ArchiveRequest::compress(vec![source], destination).expect("request");
        let jobs = Arc::new(Mutex::new(ApplicationJobManager::new()));
        let executor = ArchiveExecutor::spawn(Arc::clone(&jobs)).expect("executor");
        let submission = executor.submit(request).expect("submission");

        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Completed
        );
        assert!(matches!(
            executor.take_result(submission.job_id()),
            Some(ArchiveOutcome::Compressed { .. })
        ));
        let events = lock(&jobs).drain_events();
        assert!(events.iter().any(|event| {
            event.job_id() == submission.job_id()
                && matches!(event.kind(), JobEventKind::Progressed(_))
        }));
        drop(executor);
    }

    #[test]
    fn phase_12a_archive_executor_cancels_and_bounds_results() {
        let root = tempdir().expect("root");
        let source = root.path().join("cancel.txt");
        fs::write(&source, vec![1_u8; 2 * 1024 * 1024]).expect("source");
        let jobs = Arc::new(Mutex::new(ApplicationJobManager::new()));
        let executor = ArchiveExecutor::spawn(Arc::clone(&jobs)).expect("executor");
        let request = ArchiveRequest::compress(vec![source], root.path().join("cancel.tar"))
            .expect("request");
        let submission = executor.submit(request).expect("submission");
        let cancel_result = executor.cancel(submission.job_id());
        if let Err(error) = &cancel_result {
            assert_eq!(error, &ArchiveCancelError::NotActive(submission.job_id()));
        }
        let terminal = wait_for_terminal(&jobs, submission.job_id());
        assert!(matches!(
            terminal,
            JobState::Cancelled | JobState::Completed
        ));
        assert_eq!(
            executor.take_result(submission.job_id()).is_none(),
            terminal == JobState::Cancelled
        );
        assert_eq!(ARCHIVE_QUEUE_CAPACITY, 4);
        assert_eq!(ARCHIVE_RESULT_CAPACITY, 16);
    }
}
