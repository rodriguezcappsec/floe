use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use floe_core::{
    ArchiveOutcome, ArchiveRequest, BatchRenameOutcome, BatchRenameRequest, ChecksumRequest,
    ConflictPolicy, CopyRequest, CreateKind, CreateRequest, CreateRequestError, DestructiveAction,
    DestructiveScope, DestructiveScopeError, GuardrailPermitError, JobEvent, JobId, MoveRequest,
    OperationId, PermanentDeleteRequest, PermanentDeleteRequestError, PermissionRequest,
    RenameRequest, RestoreRequest, RestoreRequestError, SymlinkPolicy, VerifiedCopyRequest,
};
use thiserror::Error;

use crate::{
    archive_executor::{
        ArchiveCancelError, ArchiveExecutor, ArchiveExecutorSpawnError, ArchiveSubmission,
        ArchiveSubmitError,
    },
    batch_rename::{BatchRenameExecutor, BatchRenameExecutorError, BatchRenameSubmission},
    checksum_executor::{
        ChecksumCancelError, ChecksumExecutor, ChecksumExecutorSpawnError, ChecksumOutcome,
        ChecksumSubmission, ChecksumSubmitError,
    },
    copy_executor::{
        CopyCancelError, CopyExecutor, CopyExecutorSpawnError, CopySubmission, CopySubmitError,
    },
    create_executor::{
        CreateCancelError, CreateExecutor, CreateExecutorSpawnError, CreateSubmission,
        CreateSubmitError,
    },
    drag_drop::{DropAction, DropDestination, DropPolicyError, DropRequest, plan_directory_drop},
    guardrail_controller::{
        GuardrailAuthorizationItem, GuardrailConfirmation, GuardrailController,
        GuardrailControllerError, GuardrailPoll, GuardrailResolution, GuardrailReviewRequest,
        GuardrailReviewSubmission, GuardrailStoreHealth,
    },
    guardrail_policy_worker::{GuardrailPolicyRequest, GuardrailPolicyWorker},
    guardrail_preflight::PreflightEnvironment,
    integrity_executor::{
        IntegrityCancelError, IntegrityExecutor, IntegrityExecutorSpawnError, IntegrityOutcome,
        IntegrityRequest, IntegritySubmission, IntegritySubmitError,
    },
    job_manager::{ApplicationJobManager, SharedJobManager},
    move_executor::{
        MoveCancelError, MoveExecutor, MoveExecutorSpawnError, MoveSubmission, MoveSubmitError,
    },
    operation_control::{BatchId, BatchSnapshot, BatchStatus, duplicate_name, keep_both_name},
    permanent_delete_executor::{
        PermanentDeleteCancelError, PermanentDeleteExecutor, PermanentDeleteExecutorSpawnError,
        PermanentDeleteSubmission, PermanentDeleteSubmitError,
    },
    permission_executor::{
        PermissionExecutor, PermissionExecutorSpawnError, PermissionSubmission,
        PermissionSubmitError,
    },
    restore_executor::{
        RestoreCancelError, RestoreExecutor, RestoreExecutorSpawnError, RestoreSubmission,
        RestoreSubmitError,
    },
    trash_executor::{
        TrashCancelError, TrashExecutor, TrashExecutorSpawnError, TrashRequest, TrashRequestError,
        TrashSubmission, TrashSubmitError,
    },
    verified_copy_executor::{
        VerifiedCopyCancelError, VerifiedCopyExecutor, VerifiedCopyExecutorSpawnError,
        VerifiedCopyResult, VerifiedCopySubmission, VerifiedCopySubmitError,
    },
};

/// Identifies the application workflow that exclusively owns a terminal
/// verified-copy result. The executor result is still removed exactly once by
/// [`OperationController`](crate::operations::OperationController); claimed
/// removable-media results are then delivered to the registered workflow
/// observer instead of the ordinary Copy and Verify result dialog.
#[derive(Debug)]
pub enum VerifiedCopyCompletion {
    Ordinary(VerifiedCopyResult),
    VerifiedUsb(VerifiedCopyResult),
}

type VerifiedUsbCompletionHandler = Box<dyn Fn(JobId, VerifiedCopyResult)>;

#[derive(Default)]
struct VerifiedUsbCompletionObserver {
    handler: RefCell<Option<VerifiedUsbCompletionHandler>>,
}

impl std::fmt::Debug for VerifiedUsbCompletionObserver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VerifiedUsbCompletionObserver")
            .field("registered", &self.handler.borrow().is_some())
            .finish()
    }
}

#[cfg(test)]
use crate::trash_executor::{TrashBackend, TrashError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferIntent {
    Copy,
    Move,
}

/// Application-owned transfer buffer retaining the original Linux path.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TransferBuffer {
    staged: Option<(TransferIntent, Vec<PathBuf>)>,
}

impl TransferBuffer {
    pub fn intent(&self) -> Option<TransferIntent> {
        self.staged.as_ref().map(|(intent, _)| *intent)
    }

    pub fn sources(&self) -> Option<&[PathBuf]> {
        self.staged.as_ref().map(|(_, sources)| sources.as_slice())
    }

    fn stage_many(
        &mut self,
        intent: TransferIntent,
        sources: Vec<PathBuf>,
    ) -> Result<(), CopyInteractionError> {
        if sources.is_empty() {
            return Err(CopyInteractionError::EmptySelection);
        }
        let mut unique = Vec::with_capacity(sources.len());
        let mut seen = HashSet::with_capacity(sources.len());
        for source in sources {
            if source.file_name().is_none() {
                return Err(CopyInteractionError::InvalidSource(source));
            }
            if seen.insert(source.clone()) {
                unique.push(source);
            }
        }
        self.staged = Some((intent, unique));
        Ok(())
    }

    fn clear_completed_move(&mut self, source: &Path) {
        if let Some((TransferIntent::Move, sources)) = self.staged.as_mut() {
            sources.retain(|staged_source| staged_source != source);
            if sources.is_empty() {
                self.staged = None;
            }
        }
    }

    fn clear_move(&mut self) {
        if matches!(self.intent(), Some(TransferIntent::Move)) {
            self.staged = None;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchSubmission {
    id: BatchId,
    queued: usize,
}

impl BatchSubmission {
    pub const fn id(self) -> BatchId {
        self.id
    }

    pub const fn queued(self) -> usize {
        self.queued
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TrackedOperation {
    Copy(CopyRequest),
    Move(MoveRequest),
    Rename(RenameRequest),
    Trash(TrashRequest),
    PermanentDelete(PermanentDeleteRequest),
    Restore(RestoreRequest),
    Create(CreateRequest),
    UndoMove {
        request: MoveRequest,
        original_job_id: JobId,
    },
}

pub const MAX_TERMINAL_HISTORY: usize = 64;
const MAX_BATCH_HISTORY: usize = 64;

#[derive(Debug)]
struct PendingBatchItem {
    batch_id: BatchId,
    operation: TrackedOperation,
    authorization: Option<GuardrailAuthorizationItem>,
}

#[derive(Clone, Debug)]
struct BatchRecord {
    id: BatchId,
    total: usize,
    completed: usize,
    skipped: usize,
    failed: usize,
    cancelled: usize,
    active_job: Option<JobId>,
    blocked_conflict: Option<JobId>,
    paused: bool,
    cancelling: bool,
    skip_conflicts: bool,
}

impl BatchRecord {
    fn new(id: BatchId, total: usize) -> Self {
        Self {
            id,
            total,
            completed: 0,
            skipped: 0,
            failed: 0,
            cancelled: 0,
            active_job: None,
            blocked_conflict: None,
            paused: false,
            cancelling: false,
            skip_conflicts: false,
        }
    }

    fn processed(&self) -> usize {
        self.completed
            .saturating_add(self.skipped)
            .saturating_add(self.failed)
            .saturating_add(self.cancelled)
    }

    fn status(&self) -> BatchStatus {
        if self.active_job.is_none()
            && self.blocked_conflict.is_none()
            && self.processed() >= self.total
        {
            if self.cancelled == self.total {
                BatchStatus::Cancelled
            } else if self.failed > 0 || self.skipped > 0 || self.cancelled > 0 {
                BatchStatus::CompletedWithIssues
            } else {
                BatchStatus::Completed
            }
        } else if self.cancelling {
            BatchStatus::Cancelling
        } else if self.paused && self.active_job.is_some() {
            BatchStatus::Pausing
        } else if self.paused || self.blocked_conflict.is_some() {
            BatchStatus::Paused
        } else if self.active_job.is_some() {
            BatchStatus::Running
        } else {
            BatchStatus::Queued
        }
    }

    fn snapshot(&self) -> BatchSnapshot {
        BatchSnapshot::new(
            self.id,
            self.status(),
            self.total,
            self.completed,
            self.skipped,
            self.failed,
            self.cancelled,
            self.active_job.is_some(),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalOutcome {
    Completed,
    Cancelled,
    Conflict,
    PartialFailure,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConflictDecision {
    KeepExisting,
    KeepBoth,
    SkipAll,
    RetryWithName(OsString),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UndoMove {
    original_job_id: JobId,
    request: MoveRequest,
}

impl UndoMove {
    pub const fn original_job_id(&self) -> JobId {
        self.original_job_id
    }

    pub fn request(&self) -> &MoveRequest {
        &self.request
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingConflict {
    job_id: JobId,
    operation_id: OperationId,
    source: PathBuf,
    destination: PathBuf,
}

impl PendingConflict {
    pub const fn job_id(&self) -> JobId {
        self.job_id
    }

    pub const fn operation_id(&self) -> OperationId {
        self.operation_id
    }

    pub fn source(&self) -> &Path {
        &self.source
    }

    pub fn destination(&self) -> &Path {
        &self.destination
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalOperation {
    job_id: JobId,
    operation_id: OperationId,
    outcome: TerminalOutcome,
    operation: TrackedOperation,
    batch_id: Option<BatchId>,
    undo: Option<UndoMove>,
}

impl TerminalOperation {
    pub const fn job_id(&self) -> JobId {
        self.job_id
    }

    pub const fn operation_id(&self) -> OperationId {
        self.operation_id
    }

    pub const fn outcome(&self) -> TerminalOutcome {
        self.outcome
    }

    pub fn operation(&self) -> &TrackedOperation {
        &self.operation
    }

    pub const fn batch_id(&self) -> Option<BatchId> {
        self.batch_id
    }

    pub fn undo(&self) -> Option<&UndoMove> {
        self.undo.as_ref()
    }
}

impl TrackedOperation {
    pub fn source(&self) -> &Path {
        match self {
            Self::Copy(request) => request.source(),
            Self::Move(request) => request.source(),
            Self::Rename(request) => request.source(),
            Self::Trash(request) => request.source(),
            Self::PermanentDelete(request) => request.targets()[0].as_path(),
            Self::Restore(request) => request.backing_path(),
            Self::Create(request) => request.source().unwrap_or_else(|| request.destination()),
            Self::UndoMove { request, .. } => request.source(),
        }
    }

    pub fn affected_directories(&self) -> Vec<PathBuf> {
        let mut directories = Vec::with_capacity(2);
        let mut add_parent = |path: &Path| {
            if let Some(parent) = path.parent() {
                let parent = parent.to_path_buf();
                if !directories.contains(&parent) {
                    directories.push(parent);
                }
            }
        };
        match self {
            Self::Copy(request) => add_parent(request.destination()),
            Self::Move(request) => {
                add_parent(request.source());
                add_parent(request.destination());
            }
            Self::Rename(request) => add_parent(request.source()),
            Self::Trash(request) => add_parent(request.source()),
            Self::PermanentDelete(request) => {
                for target in request.targets() {
                    add_parent(target);
                }
            }
            Self::Restore(request) => {
                add_parent(request.backing_path());
                add_parent(request.destination());
            }
            Self::Create(request) => {
                add_parent(request.destination());
            }
            Self::UndoMove { request, .. } => {
                add_parent(request.source());
                add_parent(request.destination());
            }
        }
        directories
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferSubmission {
    Copy(CopySubmission),
    Move(MoveSubmission),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetrySubmission {
    Copy(CopySubmission),
    Create(CreateSubmission),
    Move(MoveSubmission),
    Trash(TrashSubmission),
    PermanentDelete(PermanentDeleteSubmission),
    Restore(RestoreSubmission),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictResolution {
    KeptExisting,
    Retried(RetrySubmission),
}

/// A destructive request paired with the exact single-use authorization
/// produced by the application-owned guardrail controller.
///
/// The wrapper is deliberately not `Clone`: retry, undo, conflict, and queued
/// dispatch must each obtain a fresh permit for their exact scope.
#[derive(Debug)]
pub struct GuardrailAuthorized<T> {
    value: T,
    authorization: GuardrailAuthorizationItem,
}

#[derive(Debug)]
pub struct GuardrailAuthorizedBatchRename {
    request: BatchRenameRequest,
    authorizations: Vec<GuardrailAuthorizationItem>,
}

impl GuardrailAuthorizedBatchRename {
    pub(crate) fn new(
        request: BatchRenameRequest,
        authorizations: Vec<GuardrailAuthorizationItem>,
    ) -> Self {
        Self {
            request,
            authorizations,
        }
    }
}

impl<T> GuardrailAuthorized<T> {
    pub(crate) fn new(value: T, authorization: GuardrailAuthorizationItem) -> Self {
        Self {
            value,
            authorization,
        }
    }

    fn into_parts(self) -> (T, GuardrailAuthorizationItem) {
        (self.value, self.authorization)
    }
}

impl RetrySubmission {
    pub const fn operation_id(self) -> OperationId {
        match self {
            Self::Copy(submission) => submission.operation_id(),
            Self::Create(submission) => submission.operation_id(),
            Self::Move(submission) => submission.operation_id(),
            Self::Trash(submission) => submission.operation_id(),
            Self::PermanentDelete(submission) => submission.operation_id(),
            Self::Restore(submission) => submission.operation_id(),
        }
    }

    pub const fn job_id(self) -> JobId {
        match self {
            Self::Copy(submission) => submission.job_id(),
            Self::Create(submission) => submission.job_id(),
            Self::Move(submission) => submission.job_id(),
            Self::Trash(submission) => submission.job_id(),
            Self::PermanentDelete(submission) => submission.job_id(),
            Self::Restore(submission) => submission.job_id(),
        }
    }
}

impl TransferSubmission {
    pub const fn operation_id(self) -> OperationId {
        match self {
            Self::Copy(submission) => submission.operation_id(),
            Self::Move(submission) => submission.operation_id(),
        }
    }

    pub const fn job_id(self) -> JobId {
        match self {
            Self::Copy(submission) => submission.job_id(),
            Self::Move(submission) => submission.job_id(),
        }
    }
}

#[derive(Debug, Error)]
pub enum CopyInteractionError {
    #[error("select an item to copy first")]
    EmptyBuffer,
    #[error("select at least one item")]
    EmptySelection,
    #[error("this path cannot be copied: {}", .0.display())]
    InvalidSource(PathBuf),
    #[error("open a destination outside the copied folder, then paste again")]
    DestinationInsideSource,
    #[error("enter one filename without slashes")]
    InvalidRenameName,
    #[error("fresh guardrail authorization is required before this destructive operation")]
    AuthorizationRequired(DestructiveScope),
    #[error(transparent)]
    DestructiveScope(#[from] DestructiveScopeError),
    #[error(transparent)]
    GuardrailPermit(#[from] GuardrailPermitError),
    #[error(transparent)]
    BatchRename(#[from] BatchRenameExecutorError),
    #[error(transparent)]
    DropPolicy(#[from] DropPolicyError),
    #[error(transparent)]
    CopySubmit(#[from] CopySubmitError),
    #[error(transparent)]
    CreateRequest(#[from] CreateRequestError),
    #[error(transparent)]
    CreateSubmit(#[from] CreateSubmitError),
    #[error(transparent)]
    MoveSubmit(#[from] MoveSubmitError),
    #[error(transparent)]
    CopyCancel(#[from] CopyCancelError),
    #[error(transparent)]
    CreateCancel(#[from] CreateCancelError),
    #[error(transparent)]
    MoveCancel(#[from] MoveCancelError),
    #[error(transparent)]
    TrashRequest(#[from] TrashRequestError),
    #[error(transparent)]
    TrashSubmit(#[from] TrashSubmitError),
    #[error(transparent)]
    TrashCancel(#[from] TrashCancelError),
    #[error(transparent)]
    PermanentDeleteRequest(#[from] PermanentDeleteRequestError),
    #[error(transparent)]
    PermanentDeleteSubmit(#[from] PermanentDeleteSubmitError),
    #[error(transparent)]
    PermanentDeleteCancel(#[from] PermanentDeleteCancelError),
    #[error(transparent)]
    RestoreRequest(#[from] RestoreRequestError),
    #[error(transparent)]
    RestoreSubmit(#[from] RestoreSubmitError),
    #[error(transparent)]
    RestoreCancel(#[from] RestoreCancelError),
    #[error("permission cancellation failed: {0}")]
    PermissionCancel(String),
    #[error(transparent)]
    ChecksumCancel(#[from] ChecksumCancelError),
    #[error(transparent)]
    IntegrityCancel(#[from] IntegrityCancelError),
    #[error(transparent)]
    VerifiedCopyCancel(#[from] VerifiedCopyCancelError),
    #[error(transparent)]
    ArchiveCancel(#[from] ArchiveCancelError),
    #[error("terminal operation history does not contain job {0:?}")]
    RetryNotFound(JobId),
    #[error("completed job {0:?} cannot be retried")]
    RetryCompleted(JobId),
    #[error("partially completed destructive job {0:?} cannot be retried")]
    RetryUnsafePartial(JobId),
    #[error("job {0:?} needs an explicit conflict decision")]
    ConflictDecisionRequired(JobId),
    #[error("job {0:?} does not have a pending destination conflict")]
    ConflictNotFound(JobId),
    #[error("the conflict for job {0:?} has already been resolved")]
    ConflictAlreadyResolved(JobId),
    #[error("job {0:?} does not support destination conflict resolution")]
    ConflictUnsupported(JobId),
    #[error("batch identifier space is exhausted")]
    BatchIdentifierExhausted,
    #[error("the bounded batch queue already contains {MAX_BATCH_HISTORY} batches")]
    BatchQueueFull,
    #[error("batch {0:?} is not available")]
    BatchNotFound(BatchId),
    #[error("batch {0:?} is already complete")]
    BatchCompleted(BatchId),
    #[error("job {0:?} has no safe undo action")]
    UndoNotAvailable(JobId),
    #[error("undo for job {0:?} was already submitted")]
    UndoAlreadySubmitted(JobId),
}

#[derive(Debug, Error)]
pub enum ApplicationStateSpawnError {
    #[error(transparent)]
    Guardrail(#[from] GuardrailControllerError),
    #[error("could not start Protected Folder policy worker")]
    GuardrailPolicy(#[source] std::io::Error),
    #[error(transparent)]
    Archive(#[from] ArchiveExecutorSpawnError),
    #[error(transparent)]
    BatchRename(#[from] BatchRenameExecutorError),
    #[error(transparent)]
    Checksum(#[from] ChecksumExecutorSpawnError),
    #[error(transparent)]
    Integrity(#[from] IntegrityExecutorSpawnError),
    #[error(transparent)]
    VerifiedCopy(#[from] VerifiedCopyExecutorSpawnError),
    #[error(transparent)]
    Copy(#[from] CopyExecutorSpawnError),
    #[error(transparent)]
    Create(#[from] CreateExecutorSpawnError),
    #[error(transparent)]
    Move(#[from] MoveExecutorSpawnError),
    #[error(transparent)]
    Trash(#[from] TrashExecutorSpawnError),
    #[error(transparent)]
    PermanentDelete(#[from] PermanentDeleteExecutorSpawnError),
    #[error(transparent)]
    Permission(#[from] PermissionExecutorSpawnError),
    #[error(transparent)]
    Restore(#[from] RestoreExecutorSpawnError),
}

/// Application-wide services and state that outlive any one browser concern.
#[derive(Debug)]
pub struct ApplicationState {
    pub jobs: SharedJobManager,
    archive_executor: ArchiveExecutor,
    batch_rename_executor: BatchRenameExecutor,
    copy_executor: CopyExecutor,
    create_executor: CreateExecutor,
    move_executor: MoveExecutor,
    trash_executor: TrashExecutor,
    permanent_delete_executor: PermanentDeleteExecutor,
    permission_executor: PermissionExecutor,
    checksum_executor: ChecksumExecutor,
    integrity_executor: IntegrityExecutor,
    verified_copy_executor: VerifiedCopyExecutor,
    restore_executor: RestoreExecutor,
    guardrails: RefCell<GuardrailController>,
    guardrail_policy_worker: RefCell<GuardrailPolicyWorker>,
    guardrail_policy_pending: Cell<Option<u64>>,
    transfer_buffer: RefCell<TransferBuffer>,
    operation_requests: RefCell<HashMap<JobId, TrackedOperation>>,
    permission_requests: RefCell<HashMap<JobId, PermissionRequest>>,
    checksum_requests: RefCell<HashMap<JobId, ChecksumRequest>>,
    integrity_requests: RefCell<HashMap<JobId, IntegrityRequest>>,
    verified_copy_requests: RefCell<HashMap<JobId, VerifiedCopyRequest>>,
    verified_usb_copy_jobs: RefCell<HashSet<JobId>>,
    verified_usb_completion_observer: VerifiedUsbCompletionObserver,
    archive_requests: RefCell<HashMap<JobId, ArchiveRequest>>,
    batch_rename_requests: RefCell<HashMap<JobId, BatchRenameRequest>>,
    batch_rename_undo: RefCell<Option<BatchRenameRequest>>,
    terminal_history: RefCell<VecDeque<TerminalOperation>>,
    resolved_conflicts: RefCell<HashSet<JobId>>,
    resolved_undos: RefCell<HashSet<JobId>>,
    batch_pending: RefCell<VecDeque<PendingBatchItem>>,
    batch_active: Cell<Option<JobId>>,
    batches: RefCell<VecDeque<BatchRecord>>,
    job_batches: RefCell<HashMap<JobId, BatchId>>,
    next_batch_id: Cell<u64>,
}

impl ApplicationState {
    pub fn new() -> Result<Self, ApplicationStateSpawnError> {
        let jobs = Arc::new(Mutex::new(ApplicationJobManager::new()));
        let archive_executor = ArchiveExecutor::spawn(Arc::clone(&jobs))?;
        let batch_rename_executor = BatchRenameExecutor::spawn(Arc::clone(&jobs))?;
        let copy_executor = CopyExecutor::spawn(Arc::clone(&jobs))?;
        let create_executor = CreateExecutor::spawn(Arc::clone(&jobs))?;
        let move_executor = MoveExecutor::spawn(Arc::clone(&jobs))?;
        let trash_executor = TrashExecutor::spawn(Arc::clone(&jobs))?;
        let permanent_delete_executor = PermanentDeleteExecutor::spawn(Arc::clone(&jobs))?;
        let permission_executor = PermissionExecutor::spawn(Arc::clone(&jobs))?;
        let checksum_executor = ChecksumExecutor::spawn(Arc::clone(&jobs))?;
        let integrity_executor = IntegrityExecutor::spawn(Arc::clone(&jobs))?;
        let verified_copy_executor = VerifiedCopyExecutor::spawn(Arc::clone(&jobs))?;
        let restore_executor = RestoreExecutor::spawn(Arc::clone(&jobs))?;
        let guardrail_store_path = default_guardrail_store_path();
        let guardrails = GuardrailController::load_at(guardrail_store_path.clone())?;
        let guardrail_policy_worker = GuardrailPolicyWorker::spawn(guardrail_store_path)
            .map_err(ApplicationStateSpawnError::GuardrailPolicy)?;
        Ok(Self {
            jobs,
            archive_executor,
            batch_rename_executor,
            copy_executor,
            create_executor,
            move_executor,
            trash_executor,
            permanent_delete_executor,
            permission_executor,
            checksum_executor,
            integrity_executor,
            verified_copy_executor,
            restore_executor,
            guardrails: RefCell::new(guardrails),
            guardrail_policy_worker: RefCell::new(guardrail_policy_worker),
            guardrail_policy_pending: Cell::new(None),
            transfer_buffer: RefCell::new(TransferBuffer::default()),
            operation_requests: RefCell::new(HashMap::new()),
            permission_requests: RefCell::new(HashMap::new()),
            checksum_requests: RefCell::new(HashMap::new()),
            integrity_requests: RefCell::new(HashMap::new()),
            verified_copy_requests: RefCell::new(HashMap::new()),
            verified_usb_copy_jobs: RefCell::new(HashSet::new()),
            verified_usb_completion_observer: VerifiedUsbCompletionObserver::default(),
            archive_requests: RefCell::new(HashMap::new()),
            batch_rename_requests: RefCell::new(HashMap::new()),
            batch_rename_undo: RefCell::new(None),
            terminal_history: RefCell::new(VecDeque::new()),
            resolved_conflicts: RefCell::new(HashSet::new()),
            resolved_undos: RefCell::new(HashSet::new()),
            batch_pending: RefCell::new(VecDeque::new()),
            batch_active: Cell::new(None),
            batches: RefCell::new(VecDeque::new()),
            job_batches: RefCell::new(HashMap::new()),
            next_batch_id: Cell::new(1),
        })
    }

    pub fn guardrail_store_health(&self) -> GuardrailStoreHealth {
        self.guardrails.borrow().store_health()
    }

    pub fn guardrail_store_error_text(&self) -> Option<String> {
        self.guardrails
            .borrow()
            .store_error()
            .map(ToString::to_string)
    }

    pub fn guardrail_policy(&self) -> floe_core::ProtectedRoots {
        self.guardrails.borrow().policy().clone()
    }

    pub fn guardrail_policy_busy(&self) -> bool {
        self.guardrail_policy_pending.get().is_some()
    }

    pub fn submit_guardrail_protection_change(
        &self,
        path: PathBuf,
        protect: bool,
    ) -> Result<bool, String> {
        if self.guardrail_store_health() == GuardrailStoreHealth::Blocked {
            return Err("Protected Folder policy is blocked until it is explicitly reset".into());
        }
        if self.guardrail_policy_busy() {
            return Err("another Protected Folder policy save is still running".into());
        }
        let mut policy = self.guardrail_policy();
        let changed = if protect {
            policy.add(path)
        } else {
            policy.remove(&path)
        }
        .map_err(|error| error.to_string())?;
        if !changed {
            return Ok(false);
        }
        let request_id = self
            .guardrail_policy_worker
            .borrow_mut()
            .submit(GuardrailPolicyRequest::Persist(policy))
            .map_err(|error| error.to_string())?;
        self.guardrail_policy_pending.set(Some(request_id));
        Ok(true)
    }

    pub fn submit_guardrail_blocked_reset(&self, acknowledged: bool) -> Result<(), String> {
        if self.guardrail_store_health() != GuardrailStoreHealth::Blocked || !acknowledged {
            return Err("acknowledge the blocked policy store before resetting it".into());
        }
        if self.guardrail_policy_busy() {
            return Err("another Protected Folder policy save is still running".into());
        }
        let generation = self
            .guardrail_policy()
            .generation()
            .checked_add(1)
            .ok_or_else(|| "Protected Folder policy generation is exhausted".to_owned())?;
        let policy = floe_core::ProtectedRoots::with_generation(generation, Vec::new())
            .map_err(|error| error.to_string())?;
        let request_id = self
            .guardrail_policy_worker
            .borrow_mut()
            .submit(GuardrailPolicyRequest::ResetBlocked(policy))
            .map_err(|error| error.to_string())?;
        self.guardrail_policy_pending.set(Some(request_id));
        Ok(())
    }

    pub fn poll_guardrail_policy_update(&self) -> Option<Result<bool, String>> {
        let response = self.guardrail_policy_worker.borrow().try_response()?;
        if self.guardrail_policy_pending.get() != Some(response.request_id()) {
            return Some(Err("stale Protected Folder policy response".to_owned()));
        }
        self.guardrail_policy_pending.set(None);
        if let Err(error) = response.result() {
            return Some(Err(error.clone()));
        }
        let reset = matches!(response.request(), GuardrailPolicyRequest::ResetBlocked(_));
        match self
            .guardrails
            .borrow_mut()
            .install_persisted_policy(response.request().policy().clone())
        {
            Ok(()) => Some(Ok(reset)),
            Err(error) => Some(Err(error.to_string())),
        }
    }

    pub fn begin_guardrail_review(
        &self,
        scopes: Vec<DestructiveScope>,
        environment: PreflightEnvironment,
    ) -> Result<GuardrailReviewSubmission, GuardrailControllerError> {
        let request = GuardrailReviewRequest::new(scopes, environment)?;
        self.guardrails.borrow_mut().begin_review(request)
    }

    pub fn poll_guardrail_review(
        &self,
        generation: u64,
    ) -> Result<GuardrailPoll, GuardrailControllerError> {
        self.guardrails.borrow_mut().poll(generation)
    }

    pub fn resolve_guardrail_review(
        &self,
        generation: u64,
        confirmation: GuardrailConfirmation,
    ) -> Result<GuardrailResolution, GuardrailControllerError> {
        self.guardrails
            .borrow_mut()
            .resolve_review(generation, confirmation)
    }

    pub fn cancel_guardrail_review(&self, generation: u64) -> Result<(), GuardrailControllerError> {
        self.guardrails.borrow_mut().cancel(generation)
    }

    pub fn stage_copy(&self, source: PathBuf) -> Result<(), CopyInteractionError> {
        self.stage_copy_many(vec![source])
    }

    pub fn stage_copy_many(&self, sources: Vec<PathBuf>) -> Result<(), CopyInteractionError> {
        self.transfer_buffer
            .borrow_mut()
            .stage_many(TransferIntent::Copy, sources)
    }

    pub fn stage_move(&self, source: PathBuf) -> Result<(), CopyInteractionError> {
        self.stage_move_many(vec![source])
    }

    pub fn stage_move_many(&self, sources: Vec<PathBuf>) -> Result<(), CopyInteractionError> {
        self.transfer_buffer
            .borrow_mut()
            .stage_many(TransferIntent::Move, sources)
    }

    pub fn staged_transfer(&self) -> Option<(TransferIntent, PathBuf)> {
        let buffer = self.transfer_buffer.borrow();
        Some((buffer.intent()?, buffer.sources()?.first()?.clone()))
    }

    pub fn staged_transfers(&self) -> Option<(TransferIntent, Vec<PathBuf>)> {
        let buffer = self.transfer_buffer.borrow();
        Some((buffer.intent()?, buffer.sources()?.to_vec()))
    }

    pub fn submit_paste_batch(
        &self,
        destination_directory: &Path,
    ) -> Result<BatchSubmission, CopyInteractionError> {
        let (intent, sources) = self
            .staged_transfers()
            .ok_or(CopyInteractionError::EmptyBuffer)?;
        let mut operations = Vec::with_capacity(sources.len());
        for source in &sources {
            let destination = transfer_destination(source, destination_directory)?;
            let operation = match intent {
                TransferIntent::Copy => TrackedOperation::Copy(CopyRequest::new(
                    source,
                    destination,
                    ConflictPolicy::FailIfExists,
                    SymlinkPolicy::Preserve,
                )),
                TransferIntent::Move => TrackedOperation::Move(MoveRequest::new(
                    source,
                    destination,
                    ConflictPolicy::FailIfExists,
                )),
            };
            operations.push(operation);
        }
        let batch = self.enqueue_batch(operations)?;
        if intent == TransferIntent::Move {
            self.transfer_buffer.borrow_mut().clear_move();
        }
        Ok(batch)
    }

    pub fn submit_transfer_batch(
        &self,
        intent: TransferIntent,
        sources: Vec<PathBuf>,
        destination_directory: &Path,
    ) -> Result<BatchSubmission, CopyInteractionError> {
        if sources.is_empty() {
            return Err(CopyInteractionError::EmptySelection);
        }
        let mut unique = HashSet::with_capacity(sources.len());
        let mut operations = Vec::with_capacity(sources.len());
        for source in sources {
            if !unique.insert(source.clone()) {
                continue;
            }
            let destination = transfer_destination(&source, destination_directory)?;
            let operation = match intent {
                TransferIntent::Copy => TrackedOperation::Copy(CopyRequest::new(
                    source,
                    destination,
                    ConflictPolicy::FailIfExists,
                    SymlinkPolicy::Preserve,
                )),
                TransferIntent::Move => TrackedOperation::Move(MoveRequest::new(
                    source,
                    destination,
                    ConflictPolicy::FailIfExists,
                )),
            };
            operations.push(operation);
        }
        self.enqueue_batch(operations)
    }

    pub fn submit_trash_batch(
        &self,
        sources: Vec<PathBuf>,
    ) -> Result<BatchSubmission, CopyInteractionError> {
        if sources.is_empty() {
            return Err(CopyInteractionError::EmptySelection);
        }
        let mut unique = HashSet::with_capacity(sources.len());
        let mut operations = Vec::with_capacity(sources.len());
        for source in sources {
            if unique.insert(source.clone()) {
                operations.push(TrackedOperation::Trash(TrashRequest::new(source)?));
            }
        }
        self.enqueue_batch(operations)
    }

    pub fn submit_drop(
        &self,
        request: DropRequest,
    ) -> Result<BatchSubmission, CopyInteractionError> {
        if matches!(request.destination(), DropDestination::Trash) {
            return self.submit_trash_batch(request.sources().to_vec());
        }

        let action = request.action();
        let operations = plan_directory_drop(&request)?
            .into_iter()
            .map(|item| match action {
                DropAction::Copy => Ok(TrackedOperation::Copy(CopyRequest::new(
                    item.source,
                    item.destination,
                    ConflictPolicy::FailIfExists,
                    SymlinkPolicy::Preserve,
                ))),
                DropAction::Move => Ok(TrackedOperation::Move(MoveRequest::new(
                    item.source,
                    item.destination,
                    ConflictPolicy::FailIfExists,
                ))),
                DropAction::Link => CreateRequest::symbolic_link(item.source, item.destination)
                    .map(TrackedOperation::Create)
                    .map_err(CopyInteractionError::from),
                DropAction::Trash => unreachable!("Trash destination returned above"),
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.enqueue_batch(operations)
    }

    pub fn submit_restore_batch(
        &self,
        requests: Vec<RestoreRequest>,
    ) -> Result<BatchSubmission, CopyInteractionError> {
        if requests.is_empty() {
            return Err(CopyInteractionError::EmptySelection);
        }
        let mut unique = HashSet::with_capacity(requests.len());
        let operations = requests
            .into_iter()
            .filter(|request| unique.insert(request.backing_path().to_path_buf()))
            .map(TrackedOperation::Restore)
            .collect::<Vec<_>>();
        self.enqueue_batch(operations)
    }

    pub fn submit_duplicate_batch(
        &self,
        sources: Vec<PathBuf>,
    ) -> Result<BatchSubmission, CopyInteractionError> {
        if sources.is_empty() {
            return Err(CopyInteractionError::EmptySelection);
        }

        let mut unique = HashSet::with_capacity(sources.len());
        let mut operations = Vec::with_capacity(sources.len());
        for source in sources {
            if !unique.insert(source.clone()) {
                continue;
            }
            let original_name = source
                .file_name()
                .ok_or_else(|| CopyInteractionError::InvalidSource(source.clone()))?;
            let duplicate = duplicate_name(original_name, 1)
                .ok_or_else(|| CopyInteractionError::InvalidSource(source.clone()))?;
            let parent = source
                .parent()
                .ok_or_else(|| CopyInteractionError::InvalidSource(source.clone()))?;
            let request = CreateRequest::duplicate(&source, parent.join(duplicate))?;
            operations.push(TrackedOperation::Create(request));
        }

        self.enqueue_batch(operations)
    }

    pub fn submit_paste(
        &self,
        destination_directory: &Path,
    ) -> Result<TransferSubmission, CopyInteractionError> {
        let (intent, source) = self
            .staged_transfer()
            .ok_or(CopyInteractionError::EmptyBuffer)?;
        let destination = transfer_destination(&source, destination_directory)?;
        match intent {
            TransferIntent::Copy => {
                let request = CopyRequest::new(
                    source,
                    destination,
                    ConflictPolicy::FailIfExists,
                    SymlinkPolicy::Preserve,
                );
                match self.copy_executor.submit_copy(request.clone()) {
                    Ok(submission) => {
                        self.track(submission.job_id(), TrackedOperation::Copy(request));
                        Ok(TransferSubmission::Copy(submission))
                    }
                    Err(error) => {
                        if let Some(job_id) = error.job_id() {
                            self.track(job_id, TrackedOperation::Copy(request));
                        }
                        Err(error.into())
                    }
                }
            }
            TransferIntent::Move => {
                let request = MoveRequest::new(source, destination, ConflictPolicy::FailIfExists);
                let scope = destructive_scope_for_move(&request)?;
                Err(CopyInteractionError::AuthorizationRequired(scope))
            }
        }
    }

    pub fn submit_move_authorized(
        &self,
        authorized: GuardrailAuthorized<MoveRequest>,
    ) -> Result<TransferSubmission, CopyInteractionError> {
        let scope = destructive_scope_for_move(&authorized.value)?;
        self.consume_then_dispatch(scope, authorized, |request| {
            match self.move_executor.submit_move(request.clone()) {
                Ok(submission) => {
                    self.track(submission.job_id(), TrackedOperation::Move(request));
                    Ok(TransferSubmission::Move(submission))
                }
                Err(error) => {
                    if let Some(job_id) = error.job_id() {
                        self.track(job_id, TrackedOperation::Move(request));
                    }
                    Err(error.into())
                }
            }
        })
    }

    pub fn submit_rename(
        &self,
        source: PathBuf,
        new_name: OsString,
    ) -> Result<MoveSubmission, CopyInteractionError> {
        validate_rename_name(&new_name)?;
        let request = RenameRequest::new(source, new_name, ConflictPolicy::FailIfExists);
        let scope = destructive_scope_for_rename(&request)?;
        Err(CopyInteractionError::AuthorizationRequired(scope))
    }

    pub fn submit_rename_authorized(
        &self,
        authorized: GuardrailAuthorized<RenameRequest>,
    ) -> Result<MoveSubmission, CopyInteractionError> {
        let scope = destructive_scope_for_rename(&authorized.value)?;
        self.consume_then_dispatch(scope, authorized, |request| {
            match self.move_executor.submit_rename(request.clone()) {
                Ok(submission) => {
                    self.track(submission.job_id(), TrackedOperation::Rename(request));
                    Ok(submission)
                }
                Err(error) => {
                    if let Some(job_id) = error.job_id() {
                        self.track(job_id, TrackedOperation::Rename(request));
                    }
                    Err(error.into())
                }
            }
        })
    }

    pub fn submit_create(
        &self,
        request: CreateRequest,
    ) -> Result<CreateSubmission, CopyInteractionError> {
        match self.create_executor.submit(request.clone()) {
            Ok(submission) => {
                self.track(submission.job_id(), TrackedOperation::Create(request));
                Ok(submission)
            }
            Err(error) => {
                if let Some(job_id) = error.job_id() {
                    self.track(job_id, TrackedOperation::Create(request));
                }
                Err(error.into())
            }
        }
    }

    pub fn operation_request(&self, job_id: JobId) -> Option<TrackedOperation> {
        self.operation_requests.borrow().get(&job_id).cloned()
    }

    pub fn permission_affected_directories(&self, job_id: JobId) -> Vec<PathBuf> {
        let requests = self.permission_requests.borrow();
        let Some(request) = requests.get(&job_id) else {
            return Vec::new();
        };
        let mut directories = Vec::new();
        for target in request.targets() {
            if let Some(parent) = target.parent() {
                let parent = parent.to_path_buf();
                if !directories.contains(&parent) {
                    directories.push(parent);
                }
            }
        }
        directories
    }

    pub fn is_permission_operation(&self, job_id: JobId) -> bool {
        self.permission_requests.borrow().contains_key(&job_id)
    }

    pub fn is_checksum_operation(&self, job_id: JobId) -> bool {
        self.checksum_requests.borrow().contains_key(&job_id)
    }

    /// OperationController should use this before terminal presentation, then call
    /// [`Self::finish_integrity`] to obtain the backend-owned structured outcome.
    pub fn is_integrity_operation(&self, job_id: JobId) -> bool {
        self.integrity_requests.borrow().contains_key(&job_id)
    }

    pub fn is_verified_copy_operation(&self, job_id: JobId) -> bool {
        self.verified_copy_requests.borrow().contains_key(&job_id)
    }

    pub fn is_verified_usb_copy_operation(&self, job_id: JobId) -> bool {
        self.verified_usb_copy_jobs.borrow().contains(&job_id)
    }

    pub fn observe_verified_usb_completions(
        &self,
        handler: impl Fn(JobId, VerifiedCopyResult) + 'static,
    ) {
        self.verified_usb_completion_observer
            .handler
            .replace(Some(Box::new(handler)));
    }

    pub fn verified_copy_request(&self, job_id: JobId) -> Option<VerifiedCopyRequest> {
        self.verified_copy_requests.borrow().get(&job_id).cloned()
    }

    pub fn integrity_request(&self, job_id: JobId) -> Option<IntegrityRequest> {
        self.integrity_requests.borrow().get(&job_id).cloned()
    }

    pub fn finish_checksum(&self, job_id: JobId) -> Option<ChecksumOutcome> {
        self.checksum_requests.borrow_mut().remove(&job_id);
        self.checksum_executor.take_result(job_id)
    }

    /// Retrieves the structured result after the job is terminal. This is intentionally UI-free.
    pub fn finish_integrity(&self, job_id: JobId) -> Option<IntegrityOutcome> {
        self.integrity_requests.borrow_mut().remove(&job_id);
        self.integrity_executor.take_result(job_id)
    }

    pub fn finish_verified_copy(&self, job_id: JobId) -> Option<VerifiedCopyCompletion> {
        self.verified_copy_requests.borrow_mut().remove(&job_id);
        let result = self.verified_copy_executor.take_result(job_id)?;
        if self.verified_usb_copy_jobs.borrow_mut().remove(&job_id) {
            Some(VerifiedCopyCompletion::VerifiedUsb(result))
        } else {
            Some(VerifiedCopyCompletion::Ordinary(result))
        }
    }

    pub fn dispatch_verified_usb_completion(&self, job_id: JobId, result: VerifiedCopyResult) {
        if let Some(handler) = self
            .verified_usb_completion_observer
            .handler
            .borrow()
            .as_ref()
        {
            handler(job_id, result);
        } else {
            tracing::error!(
                ?job_id,
                "verified removable transfer completed without an application observer"
            );
        }
    }

    pub fn finish_permission(&self, job_id: JobId) {
        self.permission_requests.borrow_mut().remove(&job_id);
    }

    pub fn submit_trash(&self, source: PathBuf) -> Result<TrashSubmission, CopyInteractionError> {
        let request = TrashRequest::new(source)?;
        let scope = destructive_scope_for_trash(&request)?;
        Err(CopyInteractionError::AuthorizationRequired(scope))
    }

    pub fn submit_trash_authorized(
        &self,
        authorized: GuardrailAuthorized<TrashRequest>,
    ) -> Result<TrashSubmission, CopyInteractionError> {
        let scope = destructive_scope_for_trash(&authorized.value)?;
        self.consume_then_dispatch(scope, authorized, |request| {
            match self.trash_executor.submit_trash(request.clone()) {
                Ok(submission) => {
                    self.track(submission.job_id(), TrackedOperation::Trash(request));
                    Ok(submission)
                }
                Err(error) => {
                    if let Some(job_id) = error.job_id() {
                        self.track(job_id, TrackedOperation::Trash(request));
                    }
                    Err(error.into())
                }
            }
        })
    }

    pub fn submit_permanent_delete(
        &self,
        targets: Vec<PathBuf>,
    ) -> Result<PermanentDeleteSubmission, CopyInteractionError> {
        let request = PermanentDeleteRequest::new(targets)?;
        let scope = destructive_scope_for_permanent_delete(&request)?;
        Err(CopyInteractionError::AuthorizationRequired(scope))
    }

    pub fn submit_permanent_delete_authorized(
        &self,
        authorized: GuardrailAuthorized<PermanentDeleteRequest>,
    ) -> Result<PermanentDeleteSubmission, CopyInteractionError> {
        let scope = destructive_scope_for_permanent_delete(&authorized.value)?;
        self.consume_then_dispatch(scope, authorized, |request| {
            match self.permanent_delete_executor.submit(request.clone()) {
                Ok(submission) => {
                    self.track(
                        submission.job_id(),
                        TrackedOperation::PermanentDelete(request),
                    );
                    Ok(submission)
                }
                Err(error) => {
                    if let Some(job_id) = error.job_id() {
                        self.track(job_id, TrackedOperation::PermanentDelete(request));
                    }
                    Err(error.into())
                }
            }
        })
    }

    pub fn submit_permissions(
        &self,
        request: PermissionRequest,
    ) -> Result<PermissionSubmission, PermissionSubmitError> {
        match self.permission_executor.submit(request.clone()) {
            Ok(submission) => {
                self.permission_requests
                    .borrow_mut()
                    .insert(submission.job_id(), request);
                Ok(submission)
            }
            Err(error) => {
                if let Some(job_id) = error.job_id() {
                    self.permission_requests
                        .borrow_mut()
                        .insert(job_id, request);
                }
                Err(error)
            }
        }
    }

    pub fn submit_checksum(
        &self,
        request: ChecksumRequest,
    ) -> Result<ChecksumSubmission, ChecksumSubmitError> {
        match self.checksum_executor.submit(request.clone()) {
            Ok(submission) => {
                self.checksum_requests
                    .borrow_mut()
                    .insert(submission.job_id(), request);
                Ok(submission)
            }
            Err(error) => {
                if let Some(job_id) = error.job_id() {
                    self.checksum_requests.borrow_mut().insert(job_id, request);
                }
                Err(error)
            }
        }
    }

    pub fn submit_integrity(
        &self,
        request: IntegrityRequest,
    ) -> Result<IntegritySubmission, IntegritySubmitError> {
        match self.integrity_executor.submit(request.clone()) {
            Ok(submission) => {
                self.integrity_requests
                    .borrow_mut()
                    .insert(submission.job_id(), request);
                Ok(submission)
            }
            Err(error) => {
                if let Some(job_id) = error.job_id() {
                    self.integrity_requests.borrow_mut().insert(job_id, request);
                }
                Err(error)
            }
        }
    }

    pub fn submit_verified_copy(
        &self,
        request: VerifiedCopyRequest,
    ) -> Result<VerifiedCopySubmission, VerifiedCopySubmitError> {
        match self.verified_copy_executor.submit(request.clone()) {
            Ok(submission) => {
                self.verified_copy_requests
                    .borrow_mut()
                    .insert(submission.job_id(), request);
                Ok(submission)
            }
            Err(error) => {
                if let Some(job_id) = error.job_id() {
                    self.verified_copy_requests
                        .borrow_mut()
                        .insert(job_id, request);
                }
                Err(error)
            }
        }
    }

    pub fn submit_verified_usb_copy(
        &self,
        request: VerifiedCopyRequest,
    ) -> Result<VerifiedCopySubmission, VerifiedCopySubmitError> {
        match self.submit_verified_copy(request) {
            Ok(submission) => {
                self.verified_usb_copy_jobs
                    .borrow_mut()
                    .insert(submission.job_id());
                Ok(submission)
            }
            Err(error) => Err(error),
        }
    }

    pub fn submit_archive(
        &self,
        request: ArchiveRequest,
    ) -> Result<ArchiveSubmission, ArchiveSubmitError> {
        match self.archive_executor.submit(request.clone()) {
            Ok(submission) => {
                self.archive_requests
                    .borrow_mut()
                    .insert(submission.job_id(), request);
                Ok(submission)
            }
            Err(error) => {
                if let Some(job_id) = error.job_id() {
                    self.archive_requests.borrow_mut().insert(job_id, request);
                }
                Err(error)
            }
        }
    }

    pub fn cancel_archive(&self, job_id: JobId) -> Result<(), ArchiveCancelError> {
        self.archive_executor.cancel(job_id)
    }

    pub fn finish_archive(&self, job_id: JobId) -> Option<ArchiveOutcome> {
        self.archive_requests.borrow_mut().remove(&job_id);
        self.archive_executor.take_result(job_id)
    }

    pub fn is_archive_operation(&self, job_id: JobId) -> bool {
        self.archive_requests.borrow().contains_key(&job_id)
    }

    pub fn archive_request(&self, job_id: JobId) -> Option<ArchiveRequest> {
        self.archive_requests.borrow().get(&job_id).cloned()
    }

    pub fn archive_affected_directories(&self, job_id: JobId) -> Vec<PathBuf> {
        self.archive_requests
            .borrow()
            .get(&job_id)
            .and_then(ArchiveRequest::destination)
            .and_then(Path::parent)
            .map(|path| vec![path.to_path_buf()])
            .unwrap_or_default()
    }

    pub fn submit_batch_rename(
        &self,
        request: BatchRenameRequest,
    ) -> Result<BatchRenameSubmission, CopyInteractionError> {
        let scope = destructive_scopes_for_batch_rename(&request)?
            .into_iter()
            .next()
            .ok_or(CopyInteractionError::EmptySelection)?;
        Err(CopyInteractionError::AuthorizationRequired(scope))
    }

    pub fn submit_batch_rename_authorized(
        &self,
        authorized: GuardrailAuthorizedBatchRename,
    ) -> Result<BatchRenameSubmission, CopyInteractionError> {
        let GuardrailAuthorizedBatchRename {
            request,
            authorizations,
        } = authorized;
        let scopes = destructive_scopes_for_batch_rename(&request)?;
        if scopes.len() != authorizations.len() {
            return Err(CopyInteractionError::AuthorizationRequired(
                scopes
                    .into_iter()
                    .next()
                    .ok_or(CopyInteractionError::EmptySelection)?,
            ));
        }
        for (scope, authorization) in scopes.into_iter().zip(authorizations) {
            self.guardrails
                .borrow_mut()
                .consume_authorization(authorization, &scope)?;
        }
        let submission = self.batch_rename_executor.submit(request.clone())?;
        self.batch_rename_requests
            .borrow_mut()
            .insert(submission.job_id(), request);
        Ok(submission)
    }

    pub fn is_batch_rename_operation(&self, job_id: JobId) -> bool {
        self.batch_rename_requests.borrow().contains_key(&job_id)
    }

    pub fn batch_rename_affected_directories(&self, job_id: JobId) -> Vec<PathBuf> {
        let mut directories = Vec::new();
        if let Some(request) = self.batch_rename_requests.borrow().get(&job_id) {
            for pair in request.pairs() {
                if let Some(parent) = pair.source().parent() {
                    let parent = parent.to_path_buf();
                    if !directories.contains(&parent) {
                        directories.push(parent);
                    }
                }
            }
        }
        directories
    }

    pub fn finish_batch_rename(&self, job_id: JobId) -> Option<BatchRenameOutcome> {
        self.batch_rename_requests.borrow_mut().remove(&job_id);
        let outcome = self.batch_rename_executor.take_result(job_id);
        if let Some(outcome) = &outcome {
            self.batch_rename_undo.replace(outcome.undo_request().ok());
        }
        outcome
    }

    pub fn submit_batch_rename_undo(
        &self,
    ) -> Result<Option<BatchRenameSubmission>, CopyInteractionError> {
        let Some(request) = self.batch_rename_undo.borrow().clone() else {
            return Ok(None);
        };
        let scope = destructive_scopes_for_batch_rename(&request)?
            .into_iter()
            .next()
            .ok_or(CopyInteractionError::EmptySelection)?;
        Err(CopyInteractionError::AuthorizationRequired(scope))
    }

    pub fn batch_rename_undo_guardrail_scopes(
        &self,
    ) -> Result<Option<Vec<DestructiveScope>>, CopyInteractionError> {
        self.batch_rename_undo
            .borrow()
            .as_ref()
            .map(destructive_scopes_for_batch_rename)
            .transpose()
            .map_err(CopyInteractionError::from)
    }

    pub fn submit_batch_rename_undo_authorized(
        &self,
        authorizations: Vec<GuardrailAuthorizationItem>,
    ) -> Result<Option<BatchRenameSubmission>, CopyInteractionError> {
        let Some(request) = self.batch_rename_undo.borrow_mut().take() else {
            return Ok(None);
        };
        self.submit_batch_rename_authorized(GuardrailAuthorizedBatchRename::new(
            request,
            authorizations,
        ))
        .map(Some)
    }

    pub fn submit_restore(
        &self,
        request: RestoreRequest,
    ) -> Result<RestoreSubmission, CopyInteractionError> {
        let scope = destructive_scope_for_restore(&request)?;
        Err(CopyInteractionError::AuthorizationRequired(scope))
    }

    pub fn submit_restore_authorized(
        &self,
        authorized: GuardrailAuthorized<RestoreRequest>,
    ) -> Result<RestoreSubmission, CopyInteractionError> {
        let scope = destructive_scope_for_restore(&authorized.value)?;
        self.consume_then_dispatch(scope, authorized, |request| {
            match self.restore_executor.submit(request.clone()) {
                Ok(submission) => {
                    self.track(submission.job_id(), TrackedOperation::Restore(request));
                    Ok(submission)
                }
                Err(error) => {
                    if let Some(job_id) = error.job_id() {
                        self.track(job_id, TrackedOperation::Restore(request));
                    }
                    Err(error.into())
                }
            }
        })
    }

    fn enqueue_batch(
        &self,
        operations: Vec<TrackedOperation>,
    ) -> Result<BatchSubmission, CopyInteractionError> {
        let mut items = Vec::with_capacity(operations.len());
        for operation in operations {
            if let Some(scope) = destructive_scope_for_operation(&operation)? {
                return Err(CopyInteractionError::AuthorizationRequired(scope));
            }
            items.push((operation, None));
        }
        self.enqueue_batch_items(items)
    }

    pub fn enqueue_authorized_batch(
        &self,
        operations: Vec<GuardrailAuthorized<TrackedOperation>>,
    ) -> Result<BatchSubmission, CopyInteractionError> {
        let mut items = Vec::with_capacity(operations.len());
        for authorized in operations {
            let (operation, authorization) = authorized.into_parts();
            if destructive_scope_for_operation(&operation)?.is_none() {
                return Err(CopyInteractionError::InvalidSource(
                    operation.source().to_path_buf(),
                ));
            }
            items.push((operation, Some(authorization)));
        }
        self.enqueue_batch_items(items)
    }

    fn enqueue_batch_items(
        &self,
        operations: Vec<(TrackedOperation, Option<GuardrailAuthorizationItem>)>,
    ) -> Result<BatchSubmission, CopyInteractionError> {
        if operations.is_empty() {
            return Err(CopyInteractionError::EmptySelection);
        }
        {
            let mut batches = self.batches.borrow_mut();
            while batches.len() >= MAX_BATCH_HISTORY
                && batches
                    .front()
                    .is_some_and(|batch| batch.status().is_terminal())
            {
                batches.pop_front();
            }
            if batches.len() >= MAX_BATCH_HISTORY {
                return Err(CopyInteractionError::BatchQueueFull);
            }
        }
        let raw_id = self.next_batch_id.get();
        let id = BatchId::new(raw_id).ok_or(CopyInteractionError::BatchIdentifierExhausted)?;
        self.next_batch_id
            .set(raw_id.checked_add(1).unwrap_or_default());
        let queued = operations.len();
        self.batches
            .borrow_mut()
            .push_back(BatchRecord::new(id, queued));
        self.batch_pending
            .borrow_mut()
            .extend(
                operations
                    .into_iter()
                    .map(|(operation, authorization)| PendingBatchItem {
                        batch_id: id,
                        operation,
                        authorization,
                    }),
            );
        self.pump_batch();
        Ok(BatchSubmission { id, queued })
    }

    pub fn batch_snapshots(&self) -> Vec<BatchSnapshot> {
        self.batches
            .borrow()
            .iter()
            .map(BatchRecord::snapshot)
            .collect()
    }

    pub fn batch_snapshot(&self, id: BatchId) -> Option<BatchSnapshot> {
        self.batches
            .borrow()
            .iter()
            .find(|batch| batch.id == id)
            .map(BatchRecord::snapshot)
    }

    pub fn batch_for_job(&self, job_id: JobId) -> Option<BatchId> {
        self.job_batches.borrow().get(&job_id).copied()
    }

    pub fn pause_batch(&self, id: BatchId) -> Result<(), CopyInteractionError> {
        let mut batches = self.batches.borrow_mut();
        let batch = batches
            .iter_mut()
            .find(|batch| batch.id == id)
            .ok_or(CopyInteractionError::BatchNotFound(id))?;
        if batch.status().is_terminal() {
            return Err(CopyInteractionError::BatchCompleted(id));
        }
        batch.paused = true;
        Ok(())
    }

    pub fn resume_batch(&self, id: BatchId) -> Result<(), CopyInteractionError> {
        {
            let mut batches = self.batches.borrow_mut();
            let batch = batches
                .iter_mut()
                .find(|batch| batch.id == id)
                .ok_or(CopyInteractionError::BatchNotFound(id))?;
            if batch.status().is_terminal() {
                return Err(CopyInteractionError::BatchCompleted(id));
            }
            batch.paused = false;
        }
        self.pump_batch();
        Ok(())
    }

    pub fn cancel_batch(&self, id: BatchId) -> Result<(), CopyInteractionError> {
        let removed = {
            let mut pending = self.batch_pending.borrow_mut();
            let mut retained = VecDeque::with_capacity(pending.len());
            let mut removed = 0usize;
            while let Some(item) = pending.pop_front() {
                if item.batch_id == id {
                    removed = removed.saturating_add(1);
                    self.discard_pending_authorization(item.authorization);
                } else {
                    retained.push_back(item);
                }
            }
            *pending = retained;
            removed
        };
        let (active_job, blocked_conflict) = {
            let mut batches = self.batches.borrow_mut();
            let batch = batches
                .iter_mut()
                .find(|batch| batch.id == id)
                .ok_or(CopyInteractionError::BatchNotFound(id))?;
            if batch.status().is_terminal() {
                return Err(CopyInteractionError::BatchCompleted(id));
            }
            batch.cancelling = true;
            batch.cancelled = batch.cancelled.saturating_add(removed);
            let blocked_conflict = batch.blocked_conflict.take();
            if blocked_conflict.is_some() {
                batch.cancelled = batch.cancelled.saturating_add(1);
            }
            (batch.active_job, blocked_conflict)
        };
        if let Some(job_id) = blocked_conflict {
            self.resolved_conflicts.borrow_mut().insert(job_id);
        }
        if let Some(job_id) = active_job {
            let already_terminal = lock(&self.jobs)
                .record(job_id)
                .is_some_and(|record| record.state().is_terminal());
            if !already_terminal {
                self.cancel_operation(job_id)?;
            }
        } else {
            self.pump_batch();
        }
        Ok(())
    }

    fn pump_batch(&self) {
        if self.batch_active.get().is_some() {
            return;
        }
        loop {
            let Some(next) = self.batch_pending.borrow_mut().pop_front() else {
                return;
            };
            let dispatch = {
                let mut batches = self.batches.borrow_mut();
                let Some(batch) = batches.iter_mut().find(|batch| batch.id == next.batch_id) else {
                    drop(batches);
                    self.discard_pending_authorization(next.authorization);
                    continue;
                };
                if batch.cancelling {
                    batch.cancelled = batch.cancelled.saturating_add(1);
                    false
                } else if batch.paused || batch.blocked_conflict.is_some() {
                    drop(batches);
                    self.batch_pending.borrow_mut().push_front(next);
                    return;
                } else {
                    true
                }
            };
            if !dispatch {
                self.discard_pending_authorization(next.authorization);
                continue;
            }
            match self.submit_batch_operation(next.operation, next.authorization) {
                Ok(job_id) => {
                    self.batch_active.set(Some(job_id));
                    self.job_batches.borrow_mut().insert(job_id, next.batch_id);
                    if let Some(batch) = self
                        .batches
                        .borrow_mut()
                        .iter_mut()
                        .find(|batch| batch.id == next.batch_id)
                    {
                        batch.active_job = Some(job_id);
                    }
                    return;
                }
                Err(error) => {
                    tracing::error!(%error, "could not dispatch queued batch operation");
                    if let Some(batch) = self
                        .batches
                        .borrow_mut()
                        .iter_mut()
                        .find(|batch| batch.id == next.batch_id)
                    {
                        batch.failed = batch.failed.saturating_add(1);
                    }
                }
            }
        }
    }

    fn submit_batch_operation(
        &self,
        operation: TrackedOperation,
        authorization: Option<GuardrailAuthorizationItem>,
    ) -> Result<JobId, CopyInteractionError> {
        let Some(scope) = destructive_scope_for_operation(&operation)? else {
            return self.dispatch_batch_operation_after_guardrail(operation);
        };
        let authorization = authorization
            .ok_or_else(|| CopyInteractionError::AuthorizationRequired(scope.clone()))?;
        self.consume_then_dispatch(
            scope,
            GuardrailAuthorized::new(operation, authorization),
            |operation| self.dispatch_batch_operation_after_guardrail(operation),
        )
    }

    fn dispatch_batch_operation_after_guardrail(
        &self,
        operation: TrackedOperation,
    ) -> Result<JobId, CopyInteractionError> {
        match &operation {
            TrackedOperation::Copy(request) => {
                match self.copy_executor.submit_copy(request.clone()) {
                    Ok(submission) => {
                        self.track(submission.job_id(), operation.clone());
                        Ok(submission.job_id())
                    }
                    Err(error) => {
                        if let Some(job_id) = error.job_id() {
                            self.track(job_id, operation.clone());
                            Ok(job_id)
                        } else {
                            Err(error.into())
                        }
                    }
                }
            }
            TrackedOperation::Move(request) => {
                match self.move_executor.submit_move(request.clone()) {
                    Ok(submission) => {
                        self.track(submission.job_id(), operation.clone());
                        Ok(submission.job_id())
                    }
                    Err(error) => {
                        if let Some(job_id) = error.job_id() {
                            self.track(job_id, operation.clone());
                            Ok(job_id)
                        } else {
                            Err(error.into())
                        }
                    }
                }
            }
            TrackedOperation::Trash(request) => {
                match self.trash_executor.submit_trash(request.clone()) {
                    Ok(submission) => {
                        self.track(submission.job_id(), operation.clone());
                        Ok(submission.job_id())
                    }
                    Err(error) => {
                        if let Some(job_id) = error.job_id() {
                            self.track(job_id, operation.clone());
                            Ok(job_id)
                        } else {
                            Err(error.into())
                        }
                    }
                }
            }
            TrackedOperation::Restore(request) => {
                match self.restore_executor.submit(request.clone()) {
                    Ok(submission) => {
                        self.track(submission.job_id(), operation.clone());
                        Ok(submission.job_id())
                    }
                    Err(error) => {
                        if let Some(job_id) = error.job_id() {
                            self.track(job_id, operation.clone());
                            Ok(job_id)
                        } else {
                            Err(error.into())
                        }
                    }
                }
            }
            TrackedOperation::Create(request) => {
                match self.create_executor.submit(request.clone()) {
                    Ok(submission) => {
                        self.track(submission.job_id(), operation.clone());
                        Ok(submission.job_id())
                    }
                    Err(error) => {
                        if let Some(job_id) = error.job_id() {
                            self.track(job_id, operation.clone());
                            Ok(job_id)
                        } else {
                            Err(error.into())
                        }
                    }
                }
            }
            TrackedOperation::Rename(_)
            | TrackedOperation::PermanentDelete(_)
            | TrackedOperation::UndoMove { .. } => {
                unreachable!("operation is never queued as a per-item multi-selection batch")
            }
        }
    }

    pub fn finish_operation(
        &self,
        job_id: JobId,
        outcome: TerminalOutcome,
    ) -> Option<TrackedOperation> {
        let operation_id = lock(&self.jobs)
            .record(job_id)
            .map(|record| record.operation_id());
        let operation = self.operation_requests.borrow_mut().remove(&job_id);
        let batch_id = self.batch_for_job(job_id);
        if outcome == TerminalOutcome::Completed
            && let Some(TrackedOperation::Move(request)) = operation.as_ref()
        {
            self.transfer_buffer
                .borrow_mut()
                .clear_completed_move(request.source());
        }
        if matches!(operation, Some(TrackedOperation::Create(_))) {
            let _ = self.create_executor.take_outcome(job_id);
        }
        let undo = if outcome == TerminalOutcome::Completed {
            self.move_executor
                .take_outcome(job_id)
                .and_then(|move_outcome| match operation.as_ref()? {
                    TrackedOperation::Move(request) => Some(UndoMove {
                        original_job_id: job_id,
                        request: MoveRequest::new(
                            request.destination(),
                            request.source(),
                            ConflictPolicy::FailIfExists,
                        )
                        .with_expected_source_identity(move_outcome.destination_identity()),
                    }),
                    TrackedOperation::Rename(request) => {
                        let destination = request.source().parent()?.join(request.new_name());
                        Some(UndoMove {
                            original_job_id: job_id,
                            request: MoveRequest::new(
                                destination,
                                request.source(),
                                ConflictPolicy::FailIfExists,
                            )
                            .with_expected_source_identity(move_outcome.destination_identity()),
                        })
                    }
                    _ => None,
                })
        } else {
            None
        };

        if let Some(batch_id) = batch_id {
            let mut batches = self.batches.borrow_mut();
            if let Some(batch) = batches.iter_mut().find(|batch| batch.id == batch_id) {
                batch.active_job = None;
                match outcome {
                    TerminalOutcome::Completed => {
                        batch.completed = batch.completed.saturating_add(1);
                    }
                    TerminalOutcome::Conflict if batch.skip_conflicts => {
                        batch.skipped = batch.skipped.saturating_add(1);
                        self.resolved_conflicts.borrow_mut().insert(job_id);
                    }
                    TerminalOutcome::Conflict => batch.blocked_conflict = Some(job_id),
                    TerminalOutcome::Cancelled => {
                        batch.cancelled = batch.cancelled.saturating_add(1);
                    }
                    TerminalOutcome::PartialFailure | TerminalOutcome::Failed => {
                        batch.failed = batch.failed.saturating_add(1);
                    }
                }
            }
        }
        if let (Some(operation_id), Some(operation)) = (operation_id, operation.as_ref()) {
            let mut history = self.terminal_history.borrow_mut();
            if history.len() == MAX_TERMINAL_HISTORY {
                if let Some(evicted) = history.pop_front() {
                    self.resolved_conflicts
                        .borrow_mut()
                        .remove(&evicted.job_id());
                    self.resolved_undos.borrow_mut().remove(&evicted.job_id());
                    self.job_batches.borrow_mut().remove(&evicted.job_id());
                    lock(&self.jobs).forget_terminal(evicted.job_id());
                }
            }
            history.push_back(TerminalOperation {
                job_id,
                operation_id,
                outcome,
                operation: operation.clone(),
                batch_id,
                undo,
            });
        }
        if self.batch_active.get() == Some(job_id) {
            self.batch_active.set(None);
        }
        self.pump_batch();
        operation
    }

    pub fn terminal_history(&self) -> Vec<TerminalOperation> {
        self.terminal_history.borrow().iter().cloned().collect()
    }

    pub fn can_undo(&self, job_id: JobId) -> bool {
        !self.resolved_undos.borrow().contains(&job_id)
            && self
                .terminal_history
                .borrow()
                .iter()
                .any(|entry| entry.job_id() == job_id && entry.undo().is_some())
    }

    pub fn clear_completed_history(&self) -> usize {
        let removed = {
            let mut history = self.terminal_history.borrow_mut();
            let removed = history
                .iter()
                .filter(|entry| entry.outcome() == TerminalOutcome::Completed)
                .cloned()
                .collect::<Vec<_>>();
            history.retain(|entry| entry.outcome() != TerminalOutcome::Completed);
            removed
        };
        for entry in &removed {
            self.resolved_conflicts.borrow_mut().remove(&entry.job_id());
            self.resolved_undos.borrow_mut().remove(&entry.job_id());
            self.job_batches.borrow_mut().remove(&entry.job_id());
            lock(&self.jobs).forget_terminal(entry.job_id());
        }
        removed.len()
    }

    pub fn undo_operation(
        &self,
        original_job_id: JobId,
    ) -> Result<MoveSubmission, CopyInteractionError> {
        let scope = self.undo_operation_guardrail_scope(original_job_id)?;
        Err(CopyInteractionError::AuthorizationRequired(scope))
    }

    pub fn undo_operation_guardrail_scope(
        &self,
        original_job_id: JobId,
    ) -> Result<DestructiveScope, CopyInteractionError> {
        if self.resolved_undos.borrow().contains(&original_job_id) {
            return Err(CopyInteractionError::UndoAlreadySubmitted(original_job_id));
        }
        let undo = self
            .terminal_history
            .borrow()
            .iter()
            .find(|entry| entry.job_id() == original_job_id)
            .and_then(|entry| entry.undo().cloned())
            .ok_or(CopyInteractionError::UndoNotAvailable(original_job_id))?;
        destructive_scope_for_move(&undo.request).map_err(CopyInteractionError::from)
    }

    pub fn undo_operation_authorized(
        &self,
        original_job_id: JobId,
        authorization: GuardrailAuthorizationItem,
    ) -> Result<MoveSubmission, CopyInteractionError> {
        if self.resolved_undos.borrow().contains(&original_job_id) {
            return Err(CopyInteractionError::UndoAlreadySubmitted(original_job_id));
        }
        let undo = self
            .terminal_history
            .borrow()
            .iter()
            .find(|entry| entry.job_id() == original_job_id)
            .and_then(|entry| entry.undo().cloned())
            .ok_or(CopyInteractionError::UndoNotAvailable(original_job_id))?;
        let operation = TrackedOperation::UndoMove {
            request: undo.request.clone(),
            original_job_id,
        };
        let scope = destructive_scope_for_move(&undo.request)?;
        let submission = self.consume_then_dispatch(
            scope,
            GuardrailAuthorized::new(undo.request, authorization),
            |request| match self.move_executor.submit_move(request) {
                Ok(submission) => {
                    self.track(submission.job_id(), operation);
                    Ok(submission)
                }
                Err(error) => {
                    if let Some(job_id) = error.job_id() {
                        self.track(job_id, operation);
                    }
                    Err(error.into())
                }
            },
        )?;
        self.resolved_undos.borrow_mut().insert(original_job_id);
        Ok(submission)
    }

    pub fn retry_operation(
        &self,
        failed_job_id: JobId,
    ) -> Result<RetrySubmission, CopyInteractionError> {
        let operation = self.retryable_operation(failed_job_id)?;
        if let Some(scope) = destructive_scope_for_operation(&operation)? {
            return Err(CopyInteractionError::AuthorizationRequired(scope));
        }
        self.dispatch_retry_operation(failed_job_id, operation)
    }

    pub fn retry_operation_guardrail_scope(
        &self,
        failed_job_id: JobId,
    ) -> Result<Option<DestructiveScope>, CopyInteractionError> {
        let operation = self.retryable_operation(failed_job_id)?;
        destructive_scope_for_operation(&operation).map_err(CopyInteractionError::from)
    }

    pub fn retry_operation_authorized(
        &self,
        failed_job_id: JobId,
        authorization: GuardrailAuthorizationItem,
    ) -> Result<RetrySubmission, CopyInteractionError> {
        let operation = self.retryable_operation(failed_job_id)?;
        let scope = destructive_scope_for_operation(&operation)?
            .ok_or_else(|| CopyInteractionError::InvalidSource(operation.source().to_path_buf()))?;
        self.consume_then_dispatch(
            scope,
            GuardrailAuthorized::new(operation, authorization),
            |operation| self.dispatch_retry_operation(failed_job_id, operation),
        )
    }

    fn retryable_operation(
        &self,
        failed_job_id: JobId,
    ) -> Result<TrackedOperation, CopyInteractionError> {
        let terminal = self
            .terminal_history
            .borrow()
            .iter()
            .find(|entry| entry.job_id() == failed_job_id)
            .cloned()
            .ok_or(CopyInteractionError::RetryNotFound(failed_job_id))?;
        if terminal.outcome() == TerminalOutcome::Completed {
            return Err(CopyInteractionError::RetryCompleted(failed_job_id));
        }
        if terminal.outcome() == TerminalOutcome::Conflict {
            return Err(CopyInteractionError::ConflictDecisionRequired(
                failed_job_id,
            ));
        }
        if terminal.outcome() == TerminalOutcome::PartialFailure {
            return Err(CopyInteractionError::RetryUnsafePartial(failed_job_id));
        }
        Ok(terminal.operation().clone())
    }

    fn dispatch_retry_operation(
        &self,
        failed_job_id: JobId,
        operation: TrackedOperation,
    ) -> Result<RetrySubmission, CopyInteractionError> {
        match operation {
            TrackedOperation::Copy(request) => {
                match self
                    .copy_executor
                    .submit_retry(failed_job_id, request.clone())
                {
                    Ok(submission) => {
                        self.track(submission.job_id(), TrackedOperation::Copy(request));
                        Ok(RetrySubmission::Copy(submission))
                    }
                    Err(error) => {
                        if let Some(job_id) = error.job_id() {
                            self.track(job_id, TrackedOperation::Copy(request));
                        }
                        Err(error.into())
                    }
                }
            }
            TrackedOperation::Create(request) => {
                match self
                    .create_executor
                    .submit_retry(failed_job_id, request.clone())
                {
                    Ok(submission) => {
                        self.track(submission.job_id(), TrackedOperation::Create(request));
                        Ok(RetrySubmission::Create(submission))
                    }
                    Err(error) => {
                        if let Some(job_id) = error.job_id() {
                            self.track(job_id, TrackedOperation::Create(request));
                        }
                        Err(error.into())
                    }
                }
            }
            TrackedOperation::Move(request) => {
                match self
                    .move_executor
                    .submit_move_retry(failed_job_id, request.clone())
                {
                    Ok(submission) => {
                        self.track(submission.job_id(), TrackedOperation::Move(request));
                        Ok(RetrySubmission::Move(submission))
                    }
                    Err(error) => {
                        if let Some(job_id) = error.job_id() {
                            self.track(job_id, TrackedOperation::Move(request));
                        }
                        Err(error.into())
                    }
                }
            }
            TrackedOperation::UndoMove {
                request,
                original_job_id,
            } => {
                match self
                    .move_executor
                    .submit_move_retry(failed_job_id, request.clone())
                {
                    Ok(submission) => {
                        self.track(
                            submission.job_id(),
                            TrackedOperation::UndoMove {
                                request,
                                original_job_id,
                            },
                        );
                        Ok(RetrySubmission::Move(submission))
                    }
                    Err(error) => {
                        if let Some(job_id) = error.job_id() {
                            self.track(
                                job_id,
                                TrackedOperation::UndoMove {
                                    request,
                                    original_job_id,
                                },
                            );
                        }
                        Err(error.into())
                    }
                }
            }
            TrackedOperation::Rename(request) => {
                match self
                    .move_executor
                    .submit_rename_retry(failed_job_id, request.clone())
                {
                    Ok(submission) => {
                        self.track(submission.job_id(), TrackedOperation::Rename(request));
                        Ok(RetrySubmission::Move(submission))
                    }
                    Err(error) => {
                        if let Some(job_id) = error.job_id() {
                            self.track(job_id, TrackedOperation::Rename(request));
                        }
                        Err(error.into())
                    }
                }
            }
            TrackedOperation::Trash(request) => {
                match self
                    .trash_executor
                    .submit_trash_retry(failed_job_id, request.clone())
                {
                    Ok(submission) => {
                        self.track(submission.job_id(), TrackedOperation::Trash(request));
                        Ok(RetrySubmission::Trash(submission))
                    }
                    Err(error) => {
                        if let Some(job_id) = error.job_id() {
                            self.track(job_id, TrackedOperation::Trash(request));
                        }
                        Err(error.into())
                    }
                }
            }
            TrackedOperation::PermanentDelete(request) => {
                match self
                    .permanent_delete_executor
                    .submit_retry(failed_job_id, request.clone())
                {
                    Ok(submission) => {
                        self.track(
                            submission.job_id(),
                            TrackedOperation::PermanentDelete(request),
                        );
                        Ok(RetrySubmission::PermanentDelete(submission))
                    }
                    Err(error) => {
                        if let Some(job_id) = error.job_id() {
                            self.track(job_id, TrackedOperation::PermanentDelete(request));
                        }
                        Err(error.into())
                    }
                }
            }
            TrackedOperation::Restore(request) => {
                match self
                    .restore_executor
                    .submit_retry(failed_job_id, request.clone())
                {
                    Ok(submission) => {
                        self.track(submission.job_id(), TrackedOperation::Restore(request));
                        Ok(RetrySubmission::Restore(submission))
                    }
                    Err(error) => {
                        if let Some(job_id) = error.job_id() {
                            self.track(job_id, TrackedOperation::Restore(request));
                        }
                        Err(error.into())
                    }
                }
            }
        }
    }

    pub fn pending_conflict(&self, job_id: JobId) -> Result<PendingConflict, CopyInteractionError> {
        let terminal = self.pending_conflict_operation(job_id)?;
        let destination = match terminal.operation() {
            TrackedOperation::Copy(request) => request.destination().to_path_buf(),
            TrackedOperation::Create(request) => request.destination().to_path_buf(),
            TrackedOperation::Move(request) => request.destination().to_path_buf(),
            TrackedOperation::Rename(request) => request
                .source()
                .parent()
                .ok_or(CopyInteractionError::ConflictUnsupported(job_id))?
                .join(request.new_name()),
            TrackedOperation::Trash(_) => {
                return Err(CopyInteractionError::ConflictUnsupported(job_id));
            }
            TrackedOperation::PermanentDelete(_) => {
                return Err(CopyInteractionError::ConflictUnsupported(job_id));
            }
            TrackedOperation::Restore(request) => request.destination().to_path_buf(),
            TrackedOperation::UndoMove { .. } => {
                return Err(CopyInteractionError::ConflictUnsupported(job_id));
            }
        };

        Ok(PendingConflict {
            job_id,
            operation_id: terminal.operation_id(),
            source: terminal.operation().source().to_path_buf(),
            destination,
        })
    }

    pub fn resolve_conflict(
        &self,
        job_id: JobId,
        decision: ConflictDecision,
    ) -> Result<ConflictResolution, CopyInteractionError> {
        self.resolve_conflict_with_authorization(job_id, decision, None)
    }

    pub fn resolve_conflict_authorized(
        &self,
        job_id: JobId,
        decision: ConflictDecision,
        authorization: GuardrailAuthorizationItem,
    ) -> Result<ConflictResolution, CopyInteractionError> {
        self.resolve_conflict_with_authorization(job_id, decision, Some(authorization))
    }

    pub fn conflict_guardrail_scope(
        &self,
        job_id: JobId,
        decision: &ConflictDecision,
    ) -> Result<Option<DestructiveScope>, CopyInteractionError> {
        let terminal = self.pending_conflict_operation(job_id)?;
        let new_name = match decision {
            ConflictDecision::KeepExisting | ConflictDecision::SkipAll => return Ok(None),
            ConflictDecision::RetryWithName(new_name) => {
                validate_rename_name(new_name)?;
                new_name.clone()
            }
            ConflictDecision::KeepBoth => {
                let attempt = self
                    .terminal_history
                    .borrow()
                    .iter()
                    .filter(|entry| {
                        entry.operation_id() == terminal.operation_id()
                            && entry.outcome() == TerminalOutcome::Conflict
                    })
                    .count()
                    .try_into()
                    .unwrap_or(u32::MAX);
                let base_name = self
                    .terminal_history
                    .borrow()
                    .iter()
                    .find(|entry| entry.operation_id() == terminal.operation_id())
                    .and_then(|entry| conflict_destination(entry.operation()))
                    .and_then(|path| path.file_name().map(OsStr::to_os_string))
                    .ok_or(CopyInteractionError::ConflictUnsupported(job_id))?;
                match terminal.operation() {
                    TrackedOperation::Create(request)
                        if matches!(request.kind(), CreateKind::Duplicate { .. }) =>
                    {
                        let source_name = request
                            .source()
                            .and_then(Path::file_name)
                            .ok_or(CopyInteractionError::ConflictUnsupported(job_id))?;
                        duplicate_name(source_name, attempt.saturating_add(1))
                    }
                    _ => keep_both_name(&base_name, attempt),
                }
                .ok_or(CopyInteractionError::ConflictUnsupported(job_id))?
            }
        };

        match terminal.operation() {
            TrackedOperation::Copy(_) | TrackedOperation::Create(_) => Ok(None),
            TrackedOperation::Move(request) => {
                let destination = retry_destination(request.destination(), &new_name, job_id)?;
                destructive_scope_for_move(&MoveRequest::new(
                    request.source(),
                    destination,
                    ConflictPolicy::FailIfExists,
                ))
                .map(Some)
                .map_err(CopyInteractionError::from)
            }
            TrackedOperation::Rename(request) => destructive_scope_for_rename(&RenameRequest::new(
                request.source(),
                new_name,
                ConflictPolicy::FailIfExists,
            ))
            .map(Some)
            .map_err(CopyInteractionError::from),
            TrackedOperation::Restore(request) => {
                let destination = retry_destination(request.destination(), &new_name, job_id)?;
                let revised = request.with_destination(destination)?;
                destructive_scope_for_restore(&revised)
                    .map(Some)
                    .map_err(CopyInteractionError::from)
            }
            TrackedOperation::Trash(_)
            | TrackedOperation::PermanentDelete(_)
            | TrackedOperation::UndoMove { .. } => {
                Err(CopyInteractionError::ConflictUnsupported(job_id))
            }
        }
    }

    fn resolve_conflict_with_authorization(
        &self,
        job_id: JobId,
        decision: ConflictDecision,
        mut authorization: Option<GuardrailAuthorizationItem>,
    ) -> Result<ConflictResolution, CopyInteractionError> {
        let terminal = self.pending_conflict_operation(job_id)?;
        if matches!(terminal.operation(), TrackedOperation::Trash(_)) {
            return Err(CopyInteractionError::ConflictUnsupported(job_id));
        }

        match decision {
            ConflictDecision::KeepExisting => {
                self.resolved_conflicts.borrow_mut().insert(job_id);
                self.complete_batch_conflict(job_id, None, true, false);
                Ok(ConflictResolution::KeptExisting)
            }
            ConflictDecision::SkipAll => {
                if self.batch_for_job(job_id).is_none() {
                    return Err(CopyInteractionError::ConflictUnsupported(job_id));
                }
                self.resolved_conflicts.borrow_mut().insert(job_id);
                self.complete_batch_conflict(job_id, None, true, true);
                Ok(ConflictResolution::KeptExisting)
            }
            ConflictDecision::KeepBoth => {
                let attempt = self
                    .terminal_history
                    .borrow()
                    .iter()
                    .filter(|entry| {
                        entry.operation_id() == terminal.operation_id()
                            && entry.outcome() == TerminalOutcome::Conflict
                    })
                    .count()
                    .try_into()
                    .unwrap_or(u32::MAX);
                let base_name = self
                    .terminal_history
                    .borrow()
                    .iter()
                    .find(|entry| entry.operation_id() == terminal.operation_id())
                    .and_then(|entry| conflict_destination(entry.operation()))
                    .and_then(|path| path.file_name().map(OsStr::to_os_string))
                    .ok_or(CopyInteractionError::ConflictUnsupported(job_id))?;
                let new_name = match terminal.operation() {
                    TrackedOperation::Create(request)
                        if matches!(request.kind(), CreateKind::Duplicate { .. }) =>
                    {
                        let source_name = request
                            .source()
                            .and_then(Path::file_name)
                            .ok_or(CopyInteractionError::ConflictUnsupported(job_id))?;
                        duplicate_name(source_name, attempt.saturating_add(1))
                    }
                    _ => keep_both_name(&base_name, attempt),
                }
                .ok_or(CopyInteractionError::ConflictUnsupported(job_id))?;
                let submission = self.submit_conflict_retry(
                    job_id,
                    terminal.operation(),
                    new_name,
                    authorization.take(),
                )?;
                self.resolved_conflicts.borrow_mut().insert(job_id);
                self.complete_batch_conflict(job_id, Some(submission.job_id()), false, false);
                Ok(ConflictResolution::Retried(submission))
            }
            ConflictDecision::RetryWithName(new_name) => {
                validate_rename_name(&new_name)?;
                let submission = self.submit_conflict_retry(
                    job_id,
                    terminal.operation(),
                    new_name,
                    authorization.take(),
                )?;
                self.resolved_conflicts.borrow_mut().insert(job_id);
                self.complete_batch_conflict(job_id, Some(submission.job_id()), false, false);
                Ok(ConflictResolution::Retried(submission))
            }
        }
    }

    fn complete_batch_conflict(
        &self,
        failed_job_id: JobId,
        retry_job_id: Option<JobId>,
        skipped: bool,
        skip_all: bool,
    ) {
        let Some(batch_id) = self.batch_for_job(failed_job_id) else {
            return;
        };
        if let Some(retry_job_id) = retry_job_id {
            self.job_batches.borrow_mut().insert(retry_job_id, batch_id);
            self.batch_active.set(Some(retry_job_id));
        }
        if let Some(batch) = self
            .batches
            .borrow_mut()
            .iter_mut()
            .find(|batch| batch.id == batch_id)
        {
            batch.blocked_conflict = None;
            batch.skip_conflicts |= skip_all;
            if skipped {
                batch.skipped = batch.skipped.saturating_add(1);
            }
            batch.active_job = retry_job_id;
        }
        if retry_job_id.is_none() {
            self.pump_batch();
        }
    }

    fn pending_conflict_operation(
        &self,
        job_id: JobId,
    ) -> Result<TerminalOperation, CopyInteractionError> {
        let terminal = self
            .terminal_history
            .borrow()
            .iter()
            .find(|entry| entry.job_id() == job_id)
            .cloned()
            .ok_or(CopyInteractionError::ConflictNotFound(job_id))?;
        if terminal.outcome() != TerminalOutcome::Conflict {
            return Err(CopyInteractionError::ConflictNotFound(job_id));
        }
        if self.resolved_conflicts.borrow().contains(&job_id) {
            return Err(CopyInteractionError::ConflictAlreadyResolved(job_id));
        }
        Ok(terminal)
    }

    fn submit_conflict_retry(
        &self,
        failed_job_id: JobId,
        operation: &TrackedOperation,
        new_name: OsString,
        authorization: Option<GuardrailAuthorizationItem>,
    ) -> Result<RetrySubmission, CopyInteractionError> {
        match operation {
            TrackedOperation::Copy(request) => {
                let destination =
                    retry_destination(request.destination(), &new_name, failed_job_id)?;
                let revised = CopyRequest::new(
                    request.source(),
                    destination,
                    ConflictPolicy::FailIfExists,
                    request.symlink_policy(),
                );
                match self
                    .copy_executor
                    .submit_retry(failed_job_id, revised.clone())
                {
                    Ok(submission) => {
                        self.track(submission.job_id(), TrackedOperation::Copy(revised));
                        Ok(RetrySubmission::Copy(submission))
                    }
                    Err(error) => {
                        if let Some(job_id) = error.job_id() {
                            self.track(job_id, TrackedOperation::Copy(revised));
                        }
                        Err(error.into())
                    }
                }
            }
            TrackedOperation::Create(request) => {
                let destination =
                    retry_destination(request.destination(), &new_name, failed_job_id)?;
                let revised = request.with_destination(destination)?;
                match self
                    .create_executor
                    .submit_retry(failed_job_id, revised.clone())
                {
                    Ok(submission) => {
                        self.track(submission.job_id(), TrackedOperation::Create(revised));
                        Ok(RetrySubmission::Create(submission))
                    }
                    Err(error) => {
                        if let Some(job_id) = error.job_id() {
                            self.track(job_id, TrackedOperation::Create(revised));
                        }
                        Err(error.into())
                    }
                }
            }
            TrackedOperation::Move(request) => {
                let destination =
                    retry_destination(request.destination(), &new_name, failed_job_id)?;
                let revised =
                    MoveRequest::new(request.source(), destination, ConflictPolicy::FailIfExists);
                let scope = destructive_scope_for_move(&revised)?;
                let authorization = authorization
                    .ok_or_else(|| CopyInteractionError::AuthorizationRequired(scope.clone()))?;
                self.consume_then_dispatch(
                    scope,
                    GuardrailAuthorized::new(revised, authorization),
                    |revised| match self
                        .move_executor
                        .submit_move_retry(failed_job_id, revised.clone())
                    {
                        Ok(submission) => {
                            self.track(submission.job_id(), TrackedOperation::Move(revised));
                            Ok(RetrySubmission::Move(submission))
                        }
                        Err(error) => {
                            if let Some(job_id) = error.job_id() {
                                self.track(job_id, TrackedOperation::Move(revised));
                            }
                            Err(error.into())
                        }
                    },
                )
            }
            TrackedOperation::Rename(request) => {
                let revised =
                    RenameRequest::new(request.source(), new_name, ConflictPolicy::FailIfExists);
                let scope = destructive_scope_for_rename(&revised)?;
                let authorization = authorization
                    .ok_or_else(|| CopyInteractionError::AuthorizationRequired(scope.clone()))?;
                self.guardrails
                    .borrow_mut()
                    .consume_authorization(authorization, &scope)?;
                match self
                    .move_executor
                    .submit_rename_retry(failed_job_id, revised.clone())
                {
                    Ok(submission) => {
                        self.track(submission.job_id(), TrackedOperation::Rename(revised));
                        Ok(RetrySubmission::Move(submission))
                    }
                    Err(error) => {
                        if let Some(job_id) = error.job_id() {
                            self.track(job_id, TrackedOperation::Rename(revised));
                        }
                        Err(error.into())
                    }
                }
            }
            TrackedOperation::Restore(request) => {
                let destination =
                    retry_destination(request.destination(), &new_name, failed_job_id)?;
                let revised = request.with_destination(destination)?;
                let scope = destructive_scope_for_restore(&revised)?;
                let authorization = authorization
                    .ok_or_else(|| CopyInteractionError::AuthorizationRequired(scope.clone()))?;
                self.guardrails
                    .borrow_mut()
                    .consume_authorization(authorization, &scope)?;
                match self
                    .restore_executor
                    .submit_retry(failed_job_id, revised.clone())
                {
                    Ok(submission) => {
                        self.track(submission.job_id(), TrackedOperation::Restore(revised));
                        Ok(RetrySubmission::Restore(submission))
                    }
                    Err(error) => {
                        if let Some(job_id) = error.job_id() {
                            self.track(job_id, TrackedOperation::Restore(revised));
                        }
                        Err(error.into())
                    }
                }
            }
            TrackedOperation::Trash(_) => {
                Err(CopyInteractionError::ConflictUnsupported(failed_job_id))
            }
            TrackedOperation::PermanentDelete(_) => {
                Err(CopyInteractionError::ConflictUnsupported(failed_job_id))
            }
            TrackedOperation::UndoMove { .. } => {
                Err(CopyInteractionError::ConflictUnsupported(failed_job_id))
            }
        }
    }

    pub fn cancel_operation(&self, job_id: JobId) -> Result<(), CopyInteractionError> {
        if self.batch_rename_requests.borrow().contains_key(&job_id) {
            self.batch_rename_executor
                .cancel(job_id)
                .map_err(|error| CopyInteractionError::PermissionCancel(error.to_string()))?;
            return Ok(());
        }
        if self.archive_requests.borrow().contains_key(&job_id) {
            self.archive_executor.cancel(job_id)?;
            return Ok(());
        }
        if self.checksum_requests.borrow().contains_key(&job_id) {
            self.checksum_executor.cancel(job_id)?;
            return Ok(());
        }
        if self.integrity_requests.borrow().contains_key(&job_id) {
            self.integrity_executor.cancel(job_id)?;
            return Ok(());
        }
        if self.verified_copy_requests.borrow().contains_key(&job_id) {
            self.verified_copy_executor.cancel(job_id)?;
            return Ok(());
        }
        if self.permission_requests.borrow().contains_key(&job_id) {
            self.permission_executor
                .cancel(job_id)
                .map_err(|error| CopyInteractionError::PermissionCancel(error.to_string()))?;
            return Ok(());
        }
        match self.operation_request(job_id) {
            Some(TrackedOperation::Copy(_)) => self.copy_executor.cancel(job_id)?,
            Some(TrackedOperation::Create(_)) => self.create_executor.cancel(job_id)?,
            Some(
                TrackedOperation::Move(_)
                | TrackedOperation::Rename(_)
                | TrackedOperation::UndoMove { .. },
            ) => {
                self.move_executor.cancel(job_id)?;
            }
            Some(TrackedOperation::Trash(_)) => self.trash_executor.cancel(job_id)?,
            Some(TrackedOperation::PermanentDelete(_)) => {
                self.permanent_delete_executor.cancel(job_id)?;
            }
            Some(TrackedOperation::Restore(_)) => self.restore_executor.cancel(job_id)?,
            None => return Err(MoveCancelError::NotActive(job_id).into()),
        }
        Ok(())
    }

    pub fn drain_job_events(&self) -> Vec<JobEvent> {
        lock(&self.jobs).drain_events()
    }

    fn track(&self, job_id: JobId, operation: TrackedOperation) {
        self.operation_requests
            .borrow_mut()
            .insert(job_id, operation);
    }

    /// The sole final application boundary for guarded executor dispatch.
    /// Permit consumption is intentionally the last operation before the
    /// executor closure is invoked.
    fn consume_then_dispatch<T, R>(
        &self,
        exact_scope: DestructiveScope,
        authorized: GuardrailAuthorized<T>,
        dispatch: impl FnOnce(T) -> Result<R, CopyInteractionError>,
    ) -> Result<R, CopyInteractionError> {
        let (value, authorization) = authorized.into_parts();
        self.guardrails
            .borrow_mut()
            .consume_authorization(authorization, &exact_scope)?;
        dispatch(value)
    }

    fn discard_pending_authorization(&self, authorization: Option<GuardrailAuthorizationItem>) {
        if let Some(authorization) = authorization
            && let Err(error) = self
                .guardrails
                .borrow_mut()
                .discard_authorization(authorization)
        {
            tracing::debug!(%error, "queued guardrail authorization was already revoked");
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_trash_backend(
        backend: Arc<dyn TrashBackend>,
    ) -> Result<Self, ApplicationStateSpawnError> {
        Self::new_with_trash_backend_and_guardrail_store(backend, test_guardrail_store_path())
    }

    #[cfg(test)]
    fn new_with_trash_backend_and_guardrail_store(
        backend: Arc<dyn TrashBackend>,
        guardrail_store_path: PathBuf,
    ) -> Result<Self, ApplicationStateSpawnError> {
        let jobs = Arc::new(Mutex::new(ApplicationJobManager::new()));
        let archive_executor = ArchiveExecutor::spawn(Arc::clone(&jobs))?;
        let batch_rename_executor = BatchRenameExecutor::spawn(Arc::clone(&jobs))?;
        let copy_executor = CopyExecutor::spawn(Arc::clone(&jobs))?;
        let create_executor = CreateExecutor::spawn(Arc::clone(&jobs))?;
        let move_executor = MoveExecutor::spawn(Arc::clone(&jobs))?;
        let trash_executor = TrashExecutor::spawn_with_backend(Arc::clone(&jobs), 8, backend)?;
        let permanent_delete_executor = PermanentDeleteExecutor::spawn(Arc::clone(&jobs))?;
        let permission_executor = PermissionExecutor::spawn(Arc::clone(&jobs))?;
        let checksum_executor = ChecksumExecutor::spawn(Arc::clone(&jobs))?;
        let integrity_executor = IntegrityExecutor::spawn(Arc::clone(&jobs))?;
        let verified_copy_executor = VerifiedCopyExecutor::spawn(Arc::clone(&jobs))?;
        let restore_executor = RestoreExecutor::spawn(Arc::clone(&jobs))?;
        let guardrails = GuardrailController::load_at(guardrail_store_path.clone())?;
        let guardrail_policy_worker = GuardrailPolicyWorker::spawn(guardrail_store_path)
            .map_err(ApplicationStateSpawnError::GuardrailPolicy)?;
        Ok(Self {
            jobs,
            archive_executor,
            batch_rename_executor,
            copy_executor,
            create_executor,
            move_executor,
            trash_executor,
            permanent_delete_executor,
            permission_executor,
            checksum_executor,
            integrity_executor,
            verified_copy_executor,
            restore_executor,
            guardrails: RefCell::new(guardrails),
            guardrail_policy_worker: RefCell::new(guardrail_policy_worker),
            guardrail_policy_pending: Cell::new(None),
            transfer_buffer: RefCell::new(TransferBuffer::default()),
            operation_requests: RefCell::new(HashMap::new()),
            permission_requests: RefCell::new(HashMap::new()),
            checksum_requests: RefCell::new(HashMap::new()),
            integrity_requests: RefCell::new(HashMap::new()),
            verified_copy_requests: RefCell::new(HashMap::new()),
            verified_usb_copy_jobs: RefCell::new(HashSet::new()),
            verified_usb_completion_observer: VerifiedUsbCompletionObserver::default(),
            archive_requests: RefCell::new(HashMap::new()),
            batch_rename_requests: RefCell::new(HashMap::new()),
            batch_rename_undo: RefCell::new(None),
            terminal_history: RefCell::new(VecDeque::new()),
            resolved_conflicts: RefCell::new(HashSet::new()),
            resolved_undos: RefCell::new(HashSet::new()),
            batch_pending: RefCell::new(VecDeque::new()),
            batch_active: Cell::new(None),
            batches: RefCell::new(VecDeque::new()),
            job_batches: RefCell::new(HashMap::new()),
            next_batch_id: Cell::new(1),
        })
    }

    #[cfg(test)]
    fn submit_paste_with_cancellation(
        &self,
        destination_directory: &Path,
        cancellation: floe_core::CopyCancellation,
    ) -> Result<TransferSubmission, CopyInteractionError> {
        let (intent, source) = self
            .staged_transfer()
            .ok_or(CopyInteractionError::EmptyBuffer)?;
        if intent != TransferIntent::Copy {
            return Err(CopyInteractionError::EmptyBuffer);
        }
        let request = CopyRequest::new(
            source.clone(),
            transfer_destination(&source, destination_directory)?,
            ConflictPolicy::FailIfExists,
            SymlinkPolicy::Preserve,
        );
        let submission = self
            .copy_executor
            .submit_copy_with_cancellation(request.clone(), cancellation)?;
        self.track(submission.job_id(), TrackedOperation::Copy(request));
        Ok(TransferSubmission::Copy(submission))
    }
}

fn default_guardrail_store_path() -> PathBuf {
    glib::user_config_dir()
        .join("floe")
        .join("guardrails-v1.bin")
}

pub(crate) fn destructive_scope_for_move(
    request: &MoveRequest,
) -> Result<DestructiveScope, DestructiveScopeError> {
    DestructiveScope::new(
        DestructiveAction::Move,
        vec![request.source().to_path_buf()],
        Some(request.destination().to_path_buf()),
    )
}

pub(crate) fn destructive_scope_for_rename(
    request: &RenameRequest,
) -> Result<DestructiveScope, DestructiveScopeError> {
    let destination = request
        .source()
        .parent()
        .map(|parent| parent.join(request.new_name()));
    DestructiveScope::new(
        DestructiveAction::Rename,
        vec![request.source().to_path_buf()],
        destination,
    )
}

pub(crate) fn destructive_scope_for_trash(
    request: &TrashRequest,
) -> Result<DestructiveScope, DestructiveScopeError> {
    DestructiveScope::new(
        DestructiveAction::Trash,
        vec![request.source().to_path_buf()],
        None,
    )
}

pub(crate) fn destructive_scope_for_permanent_delete(
    request: &PermanentDeleteRequest,
) -> Result<DestructiveScope, DestructiveScopeError> {
    DestructiveScope::new(
        DestructiveAction::PermanentDelete,
        request.targets().to_vec(),
        None,
    )
}

pub(crate) fn destructive_scope_for_restore(
    request: &RestoreRequest,
) -> Result<DestructiveScope, DestructiveScopeError> {
    DestructiveScope::new(
        DestructiveAction::Move,
        vec![request.backing_path().to_path_buf()],
        Some(request.destination().to_path_buf()),
    )
}

pub(crate) fn destructive_scope_for_operation(
    operation: &TrackedOperation,
) -> Result<Option<DestructiveScope>, DestructiveScopeError> {
    match operation {
        TrackedOperation::Copy(_) | TrackedOperation::Create(_) => Ok(None),
        TrackedOperation::Move(request) => destructive_scope_for_move(request).map(Some),
        TrackedOperation::Rename(request) => destructive_scope_for_rename(request).map(Some),
        TrackedOperation::Trash(request) => destructive_scope_for_trash(request).map(Some),
        TrackedOperation::PermanentDelete(request) => {
            destructive_scope_for_permanent_delete(request).map(Some)
        }
        TrackedOperation::Restore(request) => destructive_scope_for_restore(request).map(Some),
        TrackedOperation::UndoMove { request, .. } => destructive_scope_for_move(request).map(Some),
    }
}

pub(crate) fn destructive_scopes_for_batch_rename(
    request: &BatchRenameRequest,
) -> Result<Vec<DestructiveScope>, DestructiveScopeError> {
    request
        .pairs()
        .iter()
        .map(|pair| {
            DestructiveScope::new(
                DestructiveAction::Rename,
                vec![pair.source().to_path_buf()],
                Some(pair.destination().to_path_buf()),
            )
        })
        .collect()
}

#[cfg(test)]
fn test_guardrail_store_path() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    std::env::temp_dir().join(format!(
        "floe-phase-18x-state-{}-{}.bin",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ))
}

fn retry_destination(
    destination: &Path,
    new_name: &OsStr,
    job_id: JobId,
) -> Result<PathBuf, CopyInteractionError> {
    destination
        .parent()
        .map(|parent| parent.join(new_name))
        .ok_or(CopyInteractionError::ConflictUnsupported(job_id))
}

fn conflict_destination(operation: &TrackedOperation) -> Option<PathBuf> {
    match operation {
        TrackedOperation::Copy(request) => Some(request.destination().to_path_buf()),
        TrackedOperation::Create(request) => Some(request.destination().to_path_buf()),
        TrackedOperation::Move(request) => Some(request.destination().to_path_buf()),
        TrackedOperation::Rename(request) => {
            Some(request.source().parent()?.join(request.new_name()))
        }
        TrackedOperation::Restore(request) => Some(request.destination().to_path_buf()),
        TrackedOperation::Trash(_)
        | TrackedOperation::PermanentDelete(_)
        | TrackedOperation::UndoMove { .. } => None,
    }
}

pub(crate) fn transfer_destination(
    source: &Path,
    destination_directory: &Path,
) -> Result<PathBuf, CopyInteractionError> {
    let name = source
        .file_name()
        .ok_or_else(|| CopyInteractionError::InvalidSource(source.to_path_buf()))?;
    if lexically_normalized(destination_directory).starts_with(lexically_normalized(source)) {
        return Err(CopyInteractionError::DestinationInsideSource);
    }
    Ok(destination_directory.join(name))
}

pub fn validate_rename_name(name: &OsStr) -> Result<(), CopyInteractionError> {
    let path = Path::new(name);
    let mut components = path.components();
    let valid = matches!(
        components.next(),
        Some(std::path::Component::Normal(component)) if component == name
    );
    if !valid || components.next().is_some() || name.as_bytes().contains(&0) {
        Err(CopyInteractionError::InvalidRenameName)
    } else {
        Ok(())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn lexically_normalized(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir if normalized.file_name().is_some() => {
                normalized.pop();
            }
            Component::ParentDir if !path.has_root() => normalized.push(component.as_os_str()),
            Component::ParentDir => {}
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        fs,
        os::unix::{
            ffi::{OsStrExt, OsStringExt},
            fs::PermissionsExt,
        },
        sync::atomic::{AtomicUsize, Ordering},
        thread,
        time::{Duration, Instant},
    };

    use floe_core::{
        CopyCancellation, JobEventKind, JobFailureKind, JobState, PermissionChange, PermissionScope,
    };
    use tempfile::tempdir;

    use super::*;
    use crate::{
        guardrail_controller::GuardrailBlock, guardrail_preflight::GuardrailPreflightError,
        guardrail_store::GuardrailStore,
    };

    fn authorize_scope(
        state: &ApplicationState,
        scope: DestructiveScope,
    ) -> GuardrailAuthorizationItem {
        let deterministic_scope = scope.clone();
        let submission = state
            .begin_guardrail_review(vec![scope], PreflightEnvironment::default())
            .expect("guardrail review should start");
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match state
                .poll_guardrail_review(submission.generation())
                .expect("guardrail review should poll")
            {
                GuardrailPoll::Pending => {
                    assert!(Instant::now() < deadline, "guardrail review timed out");
                    thread::yield_now();
                }
                GuardrailPoll::Allowed(authorization) => {
                    return authorization
                        .into_items()
                        .into_iter()
                        .next()
                        .expect("one exact authorization item");
                }
                GuardrailPoll::ReviewRequired(review) => {
                    return match state
                        .resolve_guardrail_review(
                            review.generation(),
                            GuardrailConfirmation::Confirm,
                        )
                        .expect("guardrail review should resolve")
                    {
                        GuardrailResolution::Allowed(authorization) => authorization
                            .into_items()
                            .into_iter()
                            .next()
                            .expect("one exact authorization item"),
                        other => panic!("expected authorization, received {other:?}"),
                    };
                }
                GuardrailPoll::Blocked(GuardrailBlock::Preflight(
                    GuardrailPreflightError::TargetUnavailable { .. },
                )) => {
                    return state
                        .guardrails
                        .borrow_mut()
                        .authorize_deterministic_scope_for_test(deterministic_scope)
                        .expect("deterministic reviewed test scope should authorize");
                }
                other => panic!("expected authorization review, received {other:?}"),
            }
        }
    }

    fn submit_paste(
        state: &ApplicationState,
        destination_directory: &Path,
    ) -> Result<TransferSubmission, CopyInteractionError> {
        let Some((intent, sources)) = state.staged_transfers() else {
            return Err(CopyInteractionError::EmptyBuffer);
        };
        if intent == TransferIntent::Copy {
            return state.submit_paste(destination_directory);
        }
        let source = sources
            .into_iter()
            .next()
            .ok_or(CopyInteractionError::EmptyBuffer)?;
        let request = MoveRequest::new(
            source.clone(),
            transfer_destination(&source, destination_directory)?,
            ConflictPolicy::FailIfExists,
        );
        let authorization = authorize_scope(state, destructive_scope_for_move(&request)?);
        let submission =
            state.submit_move_authorized(GuardrailAuthorized::new(request, authorization))?;
        state.transfer_buffer.borrow_mut().clear_move();
        Ok(submission)
    }

    fn submit_paste_batch(
        state: &ApplicationState,
        destination_directory: &Path,
    ) -> Result<BatchSubmission, CopyInteractionError> {
        let Some((intent, sources)) = state.staged_transfers() else {
            return Err(CopyInteractionError::EmptyBuffer);
        };
        if intent == TransferIntent::Copy {
            return state.submit_paste_batch(destination_directory);
        }
        let guarded = sources
            .into_iter()
            .map(|source| {
                let request = MoveRequest::new(
                    source.clone(),
                    transfer_destination(&source, destination_directory)?,
                    ConflictPolicy::FailIfExists,
                );
                let authorization = authorize_scope(state, destructive_scope_for_move(&request)?);
                Ok(GuardrailAuthorized::new(
                    TrackedOperation::Move(request),
                    authorization,
                ))
            })
            .collect::<Result<Vec<_>, CopyInteractionError>>()?;
        let submission = state.enqueue_authorized_batch(guarded)?;
        state.transfer_buffer.borrow_mut().clear_move();
        Ok(submission)
    }

    fn submit_transfer_batch(
        state: &ApplicationState,
        intent: TransferIntent,
        sources: Vec<PathBuf>,
        destination_directory: &Path,
    ) -> Result<BatchSubmission, CopyInteractionError> {
        if intent == TransferIntent::Copy {
            return state.submit_transfer_batch(intent, sources, destination_directory);
        }
        let guarded = sources
            .into_iter()
            .map(|source| {
                let request = MoveRequest::new(
                    source.clone(),
                    transfer_destination(&source, destination_directory)?,
                    ConflictPolicy::FailIfExists,
                );
                let authorization = authorize_scope(state, destructive_scope_for_move(&request)?);
                Ok(GuardrailAuthorized::new(
                    TrackedOperation::Move(request),
                    authorization,
                ))
            })
            .collect::<Result<Vec<_>, CopyInteractionError>>()?;
        state.enqueue_authorized_batch(guarded)
    }

    fn submit_rename(
        state: &ApplicationState,
        source: PathBuf,
        new_name: OsString,
    ) -> Result<MoveSubmission, CopyInteractionError> {
        let request = RenameRequest::new(source, new_name, ConflictPolicy::FailIfExists);
        let authorization = authorize_scope(state, destructive_scope_for_rename(&request)?);
        state.submit_rename_authorized(GuardrailAuthorized::new(request, authorization))
    }

    fn submit_trash(
        state: &ApplicationState,
        source: PathBuf,
    ) -> Result<TrashSubmission, CopyInteractionError> {
        let request = TrashRequest::new(source)?;
        let authorization = authorize_scope(state, destructive_scope_for_trash(&request)?);
        state.submit_trash_authorized(GuardrailAuthorized::new(request, authorization))
    }

    fn submit_trash_batch(
        state: &ApplicationState,
        sources: Vec<PathBuf>,
    ) -> Result<BatchSubmission, CopyInteractionError> {
        let guarded = sources
            .into_iter()
            .map(|source| {
                let request = TrashRequest::new(source)?;
                let authorization = authorize_scope(state, destructive_scope_for_trash(&request)?);
                Ok(GuardrailAuthorized::new(
                    TrackedOperation::Trash(request),
                    authorization,
                ))
            })
            .collect::<Result<Vec<_>, CopyInteractionError>>()?;
        state.enqueue_authorized_batch(guarded)
    }

    fn submit_drop(
        state: &ApplicationState,
        request: DropRequest,
    ) -> Result<BatchSubmission, CopyInteractionError> {
        if matches!(request.action(), DropAction::Copy | DropAction::Link) {
            return state.submit_drop(request);
        }
        let operations = if matches!(request.destination(), DropDestination::Trash) {
            request
                .sources()
                .iter()
                .cloned()
                .map(TrashRequest::new)
                .map(|request| request.map(TrackedOperation::Trash))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            plan_directory_drop(&request)?
                .into_iter()
                .map(|item| {
                    TrackedOperation::Move(MoveRequest::new(
                        item.source,
                        item.destination,
                        ConflictPolicy::FailIfExists,
                    ))
                })
                .collect()
        };
        let guarded = operations
            .into_iter()
            .map(|operation| {
                let scope = destructive_scope_for_operation(&operation)?.ok_or_else(|| {
                    CopyInteractionError::InvalidSource(operation.source().into())
                })?;
                Ok(GuardrailAuthorized::new(
                    operation,
                    authorize_scope(state, scope),
                ))
            })
            .collect::<Result<Vec<_>, CopyInteractionError>>()?;
        state.enqueue_authorized_batch(guarded)
    }

    fn submit_restore(
        state: &ApplicationState,
        request: RestoreRequest,
    ) -> Result<RestoreSubmission, CopyInteractionError> {
        let authorization = authorize_scope(state, destructive_scope_for_restore(&request)?);
        state.submit_restore_authorized(GuardrailAuthorized::new(request, authorization))
    }

    fn retry_operation(
        state: &ApplicationState,
        job_id: JobId,
    ) -> Result<RetrySubmission, CopyInteractionError> {
        match state.retry_operation_guardrail_scope(job_id)? {
            Some(scope) => state.retry_operation_authorized(job_id, authorize_scope(state, scope)),
            None => state.retry_operation(job_id),
        }
    }

    fn resolve_conflict(
        state: &ApplicationState,
        job_id: JobId,
        decision: ConflictDecision,
    ) -> Result<ConflictResolution, CopyInteractionError> {
        match state.conflict_guardrail_scope(job_id, &decision)? {
            Some(scope) => {
                state.resolve_conflict_authorized(job_id, decision, authorize_scope(state, scope))
            }
            None => state.resolve_conflict(job_id, decision),
        }
    }

    fn wait_for_terminal(state: &ApplicationState, job_id: JobId) -> JobState {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(job_state) = lock(&state.jobs)
                .record(job_id)
                .map(|record| record.state())
                && job_state.is_terminal()
            {
                return job_state;
            }
            assert!(
                Instant::now() < deadline,
                "copy job did not become terminal"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn phase_10d_permission_state_tracks_lifecycle_and_affected_parent() {
        let fixture = tempdir().expect("fixture");
        let target = fixture.path().join("item");
        fs::write(&target, b"floe").expect("target");
        let request = PermissionRequest::new(
            vec![target.clone()],
            PermissionScope::Direct,
            PermissionChange::new(Some(0o600), None, None, None, None).expect("change"),
        )
        .expect("request");
        let state = ApplicationState::new().expect("application state");
        let submission = state.submit_permissions(request).expect("submission");
        assert_eq!(
            state.permission_affected_directories(submission.job_id()),
            vec![fixture.path().to_path_buf()]
        );
        assert_eq!(
            wait_for_terminal(&state, submission.job_id()),
            JobState::Completed
        );
        assert!(state.drain_job_events().iter().any(|event| {
            event.job_id() == submission.job_id() && event.kind() == &JobEventKind::Completed
        }));
        state.finish_permission(submission.job_id());
        assert!(
            state
                .permission_affected_directories(submission.job_id())
                .is_empty()
        );
    }

    #[test]
    fn phase_4d_copy_stages_original_path_and_builds_exact_destination() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source_directory = fixture.path().join("source");
        let destination_directory = fixture.path().join("destination");
        fs::create_dir(&source_directory).expect("source directory should be creatable");
        fs::create_dir(&destination_directory).expect("destination directory should be creatable");
        let name = OsString::from_vec(b"copy-\xff".to_vec());
        let source = source_directory.join(&name);
        fs::write(&source, b"floe").expect("source fixture should be writable");
        let state = ApplicationState::new().expect("application state should start");

        state
            .stage_copy(source.clone())
            .expect("source should be staged");
        let submission = state
            .submit_paste(&destination_directory)
            .expect("paste should be submitted");

        assert_eq!(
            wait_for_terminal(&state, submission.job_id()),
            JobState::Completed
        );
        let copied = fs::read_dir(&destination_directory)
            .expect("destination should be readable")
            .next()
            .expect("destination should contain copied item")
            .expect("copied entry should be readable");
        assert_eq!(copied.file_name().as_bytes(), name.as_bytes());
        assert_eq!(
            fs::read(copied.path()).expect("copy should be readable"),
            b"floe"
        );
    }

    #[test]
    fn phase_4d_rejects_paste_without_staged_source() {
        let fixture = tempdir().expect("temporary directory should be available");
        let state = ApplicationState::new().expect("application state should start");

        let error = state
            .submit_paste(fixture.path())
            .expect_err("empty copy buffer must be rejected");

        assert!(matches!(error, CopyInteractionError::EmptyBuffer));
    }

    #[test]
    fn phase_4d_rejects_destination_inside_staged_folder() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let nested_destination = source.join("nested");
        fs::create_dir(&source).expect("source directory should be creatable");
        fs::create_dir(&nested_destination).expect("nested directory should be creatable");
        let state = ApplicationState::new().expect("application state should start");
        state.stage_copy(source).expect("source should be staged");

        let error = state
            .submit_paste(&nested_destination)
            .expect_err("paste into copied folder must be rejected");

        assert!(matches!(
            error,
            CopyInteractionError::DestinationInsideSource
        ));
    }

    #[test]
    fn phase_4d_surfaces_conflict_failure_event() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source_directory = fixture.path().join("source");
        let destination_directory = fixture.path().join("destination");
        fs::create_dir(&source_directory).expect("source directory should be creatable");
        fs::create_dir(&destination_directory).expect("destination directory should be creatable");
        let source = source_directory.join("item");
        fs::write(&source, b"new").expect("source fixture should be writable");
        fs::write(destination_directory.join("item"), b"keep")
            .expect("conflict fixture should be writable");
        let state = ApplicationState::new().expect("application state should start");
        state.stage_copy(source).expect("source should be staged");

        let submission = state
            .submit_paste(&destination_directory)
            .expect("conflicting paste should still be submitted");

        assert_eq!(
            wait_for_terminal(&state, submission.job_id()),
            JobState::Failed
        );
        let events = state.drain_job_events();
        assert!(events.iter().any(|event| {
            event.job_id() == submission.job_id()
                && matches!(
                    event.kind(),
                    JobEventKind::Failed(failure)
                        if failure.kind() == JobFailureKind::Conflict
                )
        }));
    }

    #[test]
    fn phase_4d_maps_cancellation_and_success_lifecycle() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source_directory = fixture.path().join("source");
        let destination_directory = fixture.path().join("destination");
        fs::create_dir(&source_directory).expect("source directory should be creatable");
        fs::create_dir(&destination_directory).expect("destination directory should be creatable");
        let source = source_directory.join("item");
        fs::write(&source, b"content").expect("source fixture should be writable");
        let state = ApplicationState::new().expect("application state should start");
        state
            .stage_copy(source.clone())
            .expect("source should be staged");
        let cancellation = CopyCancellation::new();
        cancellation.cancel();

        let cancelled = state
            .submit_paste_with_cancellation(&destination_directory, cancellation)
            .expect("cancelled paste should be submitted");
        assert_eq!(
            wait_for_terminal(&state, cancelled.job_id()),
            JobState::Cancelled
        );

        let completed_directory = fixture.path().join("completed");
        fs::create_dir(&completed_directory).expect("completion directory should be creatable");
        let completed = state
            .submit_paste(&completed_directory)
            .expect("second paste should be submitted");
        assert_eq!(
            wait_for_terminal(&state, completed.job_id()),
            JobState::Completed
        );

        let events = state.drain_job_events();
        assert!(events.iter().any(|event| {
            event.job_id() == cancelled.job_id() && event.kind() == &JobEventKind::Cancelled
        }));
        assert!(events.iter().any(|event| {
            event.job_id() == completed.job_id() && event.kind() == &JobEventKind::Completed
        }));
    }

    #[test]
    fn phase_4d_move_replaces_copy_and_rename_preserves_original_paths() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source_directory = fixture.path().join("source");
        let destination_directory = fixture.path().join("destination");
        fs::create_dir(&source_directory).expect("source directory should be creatable");
        fs::create_dir(&destination_directory).expect("destination directory should be creatable");
        let original_name = OsString::from_vec(b"move-\xff".to_vec());
        let source = source_directory.join(&original_name);
        fs::write(&source, b"move").expect("source fixture should be writable");
        let state = ApplicationState::new().expect("application state should start");
        state
            .stage_copy(source.clone())
            .expect("copy should be staged first");
        state
            .stage_move(source.clone())
            .expect("move should replace staged copy");
        assert_eq!(
            state.staged_transfer(),
            Some((TransferIntent::Move, source.clone()))
        );

        let moved =
            submit_paste(&state, &destination_directory).expect("move paste should be submitted");
        assert_eq!(
            wait_for_terminal(&state, moved.job_id()),
            JobState::Completed
        );
        let moved_path = destination_directory.join(&original_name);
        assert!(!source.exists());
        assert_eq!(fs::read(&moved_path).expect("move should finish"), b"move");
        let tracked = state
            .operation_request(moved.job_id())
            .expect("move request should remain observable");
        assert!(matches!(tracked, TrackedOperation::Move(_)));
        assert_eq!(tracked.affected_directories().len(), 2);
        state.finish_operation(moved.job_id(), TerminalOutcome::Completed);
        assert_eq!(state.staged_transfer(), None);

        let renamed_name = OsString::from_vec(b"renamed-\xfe".to_vec());
        let renamed = submit_rename(&state, moved_path.clone(), renamed_name.clone())
            .expect("rename should be submitted");
        assert_eq!(
            wait_for_terminal(&state, renamed.job_id()),
            JobState::Completed
        );
        assert!(!moved_path.exists());
        assert_eq!(
            fs::read(destination_directory.join(&renamed_name))
                .expect("renamed path should remain byte-exact"),
            b"move"
        );
        assert!(validate_rename_name(OsStr::new("nested/name")).is_err());
    }

    #[derive(Debug)]
    struct SuccessfulTrashBackend;

    impl TrashBackend for SuccessfulTrashBackend {
        fn trash(
            &self,
            _request: &TrashRequest,
            _cancellable: &gtk::gio::Cancellable,
        ) -> Result<(), TrashError> {
            Ok(())
        }
    }

    fn wait_for_guardrail_policy_update(state: &ApplicationState) -> Result<bool, String> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(result) = state.poll_guardrail_policy_update() {
                return result;
            }
            assert!(Instant::now() < deadline, "guardrail policy save timed out");
            thread::yield_now();
        }
    }

    #[test]
    fn phase_18x_policy_change_is_busy_until_persisted_and_preserves_raw_path() {
        let fixture = tempdir().expect("guardrail fixture");
        let store = fixture.path().join("private/guardrails.bin");
        let state = ApplicationState::new_with_trash_backend_and_guardrail_store(
            Arc::new(SuccessfulTrashBackend),
            store.clone(),
        )
        .expect("application state");
        let target = fixture
            .path()
            .join(OsString::from_vec(b"protected-\xff".to_vec()));

        assert!(
            state
                .submit_guardrail_protection_change(target.clone(), true)
                .expect("queue protection")
        );
        assert!(state.guardrail_policy_busy());
        assert!(
            state.guardrail_policy().roots().is_empty(),
            "in-memory policy must not install before persistence succeeds"
        );
        assert!(
            state
                .submit_guardrail_protection_change(target.clone(), true)
                .expect_err("one pending update at a time")
                .contains("still running")
        );

        assert_eq!(wait_for_guardrail_policy_update(&state), Ok(false));
        assert!(!state.guardrail_policy_busy());
        assert_eq!(
            state.guardrail_policy().roots(),
            std::slice::from_ref(&target)
        );
        let persisted = GuardrailStore::load(&store)
            .expect("read persisted guardrail store")
            .expect("persisted guardrail policy");
        assert_eq!(
            persisted.roots()[0].as_os_str().as_bytes(),
            target.as_os_str().as_bytes()
        );
    }

    #[test]
    fn phase_18x_failed_policy_persistence_never_installs_requested_policy() {
        let fixture = tempdir().expect("guardrail fixture");
        let blocked_parent = fixture.path().join("private");
        fs::create_dir(&blocked_parent).expect("initial valid store parent");
        fs::set_permissions(&blocked_parent, fs::Permissions::from_mode(0o700))
            .expect("private permissions");
        let state = ApplicationState::new_with_trash_backend_and_guardrail_store(
            Arc::new(SuccessfulTrashBackend),
            blocked_parent.join("guardrails.bin"),
        )
        .expect("application state");
        fs::remove_dir(&blocked_parent).expect("replace empty parent");
        fs::write(&blocked_parent, b"file").expect("blocking parent file");
        let target = fixture.path().join("protected");

        assert!(
            state
                .submit_guardrail_protection_change(target, true)
                .expect("queue protection")
        );
        let error = wait_for_guardrail_policy_update(&state)
            .expect_err("persistence through a regular-file parent must fail");
        assert!(!error.is_empty());
        assert!(!state.guardrail_policy_busy());
        assert!(state.guardrail_policy().roots().is_empty());
    }

    #[test]
    fn phase_18x_blocked_store_requires_acknowledged_async_reset() {
        let fixture = tempdir().expect("guardrail fixture");
        let private = fixture.path().join("private");
        fs::create_dir(&private).expect("private directory");
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700))
            .expect("private permissions");
        let store = private.join("guardrails.bin");
        fs::write(&store, b"corrupt guardrail store").expect("corrupt store");
        fs::set_permissions(&store, fs::Permissions::from_mode(0o600)).expect("store permissions");
        let state = ApplicationState::new_with_trash_backend_and_guardrail_store(
            Arc::new(SuccessfulTrashBackend),
            store.clone(),
        )
        .expect("application state");

        assert_eq!(
            state.guardrail_store_health(),
            GuardrailStoreHealth::Blocked
        );
        assert!(
            state
                .submit_guardrail_protection_change(fixture.path().join("target"), true)
                .is_err()
        );
        assert!(state.submit_guardrail_blocked_reset(false).is_err());
        state
            .submit_guardrail_blocked_reset(true)
            .expect("acknowledged reset queues");
        assert!(state.guardrail_policy_busy());
        assert_eq!(
            state.guardrail_store_health(),
            GuardrailStoreHealth::Blocked
        );
        assert!(state.submit_guardrail_blocked_reset(true).is_err());

        assert_eq!(wait_for_guardrail_policy_update(&state), Ok(true));
        assert_eq!(state.guardrail_store_health(), GuardrailStoreHealth::Ready);
        assert!(state.guardrail_policy().roots().is_empty());
        assert!(
            GuardrailStore::load(&store)
                .expect("read reset store")
                .expect("reset policy")
                .roots()
                .is_empty()
        );
    }

    #[derive(Debug, Default)]
    struct RetryTrashBackend {
        attempts: AtomicUsize,
    }

    impl TrashBackend for RetryTrashBackend {
        fn trash(
            &self,
            _request: &TrashRequest,
            _cancellable: &gtk::gio::Cancellable,
        ) -> Result<(), TrashError> {
            if self.attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(TrashError::Io {
                    message: "first attempt fails".to_owned(),
                })
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn phase_4e_state_tracks_original_trash_path_and_parent() {
        let fixture = tempdir().expect("temporary directory should be available");
        let name = OsString::from_vec(b"trash-state-\xff".to_vec());
        let source = fixture.path().join(&name);
        let state = ApplicationState::new_with_trash_backend(Arc::new(SuccessfulTrashBackend))
            .expect("application state should start with test trash backend");
        let submission =
            submit_trash(&state, source.clone()).expect("trash request should be submitted");

        assert_eq!(
            wait_for_terminal(&state, submission.job_id()),
            JobState::Completed
        );
        let tracked = state
            .operation_request(submission.job_id())
            .expect("trash request should remain observable");
        assert_eq!(tracked.source(), source);
        assert_eq!(
            tracked.affected_directories(),
            vec![fixture.path().to_path_buf()]
        );
        assert!(matches!(tracked, TrackedOperation::Trash(_)));
    }

    #[test]
    fn phase_5a_retries_copy_move_and_rename_with_stable_operation_identity() {
        let fixture = tempdir().expect("temporary directory should be available");
        let state = ApplicationState::new().expect("application state should start");

        let copy_source_dir = fixture.path().join("copy-source");
        let copy_destination_dir = fixture.path().join("copy-destination");
        fs::create_dir(&copy_source_dir).expect("copy source directory");
        fs::create_dir(&copy_destination_dir).expect("copy destination directory");
        let copy_source = copy_source_dir.join("copy-item");
        fs::write(&copy_source, b"copy").expect("copy source fixture");
        state.stage_copy(copy_source).expect("copy should stage");
        let cancellation = CopyCancellation::new();
        cancellation.cancel();
        let cancelled_copy = state
            .submit_paste_with_cancellation(&copy_destination_dir, cancellation)
            .expect("cancelled copy should submit");
        assert_eq!(
            wait_for_terminal(&state, cancelled_copy.job_id()),
            JobState::Cancelled
        );
        state.finish_operation(cancelled_copy.job_id(), TerminalOutcome::Cancelled);
        let retried_copy =
            retry_operation(&state, cancelled_copy.job_id()).expect("cancelled copy should retry");
        assert_eq!(retried_copy.operation_id(), cancelled_copy.operation_id());
        assert_ne!(retried_copy.job_id(), cancelled_copy.job_id());
        assert_eq!(
            wait_for_terminal(&state, retried_copy.job_id()),
            JobState::Completed
        );

        let move_source_dir = fixture.path().join("move-source");
        let move_destination_dir = fixture.path().join("move-destination");
        fs::create_dir(&move_source_dir).expect("move source directory");
        fs::create_dir(&move_destination_dir).expect("move destination directory");
        let move_source = move_source_dir.join("move-item");
        let move_conflict = move_destination_dir.join("move-item");
        fs::write(&move_source, b"move").expect("move source fixture");
        fs::write(&move_conflict, b"conflict").expect("move conflict fixture");
        state.stage_move(move_source).expect("move should stage");
        let failed_move = submit_paste(&state, &move_destination_dir).expect("move should submit");
        assert_eq!(
            wait_for_terminal(&state, failed_move.job_id()),
            JobState::Failed
        );
        state.finish_operation(failed_move.job_id(), TerminalOutcome::Failed);
        fs::remove_file(&move_conflict).expect("move conflict should be removable");
        let retried_move =
            retry_operation(&state, failed_move.job_id()).expect("failed move should retry");
        assert_eq!(retried_move.operation_id(), failed_move.operation_id());
        assert_ne!(retried_move.job_id(), failed_move.job_id());
        assert_eq!(
            wait_for_terminal(&state, retried_move.job_id()),
            JobState::Completed
        );

        let rename_source = fixture.path().join("rename-source");
        let rename_conflict = fixture.path().join("rename-target");
        fs::write(&rename_source, b"rename").expect("rename source fixture");
        fs::write(&rename_conflict, b"conflict").expect("rename conflict fixture");
        let failed_rename = submit_rename(&state, rename_source, OsString::from("rename-target"))
            .expect("rename should submit");
        assert_eq!(
            wait_for_terminal(&state, failed_rename.job_id()),
            JobState::Failed
        );
        state.finish_operation(failed_rename.job_id(), TerminalOutcome::Failed);
        fs::remove_file(&rename_conflict).expect("rename conflict should be removable");
        let retried_rename =
            retry_operation(&state, failed_rename.job_id()).expect("failed rename should retry");
        assert_eq!(retried_rename.operation_id(), failed_rename.operation_id());
        assert_ne!(retried_rename.job_id(), failed_rename.job_id());
        assert_eq!(
            wait_for_terminal(&state, retried_rename.job_id()),
            JobState::Completed
        );
    }

    #[test]
    fn phase_5a_retries_failed_trash_with_original_non_utf8_path() {
        let backend = Arc::new(RetryTrashBackend::default());
        let state = ApplicationState::new_with_trash_backend(backend.clone())
            .expect("application state should start");
        let source = PathBuf::from("/virtual").join(OsString::from_vec(b"retry-\xff".to_vec()));
        let failed = submit_trash(&state, source.clone()).expect("trash should submit");
        assert_eq!(wait_for_terminal(&state, failed.job_id()), JobState::Failed);
        state.finish_operation(failed.job_id(), TerminalOutcome::Failed);

        let retried = retry_operation(&state, failed.job_id()).expect("failed trash should retry");
        assert_eq!(retried.operation_id(), failed.operation_id());
        assert_ne!(retried.job_id(), failed.job_id());
        assert_eq!(
            wait_for_terminal(&state, retried.job_id()),
            JobState::Completed
        );
        assert_eq!(
            state
                .operation_request(retried.job_id())
                .expect("retry request should be tracked")
                .source(),
            source
        );
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn phase_5a_terminal_history_is_bounded_and_completed_retry_is_rejected() {
        let state = ApplicationState::new_with_trash_backend(Arc::new(SuccessfulTrashBackend))
            .expect("application state should start");
        let mut first_job = None;
        let mut last_job = None;
        for index in 0..=MAX_TERMINAL_HISTORY {
            let submission =
                submit_trash(&state, PathBuf::from(format!("/virtual/history-{index}")))
                    .expect("history trash should submit");
            assert_eq!(
                wait_for_terminal(&state, submission.job_id()),
                JobState::Completed
            );
            state.finish_operation(submission.job_id(), TerminalOutcome::Completed);
            first_job.get_or_insert(submission.job_id());
            last_job = Some(submission.job_id());
        }

        let history = state.terminal_history();
        assert_eq!(history.len(), MAX_TERMINAL_HISTORY);
        assert!(
            !history
                .iter()
                .any(|entry| entry.job_id() == first_job.expect("first job"))
        );
        assert!(
            lock(&state.jobs)
                .record(first_job.expect("first job"))
                .is_none()
        );
        assert!(matches!(
            retry_operation(&state, first_job.expect("first job")),
            Err(CopyInteractionError::RetryNotFound(_))
        ));
        assert!(matches!(
            retry_operation(&state, last_job.expect("last job")),
            Err(CopyInteractionError::RetryCompleted(_))
        ));
    }

    #[test]
    fn phase_5e_copy_conflict_requires_a_decision_and_retries_with_raw_name() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source_directory = fixture.path().join("source");
        let destination_directory = fixture.path().join("destination");
        fs::create_dir(&source_directory).expect("source directory");
        fs::create_dir(&destination_directory).expect("destination directory");
        let source = source_directory.join("item");
        let destination = destination_directory.join("item");
        fs::write(&source, b"new").expect("source fixture");
        fs::write(&destination, b"keep").expect("conflict fixture");

        let state = ApplicationState::new().expect("application state should start");
        state.stage_copy(source.clone()).expect("copy should stage");
        let failed = state
            .submit_paste(&destination_directory)
            .expect("conflicting copy should submit");
        assert_eq!(wait_for_terminal(&state, failed.job_id()), JobState::Failed);
        state.finish_operation(failed.job_id(), TerminalOutcome::Conflict);

        let pending = state
            .pending_conflict(failed.job_id())
            .expect("copy conflict should be pending");
        assert_eq!(pending.job_id(), failed.job_id());
        assert_eq!(pending.operation_id(), failed.operation_id());
        assert_eq!(pending.source(), source);
        assert_eq!(pending.destination(), destination);
        assert!(matches!(
            retry_operation(&state, failed.job_id()),
            Err(CopyInteractionError::ConflictDecisionRequired(job_id))
                if job_id == failed.job_id()
        ));

        assert!(matches!(
            resolve_conflict(
                &state,
                failed.job_id(),
                ConflictDecision::RetryWithName(OsString::from("nested/name")),
            ),
            Err(CopyInteractionError::InvalidRenameName)
        ));
        assert!(state.pending_conflict(failed.job_id()).is_ok());

        let revised_name = OsString::from_vec(b"item-\xff".to_vec());
        let resolution = resolve_conflict(
            &state,
            failed.job_id(),
            ConflictDecision::RetryWithName(revised_name.clone()),
        )
        .expect("valid revised copy should submit");
        let ConflictResolution::Retried(retried) = resolution else {
            panic!("copy conflict should create a revised retry");
        };
        assert_eq!(retried.operation_id(), failed.operation_id());
        assert_ne!(retried.job_id(), failed.job_id());
        assert_eq!(
            wait_for_terminal(&state, retried.job_id()),
            JobState::Completed
        );
        assert_eq!(fs::read(&destination).expect("existing item"), b"keep");
        assert_eq!(
            fs::read(destination_directory.join(&revised_name)).expect("revised copy"),
            b"new"
        );
        assert!(matches!(
            resolve_conflict(&state, failed.job_id(), ConflictDecision::KeepExisting),
            Err(CopyInteractionError::ConflictAlreadyResolved(job_id))
                if job_id == failed.job_id()
        ));
    }

    #[test]
    fn phase_5e_move_and_rename_conflicts_retry_as_siblings_without_overwrite() {
        let fixture = tempdir().expect("temporary directory should be available");
        let state = ApplicationState::new().expect("application state should start");

        let move_source_directory = fixture.path().join("move-source");
        let move_destination_directory = fixture.path().join("move-destination");
        fs::create_dir(&move_source_directory).expect("move source directory");
        fs::create_dir(&move_destination_directory).expect("move destination directory");
        let move_source = move_source_directory.join("item");
        let move_destination = move_destination_directory.join("item");
        fs::write(&move_source, b"move-new").expect("move source fixture");
        fs::write(&move_destination, b"move-keep").expect("move conflict fixture");
        state
            .stage_move(move_source.clone())
            .expect("move should stage");
        let failed_move = submit_paste(&state, &move_destination_directory)
            .expect("conflicting move should submit");
        assert_eq!(
            wait_for_terminal(&state, failed_move.job_id()),
            JobState::Failed
        );
        state.finish_operation(failed_move.job_id(), TerminalOutcome::Conflict);
        let moved_name = OsString::from("moved-item");
        let ConflictResolution::Retried(retried_move) = resolve_conflict(
            &state,
            failed_move.job_id(),
            ConflictDecision::RetryWithName(moved_name.clone()),
        )
        .expect("revised move should submit") else {
            panic!("move conflict should create a revised retry");
        };
        assert_eq!(retried_move.operation_id(), failed_move.operation_id());
        assert_eq!(
            wait_for_terminal(&state, retried_move.job_id()),
            JobState::Completed
        );
        assert!(!move_source.exists());
        assert_eq!(
            fs::read(&move_destination).expect("existing move item"),
            b"move-keep"
        );
        assert_eq!(
            fs::read(move_destination_directory.join(moved_name)).expect("revised move"),
            b"move-new"
        );

        let rename_source = fixture.path().join("rename-source");
        let rename_destination = fixture.path().join("rename-target");
        fs::write(&rename_source, b"rename-new").expect("rename source fixture");
        fs::write(&rename_destination, b"rename-keep").expect("rename conflict fixture");
        let failed_rename = submit_rename(
            &state,
            rename_source.clone(),
            OsString::from("rename-target"),
        )
        .expect("conflicting rename should submit");
        assert_eq!(
            wait_for_terminal(&state, failed_rename.job_id()),
            JobState::Failed
        );
        state.finish_operation(failed_rename.job_id(), TerminalOutcome::Conflict);
        let pending = state
            .pending_conflict(failed_rename.job_id())
            .expect("rename conflict should be pending");
        assert_eq!(pending.source(), rename_source);
        assert_eq!(pending.destination(), rename_destination);
        let ConflictResolution::Retried(retried_rename) = resolve_conflict(
            &state,
            failed_rename.job_id(),
            ConflictDecision::RetryWithName(OsString::from("renamed-item")),
        )
        .expect("revised rename should submit") else {
            panic!("rename conflict should create a revised retry");
        };
        assert_eq!(retried_rename.operation_id(), failed_rename.operation_id());
        assert_eq!(
            wait_for_terminal(&state, retried_rename.job_id()),
            JobState::Completed
        );
        assert_eq!(
            fs::read(&rename_destination).expect("existing rename item"),
            b"rename-keep"
        );
        assert_eq!(
            fs::read(fixture.path().join("renamed-item")).expect("revised rename"),
            b"rename-new"
        );
    }

    #[test]
    fn phase_5e_keep_existing_submits_nothing_and_is_single_use() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source_directory = fixture.path().join("source");
        let destination_directory = fixture.path().join("destination");
        fs::create_dir(&source_directory).expect("source directory");
        fs::create_dir(&destination_directory).expect("destination directory");
        let source = source_directory.join("item");
        let destination = destination_directory.join("item");
        fs::write(&source, b"new").expect("source fixture");
        fs::write(&destination, b"keep").expect("conflict fixture");
        let state = ApplicationState::new().expect("application state should start");
        state.stage_copy(source.clone()).expect("copy should stage");
        let failed = state
            .submit_paste(&destination_directory)
            .expect("conflicting copy should submit");
        assert_eq!(wait_for_terminal(&state, failed.job_id()), JobState::Failed);
        state.drain_job_events();
        state.finish_operation(failed.job_id(), TerminalOutcome::Conflict);

        assert_eq!(
            state
                .resolve_conflict(failed.job_id(), ConflictDecision::KeepExisting)
                .expect("keep-existing should resolve conflict"),
            ConflictResolution::KeptExisting
        );
        assert!(state.drain_job_events().is_empty());
        assert_eq!(state.terminal_history().len(), 1);
        assert_eq!(fs::read(source).expect("source remains"), b"new");
        assert_eq!(fs::read(destination).expect("destination remains"), b"keep");
        assert!(matches!(
            state.pending_conflict(failed.job_id()),
            Err(CopyInteractionError::ConflictAlreadyResolved(job_id))
                if job_id == failed.job_id()
        ));
    }

    #[test]
    fn phase_5e_trash_conflicts_are_unsupported() {
        let state = ApplicationState::new_with_trash_backend(Arc::new(SuccessfulTrashBackend))
            .expect("application state should start");
        let submission =
            submit_trash(&state, PathBuf::from("/virtual/conflict")).expect("trash should submit");
        assert_eq!(
            wait_for_terminal(&state, submission.job_id()),
            JobState::Completed
        );
        state.finish_operation(submission.job_id(), TerminalOutcome::Conflict);

        assert!(matches!(
            state.pending_conflict(submission.job_id()),
            Err(CopyInteractionError::ConflictUnsupported(job_id))
                if job_id == submission.job_id()
        ));
        assert!(matches!(
            state.resolve_conflict(submission.job_id(), ConflictDecision::KeepExisting),
            Err(CopyInteractionError::ConflictUnsupported(job_id))
                if job_id == submission.job_id()
        ));
    }

    #[test]
    fn phase_6j_multi_copy_batch_runs_beyond_worker_queue_capacity() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source_directory = fixture.path().join("source");
        let destination_directory = fixture.path().join("destination");
        fs::create_dir(&source_directory).expect("source directory should be creatable");
        fs::create_dir(&destination_directory).expect("destination directory should be creatable");

        let sources = (0..12)
            .map(|index| {
                let path = source_directory.join(format!("item-{index}"));
                fs::write(&path, format!("content-{index}"))
                    .expect("source fixture should be writable");
                path
            })
            .collect::<Vec<_>>();
        let state = ApplicationState::new().expect("application state should start");
        state
            .stage_copy_many(sources.clone())
            .expect("all exact paths should stage");
        let batch =
            submit_paste_batch(&state, &destination_directory).expect("batch should be accepted");
        assert_eq!(batch.queued(), sources.len());

        for _ in &sources {
            let job_id = state
                .batch_active
                .get()
                .expect("one bounded batch job should be active");
            assert_eq!(wait_for_terminal(&state, job_id), JobState::Completed);
            state.finish_operation(job_id, TerminalOutcome::Completed);
        }
        assert!(state.batch_pending.borrow().is_empty());
        assert!(state.batch_active.get().is_none());
        for source in sources {
            let name = source.file_name().expect("fixture has a filename");
            assert_eq!(
                fs::read(destination_directory.join(name)).expect("batch output should exist"),
                fs::read(source).expect("source should remain")
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn phase_6j_transfer_buffer_deduplicates_exact_non_utf8_paths() {
        let first = PathBuf::from(OsString::from_vec(b"first-\xff".to_vec()));
        let second = PathBuf::from(OsString::from_vec(b"second-\xfe".to_vec()));
        let state = ApplicationState::new().expect("application state should start");
        state
            .stage_move_many(vec![first.clone(), second.clone(), first.clone()])
            .expect("valid exact paths should stage");

        assert_eq!(
            state.staged_transfers(),
            Some((TransferIntent::Move, vec![first, second]))
        );
    }

    #[test]
    fn phase_6j_multi_move_batch_moves_every_source_and_clears_cut_buffer() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source_directory = fixture.path().join("source");
        let destination_directory = fixture.path().join("destination");
        fs::create_dir(&source_directory).expect("source directory should be creatable");
        fs::create_dir(&destination_directory).expect("destination directory should be creatable");
        let sources = (0..3)
            .map(|index| {
                let source = source_directory.join(format!("move-{index}"));
                fs::write(&source, format!("move-content-{index}"))
                    .expect("move fixture should be writable");
                source
            })
            .collect::<Vec<_>>();
        let state = ApplicationState::new().expect("application state should start");
        state
            .stage_move_many(sources.clone())
            .expect("move paths should stage");
        let batch =
            submit_paste_batch(&state, &destination_directory).expect("move batch should queue");
        assert_eq!(batch.queued(), sources.len());
        assert_eq!(state.staged_transfers(), None);

        for _ in &sources {
            let job_id = state
                .batch_active
                .get()
                .expect("batch job should be active");
            assert_eq!(wait_for_terminal(&state, job_id), JobState::Completed);
            state.finish_operation(job_id, TerminalOutcome::Completed);
        }
        for source in sources {
            assert!(!source.exists());
            assert!(
                destination_directory
                    .join(source.file_name().expect("move fixture has a filename"))
                    .exists()
            );
        }
    }

    #[test]
    fn phase_6j_multi_trash_batch_dispatches_every_exact_path() {
        let sources = (0..3)
            .map(|index| PathBuf::from(format!("/virtual/trash-{index}")))
            .collect::<Vec<_>>();
        let state = ApplicationState::new_with_trash_backend(Arc::new(SuccessfulTrashBackend))
            .expect("application state should start");
        let batch = submit_trash_batch(&state, sources.clone()).expect("trash batch should queue");
        assert_eq!(batch.queued(), sources.len());

        for _ in &sources {
            let job_id = state
                .batch_active
                .get()
                .expect("batch job should be active");
            assert_eq!(wait_for_terminal(&state, job_id), JobState::Completed);
            state.finish_operation(job_id, TerminalOutcome::Completed);
        }
        let completed_sources = state
            .terminal_history()
            .into_iter()
            .map(|entry| entry.operation().source().to_path_buf())
            .collect::<HashSet<_>>();
        assert!(
            sources
                .iter()
                .all(|source| completed_sources.contains(source))
        );
    }

    #[test]
    fn phase_6m_partial_permanent_delete_cannot_be_retried() {
        let state = ApplicationState::new().expect("application state should start");
        let queued = lock(&state.jobs)
            .queue_operation()
            .expect("fixture job should queue");
        let request = PermanentDeleteRequest::new(vec![PathBuf::from("/virtual/item")])
            .expect("fixture request should be valid");
        state.track(queued.job_id(), TrackedOperation::PermanentDelete(request));
        state.finish_operation(queued.job_id(), TerminalOutcome::PartialFailure);

        assert!(matches!(
            retry_operation(&state, queued.job_id()),
            Err(CopyInteractionError::RetryUnsafePartial(job_id)) if job_id == queued.job_id()
        ));
    }

    #[test]
    fn phase_5e_terminal_eviction_clears_conflict_resolution_bookkeeping() {
        let state = ApplicationState::new_with_trash_backend(Arc::new(SuccessfulTrashBackend))
            .expect("application state should start");
        let fixture = tempdir().expect("temporary directory should be available");
        let source_directory = fixture.path().join("source");
        let destination_directory = fixture.path().join("destination");
        fs::create_dir(&source_directory).expect("source directory");
        fs::create_dir(&destination_directory).expect("destination directory");
        let source = source_directory.join("item");
        fs::write(&source, b"new").expect("source fixture");
        fs::write(destination_directory.join("item"), b"keep").expect("conflict fixture");
        state.stage_copy(source).expect("copy should stage");
        let conflict = state
            .submit_paste(&destination_directory)
            .expect("conflicting copy should submit");
        assert_eq!(
            wait_for_terminal(&state, conflict.job_id()),
            JobState::Failed
        );
        state.finish_operation(conflict.job_id(), TerminalOutcome::Conflict);
        assert_eq!(
            state
                .resolve_conflict(conflict.job_id(), ConflictDecision::KeepExisting)
                .expect("keep-existing should resolve conflict"),
            ConflictResolution::KeptExisting
        );

        for index in 0..MAX_TERMINAL_HISTORY {
            let submission = submit_trash(&state, PathBuf::from(format!("/virtual/evict-{index}")))
                .expect("trash should submit");
            assert_eq!(
                wait_for_terminal(&state, submission.job_id()),
                JobState::Completed
            );
            state.finish_operation(submission.job_id(), TerminalOutcome::Completed);
        }

        assert!(
            !state
                .resolved_conflicts
                .borrow()
                .contains(&conflict.job_id())
        );
        assert!(matches!(
            state.pending_conflict(conflict.job_id()),
            Err(CopyInteractionError::ConflictNotFound(job_id))
                if job_id == conflict.job_id()
        ));
    }

    #[test]
    fn phase_6p_state_batch_pauses_at_item_boundaries_resumes_fifo_and_completes() {
        let fixture = tempdir().expect("temporary fixture");
        let source_directory = fixture.path().join("source");
        let destination_directory = fixture.path().join("destination");
        fs::create_dir(&source_directory).expect("source directory");
        fs::create_dir(&destination_directory).expect("destination directory");
        let sources = (0..3)
            .map(|index| {
                let path = source_directory.join(format!("item-{index}"));
                fs::write(&path, format!("payload-{index}")).expect("source payload");
                path
            })
            .collect::<Vec<_>>();
        let state = ApplicationState::new().expect("application state");
        state
            .stage_copy_many(sources.clone())
            .expect("copy batch should stage");
        let batch =
            submit_paste_batch(&state, &destination_directory).expect("copy batch should submit");
        state.pause_batch(batch.id()).expect("batch should pause");

        let first = state
            .batch_active
            .get()
            .expect("first item should be active");
        assert_eq!(wait_for_terminal(&state, first), JobState::Completed);
        state.finish_operation(first, TerminalOutcome::Completed);
        let paused = state.batch_snapshot(batch.id()).expect("batch snapshot");
        assert_eq!(paused.status(), BatchStatus::Paused);
        assert_eq!(paused.completed(), 1);
        assert_eq!(paused.remaining(), 2);
        assert!(state.batch_active.get().is_none());

        state.resume_batch(batch.id()).expect("batch should resume");
        while let Some(job_id) = state.batch_active.get() {
            assert_eq!(wait_for_terminal(&state, job_id), JobState::Completed);
            state.finish_operation(job_id, TerminalOutcome::Completed);
        }
        let completed = state.batch_snapshot(batch.id()).expect("batch snapshot");
        assert_eq!(completed.status(), BatchStatus::Completed);
        assert_eq!(completed.completed(), sources.len());
        let completed_sources = state
            .terminal_history()
            .into_iter()
            .filter(|entry| entry.batch_id() == Some(batch.id()))
            .map(|entry| entry.operation().source().to_path_buf())
            .collect::<Vec<_>>();
        assert_eq!(completed_sources, sources);
    }

    #[test]
    fn phase_6p_conflict_keep_both_and_batch_skip_all_never_replace() {
        let fixture = tempdir().expect("temporary fixture");
        let destination = fixture.path().join("destination");
        let first_source = fixture.path().join("first");
        let second_source = fixture.path().join("second");
        let third_source = fixture.path().join("third");
        fs::create_dir(&destination).expect("destination directory");
        fs::create_dir(&first_source).expect("first source directory");
        fs::create_dir(&second_source).expect("second source directory");
        fs::create_dir(&third_source).expect("third source directory");
        let first = first_source.join("item.txt");
        let second = second_source.join("item.txt");
        let third = third_source.join("item.txt");
        fs::write(&first, b"first").expect("first source");
        fs::write(&second, b"second").expect("second source");
        fs::write(&third, b"third").expect("third source");
        fs::write(destination.join("item.txt"), b"existing").expect("existing destination");

        let state = ApplicationState::new().expect("application state");
        state
            .stage_copy_many(vec![first, second, third])
            .expect("batch should stage");
        let batch = submit_paste_batch(&state, &destination).expect("batch should submit");
        let first_conflict = state.batch_active.get().expect("first active job");
        assert_eq!(wait_for_terminal(&state, first_conflict), JobState::Failed);
        state.finish_operation(first_conflict, TerminalOutcome::Conflict);
        let ConflictResolution::Retried(keep_both) = state
            .resolve_conflict(first_conflict, ConflictDecision::KeepBoth)
            .expect("Keep Both should retry safely")
        else {
            panic!("Keep Both must submit a new attempt");
        };
        assert_eq!(
            wait_for_terminal(&state, keep_both.job_id()),
            JobState::Completed
        );
        state.finish_operation(keep_both.job_id(), TerminalOutcome::Completed);

        let second_conflict = state.batch_active.get().expect("second active job");
        assert_eq!(wait_for_terminal(&state, second_conflict), JobState::Failed);
        state.finish_operation(second_conflict, TerminalOutcome::Conflict);
        assert_eq!(
            state
                .resolve_conflict(second_conflict, ConflictDecision::SkipAll)
                .expect("Skip All should resolve the batch conflict"),
            ConflictResolution::KeptExisting
        );
        let auto_skipped = state.batch_active.get().expect("third active job");
        assert_eq!(wait_for_terminal(&state, auto_skipped), JobState::Failed);
        state.finish_operation(auto_skipped, TerminalOutcome::Conflict);
        assert!(matches!(
            state.pending_conflict(auto_skipped),
            Err(CopyInteractionError::ConflictAlreadyResolved(job_id)) if job_id == auto_skipped
        ));
        let snapshot = state.batch_snapshot(batch.id()).expect("batch snapshot");
        assert_eq!(snapshot.status(), BatchStatus::CompletedWithIssues);
        assert_eq!(snapshot.completed(), 1);
        assert_eq!(snapshot.skipped(), 2);
        assert_eq!(
            fs::read(destination.join("item.txt")).expect("existing target should remain"),
            b"existing"
        );
        assert_eq!(
            fs::read(destination.join("item (copy).txt")).expect("Keep Both sibling should exist"),
            b"first"
        );
    }

    #[test]
    fn phase_6p_batch_cancel_removes_queued_items_and_resolves_blocked_conflict() {
        let fixture = tempdir().expect("temporary fixture");
        let destination = fixture.path().join("destination");
        let first_source = fixture.path().join("first");
        let second_source = fixture.path().join("second");
        fs::create_dir(&destination).expect("destination directory");
        fs::create_dir(&first_source).expect("first source directory");
        fs::create_dir(&second_source).expect("second source directory");
        let first = first_source.join("item");
        let second = second_source.join("item");
        fs::write(&first, b"first").expect("first source");
        fs::write(&second, b"second").expect("second source");
        fs::write(destination.join("item"), b"existing").expect("existing target");
        let state = ApplicationState::new().expect("application state");
        state
            .stage_copy_many(vec![first, second])
            .expect("batch should stage");
        let batch = submit_paste_batch(&state, &destination).expect("batch should submit");
        let conflict = state.batch_active.get().expect("active conflict");
        assert_eq!(wait_for_terminal(&state, conflict), JobState::Failed);
        state.finish_operation(conflict, TerminalOutcome::Conflict);
        state.cancel_batch(batch.id()).expect("batch should cancel");

        let snapshot = state.batch_snapshot(batch.id()).expect("batch snapshot");
        assert_eq!(snapshot.status(), BatchStatus::Cancelled);
        assert_eq!(snapshot.cancelled(), 2);
        assert_eq!(snapshot.remaining(), 0);
        assert!(state.batch_active.get().is_none());
        assert!(matches!(
            state.pending_conflict(conflict),
            Err(CopyInteractionError::ConflictAlreadyResolved(job_id)) if job_id == conflict
        ));
    }

    #[test]
    fn phase_6p_batch_cancel_accepts_current_item_that_already_committed() {
        let state = ApplicationState::new_with_trash_backend(Arc::new(SuccessfulTrashBackend))
            .expect("application state");
        let batch = submit_trash_batch(
            &state,
            vec![
                PathBuf::from("/virtual/first"),
                PathBuf::from("/virtual/second"),
            ],
        )
        .expect("trash batch should submit");
        let first = state
            .batch_active
            .get()
            .expect("first job should be active");
        assert_eq!(wait_for_terminal(&state, first), JobState::Completed);
        state
            .cancel_batch(batch.id())
            .expect("already-committed current item should not reject queued cancellation");
        state.finish_operation(first, TerminalOutcome::Completed);

        let snapshot = state.batch_snapshot(batch.id()).expect("batch snapshot");
        assert_eq!(snapshot.status(), BatchStatus::CompletedWithIssues);
        assert_eq!(snapshot.completed(), 1);
        assert_eq!(snapshot.cancelled(), 1);
    }

    #[test]
    fn phase_6p_history_clear_completed_preserves_actionable_evidence() {
        let fixture = tempdir().expect("temporary fixture");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::create_dir(&source).expect("source directory");
        fs::create_dir(&destination).expect("destination directory");
        let state = ApplicationState::new().expect("application state");
        let completed_source = source.join("completed");
        fs::write(&completed_source, b"completed").expect("completed source");
        state
            .stage_copy(completed_source)
            .expect("stage completed copy");
        let completed = state.submit_paste(&destination).expect("completed copy");
        assert_eq!(
            wait_for_terminal(&state, completed.job_id()),
            JobState::Completed
        );
        state.finish_operation(completed.job_id(), TerminalOutcome::Completed);

        let conflict_source = source.join("conflict");
        fs::write(&conflict_source, b"incoming").expect("conflict source");
        fs::write(destination.join("conflict"), b"existing").expect("conflict target");
        state
            .stage_copy(conflict_source)
            .expect("stage conflict copy");
        let conflict = state.submit_paste(&destination).expect("conflict copy");
        assert_eq!(
            wait_for_terminal(&state, conflict.job_id()),
            JobState::Failed
        );
        state.finish_operation(conflict.job_id(), TerminalOutcome::Conflict);

        assert_eq!(state.clear_completed_history(), 1);
        let history = state.terminal_history();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].job_id(), conflict.job_id());
        assert_eq!(history[0].outcome(), TerminalOutcome::Conflict);
    }

    #[test]
    fn phase_6p_undo_state_restores_completed_move_and_rejects_non_undoable_copy() {
        let fixture = tempdir().expect("temporary fixture");
        let source_directory = fixture.path().join("source");
        let destination_directory = fixture.path().join("destination");
        fs::create_dir(&source_directory).expect("source directory");
        fs::create_dir(&destination_directory).expect("destination directory");
        let source = source_directory.join(OsString::from_vec(b"undo-\xff".to_vec()));
        fs::write(&source, b"payload").expect("move source");
        let moved_path = destination_directory.join(
            source
                .file_name()
                .expect("fixture source should retain its raw filename"),
        );
        let state = ApplicationState::new().expect("application state");
        let move_request = MoveRequest::new(
            source.clone(),
            moved_path.clone(),
            ConflictPolicy::FailIfExists,
        );
        let move_authorization = authorize_scope(
            &state,
            destructive_scope_for_move(&move_request).expect("move scope"),
        );
        let moved = state
            .submit_move_authorized(GuardrailAuthorized::new(move_request, move_authorization))
            .expect("move submit");
        assert_eq!(
            wait_for_terminal(&state, moved.job_id()),
            JobState::Completed
        );
        state.finish_operation(moved.job_id(), TerminalOutcome::Completed);
        assert!(state.can_undo(moved.job_id()));

        let undo_authorization = authorize_scope(
            &state,
            state
                .undo_operation_guardrail_scope(moved.job_id())
                .expect("undo scope"),
        );
        let undo = state
            .undo_operation_authorized(moved.job_id(), undo_authorization)
            .expect("undo should submit");
        assert_eq!(
            wait_for_terminal(&state, undo.job_id()),
            JobState::Completed
        );
        state.finish_operation(undo.job_id(), TerminalOutcome::Completed);
        assert_eq!(
            fs::read(&source).expect("undo should restore source payload"),
            b"payload"
        );
        assert!(!moved_path.exists());
        assert!(!state.can_undo(moved.job_id()));
        assert!(matches!(
            state.undo_operation(moved.job_id()),
            Err(CopyInteractionError::UndoAlreadySubmitted(job_id)) if job_id == moved.job_id()
        ));

        let copy_source = source_directory.join("copy");
        fs::write(&copy_source, b"copy").expect("copy source");
        state.stage_copy(copy_source).expect("copy should stage");
        let copy = state
            .submit_paste(&destination_directory)
            .expect("copy submit");
        assert_eq!(
            wait_for_terminal(&state, copy.job_id()),
            JobState::Completed
        );
        state.finish_operation(copy.job_id(), TerminalOutcome::Completed);
        assert!(!state.can_undo(copy.job_id()));
        assert!(matches!(
            state.undo_operation(copy.job_id()),
            Err(CopyInteractionError::UndoNotAvailable(job_id)) if job_id == copy.job_id()
        ));
    }

    #[test]
    fn phase_6n_state_restore_conflict_retries_with_safe_sibling_name() {
        let fixture = tempdir().expect("temporary fixture");
        let trash = fixture.path().join("Trash/files/item");
        let info = fixture.path().join("Trash/info/item.trashinfo");
        let destination = fixture.path().join("original/item");
        fs::create_dir_all(trash.parent().expect("files parent")).expect("files directory");
        fs::create_dir_all(info.parent().expect("info parent")).expect("info directory");
        fs::create_dir(destination.parent().expect("destination parent"))
            .expect("destination parent");
        fs::write(&trash, b"trashed").expect("Trash payload");
        fs::write(&info, b"metadata").expect("Trash metadata");
        fs::write(&destination, b"existing").expect("existing destination");
        let state = ApplicationState::new().expect("application state");
        let request = RestoreRequest::new(&trash, &info, &destination).expect("restore request");
        let failed = submit_restore(&state, request).expect("restore submission");
        assert_eq!(wait_for_terminal(&state, failed.job_id()), JobState::Failed);
        state.finish_operation(failed.job_id(), TerminalOutcome::Conflict);
        assert_eq!(
            state
                .pending_conflict(failed.job_id())
                .expect("pending restore conflict")
                .destination(),
            destination
        );

        let ConflictResolution::Retried(retried) = resolve_conflict(
            &state,
            failed.job_id(),
            ConflictDecision::RetryWithName(OsString::from("restored-item")),
        )
        .expect("safe revised restore") else {
            panic!("restore should create revised retry");
        };
        assert_eq!(retried.operation_id(), failed.operation_id());
        assert_eq!(
            wait_for_terminal(&state, retried.job_id()),
            JobState::Completed
        );
        assert_eq!(
            fs::read(fixture.path().join("original/restored-item")).expect("restored item"),
            b"trashed"
        );
        assert_eq!(fs::read(destination).expect("existing item"), b"existing");
        assert!(!info.exists());
    }

    #[test]
    fn phase_6q_duplicate_batch_is_fifo_raw_path_safe_and_preserves_symlinks() {
        let fixture = tempdir().expect("temporary fixture");
        let first = fixture
            .path()
            .join(OsString::from_vec(b"first-\xff.txt".to_vec()));
        let link = fixture.path().join("shortcut");
        let raw_target = OsString::from_vec(b"missing-\xfe".to_vec());
        fs::write(&first, b"first payload").expect("first source");
        std::os::unix::fs::symlink(&raw_target, &link).expect("symbolic-link source");

        let sources = vec![first.clone(), link.clone()];
        let state = ApplicationState::new().expect("application state");
        let batch = state
            .submit_duplicate_batch(sources.clone())
            .expect("duplicate batch");
        assert_eq!(batch.queued(), 2);

        for _ in 0..sources.len() {
            let job_id = state.batch_active.get().expect("active duplicate job");
            assert_eq!(wait_for_terminal(&state, job_id), JobState::Completed);
            state.finish_operation(job_id, TerminalOutcome::Completed);
        }

        let first_copy = fixture.path().join(
            duplicate_name(first.file_name().expect("raw source name"), 1).expect("duplicate name"),
        );
        assert_eq!(
            fs::read(first_copy).expect("duplicate payload"),
            b"first payload"
        );
        let link_copy = fixture.path().join("shortcut (copy)");
        assert_eq!(
            fs::read_link(link_copy)
                .expect("duplicate should remain a symbolic link")
                .as_os_str()
                .as_bytes(),
            raw_target.as_bytes()
        );

        let snapshot = state.batch_snapshot(batch.id()).expect("batch snapshot");
        assert_eq!(snapshot.status(), BatchStatus::Completed);
        let completed_sources = state
            .terminal_history()
            .into_iter()
            .filter(|entry| entry.batch_id() == Some(batch.id()))
            .map(|entry| entry.operation().source().to_path_buf())
            .collect::<Vec<_>>();
        assert_eq!(completed_sources, sources);
    }

    #[test]
    fn phase_6q_state_create_conflict_retry_keeps_identity_and_never_overwrites() {
        let fixture = tempdir().expect("temporary fixture");
        let source = fixture.path().join("report.txt");
        let occupied = fixture.path().join("report (copy).txt");
        fs::write(&source, b"incoming").expect("source payload");
        fs::write(&occupied, b"existing").expect("occupied sibling");

        let state = ApplicationState::new().expect("application state");
        let batch = state
            .submit_duplicate_batch(vec![source.clone()])
            .expect("duplicate batch");
        let failed_job = state.batch_active.get().expect("active duplicate");
        assert_eq!(wait_for_terminal(&state, failed_job), JobState::Failed);
        state.finish_operation(failed_job, TerminalOutcome::Conflict);
        let pending = state
            .pending_conflict(failed_job)
            .expect("creation conflict should remain actionable");
        assert_eq!(pending.destination(), occupied.as_path());

        let ConflictResolution::Retried(retry) = state
            .resolve_conflict(failed_job, ConflictDecision::KeepBoth)
            .expect("Keep Both should choose the next duplicate name")
        else {
            panic!("Keep Both should submit a retry");
        };
        assert_eq!(retry.operation_id(), pending.operation_id());
        assert_ne!(retry.job_id(), failed_job);
        assert_eq!(
            wait_for_terminal(&state, retry.job_id()),
            JobState::Completed
        );
        state.finish_operation(retry.job_id(), TerminalOutcome::Completed);

        assert_eq!(fs::read(&occupied).expect("occupied sibling"), b"existing");
        assert_eq!(
            fs::read(fixture.path().join("report (copy 2).txt")).expect("revised duplicate"),
            b"incoming"
        );
        let snapshot = state.batch_snapshot(batch.id()).expect("batch snapshot");
        assert_eq!(snapshot.status(), BatchStatus::Completed);
        assert_eq!(snapshot.completed(), 1);
    }

    #[test]
    fn phase_6q_state_tracks_create_history_and_affected_directory() {
        let fixture = tempdir().expect("temporary fixture");
        let destination = fixture.path().join("created-folder");
        let state = ApplicationState::new().expect("application state");
        let submission = state
            .submit_create(CreateRequest::directory(&destination).expect("create request"))
            .expect("creation submission");
        assert_eq!(
            wait_for_terminal(&state, submission.job_id()),
            JobState::Completed
        );
        let tracked = state
            .finish_operation(submission.job_id(), TerminalOutcome::Completed)
            .expect("tracked creation");
        assert_eq!(
            tracked.affected_directories(),
            vec![fixture.path().to_path_buf()]
        );
        assert!(destination.is_dir());
        let terminal = state.terminal_history();
        assert_eq!(terminal.len(), 1);
        assert_eq!(terminal[0].operation_id(), submission.operation_id());
        assert!(matches!(
            terminal[0].operation(),
            TrackedOperation::Create(_)
        ));
    }

    #[test]
    fn phase_6r_state_routes_copy_move_and_link_drops_through_fifo_batches() {
        let fixture = tempdir().expect("temporary fixture");
        let copy_source = fixture.path().join("copy-source");
        let move_source = fixture.path().join("move-source");
        let link_source = fixture.path().join("link-source");
        fs::write(&copy_source, b"copy").expect("copy source");
        fs::write(&move_source, b"move").expect("move source");
        fs::write(&link_source, b"link").expect("link source");
        let state = ApplicationState::new().expect("application state");

        for (source, directory, action) in [
            (
                copy_source.clone(),
                fixture.path().join("copies"),
                DropAction::Copy,
            ),
            (
                move_source.clone(),
                fixture.path().join("moves"),
                DropAction::Move,
            ),
            (
                link_source.clone(),
                fixture.path().join("links"),
                DropAction::Link,
            ),
        ] {
            fs::create_dir(&directory).expect("drop destination");
            let request = DropRequest::new(
                vec![source.clone()],
                DropDestination::Directory(directory.clone()),
                action,
            )
            .expect("drop request");
            let batch = submit_drop(&state, request).expect("drop batch");
            assert_eq!(batch.queued(), 1);
            let job_id = state.batch_active.get().expect("active drop job");
            assert_eq!(wait_for_terminal(&state, job_id), JobState::Completed);
            state.finish_operation(job_id, TerminalOutcome::Completed);
            let snapshot = state.batch_snapshot(batch.id()).expect("batch snapshot");
            assert_eq!(snapshot.status(), BatchStatus::Completed);
            assert_eq!(snapshot.completed(), 1);
            assert!(
                directory
                    .join(source.file_name().expect("source name"))
                    .exists()
            );
        }

        assert!(copy_source.exists());
        assert!(!move_source.exists());
        let linked = fixture.path().join("links/link-source");
        assert_eq!(
            fs::read_link(linked).expect("symbolic link target"),
            link_source
        );
    }

    #[test]
    fn phase_7e_opposite_pane_transfer_queues_direct_copy_and_move_without_staging() {
        let fixture = tempdir().expect("temporary fixture");
        let source_directory = fixture.path().join("source");
        let destination = fixture.path().join("opposite");
        fs::create_dir(&source_directory).expect("source directory");
        fs::create_dir(&destination).expect("opposite directory");
        let copy_source = source_directory.join("copy.txt");
        let move_source = source_directory.join("move.txt");
        fs::write(&copy_source, b"copy").expect("copy source");
        fs::write(&move_source, b"move").expect("move source");
        let state = ApplicationState::new().expect("application state");

        let copy = submit_transfer_batch(
            &state,
            TransferIntent::Copy,
            vec![copy_source.clone(), copy_source.clone()],
            &destination,
        )
        .expect("direct copy batch");
        assert_eq!(copy.queued(), 1);
        assert_eq!(state.staged_transfers(), None);
        let copy_job = state.batch_active.get().expect("copy job");
        assert_eq!(wait_for_terminal(&state, copy_job), JobState::Completed);
        state.finish_operation(copy_job, TerminalOutcome::Completed);
        assert_eq!(
            fs::read(destination.join("copy.txt")).expect("copied output"),
            b"copy"
        );
        assert!(copy_source.exists());

        let moved = submit_transfer_batch(
            &state,
            TransferIntent::Move,
            vec![move_source.clone()],
            &destination,
        )
        .expect("direct move batch");
        assert_eq!(moved.queued(), 1);
        assert_eq!(state.staged_transfers(), None);
        let move_job = state.batch_active.get().expect("move job");
        assert_eq!(wait_for_terminal(&state, move_job), JobState::Completed);
        state.finish_operation(move_job, TerminalOutcome::Completed);
        assert_eq!(
            fs::read(destination.join("move.txt")).expect("moved output"),
            b"move"
        );
        assert!(!move_source.exists());
    }

    #[test]
    fn phase_7f_split_drop_jobs_reuse_copy_move_link_fifo() {
        let fixture = tempdir().expect("temporary fixture");
        let destination = fixture.path().join("opposite");
        fs::create_dir(&destination).expect("opposite directory");
        let state = ApplicationState::new().expect("application state");

        for (name, action) in [
            ("copy.txt", DropAction::Copy),
            ("move.txt", DropAction::Move),
            ("link.txt", DropAction::Link),
        ] {
            let source = fixture.path().join(name);
            fs::write(&source, name.as_bytes()).expect("source file");
            let request = DropRequest::new(
                vec![source.clone()],
                DropDestination::Directory(destination.clone()),
                action,
            )
            .expect("valid split drop");
            let batch = submit_drop(&state, request).expect("queued split drop");
            assert_eq!(batch.queued(), 1);
            let job = state.batch_active.get().expect("active split drop");
            assert_eq!(wait_for_terminal(&state, job), JobState::Completed);
            state.finish_operation(job, TerminalOutcome::Completed);
            assert!(destination.join(name).exists());
        }

        assert!(fixture.path().join("copy.txt").exists());
        assert!(!fixture.path().join("move.txt").exists());
        assert_eq!(
            fs::read_link(destination.join("link.txt")).expect("symbolic link target"),
            fixture.path().join("link.txt")
        );
    }

    #[test]
    fn phase_18w_workflow_claims_verified_copy_result_exactly_once() {
        let fixture = tempdir().expect("temporary fixture");
        let source = fixture.path().join("source.bin");
        let destination = fixture.path().join("destination.bin");
        fs::write(&source, b"verified removable payload").expect("source payload");
        let state = ApplicationState::new().expect("application state");
        let submission = state
            .submit_verified_usb_copy(VerifiedCopyRequest::new(source, destination))
            .expect("verified removable child submission");
        let job_id = submission.job_id();
        assert!(state.is_verified_usb_copy_operation(job_id));
        assert_eq!(wait_for_terminal(&state, job_id), JobState::Completed);

        let completion = state
            .finish_verified_copy(job_id)
            .expect("claimed terminal result");
        assert!(matches!(
            completion,
            VerifiedCopyCompletion::VerifiedUsb(Ok(_))
        ));
        assert!(!state.is_verified_usb_copy_operation(job_id));
        assert!(state.finish_verified_copy(job_id).is_none());
    }
}
