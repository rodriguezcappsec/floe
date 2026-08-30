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
    JobCommand, JobFailure, JobFailureKind, JobId, OperationId, ReplaceCancellation, ReplaceError,
    ReplaceRequest, execute_replace,
};
use thiserror::Error;

use crate::{
    job_manager::{JobManagerError, SharedJobManager},
    undo_history::{UndoHistoryCoordinator, UndoRecipe},
};

pub const DEFAULT_REPLACE_QUEUE_CAPACITY: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplaceSubmission {
    operation_id: OperationId,
    job_id: JobId,
}

impl ReplaceSubmission {
    pub const fn operation_id(self) -> OperationId {
        self.operation_id
    }

    pub const fn job_id(self) -> JobId {
        self.job_id
    }
}

#[derive(Debug, Error)]
pub enum ReplaceExecutorSpawnError {
    #[error("replace executor queue capacity must be greater than zero")]
    ZeroCapacity,
    #[error("could not start replace executor: {0}")]
    Thread(#[source] io::Error),
}

#[derive(Debug, Error)]
pub enum ReplaceSubmitError {
    #[error(transparent)]
    JobManager(#[from] JobManagerError),
    #[error("replace queue is at capacity; job {job_id:?} was failed")]
    QueueFull { job_id: JobId },
    #[error("replace executor has stopped; job {job_id:?} was failed")]
    ExecutorStopped { job_id: JobId },
}

impl ReplaceSubmitError {
    pub const fn job_id(&self) -> Option<JobId> {
        match self {
            Self::QueueFull { job_id } | Self::ExecutorStopped { job_id } => Some(*job_id),
            Self::JobManager(_) => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum ReplaceCancelError {
    #[error("replace job is not active: {0:?}")]
    NotActive(JobId),
}

#[derive(Debug)]
enum ReplaceCommand {
    Execute(ReplaceTask),
    Shutdown,
}

#[derive(Debug)]
struct ReplaceTask {
    job_id: JobId,
    request: ReplaceRequest,
    cancellation: ReplaceCancellation,
}

#[derive(Debug)]
pub struct ReplaceExecutor {
    sender: Option<SyncSender<ReplaceCommand>>,
    cancellations: Arc<Mutex<HashMap<JobId, ReplaceCancellation>>>,
    jobs: SharedJobManager,
    worker: Option<JoinHandle<()>>,
}

impl ReplaceExecutor {
    pub fn spawn(
        jobs: SharedJobManager,
        history: UndoHistoryCoordinator,
    ) -> Result<Self, ReplaceExecutorSpawnError> {
        Self::spawn_with_capacity(jobs, history, DEFAULT_REPLACE_QUEUE_CAPACITY)
    }

    fn spawn_with_capacity(
        jobs: SharedJobManager,
        history: UndoHistoryCoordinator,
        capacity: usize,
    ) -> Result<Self, ReplaceExecutorSpawnError> {
        if capacity == 0 {
            return Err(ReplaceExecutorSpawnError::ZeroCapacity);
        }
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let cancellations = Arc::new(Mutex::new(HashMap::new()));
        let worker_jobs = Arc::clone(&jobs);
        let worker_cancellations = Arc::clone(&cancellations);
        let worker = thread::Builder::new()
            .name("floe-replace-worker".to_owned())
            .spawn(move || {
                run_worker(receiver, worker_jobs, worker_cancellations, history);
            })
            .map_err(ReplaceExecutorSpawnError::Thread)?;
        Ok(Self {
            sender: Some(sender),
            cancellations,
            jobs,
            worker: Some(worker),
        })
    }

    pub fn submit(&self, request: ReplaceRequest) -> Result<ReplaceSubmission, ReplaceSubmitError> {
        let queued = lock(&self.jobs).queue_operation()?;
        let cancellation = ReplaceCancellation::new();
        lock(&self.cancellations).insert(queued.job_id(), cancellation.clone());
        let command = ReplaceCommand::Execute(ReplaceTask {
            job_id: queued.job_id(),
            request,
            cancellation,
        });
        let Some(sender) = &self.sender else {
            lock(&self.cancellations).remove(&queued.job_id());
            fail_submission(&self.jobs, queued.job_id(), "replace executor has stopped");
            return Err(ReplaceSubmitError::ExecutorStopped {
                job_id: queued.job_id(),
            });
        };
        match sender.try_send(command) {
            Ok(()) => Ok(ReplaceSubmission {
                operation_id: queued.operation_id(),
                job_id: queued.job_id(),
            }),
            Err(TrySendError::Full(_)) => {
                lock(&self.cancellations).remove(&queued.job_id());
                fail_submission(&self.jobs, queued.job_id(), "replace queue is at capacity");
                Err(ReplaceSubmitError::QueueFull {
                    job_id: queued.job_id(),
                })
            }
            Err(TrySendError::Disconnected(_)) => {
                lock(&self.cancellations).remove(&queued.job_id());
                fail_submission(&self.jobs, queued.job_id(), "replace executor has stopped");
                Err(ReplaceSubmitError::ExecutorStopped {
                    job_id: queued.job_id(),
                })
            }
        }
    }

    pub fn cancel(&self, job_id: JobId) -> Result<(), ReplaceCancelError> {
        let cancellation = lock(&self.cancellations)
            .get(&job_id)
            .ok_or(ReplaceCancelError::NotActive(job_id))?
            .clone();
        cancellation.cancel();
        Ok(())
    }
}

impl Drop for ReplaceExecutor {
    fn drop(&mut self) {
        for cancellation in lock(&self.cancellations).values() {
            cancellation.cancel();
        }
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(ReplaceCommand::Shutdown);
        }
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("replace worker panicked during shutdown");
        }
    }
}

fn run_worker(
    receiver: Receiver<ReplaceCommand>,
    jobs: SharedJobManager,
    cancellations: Arc<Mutex<HashMap<JobId, ReplaceCancellation>>>,
    history: UndoHistoryCoordinator,
) {
    while let Ok(command) = receiver.recv() {
        match command {
            ReplaceCommand::Execute(task) => execute_task(task, &jobs, &cancellations, &history),
            ReplaceCommand::Shutdown => break,
        }
    }
}

fn execute_task(
    task: ReplaceTask,
    jobs: &SharedJobManager,
    cancellations: &Arc<Mutex<HashMap<JobId, ReplaceCancellation>>>,
    history: &UndoHistoryCoordinator,
) {
    if transition(jobs, task.job_id, JobCommand::Start).is_err() {
        lock(cancellations).remove(&task.job_id);
        return;
    }
    let ticket = match history.begin(UndoRecipe::replace(
        task.request.source(),
        task.request.destination(),
        task.request.backup(),
        task.request.mode(),
        task.request.symlink_policy(),
    )) {
        Ok(ticket) => ticket,
        Err(error) => {
            let _ = transition(
                jobs,
                task.job_id,
                JobCommand::Fail(JobFailure::new(
                    JobFailureKind::Internal,
                    format!("durable replacement history could not be prepared: {error}"),
                )),
            );
            lock(cancellations).remove(&task.job_id);
            return;
        }
    };

    let result = execute_replace(&task.request, &task.cancellation);
    match &result {
        Ok(outcome) => {
            if let Err(error) = history.complete_replace(
                ticket,
                outcome.destination_identity(),
                outcome.backup_identity(),
            ) {
                tracing::error!(job_id = task.job_id.get(), %error, "replacement committed but durable history could not be completed");
                let _ = history.retain_if_destination_exists(ticket, task.request.destination());
            }
        }
        Err(error) if error.is_partial() => {
            let _ = history.retain_if_destination_exists(ticket, task.request.destination());
        }
        Err(_) => {
            let _ = history.resolve(ticket.id());
        }
    }

    let command = match result {
        Ok(_) => JobCommand::Complete,
        Err(error) if error.is_cancelled() => JobCommand::Cancel,
        Err(error) => JobCommand::Fail(replace_failure(&error)),
    };
    let _ = transition(jobs, task.job_id, command);
    lock(cancellations).remove(&task.job_id);
}

fn replace_failure(error: &ReplaceError) -> JobFailure {
    let kind = if error.is_partial() {
        JobFailureKind::Partial
    } else if error.is_conflict() {
        JobFailureKind::Conflict
    } else if error.is_unsupported() {
        JobFailureKind::Unsupported
    } else if error.io_kind() == Some(io::ErrorKind::PermissionDenied) {
        JobFailureKind::PermissionDenied
    } else if error.io_kind().is_some() {
        JobFailureKind::Io
    } else {
        JobFailureKind::Internal
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
    use std::{fs, io, path::PathBuf, sync::Arc, time::Duration};

    use floe_core::{
        CopyError, FileIdentity, JobEventKind, JobState, MoveError, ReplaceError, ReplaceMode,
        SymlinkPolicy, allocate_replace_backup,
    };
    use tempfile::tempdir;

    use super::*;
    use crate::{job_manager::ApplicationJobManager, undo_history::UndoHistoryStore};

    #[test]
    fn reliability_replace_failure_preserves_error_kind_and_nested_cancellation() {
        let path = PathBuf::from("/tmp/floe-replace-classification");
        let permission = ReplaceError::Io {
            action: "inspect",
            path: path.clone(),
            source: io::Error::from(io::ErrorKind::PermissionDenied),
        };
        assert_eq!(
            replace_failure(&permission).kind(),
            JobFailureKind::PermissionDenied
        );

        let ordinary_io = ReplaceError::Move(MoveError::Io {
            action: "rename",
            path: path.clone(),
            source: io::Error::other("injected I/O failure"),
        });
        assert_eq!(replace_failure(&ordinary_io).kind(), JobFailureKind::Io);

        let conflict = ReplaceError::DestinationChanged(path.clone());
        assert_eq!(replace_failure(&conflict).kind(), JobFailureKind::Conflict);

        let unsupported = ReplaceError::Copy(CopyError::UnsupportedFileType(path.clone()));
        assert_eq!(
            replace_failure(&unsupported).kind(),
            JobFailureKind::Unsupported
        );

        let partial = ReplaceError::Move(MoveError::Partial {
            source_path: path.clone(),
            destination_path: path.clone(),
            reason: "injected post-commit uncertainty".to_owned(),
        });
        assert_eq!(replace_failure(&partial).kind(), JobFailureKind::Partial);

        assert!(ReplaceError::Copy(CopyError::Cancelled).is_cancelled());
        assert!(ReplaceError::Move(MoveError::Cancelled).is_cancelled());
        assert_eq!(
            replace_failure(&ReplaceError::InvalidBackup(path)).kind(),
            JobFailureKind::Internal
        );
    }

    #[test]
    fn phase_6u_recovery_executor_commits_two_version_durable_history() {
        let fixture = tempdir().expect("fixture");
        let source = fixture.path().join("incoming");
        let destination = fixture.path().join("item");
        fs::write(&source, b"new").expect("source");
        fs::write(&destination, b"old").expect("destination");
        let backup = allocate_replace_backup(&destination, 41).expect("backup");
        let history_path = fixture.path().join("state").join("undo.bin");
        let history = UndoHistoryCoordinator::from_store(
            UndoHistoryStore::open_at(history_path).expect("history"),
        );
        let jobs = Arc::new(Mutex::new(ApplicationJobManager::new()));
        let executor =
            ReplaceExecutor::spawn(Arc::clone(&jobs), history.clone()).expect("executor");
        let submission = executor
            .submit(ReplaceRequest::new(
                &source,
                &destination,
                &backup,
                ReplaceMode::Copy,
                SymlinkPolicy::Preserve,
                FileIdentity::capture(&source).expect("source identity"),
                FileIdentity::capture(&destination).expect("destination identity"),
            ))
            .expect("submit");
        for _ in 0..200 {
            if matches!(
                lock(&jobs)
                    .record(submission.job_id())
                    .map(|record| record.state()),
                Some(JobState::Completed)
            ) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(lock(&jobs).drain_events().iter().any(|event| {
            event.job_id() == submission.job_id() && matches!(event.kind(), JobEventKind::Completed)
        }));
        let records = history.history().expect("history records");
        assert_eq!(records.len(), 1);
        assert!(matches!(records[0].recipe(), UndoRecipe::Replace { .. }));
        assert!(records[0].current_identity().is_some());
        assert!(records[0].alternate_identity().is_some());
        drop(executor);
    }
}
