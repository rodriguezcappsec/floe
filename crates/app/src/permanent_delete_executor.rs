use std::{
    collections::HashMap,
    io,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

use floe_core::{
    JobCommand, JobFailure, JobFailureKind, JobId, JobProgress, OperationId, PermanentDeleteError,
    PermanentDeleteProgress, PermanentDeleteRequest, execute_permanent_delete,
};
use thiserror::Error;

use crate::job_manager::{JobManagerError, SharedJobManager};

pub const DEFAULT_PERMANENT_DELETE_QUEUE_CAPACITY: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PermanentDeleteSubmission {
    operation_id: OperationId,
    job_id: JobId,
}

impl PermanentDeleteSubmission {
    pub const fn operation_id(self) -> OperationId {
        self.operation_id
    }

    pub const fn job_id(self) -> JobId {
        self.job_id
    }
}

#[derive(Debug, Error)]
pub enum PermanentDeleteExecutorSpawnError {
    #[error("permanent-delete executor capacity must be greater than zero")]
    ZeroCapacity,
    #[error("could not start permanent-delete worker: {0}")]
    Thread(#[source] io::Error),
}

#[derive(Debug, Error)]
pub enum PermanentDeleteSubmitError {
    #[error(transparent)]
    JobManager(#[from] JobManagerError),
    #[error("permanent-delete queue is full for job {0:?}")]
    QueueFull(JobId),
    #[error("permanent-delete executor stopped before job {0:?} could start")]
    ExecutorStopped(JobId),
}

impl PermanentDeleteSubmitError {
    pub const fn job_id(&self) -> Option<JobId> {
        match self {
            Self::QueueFull(job_id) | Self::ExecutorStopped(job_id) => Some(*job_id),
            Self::JobManager(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PermanentDeleteCancelError {
    #[error("permanent-delete job is not active: {0:?}")]
    NotActive(JobId),
}

trait PermanentDeleteBackend: Send + Sync + std::fmt::Debug {
    fn delete(
        &self,
        request: &PermanentDeleteRequest,
        cancellation: &AtomicBool,
        on_progress: &mut dyn FnMut(PermanentDeleteProgress),
    ) -> Result<(), PermanentDeleteError>;
}

#[derive(Debug)]
struct FilesystemPermanentDeleteBackend;

impl PermanentDeleteBackend for FilesystemPermanentDeleteBackend {
    fn delete(
        &self,
        request: &PermanentDeleteRequest,
        cancellation: &AtomicBool,
        on_progress: &mut dyn FnMut(PermanentDeleteProgress),
    ) -> Result<(), PermanentDeleteError> {
        execute_permanent_delete(
            request,
            || cancellation.load(Ordering::Acquire),
            on_progress,
        )
        .map(|_| ())
    }
}

struct PermanentDeleteTask {
    job_id: JobId,
    request: PermanentDeleteRequest,
    cancellation: Arc<AtomicBool>,
}

enum PermanentDeleteCommand {
    Execute(PermanentDeleteTask),
    Shutdown,
}

#[derive(Debug)]
pub struct PermanentDeleteExecutor {
    sender: Option<SyncSender<PermanentDeleteCommand>>,
    cancellations: Arc<Mutex<HashMap<JobId, Arc<AtomicBool>>>>,
    jobs: SharedJobManager,
    worker: Option<JoinHandle<()>>,
}

impl PermanentDeleteExecutor {
    pub fn spawn(jobs: SharedJobManager) -> Result<Self, PermanentDeleteExecutorSpawnError> {
        Self::spawn_with_backend_and_gate(
            jobs,
            DEFAULT_PERMANENT_DELETE_QUEUE_CAPACITY,
            Arc::new(FilesystemPermanentDeleteBackend),
            None,
        )
    }

    fn spawn_with_backend_and_gate(
        jobs: SharedJobManager,
        capacity: usize,
        backend: Arc<dyn PermanentDeleteBackend>,
        start_gate: Option<Receiver<()>>,
    ) -> Result<Self, PermanentDeleteExecutorSpawnError> {
        if capacity == 0 {
            return Err(PermanentDeleteExecutorSpawnError::ZeroCapacity);
        }
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let cancellations = Arc::new(Mutex::new(HashMap::new()));
        let worker_jobs = Arc::clone(&jobs);
        let worker_cancellations = Arc::clone(&cancellations);
        let worker = thread::Builder::new()
            .name("floe-permanent-delete-worker".to_owned())
            .spawn(move || {
                if let Some(gate) = start_gate {
                    let _ = gate.recv();
                }
                run_worker(receiver, worker_jobs, worker_cancellations, backend);
            })
            .map_err(PermanentDeleteExecutorSpawnError::Thread)?;
        Ok(Self {
            sender: Some(sender),
            cancellations,
            jobs,
            worker: Some(worker),
        })
    }

    pub fn submit(
        &self,
        request: PermanentDeleteRequest,
    ) -> Result<PermanentDeleteSubmission, PermanentDeleteSubmitError> {
        let queued = lock(&self.jobs).queue_operation()?;
        self.enqueue(queued.operation_id(), queued.job_id(), request)
    }

    pub fn submit_retry(
        &self,
        failed_job_id: JobId,
        request: PermanentDeleteRequest,
    ) -> Result<PermanentDeleteSubmission, PermanentDeleteSubmitError> {
        let queued = lock(&self.jobs).retry(failed_job_id)?;
        self.enqueue(queued.operation_id(), queued.job_id(), request)
    }

    pub fn cancel(&self, job_id: JobId) -> Result<(), PermanentDeleteCancelError> {
        let cancellation = lock(&self.cancellations)
            .get(&job_id)
            .cloned()
            .ok_or(PermanentDeleteCancelError::NotActive(job_id))?;
        cancellation.store(true, Ordering::Release);
        Ok(())
    }

    #[cfg(test)]
    pub fn shutdown(mut self) {
        self.stop();
    }

    fn enqueue(
        &self,
        operation_id: OperationId,
        job_id: JobId,
        request: PermanentDeleteRequest,
    ) -> Result<PermanentDeleteSubmission, PermanentDeleteSubmitError> {
        let cancellation = Arc::new(AtomicBool::new(false));
        lock(&self.cancellations).insert(job_id, Arc::clone(&cancellation));
        let command = PermanentDeleteCommand::Execute(PermanentDeleteTask {
            job_id,
            request,
            cancellation,
        });
        let Some(sender) = &self.sender else {
            lock(&self.cancellations).remove(&job_id);
            fail_submission(&self.jobs, job_id, "permanent-delete executor stopped");
            return Err(PermanentDeleteSubmitError::ExecutorStopped(job_id));
        };
        match sender.try_send(command) {
            Ok(()) => Ok(PermanentDeleteSubmission {
                operation_id,
                job_id,
            }),
            Err(TrySendError::Full(_)) => {
                lock(&self.cancellations).remove(&job_id);
                fail_submission(&self.jobs, job_id, "permanent-delete queue is full");
                Err(PermanentDeleteSubmitError::QueueFull(job_id))
            }
            Err(TrySendError::Disconnected(_)) => {
                lock(&self.cancellations).remove(&job_id);
                fail_submission(&self.jobs, job_id, "permanent-delete executor stopped");
                Err(PermanentDeleteSubmitError::ExecutorStopped(job_id))
            }
        }
    }

    #[cfg(test)]
    fn spawn_with_backend(
        jobs: SharedJobManager,
        capacity: usize,
        backend: Arc<dyn PermanentDeleteBackend>,
    ) -> Result<Self, PermanentDeleteExecutorSpawnError> {
        Self::spawn_with_backend_and_gate(jobs, capacity, backend, None)
    }

    #[cfg(test)]
    fn spawn_blocked(
        jobs: SharedJobManager,
        capacity: usize,
        backend: Arc<dyn PermanentDeleteBackend>,
        start_gate: Receiver<()>,
    ) -> Result<Self, PermanentDeleteExecutorSpawnError> {
        Self::spawn_with_backend_and_gate(jobs, capacity, backend, Some(start_gate))
    }

    fn stop(&mut self) {
        for cancellation in lock(&self.cancellations).values() {
            cancellation.store(true, Ordering::Release);
        }
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(PermanentDeleteCommand::Shutdown);
        }
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("permanent-delete executor worker panicked during shutdown");
        }
    }
}

impl Drop for PermanentDeleteExecutor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_worker(
    receiver: Receiver<PermanentDeleteCommand>,
    jobs: SharedJobManager,
    cancellations: Arc<Mutex<HashMap<JobId, Arc<AtomicBool>>>>,
    backend: Arc<dyn PermanentDeleteBackend>,
) {
    while let Ok(command) = receiver.recv() {
        match command {
            PermanentDeleteCommand::Execute(task) => {
                execute_task(task, &jobs, &cancellations, backend.as_ref());
            }
            PermanentDeleteCommand::Shutdown => break,
        }
    }
}

fn execute_task(
    task: PermanentDeleteTask,
    jobs: &SharedJobManager,
    cancellations: &Arc<Mutex<HashMap<JobId, Arc<AtomicBool>>>>,
    backend: &dyn PermanentDeleteBackend,
) {
    if transition(jobs, task.job_id, JobCommand::Start).is_err() {
        lock(cancellations).remove(&task.job_id);
        return;
    }

    let command = match backend.delete(&task.request, &task.cancellation, &mut |progress| {
        let progress = JobProgress::items(progress.completed(), Some(progress.total()))
            .expect("core permanent-delete progress is valid");
        let _ = transition(jobs, task.job_id, JobCommand::SetProgress(progress));
    }) {
        Ok(()) => JobCommand::Complete,
        Err(PermanentDeleteError::Cancelled) => JobCommand::Cancel,
        Err(error) => JobCommand::Fail(permanent_delete_failure(&error)),
    };
    let _ = transition(jobs, task.job_id, command);
    lock(cancellations).remove(&task.job_id);
}

fn permanent_delete_failure(error: &PermanentDeleteError) -> JobFailure {
    let kind = match error {
        PermanentDeleteError::Partial { .. } => JobFailureKind::Partial,
        PermanentDeleteError::MountedBoundary { .. } => JobFailureKind::Unsupported,
        PermanentDeleteError::Cancelled => JobFailureKind::Internal,
        PermanentDeleteError::MountInfo(_)
        | PermanentDeleteError::Preflight { .. }
        | PermanentDeleteError::SourceChanged { .. }
        | PermanentDeleteError::Io { .. } => JobFailureKind::Io,
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

fn fail_submission(jobs: &SharedJobManager, job_id: JobId, message: &str) {
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
        path::PathBuf,
        sync::atomic::AtomicUsize,
        thread,
        time::{Duration, Instant},
    };

    use floe_core::{JobEventKind, JobState};

    use super::*;
    use crate::job_manager::ApplicationJobManager;

    #[derive(Clone, Copy, Debug)]
    enum TestBehavior {
        Success,
        WaitForCancellation,
        FailPartial,
    }

    #[derive(Debug)]
    struct TestBackend {
        behavior: TestBehavior,
        calls: AtomicUsize,
    }

    impl TestBackend {
        fn new(behavior: TestBehavior) -> Self {
            Self {
                behavior,
                calls: AtomicUsize::new(0),
            }
        }
    }

    impl PermanentDeleteBackend for TestBackend {
        fn delete(
            &self,
            _request: &PermanentDeleteRequest,
            cancellation: &AtomicBool,
            on_progress: &mut dyn FnMut(PermanentDeleteProgress),
        ) -> Result<(), PermanentDeleteError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match self.behavior {
                TestBehavior::Success => {
                    on_progress(
                        PermanentDeleteProgress::new(1, 1).expect("test progress should be valid"),
                    );
                    Ok(())
                }
                TestBehavior::WaitForCancellation => {
                    let deadline = Instant::now() + Duration::from_secs(3);
                    while !cancellation.load(Ordering::Acquire) {
                        assert!(Instant::now() < deadline, "cancellation should arrive");
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(PermanentDeleteError::Cancelled)
                }
                TestBehavior::FailPartial => Err(PermanentDeleteError::Partial {
                    removed: 1,
                    total: 2,
                    path: PathBuf::from("/virtual/second"),
                    message: "fixture failure".to_owned(),
                }),
            }
        }
    }

    fn request() -> PermanentDeleteRequest {
        PermanentDeleteRequest::new(vec![PathBuf::from("/virtual/item")])
            .expect("fixture request should be valid")
    }

    fn jobs() -> SharedJobManager {
        Arc::new(Mutex::new(ApplicationJobManager::new()))
    }

    fn wait_for_terminal(jobs: &SharedJobManager, job_id: JobId) -> JobState {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(state) = lock(jobs).record(job_id).map(|record| record.state()) {
                if state.is_terminal() {
                    return state;
                }
            }
            assert!(Instant::now() < deadline, "delete job should finish");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn phase_6m_executor_completes_with_progress() {
        let jobs = jobs();
        let backend = Arc::new(TestBackend::new(TestBehavior::Success));
        let executor = PermanentDeleteExecutor::spawn_with_backend(jobs.clone(), 2, backend)
            .expect("executor should start");
        let submission = executor.submit(request()).expect("request should queue");
        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Completed
        );
        let events = lock(&jobs).drain_events();
        assert!(events.iter().any(|event| {
            event.job_id() == submission.job_id()
                && matches!(event.kind(), JobEventKind::Progressed(progress) if progress.completed() == 1 && progress.total() == Some(1))
        }));
    }

    #[test]
    fn phase_6m_executor_cancels_before_commit_and_shutdown_is_bounded() {
        let jobs = jobs();
        let backend = Arc::new(TestBackend::new(TestBehavior::WaitForCancellation));
        let executor = PermanentDeleteExecutor::spawn_with_backend(jobs.clone(), 1, backend)
            .expect("executor should start");
        let submission = executor.submit(request()).expect("request should queue");
        let deadline = Instant::now() + Duration::from_secs(3);
        while lock(&jobs)
            .record(submission.job_id())
            .map(|record| record.state())
            != Some(JobState::Running)
        {
            assert!(Instant::now() < deadline, "job should start");
            thread::sleep(Duration::from_millis(5));
        }
        executor
            .cancel(submission.job_id())
            .expect("active job should cancel");
        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Cancelled
        );
        let started = Instant::now();
        executor.shutdown();
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn phase_6m_executor_maps_partial_failure_and_rejects_capacity_overflow() {
        let partial_jobs = jobs();
        let partial = Arc::new(TestBackend::new(TestBehavior::FailPartial));
        let executor =
            PermanentDeleteExecutor::spawn_with_backend(partial_jobs.clone(), 1, partial)
                .expect("executor should start");
        let submission = executor.submit(request()).expect("request should queue");
        assert_eq!(
            wait_for_terminal(&partial_jobs, submission.job_id()),
            JobState::Failed
        );
        assert_eq!(
            lock(&partial_jobs)
                .record(submission.job_id())
                .and_then(|record| record.failure())
                .map(JobFailure::kind),
            Some(JobFailureKind::Partial)
        );

        let (gate_sender, gate_receiver) = mpsc::sync_channel(1);
        let blocked_jobs = jobs();
        let blocked = PermanentDeleteExecutor::spawn_blocked(
            blocked_jobs.clone(),
            1,
            Arc::new(TestBackend::new(TestBehavior::Success)),
            gate_receiver,
        )
        .expect("blocked executor should start");
        let first = blocked.submit(request()).expect("first should fill queue");
        let rejected = blocked
            .submit(request())
            .expect_err("second should be rejected");
        assert!(matches!(rejected, PermanentDeleteSubmitError::QueueFull(_)));
        gate_sender.send(()).expect("gate should release");
        assert_eq!(
            wait_for_terminal(&blocked_jobs, first.job_id()),
            JobState::Completed
        );
    }
}
