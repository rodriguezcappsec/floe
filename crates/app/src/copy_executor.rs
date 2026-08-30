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
    CopyCancellation, CopyError, CopyRequest, FileIdentity, JobCommand, JobFailure, JobFailureKind,
    JobId, JobProgress, OperationId, execute_copy,
};
use thiserror::Error;

#[cfg(test)]
use crate::job_manager::ApplicationJobManager;
use crate::{
    job_manager::{JobManagerError, SharedJobManager},
    operation_recovery::{RecoveryCoordinator, RecoveryJournal, RecoveryOperationKind},
    undo_history::{UndoHistoryCoordinator, UndoRecipe},
};

pub const DEFAULT_COPY_QUEUE_CAPACITY: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CopySubmission {
    operation_id: OperationId,
    job_id: JobId,
}

impl CopySubmission {
    pub const fn operation_id(self) -> OperationId {
        self.operation_id
    }

    pub const fn job_id(self) -> JobId {
        self.job_id
    }
}

#[derive(Debug, Error)]
pub enum CopyExecutorSpawnError {
    #[error("copy executor queue capacity must be greater than zero")]
    ZeroCapacity,
    #[error("could not start copy executor: {0}")]
    Thread(#[source] io::Error),
}

#[derive(Debug, Error)]
pub enum CopySubmitError {
    #[error(transparent)]
    JobManager(#[from] JobManagerError),
    #[error("copy queue is at capacity; job {job_id:?} was failed")]
    QueueFull { job_id: JobId },
    #[error("copy executor has stopped; job {job_id:?} was failed")]
    ExecutorStopped { job_id: JobId },
}

impl CopySubmitError {
    pub const fn job_id(&self) -> Option<JobId> {
        match self {
            Self::QueueFull { job_id } | Self::ExecutorStopped { job_id } => Some(*job_id),
            Self::JobManager(_) => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum CopyCancelError {
    #[error("job {0:?} is not an active copy job")]
    NotActive(JobId),
}

#[derive(Debug)]
enum CopyCommand {
    Execute(CopyTask),
    Shutdown,
}

#[derive(Debug)]
struct CopyTask {
    job_id: JobId,
    request: CopyRequest,
    cancellation: CopyCancellation,
}

/// Fixed-capacity, single-worker executor for filesystem copies.
///
/// GTK may eventually submit requests through this boundary, but the worker
/// owns all copy execution and writes only structured lifecycle events back to
/// `ApplicationJobManager`.
#[derive(Debug)]
pub struct CopyExecutor {
    sender: Option<SyncSender<CopyCommand>>,
    cancellations: Arc<Mutex<HashMap<JobId, CopyCancellation>>>,
    jobs: SharedJobManager,
    worker: Option<JoinHandle<()>>,
}

impl CopyExecutor {
    pub fn spawn(jobs: SharedJobManager) -> Result<Self, CopyExecutorSpawnError> {
        Self::spawn_with_capacity(jobs, DEFAULT_COPY_QUEUE_CAPACITY)
    }

    pub fn spawn_with_recovery(
        jobs: SharedJobManager,
        recovery: RecoveryJournal,
    ) -> Result<Self, CopyExecutorSpawnError> {
        Self::spawn_with_recovery_coordinator(jobs, RecoveryCoordinator::from_journal(recovery))
    }

    pub fn spawn_with_recovery_coordinator(
        jobs: SharedJobManager,
        recovery: RecoveryCoordinator,
    ) -> Result<Self, CopyExecutorSpawnError> {
        Self::spawn_inner(
            jobs,
            DEFAULT_COPY_QUEUE_CAPACITY,
            None,
            Some(recovery),
            None,
        )
    }

    pub fn spawn_with_recovery_and_undo(
        jobs: SharedJobManager,
        recovery: RecoveryCoordinator,
        undo_history: UndoHistoryCoordinator,
    ) -> Result<Self, CopyExecutorSpawnError> {
        Self::spawn_inner(
            jobs,
            DEFAULT_COPY_QUEUE_CAPACITY,
            None,
            Some(recovery),
            Some(undo_history),
        )
    }

    pub fn spawn_with_capacity(
        jobs: SharedJobManager,
        capacity: usize,
    ) -> Result<Self, CopyExecutorSpawnError> {
        Self::spawn_inner(jobs, capacity, None, None, None)
    }

    fn spawn_inner(
        jobs: SharedJobManager,
        capacity: usize,
        start_gate: Option<Receiver<()>>,
        recovery: Option<RecoveryCoordinator>,
        undo_history: Option<UndoHistoryCoordinator>,
    ) -> Result<Self, CopyExecutorSpawnError> {
        if capacity == 0 {
            return Err(CopyExecutorSpawnError::ZeroCapacity);
        }

        let (sender, receiver) = mpsc::sync_channel(capacity);
        let cancellations = Arc::new(Mutex::new(HashMap::new()));
        let worker_jobs = Arc::clone(&jobs);
        let worker_cancellations = Arc::clone(&cancellations);
        let worker = thread::Builder::new()
            .name("floe-copy-worker".to_owned())
            .spawn(move || {
                if let Some(gate) = start_gate {
                    let _ = gate.recv();
                }
                run_worker(
                    receiver,
                    worker_jobs,
                    worker_cancellations,
                    recovery,
                    undo_history,
                );
            })
            .map_err(CopyExecutorSpawnError::Thread)?;

        Ok(Self {
            sender: Some(sender),
            cancellations,
            jobs,
            worker: Some(worker),
        })
    }

    pub fn submit_copy(&self, request: CopyRequest) -> Result<CopySubmission, CopySubmitError> {
        self.submit_copy_with_cancellation(request, CopyCancellation::new())
    }

    pub fn submit_retry(
        &self,
        failed_job_id: JobId,
        request: CopyRequest,
    ) -> Result<CopySubmission, CopySubmitError> {
        let queued = lock(&self.jobs).retry(failed_job_id)?;
        self.enqueue(
            queued.operation_id(),
            queued.job_id(),
            request,
            CopyCancellation::new(),
        )
    }

    pub fn submit_copy_with_cancellation(
        &self,
        request: CopyRequest,
        cancellation: CopyCancellation,
    ) -> Result<CopySubmission, CopySubmitError> {
        let queued = lock(&self.jobs).queue_operation()?;
        self.enqueue(
            queued.operation_id(),
            queued.job_id(),
            request,
            cancellation,
        )
    }

    pub fn cancel(&self, job_id: JobId) -> Result<(), CopyCancelError> {
        let cancellation = lock(&self.cancellations)
            .get(&job_id)
            .cloned()
            .ok_or(CopyCancelError::NotActive(job_id))?;
        cancellation.cancel();
        Ok(())
    }

    pub fn jobs(&self) -> &SharedJobManager {
        &self.jobs
    }

    fn enqueue(
        &self,
        operation_id: OperationId,
        job_id: JobId,
        request: CopyRequest,
        cancellation: CopyCancellation,
    ) -> Result<CopySubmission, CopySubmitError> {
        lock(&self.cancellations).insert(job_id, cancellation.clone());
        let task = CopyCommand::Execute(CopyTask {
            job_id,
            request,
            cancellation,
        });
        let send_result = match &self.sender {
            Some(sender) => sender.try_send(task),
            None => Err(TrySendError::Disconnected(task)),
        };

        match send_result {
            Ok(()) => Ok(CopySubmission {
                operation_id,
                job_id,
            }),
            Err(TrySendError::Full(_)) => {
                lock(&self.cancellations).remove(&job_id);
                fail_submission(&self.jobs, job_id, "copy queue is at capacity");
                Err(CopySubmitError::QueueFull { job_id })
            }
            Err(TrySendError::Disconnected(_)) => {
                lock(&self.cancellations).remove(&job_id);
                fail_submission(&self.jobs, job_id, "copy executor has stopped");
                Err(CopySubmitError::ExecutorStopped { job_id })
            }
        }
    }

    #[cfg(test)]
    fn spawn_paused(
        jobs: SharedJobManager,
        capacity: usize,
    ) -> Result<(Self, SyncSender<()>), CopyExecutorSpawnError> {
        let (gate_sender, gate_receiver) = mpsc::sync_channel(1);
        Self::spawn_inner(jobs, capacity, Some(gate_receiver), None, None)
            .map(|executor| (executor, gate_sender))
    }
}

impl Drop for CopyExecutor {
    fn drop(&mut self) {
        for cancellation in lock(&self.cancellations).values() {
            cancellation.cancel();
        }
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(CopyCommand::Shutdown);
            drop(sender);
        }
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::error!("copy executor worker panicked during shutdown");
        }
    }
}

fn run_worker(
    receiver: Receiver<CopyCommand>,
    jobs: SharedJobManager,
    cancellations: Arc<Mutex<HashMap<JobId, CopyCancellation>>>,
    recovery: Option<RecoveryCoordinator>,
    undo_history: Option<UndoHistoryCoordinator>,
) {
    while let Ok(command) = receiver.recv() {
        match command {
            CopyCommand::Execute(task) => execute_task(
                task,
                &jobs,
                &cancellations,
                recovery.as_ref(),
                undo_history.as_ref(),
            ),
            CopyCommand::Shutdown => break,
        }
    }
}

fn execute_task(
    task: CopyTask,
    jobs: &SharedJobManager,
    cancellations: &Arc<Mutex<HashMap<JobId, CopyCancellation>>>,
    recovery: Option<&RecoveryCoordinator>,
    undo_history: Option<&UndoHistoryCoordinator>,
) {
    if transition(jobs, task.job_id, JobCommand::Start).is_err() {
        lock(cancellations).remove(&task.job_id);
        return;
    }

    let undo_ticket = match undo_history.map(|history| {
        history.begin(UndoRecipe::copy(
            task.request.source(),
            task.request.destination(),
            task.request.symlink_policy(),
        ))
    }) {
        Some(Ok(ticket)) => Some(ticket),
        Some(Err(error)) => {
            let _ = transition(
                jobs,
                task.job_id,
                JobCommand::Fail(JobFailure::new(
                    JobFailureKind::Internal,
                    format!("durable Undo history could not be prepared: {error}"),
                )),
            );
            lock(cancellations).remove(&task.job_id);
            return;
        }
        None => None,
    };
    let recovery_ticket = match recovery.map(|journal| {
        journal.begin(
            RecoveryOperationKind::Copy,
            Some(task.request.source()),
            task.request.destination(),
        )
    }) {
        Some(Ok(ticket)) => Some(ticket),
        Some(Err(error)) => {
            if let (Some(history), Some(ticket)) = (undo_history, undo_ticket) {
                let _ = history.resolve(ticket.id());
            }
            let _ = transition(
                jobs,
                task.job_id,
                JobCommand::Fail(JobFailure::new(
                    JobFailureKind::Internal,
                    format!("operation recovery could not be prepared: {error}"),
                )),
            );
            lock(cancellations).remove(&task.job_id);
            return;
        }
        None => None,
    };
    let mut last_completed = None;
    let result = execute_copy(&task.request, &task.cancellation, |progress| {
        let job_progress = if progress.total_bytes() > 0 {
            JobProgress::bytes(progress.bytes_copied(), Some(progress.total_bytes()))
        } else {
            JobProgress::items(progress.entries_copied(), Some(progress.total_entries()))
        };
        let completed = job_progress
            .as_ref()
            .ok()
            .map(|progress| progress.completed());
        if last_completed == completed {
            return;
        }
        last_completed = completed;
        match job_progress {
            Ok(progress) => {
                let _ = transition(jobs, task.job_id, JobCommand::SetProgress(progress));
            }
            Err(error) => {
                tracing::error!(job_id = task.job_id.get(), %error, "copy emitted invalid progress");
            }
        }
    });

    if let (Some(journal), Some(ticket)) = (recovery, recovery_ticket) {
        let journal_result = if result.is_ok() {
            journal.finish(ticket)
        } else {
            journal.retain_if_destination_exists(ticket, task.request.destination())
        };
        if let Err(error) = journal_result {
            tracing::error!(
                job_id = task.job_id.get(),
                %error,
                "copy recovery journal could not be finalized"
            );
        }
    }
    if let (Some(history), Some(ticket)) = (undo_history, undo_ticket) {
        if result.is_ok() {
            match FileIdentity::capture(task.request.destination()) {
                Ok(identity) => {
                    if let Err(error) = history.complete(ticket, identity) {
                        tracing::error!(job_id = task.job_id.get(), %error, "copy committed but durable Undo history could not be completed");
                    }
                }
                Err(error) => {
                    tracing::error!(job_id = task.job_id.get(), %error, "copy committed but destination identity could not be captured for Undo")
                }
            }
        } else if let Err(error) =
            history.retain_if_destination_exists(ticket, task.request.destination())
        {
            tracing::error!(job_id = task.job_id.get(), %error, "copy failure could not update durable Undo history");
        }
    }
    let command = match result {
        Ok(_) => JobCommand::Complete,
        Err(CopyError::Cancelled) => JobCommand::Cancel,
        Err(error) => JobCommand::Fail(copy_failure(&error)),
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
        tracing::error!(job_id = job_id.get(), %error, "copy job transition failed");
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

fn copy_failure(error: &CopyError) -> JobFailure {
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
        time::{Duration, Instant},
    };

    use floe_core::{ConflictPolicy, JobEventKind, JobState, SymlinkPolicy};
    use tempfile::tempdir;

    use super::*;

    fn jobs() -> SharedJobManager {
        Arc::new(Mutex::new(ApplicationJobManager::new()))
    }

    fn request(source: &std::path::Path, destination: &std::path::Path) -> CopyRequest {
        CopyRequest::new(
            source,
            destination,
            ConflictPolicy::FailIfExists,
            SymlinkPolicy::Preserve,
        )
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
                "copy job did not become terminal"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn copy_executor_completes_and_emits_lifecycle_events() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"floe").expect("source fixture should be writable");
        let jobs = jobs();
        let executor = CopyExecutor::spawn(Arc::clone(&jobs)).expect("copy executor should start");

        let submission = executor
            .submit_copy(request(&source, &destination))
            .expect("copy should be submitted");

        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Completed
        );
        assert_eq!(
            fs::read(destination).expect("copied file should be readable"),
            b"floe"
        );
        let events = lock(&jobs).drain_events();
        assert!(
            events
                .iter()
                .any(|event| event.kind() == &JobEventKind::Queued)
        );
        assert!(
            events
                .iter()
                .any(|event| event.kind() == &JobEventKind::Started)
        );
        assert!(
            events
                .iter()
                .any(|event| event.kind() == &JobEventKind::Completed)
        );
    }

    #[test]
    fn copy_executor_maps_conflict_to_failed_job() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"new").expect("source fixture should be writable");
        fs::write(&destination, b"keep").expect("destination fixture should be writable");
        let jobs = jobs();
        let executor = CopyExecutor::spawn(Arc::clone(&jobs)).expect("copy executor should start");

        let submission = executor
            .submit_copy(request(&source, &destination))
            .expect("copy should be submitted");

        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Failed
        );
        let guard = lock(&jobs);
        let failure = guard
            .record(submission.job_id())
            .expect("failed copy should remain registered")
            .failure()
            .expect("failed copy should retain failure details");
        assert_eq!(failure.kind(), JobFailureKind::Conflict);
        assert_eq!(
            fs::read(destination).expect("existing destination should remain readable"),
            b"keep"
        );
    }

    #[test]
    fn copy_executor_maps_pre_cancelled_request_to_cancelled_job() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"content").expect("source fixture should be writable");
        let jobs = jobs();
        let executor = CopyExecutor::spawn(Arc::clone(&jobs)).expect("copy executor should start");
        let cancellation = CopyCancellation::new();
        cancellation.cancel();

        let submission = executor
            .submit_copy_with_cancellation(request(&source, &destination), cancellation)
            .expect("cancelled copy should be submitted");

        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Cancelled
        );
        assert!(!destination.exists());
    }

    #[test]
    fn copy_executor_fails_excess_submission_at_capacity() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        fs::write(&source, b"content").expect("source fixture should be writable");
        let jobs = jobs();
        let (executor, start_worker) = CopyExecutor::spawn_paused(Arc::clone(&jobs), 1)
            .expect("paused copy executor should start");

        let first = executor
            .submit_copy(request(&source, &fixture.path().join("first")))
            .expect("first copy should fit in queue");
        let error = executor
            .submit_copy(request(&source, &fixture.path().join("second")))
            .expect_err("full queue must reject excess work");
        let rejected_job = match error {
            CopySubmitError::QueueFull { job_id } => job_id,
            other => panic!("unexpected submit error: {other}"),
        };
        assert_eq!(
            lock(&jobs)
                .record(rejected_job)
                .expect("rejected copy should remain registered")
                .state(),
            JobState::Failed
        );

        start_worker
            .send(())
            .expect("paused copy worker should be released");
        assert_eq!(
            wait_for_terminal(&jobs, first.job_id()),
            JobState::Completed
        );
    }

    #[test]
    fn copy_executor_retry_keeps_operation_identity_and_can_succeed() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"new").expect("source fixture should be writable");
        fs::write(&destination, b"conflict").expect("destination fixture should be writable");
        let jobs = jobs();
        let executor = CopyExecutor::spawn(Arc::clone(&jobs)).expect("copy executor should start");

        let failed = executor
            .submit_copy(request(&source, &destination))
            .expect("initial copy should be submitted");
        assert_eq!(wait_for_terminal(&jobs, failed.job_id()), JobState::Failed);
        fs::remove_file(&destination).expect("conflict fixture should be removable");
        let retry = executor
            .submit_retry(failed.job_id(), request(&source, &destination))
            .expect("retry should be submitted");

        assert_eq!(retry.operation_id(), failed.operation_id());
        assert_ne!(retry.job_id(), failed.job_id());
        assert_eq!(
            wait_for_terminal(&jobs, retry.job_id()),
            JobState::Completed
        );
        assert_eq!(
            fs::read(destination).expect("retried copy should be readable"),
            b"new"
        );
    }

    #[test]
    fn phase_18y_copy_success_clears_durable_recovery_record() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"journaled copy").expect("source fixture should be writable");
        let journal = RecoveryJournal::open_at(fixture.path().join("recovery.bin"))
            .expect("recovery journal should open");
        let jobs = jobs();
        let executor = CopyExecutor::spawn_with_recovery(Arc::clone(&jobs), journal.clone())
            .expect("copy executor should start");
        let submission = executor
            .submit_copy(request(&source, &destination))
            .expect("copy should submit");
        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Completed
        );
        assert!(journal.pending().is_empty());
        assert_eq!(
            fs::read(destination).expect("destination"),
            b"journaled copy"
        );
    }

    #[test]
    fn phase_18y_blocked_recovery_prevents_copy_mutation() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"must remain source only").expect("source fixture");
        let recovery_path = fixture.path().join("recovery.bin");
        fs::write(&recovery_path, b"corrupt").expect("corrupt recovery fixture");
        fs::set_permissions(&recovery_path, fs::Permissions::from_mode(0o600))
            .expect("private recovery fixture");
        let coordinator = RecoveryCoordinator::load_at(recovery_path);
        let jobs = jobs();
        let executor =
            CopyExecutor::spawn_with_recovery_coordinator(Arc::clone(&jobs), coordinator)
                .expect("copy executor should start in blocked mode");
        let submission = executor
            .submit_copy(request(&source, &destination))
            .expect("submission remains observable");
        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Failed
        );
        assert!(
            !destination.exists(),
            "mutation must not start without journal"
        );
        let jobs = lock(&jobs);
        let failure = jobs
            .record(submission.job_id())
            .and_then(|record| record.failure())
            .expect("blocked job should explain failure");
        assert_eq!(failure.kind(), JobFailureKind::Internal);
        assert!(failure.message().contains("operation recovery"));
    }

    #[test]
    fn copy_executor_shutdown_cancels_queued_work_and_joins() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        fs::write(&source, b"content").expect("source fixture should be writable");
        let jobs = jobs();
        let (executor, start_worker) = CopyExecutor::spawn_paused(Arc::clone(&jobs), 1)
            .expect("paused copy executor should start");
        let submission = executor
            .submit_copy(request(&source, &fixture.path().join("destination")))
            .expect("copy should be submitted");
        executor
            .cancel(submission.job_id())
            .expect("queued copy should be cancellable");
        start_worker
            .send(())
            .expect("paused copy worker should be released");

        drop(executor);

        assert_eq!(
            lock(&jobs)
                .record(submission.job_id())
                .expect("cancelled copy should remain registered")
                .state(),
            JobState::Cancelled
        );
    }
}
