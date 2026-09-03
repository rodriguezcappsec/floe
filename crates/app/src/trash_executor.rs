use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

use floe_core::{
    FileIdentity, JobCommand, JobFailure, JobFailureKind, JobId, LocalTrashReceipt, OperationId,
    TrashRoot, discover_local_trash_receipt, snapshot_trash_roots,
};
use gio::prelude::*;
use gtk::{gio, glib};
use thiserror::Error;

use crate::{
    job_manager::{JobManagerError, SharedJobManager},
    undo_history::UndoHistoryCoordinator,
};

pub const DEFAULT_TRASH_QUEUE_CAPACITY: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrashRequest {
    source: PathBuf,
    expected_source_identity: Option<FileIdentity>,
    require_empty_directory: bool,
}

impl TrashRequest {
    pub fn new(source: PathBuf) -> Result<Self, TrashRequestError> {
        if source.file_name().is_none() {
            return Err(TrashRequestError::InvalidSource(source));
        }
        Ok(Self {
            source,
            expected_source_identity: None,
            require_empty_directory: false,
        })
    }

    pub fn source(&self) -> &Path {
        &self.source
    }

    pub fn with_expected_source_identity(
        mut self,
        identity: FileIdentity,
        require_empty_directory: bool,
    ) -> Self {
        self.expected_source_identity = Some(identity);
        self.require_empty_directory = require_empty_directory;
        self
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
    #[error("item changed after it was created: {message}")]
    SourceChanged { message: String },
    #[error("created directory is no longer empty: {message}")]
    DirectoryNotEmpty { message: String },
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
    ) -> Result<Option<LocalTrashReceipt>, TrashError>;
}

#[derive(Debug, Default)]
pub(crate) struct GioTrashBackend;

impl TrashBackend for GioTrashBackend {
    fn trash(
        &self,
        request: &TrashRequest,
        cancellable: &gio::Cancellable,
    ) -> Result<Option<LocalTrashReceipt>, TrashError> {
        if cancellable.is_cancelled() {
            return Err(TrashError::Cancelled);
        }

        let source = gio::File::for_path(request.source());
        let snapshots = snapshot_trash_roots(local_trash_roots(&source, cancellable));
        source.trash(Some(cancellable)).map_err(map_gio_error)?;
        Ok(discover_local_trash_receipt(&snapshots, request.source()))
    }
}

fn local_trash_roots(source: &gio::File, cancellable: &gio::Cancellable) -> Vec<TrashRoot> {
    let mut roots = vec![TrashRoot::for_data_home(glib::user_data_dir())];
    if let Ok(mount) = source.find_enclosing_mount(Some(cancellable)) {
        if let Some(top) = mount.root().path() {
            roots.extend(TrashRoot::for_mount_top(
                &top,
                rustix::process::getuid().as_raw(),
            ));
        }
    }
    roots
}

pub(crate) fn validate_expected_source(request: &TrashRequest) -> Result<(), TrashError> {
    let Some(expected) = request.expected_source_identity else {
        return Ok(());
    };
    let metadata = fs::symlink_metadata(request.source()).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => TrashError::NotFound {
            message: "the original created item is missing".to_owned(),
        },
        io::ErrorKind::PermissionDenied => TrashError::PermissionDenied {
            message: "the original created item cannot be inspected".to_owned(),
        },
        _ => TrashError::Io {
            message: format!("could not inspect the original created item: {error}"),
        },
    })?;
    if !expected.matches(&metadata) {
        return Err(TrashError::SourceChanged {
            message: "the current path no longer identifies the item Floe created".to_owned(),
        });
    }
    if request.require_empty_directory && metadata.file_type().is_dir() {
        let mut entries = fs::read_dir(request.source()).map_err(|error| TrashError::Io {
            message: format!("could not inspect the created directory: {error}"),
        })?;
        if entries
            .next()
            .transpose()
            .map_err(|error| TrashError::Io {
                message: format!("could not inspect a created directory entry: {error}"),
            })?
            .is_some()
        {
            return Err(TrashError::DirectoryNotEmpty {
                message: "it now contains files or folders".to_owned(),
            });
        }
    }
    Ok(())
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
            None,
        )
    }

    pub fn spawn_with_undo(
        jobs: SharedJobManager,
        undo_history: UndoHistoryCoordinator,
    ) -> Result<Self, TrashExecutorSpawnError> {
        Self::spawn_with_backend_and_gate(
            jobs,
            DEFAULT_TRASH_QUEUE_CAPACITY,
            Arc::new(GioTrashBackend),
            Some(undo_history),
            None,
        )
    }

    fn spawn_with_backend_and_gate(
        jobs: SharedJobManager,
        capacity: usize,
        backend: Arc<dyn TrashBackend>,
        undo_history: Option<UndoHistoryCoordinator>,
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
                run_worker(
                    receiver,
                    worker_jobs,
                    worker_cancellations,
                    backend,
                    undo_history,
                );
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
        Self::spawn_with_backend_and_gate(jobs, capacity, backend, None, None)
    }

    #[cfg(test)]
    fn spawn_blocked(
        jobs: SharedJobManager,
        capacity: usize,
        backend: Arc<dyn TrashBackend>,
        start_gate: Receiver<()>,
    ) -> Result<Self, TrashExecutorSpawnError> {
        Self::spawn_with_backend_and_gate(jobs, capacity, backend, None, Some(start_gate))
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
    undo_history: Option<UndoHistoryCoordinator>,
) {
    while let Ok(command) = receiver.recv() {
        match command {
            TrashCommand::Execute(task) => {
                execute_task(
                    task,
                    &jobs,
                    &cancellations,
                    backend.as_ref(),
                    undo_history.as_ref(),
                );
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
    undo_history: Option<&UndoHistoryCoordinator>,
) {
    if transition(jobs, task.job_id, JobCommand::Start).is_err() {
        lock(cancellations).remove(&task.job_id);
        return;
    }

    let result = validate_expected_source(&task.request)
        .and_then(|()| backend.trash(&task.request, &task.cancellable));
    let command = match result {
        Ok(receipt) => {
            if let (Some(history), Some(receipt)) = (undo_history, receipt) {
                if let Err(error) = history.record_trash(
                    receipt.original_path(),
                    receipt.payload_path(),
                    receipt.info_path(),
                    receipt.payload_identity(),
                    receipt.info_identity(),
                ) {
                    tracing::warn!(%error, "Trash completed without durable Undo receipt");
                }
            }
            JobCommand::Complete
        }
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
        TrashError::DirectoryNotEmpty { .. } => JobFailureKind::Conflict,
        TrashError::NotFound { .. } | TrashError::Io { .. } | TrashError::SourceChanged { .. } => {
            JobFailureKind::Io
        }
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
        sync::atomic::{AtomicU64, AtomicUsize, Ordering},
        time::{Duration, Instant},
    };

    use floe_core::{JobEventKind, JobState};
    use tempfile::tempdir;

    use super::*;
    use crate::{
        job_manager::ApplicationJobManager,
        undo_history::{UndoHistoryCoordinator, UndoHistoryStore, UndoRecipe},
    };

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
        ) -> Result<Option<LocalTrashReceipt>, TrashError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            lock(&self.paths).push(request.source().to_path_buf());
            match &self.behavior {
                TestBehavior::Success => Ok(None),
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

    #[derive(Debug)]
    struct ReceiptBackend {
        root: TrashRoot,
        next: AtomicU64,
    }

    impl ReceiptBackend {
        fn new(root: TrashRoot) -> Self {
            Self {
                root,
                next: AtomicU64::new(1),
            }
        }
    }

    impl TrashBackend for ReceiptBackend {
        fn trash(
            &self,
            request: &TrashRequest,
            _cancellable: &gio::Cancellable,
        ) -> Result<Option<LocalTrashReceipt>, TrashError> {
            let snapshots = snapshot_trash_roots(vec![self.root.clone()]);
            let id = self.next.fetch_add(1, Ordering::Relaxed);
            let name = format!("item-{id}");
            let payload = self.root.files().join(&name);
            let info = self.root.info().join(format!("{name}.trashinfo"));
            fs::rename(request.source(), &payload).map_err(|error| TrashError::Io {
                message: error.to_string(),
            })?;
            fs::write(
                &info,
                format!("[Trash Info]\nPath={}\n", request.source().display()),
            )
            .map_err(|error| TrashError::Io {
                message: error.to_string(),
            })?;
            Ok(discover_local_trash_receipt(&snapshots, request.source()))
        }
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
    fn phase_18y_undo_create_revalidates_identity_and_empty_directory() {
        let fixture = tempdir().expect("temporary trash preflight root");
        let file = fixture.path().join("created-file");
        fs::write(&file, b"original").expect("created file fixture");
        let file_identity = FileIdentity::capture(&file).expect("file identity");
        let file_request = TrashRequest::new(file.clone())
            .expect("request")
            .with_expected_source_identity(file_identity, false);
        validate_expected_source(&file_request).expect("unchanged created file is undoable");
        fs::remove_file(&file).expect("remove original fixture");
        fs::write(&file, b"replacement with a new inode").expect("replacement fixture");
        assert!(matches!(
            validate_expected_source(&file_request),
            Err(TrashError::SourceChanged { .. })
        ));

        let directory = fixture.path().join("created-directory");
        fs::create_dir(&directory).expect("created directory fixture");
        let directory_identity = FileIdentity::capture(&directory).expect("directory identity");
        let directory_request = TrashRequest::new(directory.clone())
            .expect("request")
            .with_expected_source_identity(directory_identity, true);
        validate_expected_source(&directory_request)
            .expect("unchanged empty created directory is undoable");
        fs::write(directory.join("user-data"), b"keep").expect("user data fixture");
        assert!(matches!(
            validate_expected_source(&directory_request),
            Err(TrashError::SourceChanged { .. }) | Err(TrashError::DirectoryNotEmpty { .. })
        ));
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

    #[test]
    fn phase_6w_trash_receipt_records_only_complete_executor_outcomes() {
        let fixture = tempdir().expect("fixture");
        let root = TrashRoot::new(fixture.path().join("Trash"), None);
        fs::create_dir_all(root.files()).expect("files");
        fs::create_dir_all(root.info()).expect("info");
        let source = fixture.path().join("source/item");
        fs::create_dir_all(source.parent().expect("parent")).expect("source parent");
        fs::write(&source, b"payload").expect("source");
        let store =
            UndoHistoryStore::open_at(fixture.path().join("state/undo.bin")).expect("history");
        let history = UndoHistoryCoordinator::from_store(store.clone());
        let tracked_jobs = jobs();
        let executor = TrashExecutor::spawn_with_backend_and_gate(
            tracked_jobs.clone(),
            1,
            Arc::new(ReceiptBackend::new(root)),
            Some(history),
            None,
        )
        .expect("executor");
        let submission = executor
            .submit_trash(TrashRequest::new(source.clone()).expect("request"))
            .expect("submission");
        assert_eq!(
            wait_for_terminal(&tracked_jobs, submission.job_id()),
            JobState::Completed
        );
        let record = store.history().pop().expect("durable Trash record");
        assert!(matches!(record.recipe(), UndoRecipe::Trash { .. }));
        assert_eq!(record.recipe().destination(), source);
        let (_, payload, info) = record.recipe().trash_paths().expect("receipt paths");
        assert_eq!(fs::read(payload).expect("payload"), b"payload");
        assert!(info.exists());

        let unsupported_source = fixture.path().join("source/untracked");
        fs::write(&unsupported_source, b"untracked").expect("untracked source");
        let unsupported_jobs = jobs();
        let unsupported = TrashExecutor::spawn_with_backend_and_gate(
            unsupported_jobs.clone(),
            1,
            Arc::new(TestBackend::new(TestBehavior::Success)),
            Some(UndoHistoryCoordinator::from_store(store.clone())),
            None,
        )
        .expect("unsupported executor");
        let submission = unsupported
            .submit_trash(TrashRequest::new(unsupported_source).expect("request"))
            .expect("submission");
        assert_eq!(
            wait_for_terminal(&unsupported_jobs, submission.job_id()),
            JobState::Completed
        );
        assert_eq!(store.history().len(), 1, "no receipt means no Undo claim");
    }
}
