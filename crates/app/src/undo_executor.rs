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
    ConflictPolicy, CopyCancellation, CopyError, CopyRequest, CreateCancellation, CreateError,
    FileIdentity, JobCommand, JobFailure, JobFailureKind, JobId, LocalTrashReceipt,
    MoveCancellation, MoveError, MoveRequest, OperationId, ReplaceError, RestoreError,
    RestoreRequest, exchange_replace_versions, execute_copy, execute_create, execute_move,
    execute_restore,
};
use gtk::{gio, gio::prelude::*};
use thiserror::Error;

use crate::{
    job_manager::{JobManagerError, SharedJobManager},
    trash_executor::{
        GioTrashBackend, TrashBackend, TrashError, TrashRequest, validate_expected_source,
    },
    undo_history::{UndoHistoryAction, UndoHistoryCoordinator, UndoHistoryRecord, UndoRecipe},
};

pub const DEFAULT_UNDO_QUEUE_CAPACITY: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UndoSubmission {
    operation_id: OperationId,
    job_id: JobId,
    history_id: u64,
    action: UndoHistoryAction,
}

impl UndoSubmission {
    pub const fn operation_id(self) -> OperationId {
        self.operation_id
    }

    pub const fn job_id(self) -> JobId {
        self.job_id
    }

    pub const fn history_id(self) -> u64 {
        self.history_id
    }

    pub const fn action(self) -> UndoHistoryAction {
        self.action
    }
}

#[derive(Debug, Error)]
pub enum UndoExecutorSpawnError {
    #[error("Undo/Redo executor queue capacity must be greater than zero")]
    ZeroCapacity,
    #[error("could not start Undo/Redo executor: {0}")]
    Thread(#[source] io::Error),
}

#[derive(Debug, Error)]
pub enum UndoSubmitError {
    #[error(transparent)]
    JobManager(#[from] JobManagerError),
    #[error("Undo/Redo queue is at capacity; job {job_id:?} was failed")]
    QueueFull { job_id: JobId },
    #[error("Undo/Redo executor has stopped; job {job_id:?} was failed")]
    ExecutorStopped { job_id: JobId },
}

impl UndoSubmitError {
    pub const fn job_id(&self) -> Option<JobId> {
        match self {
            Self::QueueFull { job_id } | Self::ExecutorStopped { job_id } => Some(*job_id),
            Self::JobManager(_) => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum UndoCancelError {
    #[error("job {0:?} is not an active Undo/Redo job")]
    NotActive(JobId),
}

#[derive(Clone, Debug)]
struct UndoCancellation {
    copy: CopyCancellation,
    create: CreateCancellation,
    move_item: MoveCancellation,
    gio: gio::Cancellable,
}

impl UndoCancellation {
    fn new() -> Self {
        Self {
            copy: CopyCancellation::new(),
            create: CreateCancellation::new(),
            move_item: MoveCancellation::new(),
            gio: gio::Cancellable::new(),
        }
    }

    fn cancel(&self) {
        self.copy.cancel();
        self.create.cancel();
        self.move_item.cancel();
        self.gio.cancel();
    }

    fn is_cancelled(&self) -> bool {
        self.copy.is_cancelled() || self.move_item.is_cancelled() || self.gio.is_cancelled()
    }
}

#[derive(Debug)]
struct UndoTask {
    job_id: JobId,
    history_id: u64,
    action: UndoHistoryAction,
    cancellation: UndoCancellation,
}

#[derive(Debug)]
enum UndoCommand {
    Execute(UndoTask),
    Shutdown,
}

#[derive(Debug)]
pub struct UndoExecutor {
    sender: Option<SyncSender<UndoCommand>>,
    cancellations: Arc<Mutex<HashMap<JobId, UndoCancellation>>>,
    jobs: SharedJobManager,
    worker: Option<JoinHandle<()>>,
}

impl UndoExecutor {
    pub fn spawn(
        jobs: SharedJobManager,
        history: UndoHistoryCoordinator,
    ) -> Result<Self, UndoExecutorSpawnError> {
        Self::spawn_with_backend(
            jobs,
            history,
            DEFAULT_UNDO_QUEUE_CAPACITY,
            Arc::new(GioTrashBackend),
        )
    }

    fn spawn_with_backend(
        jobs: SharedJobManager,
        history: UndoHistoryCoordinator,
        capacity: usize,
        trash: Arc<dyn TrashBackend>,
    ) -> Result<Self, UndoExecutorSpawnError> {
        if capacity == 0 {
            return Err(UndoExecutorSpawnError::ZeroCapacity);
        }
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let cancellations = Arc::new(Mutex::new(HashMap::new()));
        let worker_jobs = Arc::clone(&jobs);
        let worker_cancellations = Arc::clone(&cancellations);
        let worker = thread::Builder::new()
            .name("floe-undo-worker".to_owned())
            .spawn(move || {
                run_worker(receiver, worker_jobs, worker_cancellations, history, trash);
            })
            .map_err(UndoExecutorSpawnError::Thread)?;
        Ok(Self {
            sender: Some(sender),
            cancellations,
            jobs,
            worker: Some(worker),
        })
    }

    pub fn submit(
        &self,
        history_id: u64,
        action: UndoHistoryAction,
    ) -> Result<UndoSubmission, UndoSubmitError> {
        let queued = lock(&self.jobs).queue_operation()?;
        let cancellation = UndoCancellation::new();
        lock(&self.cancellations).insert(queued.job_id(), cancellation.clone());
        let command = UndoCommand::Execute(UndoTask {
            job_id: queued.job_id(),
            history_id,
            action,
            cancellation,
        });
        let result = match &self.sender {
            Some(sender) => sender.try_send(command),
            None => Err(TrySendError::Disconnected(command)),
        };
        match result {
            Ok(()) => Ok(UndoSubmission {
                operation_id: queued.operation_id(),
                job_id: queued.job_id(),
                history_id,
                action,
            }),
            Err(TrySendError::Full(_)) => {
                lock(&self.cancellations).remove(&queued.job_id());
                fail_submission(
                    &self.jobs,
                    queued.job_id(),
                    "Undo/Redo queue is at capacity",
                );
                Err(UndoSubmitError::QueueFull {
                    job_id: queued.job_id(),
                })
            }
            Err(TrySendError::Disconnected(_)) => {
                lock(&self.cancellations).remove(&queued.job_id());
                fail_submission(
                    &self.jobs,
                    queued.job_id(),
                    "Undo/Redo executor has stopped",
                );
                Err(UndoSubmitError::ExecutorStopped {
                    job_id: queued.job_id(),
                })
            }
        }
    }

    pub fn cancel(&self, job_id: JobId) -> Result<(), UndoCancelError> {
        let cancellation = lock(&self.cancellations)
            .get(&job_id)
            .cloned()
            .ok_or(UndoCancelError::NotActive(job_id))?;
        cancellation.cancel();
        Ok(())
    }
}

impl Drop for UndoExecutor {
    fn drop(&mut self) {
        for cancellation in lock(&self.cancellations).values() {
            cancellation.cancel();
        }
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(UndoCommand::Shutdown);
            drop(sender);
        }
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                tracing::error!("Undo/Redo executor worker panicked during shutdown");
            }
        }
    }
}

fn run_worker(
    receiver: Receiver<UndoCommand>,
    jobs: SharedJobManager,
    cancellations: Arc<Mutex<HashMap<JobId, UndoCancellation>>>,
    history: UndoHistoryCoordinator,
    trash: Arc<dyn TrashBackend>,
) {
    while let Ok(command) = receiver.recv() {
        match command {
            UndoCommand::Execute(task) => {
                execute_task(task, &jobs, &cancellations, &history, trash.as_ref());
            }
            UndoCommand::Shutdown => break,
        }
    }
}

fn execute_task(
    task: UndoTask,
    jobs: &SharedJobManager,
    cancellations: &Arc<Mutex<HashMap<JobId, UndoCancellation>>>,
    history: &UndoHistoryCoordinator,
    trash: &dyn TrashBackend,
) {
    if transition(jobs, task.job_id, JobCommand::Start).is_err() {
        lock(cancellations).remove(&task.job_id);
        return;
    }
    let prepared = match history.prepare_action(task.history_id, task.action) {
        Ok(record) => record,
        Err(error) => {
            let _ = transition(
                jobs,
                task.job_id,
                JobCommand::Fail(JobFailure::new(JobFailureKind::Conflict, error.to_string())),
            );
            lock(cancellations).remove(&task.job_id);
            return;
        }
    };
    let result = execute_action(&prepared, task.action, &task.cancellation, trash);
    let command = match result {
        Ok(success) => match complete_success(history, task.history_id, task.action, success) {
            Ok(()) => JobCommand::Complete,
            Err(error) => JobCommand::Fail(JobFailure::new(
                JobFailureKind::Partial,
                format!("filesystem action completed but Undo history could not commit: {error}"),
            )),
        },
        Err(ActionFailure::Cancelled(message)) => {
            let _ = history.cancel_action(task.history_id, task.action);
            tracing::debug!(history_id = task.history_id, %message, "Undo/Redo cancelled safely");
            JobCommand::Cancel
        }
        Err(ActionFailure::Certain(message)) => {
            let _ = history.cancel_action(task.history_id, task.action);
            JobCommand::Fail(JobFailure::new(JobFailureKind::Io, message))
        }
        Err(ActionFailure::Conflict(message)) => {
            let _ = history.cancel_action(task.history_id, task.action);
            JobCommand::Fail(JobFailure::new(JobFailureKind::Conflict, message))
        }
        Err(ActionFailure::Uncertain(message)) => {
            let _ = history.mark_action_uncertain(task.history_id);
            JobCommand::Fail(JobFailure::new(JobFailureKind::Partial, message))
        }
    };
    let _ = transition(jobs, task.job_id, command);
    lock(cancellations).remove(&task.job_id);
}

#[derive(Debug)]
enum ActionFailure {
    Cancelled(String),
    Conflict(String),
    Certain(String),
    Uncertain(String),
}

enum ActionSuccess {
    Identity(Option<FileIdentity>),
    TrashUndone(FileIdentity),
    TrashRedone(LocalTrashReceipt),
}

fn complete_success(
    history: &UndoHistoryCoordinator,
    id: u64,
    action: UndoHistoryAction,
    success: ActionSuccess,
) -> Result<(), crate::undo_history::UndoHistoryError> {
    match success {
        ActionSuccess::Identity(identity) => history.complete_action(id, action, identity),
        ActionSuccess::TrashUndone(identity) => history.complete_trash_undo(id, identity),
        ActionSuccess::TrashRedone(receipt) => history.complete_trash_redo(
            id,
            receipt.original_path(),
            receipt.payload_path(),
            receipt.info_path(),
            receipt.payload_identity(),
            receipt.info_identity(),
        ),
    }
}

fn execute_action(
    record: &UndoHistoryRecord,
    action: UndoHistoryAction,
    cancellation: &UndoCancellation,
    trash: &dyn TrashBackend,
) -> Result<ActionSuccess, ActionFailure> {
    match action {
        UndoHistoryAction::Undo => execute_undo(record, cancellation, trash),
        UndoHistoryAction::Redo => execute_redo(record, cancellation, trash),
    }
}

fn execute_undo(
    record: &UndoHistoryRecord,
    cancellation: &UndoCancellation,
    trash: &dyn TrashBackend,
) -> Result<ActionSuccess, ActionFailure> {
    let identity = record.current_identity().ok_or_else(|| {
        ActionFailure::Certain("Undo record has no committed identity".to_owned())
    })?;
    match record.recipe() {
        UndoRecipe::Copy { .. } | UndoRecipe::Create(_) => {
            let request = TrashRequest::new(destination_for_record(record).to_path_buf())
                .map_err(|error| ActionFailure::Certain(error.to_string()))?
                .with_expected_source_identity(
                    identity,
                    record.recipe().require_empty_directory_on_undo(),
                );
            validate_expected_source(&request)
                .map_err(|error| ActionFailure::Certain(error.to_string()))?;
            trash
                .trash(&request, &cancellation.gio)
                .map_err(map_trash_failure)?;
            Ok(ActionSuccess::Identity(None))
        }
        UndoRecipe::Move {
            source,
            destination,
        }
        | UndoRecipe::Rename {
            source,
            destination,
        } => execute_move(
            &MoveRequest::new(destination, source, ConflictPolicy::FailIfExists)
                .with_expected_source_identity(identity),
            &cancellation.move_item,
        )
        .map(|outcome| ActionSuccess::Identity(Some(outcome.destination_identity())))
        .map_err(map_move_failure),
        UndoRecipe::Replace {
            destination,
            backup,
            ..
        } => {
            if cancellation.is_cancelled() {
                return Err(ActionFailure::Cancelled(
                    "Replace Undo cancelled before atomic exchange".to_owned(),
                ));
            }
            let backup_identity = record.alternate_identity().ok_or_else(|| {
                ActionFailure::Certain("Replace Undo record has no backup identity".to_owned())
            })?;
            exchange_replace_versions(destination, backup, identity, backup_identity)
                .map(|outcome| ActionSuccess::Identity(Some(outcome.destination_identity())))
                .map_err(map_replace_failure)
        }
        UndoRecipe::Trash {
            original,
            payload,
            info,
        } => {
            let info_identity = record.alternate_identity().ok_or_else(|| {
                ActionFailure::Certain("Trash Undo record has no metadata identity".to_owned())
            })?;
            let request = RestoreRequest::new(payload, info, original)
                .map_err(|error| ActionFailure::Certain(error.to_string()))?
                .with_expected_identities(identity, info_identity);
            execute_restore(&request, &cancellation.move_item).map_err(map_restore_failure)?;
            FileIdentity::capture(original)
                .map(ActionSuccess::TrashUndone)
                .map_err(|error| ActionFailure::Uncertain(error.to_string()))
        }
    }
}

fn execute_redo(
    record: &UndoHistoryRecord,
    cancellation: &UndoCancellation,
    trash: &dyn TrashBackend,
) -> Result<ActionSuccess, ActionFailure> {
    match record.recipe() {
        UndoRecipe::Copy {
            source,
            destination,
            symlink_policy,
        } => execute_copy(
            &CopyRequest::new(
                source,
                destination,
                ConflictPolicy::FailIfExists,
                *symlink_policy,
            ),
            &cancellation.copy,
            |_| {},
        )
        .map_err(map_copy_failure)
        .and_then(|_| {
            FileIdentity::capture(destination)
                .map(|identity| ActionSuccess::Identity(Some(identity)))
                .map_err(|error| ActionFailure::Uncertain(error.to_string()))
        }),
        UndoRecipe::Move {
            source,
            destination,
        }
        | UndoRecipe::Rename {
            source,
            destination,
        } => {
            let identity = record.current_identity().ok_or_else(|| {
                ActionFailure::Certain("Redo record has no current source identity".to_owned())
            })?;
            execute_move(
                &MoveRequest::new(source, destination, ConflictPolicy::FailIfExists)
                    .with_expected_source_identity(identity),
                &cancellation.move_item,
            )
            .map(|outcome| ActionSuccess::Identity(Some(outcome.destination_identity())))
            .map_err(map_move_failure)
        }
        UndoRecipe::Create(request) => execute_create(request, &cancellation.create, |_| {})
            .map(|outcome| ActionSuccess::Identity(Some(outcome.destination_identity())))
            .map_err(map_create_failure),
        UndoRecipe::Replace {
            destination,
            backup,
            ..
        } => {
            if cancellation.is_cancelled() {
                return Err(ActionFailure::Cancelled(
                    "Replace Redo cancelled before atomic exchange".to_owned(),
                ));
            }
            let destination_identity = record.current_identity().ok_or_else(|| {
                ActionFailure::Certain("Replace Redo record has no destination identity".to_owned())
            })?;
            let backup_identity = record.alternate_identity().ok_or_else(|| {
                ActionFailure::Certain("Replace Redo record has no backup identity".to_owned())
            })?;
            exchange_replace_versions(destination, backup, destination_identity, backup_identity)
                .map(|outcome| ActionSuccess::Identity(Some(outcome.destination_identity())))
                .map_err(map_replace_failure)
        }
        UndoRecipe::Trash { original, .. } => {
            let identity = record.current_identity().ok_or_else(|| {
                ActionFailure::Certain("Trash Redo record has no restored identity".to_owned())
            })?;
            let request = TrashRequest::new(original.to_path_buf())
                .map_err(|error| ActionFailure::Certain(error.to_string()))?
                .with_expected_source_identity(identity, false);
            validate_expected_source(&request)
                .map_err(|error| ActionFailure::Certain(error.to_string()))?;
            match trash
                .trash(&request, &cancellation.gio)
                .map_err(map_trash_failure)?
            {
                Some(receipt) => Ok(ActionSuccess::TrashRedone(receipt)),
                None => Err(ActionFailure::Uncertain(
                    "Trash Redo completed, but no complete local Undo receipt was available"
                        .to_owned(),
                )),
            }
        }
    }
}

fn destination_for_record(record: &UndoHistoryRecord) -> &std::path::Path {
    record.recipe().destination()
}

fn map_copy_failure(error: CopyError) -> ActionFailure {
    if error.is_cancelled() {
        ActionFailure::Cancelled(error.to_string())
    } else if error.is_partial() {
        ActionFailure::Uncertain(error.to_string())
    } else {
        ActionFailure::Certain(error.to_string())
    }
}

fn map_move_failure(error: MoveError) -> ActionFailure {
    if error.is_cancelled() {
        ActionFailure::Cancelled(error.to_string())
    } else if error.is_partial() {
        ActionFailure::Uncertain(error.to_string())
    } else {
        ActionFailure::Certain(error.to_string())
    }
}

fn map_replace_failure(error: ReplaceError) -> ActionFailure {
    if error.is_cancelled() {
        ActionFailure::Cancelled(error.to_string())
    } else if error.is_partial() {
        ActionFailure::Uncertain(error.to_string())
    } else {
        ActionFailure::Certain(error.to_string())
    }
}

fn map_restore_failure(error: RestoreError) -> ActionFailure {
    if error.is_cancelled() {
        ActionFailure::Cancelled(error.to_string())
    } else if error.is_partial() {
        ActionFailure::Uncertain(error.to_string())
    } else if error.is_conflict() {
        ActionFailure::Conflict(error.to_string())
    } else {
        ActionFailure::Certain(error.to_string())
    }
}

fn map_create_failure(error: CreateError) -> ActionFailure {
    match error {
        CreateError::Cancelled => ActionFailure::Cancelled(error.to_string()),
        CreateError::Copy(CopyError::CleanupFailed { .. }) => {
            ActionFailure::Uncertain(error.to_string())
        }
        _ => ActionFailure::Certain(error.to_string()),
    }
}

fn map_trash_failure(error: TrashError) -> ActionFailure {
    match error {
        TrashError::Cancelled => ActionFailure::Cancelled(error.to_string()),
        TrashError::SourceChanged { .. }
        | TrashError::NotFound { .. }
        | TrashError::DirectoryNotEmpty { .. } => ActionFailure::Certain(error.to_string()),
        _ => ActionFailure::Uncertain(error.to_string()),
    }
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
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };

    use floe_core::{
        JobState, SymlinkPolicy, TrashRoot, discover_local_trash_receipt, snapshot_trash_roots,
    };
    use tempfile::tempdir;

    use crate::{
        job_manager::ApplicationJobManager,
        undo_history::{UndoHistoryState, UndoHistoryStore},
    };

    use super::*;

    #[derive(Debug)]
    struct RenameTrashBackend {
        root: PathBuf,
        next: AtomicU64,
    }

    #[derive(Debug)]
    struct ReceiptTrashBackend {
        root: TrashRoot,
        next: AtomicU64,
    }

    impl ReceiptTrashBackend {
        fn new(root: TrashRoot) -> Self {
            Self {
                root,
                next: AtomicU64::new(1),
            }
        }
    }

    impl TrashBackend for ReceiptTrashBackend {
        fn trash(
            &self,
            request: &TrashRequest,
            cancellable: &gio::Cancellable,
        ) -> Result<Option<LocalTrashReceipt>, TrashError> {
            if cancellable.is_cancelled() {
                return Err(TrashError::Cancelled);
            }
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

    impl TrashBackend for RenameTrashBackend {
        fn trash(
            &self,
            request: &TrashRequest,
            cancellable: &gio::Cancellable,
        ) -> Result<Option<LocalTrashReceipt>, TrashError> {
            if cancellable.is_cancelled() {
                return Err(TrashError::Cancelled);
            }
            let id = self.next.fetch_add(1, Ordering::Relaxed);
            fs::rename(request.source(), self.root.join(format!("trashed-{id}"))).map_err(
                |error| TrashError::Io {
                    message: error.to_string(),
                },
            )?;
            Ok(None)
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
            assert!(Instant::now() < deadline, "Undo/Redo job did not finish");
            std::thread::yield_now();
        }
    }

    fn executor_fixture(
        root: &std::path::Path,
    ) -> (UndoHistoryStore, UndoExecutor, SharedJobManager) {
        let store = UndoHistoryStore::open_at(root.join("state/undo.bin")).expect("history store");
        let history = UndoHistoryCoordinator::from_store(store.clone());
        let jobs = jobs();
        let trash_root = root.join("trash");
        fs::create_dir(&trash_root).expect("trash root");
        let executor = UndoExecutor::spawn_with_backend(
            Arc::clone(&jobs),
            history,
            2,
            Arc::new(RenameTrashBackend {
                root: trash_root,
                next: AtomicU64::new(1),
            }),
        )
        .expect("Undo executor");
        (store, executor, jobs)
    }

    #[test]
    fn phase_18y2_local_copy_undo_and_redo_survive_as_typed_history() {
        let fixture = tempdir().expect("fixture");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"durable payload").expect("source");
        fs::copy(&source, &destination).expect("initial copy");
        let (store, executor, jobs) = executor_fixture(fixture.path());
        let ticket = store
            .begin(UndoRecipe::copy(
                &source,
                &destination,
                SymlinkPolicy::Preserve,
            ))
            .expect("begin");
        store
            .complete(
                ticket,
                FileIdentity::capture(&destination).expect("destination identity"),
            )
            .expect("complete");

        let undo = executor
            .submit(ticket.id(), UndoHistoryAction::Undo)
            .expect("submit Undo");
        assert_eq!(wait_for_terminal(&jobs, undo.job_id()), JobState::Completed);
        assert!(!destination.exists());
        assert_eq!(store.history()[0].state(), UndoHistoryState::Undone);

        let redo = executor
            .submit(ticket.id(), UndoHistoryAction::Redo)
            .expect("submit Redo");
        assert_eq!(wait_for_terminal(&jobs, redo.job_id()), JobState::Completed);
        assert_eq!(
            fs::read(&destination).expect("redone copy"),
            b"durable payload"
        );
        assert_eq!(store.history()[0].state(), UndoHistoryState::Applied);
    }

    #[test]
    fn phase_18y2_local_duplicate_action_submission_cannot_revert_another_actions_state() {
        let fixture = tempdir().expect("fixture");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"payload").expect("source");
        fs::copy(&source, &destination).expect("destination");
        let (store, executor, jobs) = executor_fixture(fixture.path());
        let ticket = store
            .begin(UndoRecipe::copy(
                &source,
                &destination,
                SymlinkPolicy::Preserve,
            ))
            .expect("begin");
        store
            .complete(
                ticket,
                FileIdentity::capture(&destination).expect("identity"),
            )
            .expect("complete");

        let first = executor
            .submit(ticket.id(), UndoHistoryAction::Undo)
            .expect("first Undo");
        let second = executor
            .submit(ticket.id(), UndoHistoryAction::Undo)
            .expect("second Undo queues before worker state is known");
        assert_eq!(
            wait_for_terminal(&jobs, first.job_id()),
            JobState::Completed
        );
        assert_eq!(wait_for_terminal(&jobs, second.job_id()), JobState::Failed);
        assert_eq!(store.history()[0].state(), UndoHistoryState::Undone);
        assert!(!destination.exists());
    }

    #[test]
    fn phase_18y2_local_move_inverse_is_identity_checked_and_never_overwrites() {
        let fixture = tempdir().expect("fixture");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&destination, b"moved item").expect("destination");
        let (store, executor, jobs) = executor_fixture(fixture.path());
        let ticket = store
            .begin(UndoRecipe::move_item(&source, &destination))
            .expect("begin");
        store
            .complete(
                ticket,
                FileIdentity::capture(&destination).expect("identity"),
            )
            .expect("complete");
        fs::write(&source, b"new occupant").expect("conflicting source");

        let undo = executor
            .submit(ticket.id(), UndoHistoryAction::Undo)
            .expect("submit Undo");
        assert_eq!(wait_for_terminal(&jobs, undo.job_id()), JobState::Failed);
        assert_eq!(fs::read(&source).expect("occupant"), b"new occupant");
        assert_eq!(fs::read(&destination).expect("moved item"), b"moved item");
        assert_eq!(store.history()[0].state(), UndoHistoryState::Applied);
    }

    #[test]
    fn phase_18y2_local_create_directory_undo_refuses_later_user_data() {
        let fixture = tempdir().expect("fixture");
        let destination = fixture.path().join("created");
        fs::create_dir(&destination).expect("created directory");
        let (store, executor, jobs) = executor_fixture(fixture.path());
        let ticket = store
            .begin(UndoRecipe::create(
                floe_core::CreateRequest::directory(&destination).expect("request"),
            ))
            .expect("begin");
        store
            .complete(
                ticket,
                FileIdentity::capture(&destination).expect("identity"),
            )
            .expect("complete");
        fs::write(destination.join("later-data"), b"protect me").expect("later data");

        let undo = executor
            .submit(ticket.id(), UndoHistoryAction::Undo)
            .expect("submit Undo");
        assert_eq!(wait_for_terminal(&jobs, undo.job_id()), JobState::Failed);
        assert!(destination.join("later-data").exists());
        assert_eq!(store.history()[0].state(), UndoHistoryState::Applied);
    }

    #[test]
    fn phase_6w_undo_redo_restores_no_overwrite_and_refreshes_receipt() {
        let fixture = tempdir().expect("fixture");
        let root = TrashRoot::new(fixture.path().join("Trash"), None);
        fs::create_dir_all(root.files()).expect("files");
        fs::create_dir_all(root.info()).expect("info");
        let original = fixture.path().join("original/item");
        fs::create_dir_all(original.parent().expect("parent")).expect("original parent");
        let payload = root.files().join("initial");
        let info = root.info().join("initial.trashinfo");
        fs::write(&payload, b"payload").expect("payload");
        fs::write(
            &info,
            format!("[Trash Info]\nPath={}\n", original.display()),
        )
        .expect("metadata");

        let store = UndoHistoryStore::open_at(fixture.path().join("state/undo.bin"))
            .expect("history store");
        let ticket = store
            .begin(UndoRecipe::trash(&original, &payload, &info))
            .expect("begin");
        store
            .complete_trash(
                ticket,
                FileIdentity::capture(&payload).expect("payload identity"),
                FileIdentity::capture(&info).expect("info identity"),
            )
            .expect("complete");
        let history = UndoHistoryCoordinator::from_store(store.clone());
        let backend = ReceiptTrashBackend::new(root.clone());
        let cancellation = UndoCancellation::new();

        let prepared = history
            .prepare_action(ticket.id(), UndoHistoryAction::Undo)
            .expect("prepare undo");
        let success = execute_action(&prepared, UndoHistoryAction::Undo, &cancellation, &backend)
            .expect("undo");
        complete_success(&history, ticket.id(), UndoHistoryAction::Undo, success)
            .expect("commit undo");
        assert_eq!(fs::read(&original).expect("restored"), b"payload");
        assert!(!payload.exists());
        assert!(!info.exists());
        assert!(store.history()[0].can_redo());

        let prepared = history
            .prepare_action(ticket.id(), UndoHistoryAction::Redo)
            .expect("prepare redo");
        let success = execute_action(&prepared, UndoHistoryAction::Redo, &cancellation, &backend)
            .expect("redo");
        complete_success(&history, ticket.id(), UndoHistoryAction::Redo, success)
            .expect("commit redo");
        assert!(!original.exists());
        let redone = store.history().pop().expect("redone record");
        let (_, redone_payload, redone_info) = redone.recipe().trash_paths().expect("paths");
        assert_eq!(
            fs::read(redone_payload).expect("redone payload"),
            b"payload"
        );
        assert!(redone_info.exists());
        assert!(redone.can_undo());

        fs::write(&original, b"unrelated occupant").expect("conflict occupant");
        let prepared = history
            .prepare_action(ticket.id(), UndoHistoryAction::Undo)
            .expect("prepare conflict undo");
        assert!(matches!(
            execute_action(&prepared, UndoHistoryAction::Undo, &cancellation, &backend),
            Err(ActionFailure::Conflict(_))
        ));
        history
            .cancel_action(ticket.id(), UndoHistoryAction::Undo)
            .expect("restore action state");
        assert_eq!(
            fs::read(&original).expect("occupant retained"),
            b"unrelated occupant"
        );
        assert!(redone_payload.exists());
        assert!(redone_info.exists());
    }
}
