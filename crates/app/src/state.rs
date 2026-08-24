use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use floe_core::{
    ConflictPolicy, CopyRequest, JobEvent, JobId, MoveRequest, OperationId, RenameRequest,
    SymlinkPolicy,
};
use thiserror::Error;

use crate::{
    copy_executor::{
        CopyCancelError, CopyExecutor, CopyExecutorSpawnError, CopySubmission, CopySubmitError,
    },
    job_manager::{ApplicationJobManager, SharedJobManager},
    move_executor::{
        MoveCancelError, MoveExecutor, MoveExecutorSpawnError, MoveSubmission, MoveSubmitError,
    },
    trash_executor::{
        TrashCancelError, TrashExecutor, TrashExecutorSpawnError, TrashRequest, TrashRequestError,
        TrashSubmission, TrashSubmitError,
    },
};

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
    queued: usize,
}

impl BatchSubmission {
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
}

pub const MAX_TERMINAL_HISTORY: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalOutcome {
    Completed,
    Cancelled,
    Conflict,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConflictDecision {
    KeepExisting,
    RetryWithName(OsString),
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
}

impl TrackedOperation {
    pub fn source(&self) -> &Path {
        match self {
            Self::Copy(request) => request.source(),
            Self::Move(request) => request.source(),
            Self::Rename(request) => request.source(),
            Self::Trash(request) => request.source(),
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
    Move(MoveSubmission),
    Trash(TrashSubmission),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictResolution {
    KeptExisting,
    Retried(RetrySubmission),
}

impl RetrySubmission {
    pub const fn operation_id(self) -> OperationId {
        match self {
            Self::Copy(submission) => submission.operation_id(),
            Self::Move(submission) => submission.operation_id(),
            Self::Trash(submission) => submission.operation_id(),
        }
    }

    pub const fn job_id(self) -> JobId {
        match self {
            Self::Copy(submission) => submission.job_id(),
            Self::Move(submission) => submission.job_id(),
            Self::Trash(submission) => submission.job_id(),
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
    #[error(transparent)]
    CopySubmit(#[from] CopySubmitError),
    #[error(transparent)]
    MoveSubmit(#[from] MoveSubmitError),
    #[error(transparent)]
    CopyCancel(#[from] CopyCancelError),
    #[error(transparent)]
    MoveCancel(#[from] MoveCancelError),
    #[error(transparent)]
    TrashRequest(#[from] TrashRequestError),
    #[error(transparent)]
    TrashSubmit(#[from] TrashSubmitError),
    #[error(transparent)]
    TrashCancel(#[from] TrashCancelError),
    #[error("terminal operation history does not contain job {0:?}")]
    RetryNotFound(JobId),
    #[error("completed job {0:?} cannot be retried")]
    RetryCompleted(JobId),
    #[error("job {0:?} needs an explicit conflict decision")]
    ConflictDecisionRequired(JobId),
    #[error("job {0:?} does not have a pending destination conflict")]
    ConflictNotFound(JobId),
    #[error("the conflict for job {0:?} has already been resolved")]
    ConflictAlreadyResolved(JobId),
    #[error("job {0:?} does not support destination conflict resolution")]
    ConflictUnsupported(JobId),
}

#[derive(Debug, Error)]
pub enum ApplicationStateSpawnError {
    #[error(transparent)]
    Copy(#[from] CopyExecutorSpawnError),
    #[error(transparent)]
    Move(#[from] MoveExecutorSpawnError),
    #[error(transparent)]
    Trash(#[from] TrashExecutorSpawnError),
}

/// Application-wide services and state that outlive any one browser concern.
#[derive(Debug)]
pub struct ApplicationState {
    pub jobs: SharedJobManager,
    copy_executor: CopyExecutor,
    move_executor: MoveExecutor,
    trash_executor: TrashExecutor,
    transfer_buffer: RefCell<TransferBuffer>,
    operation_requests: RefCell<HashMap<JobId, TrackedOperation>>,
    terminal_history: RefCell<VecDeque<TerminalOperation>>,
    resolved_conflicts: RefCell<HashSet<JobId>>,
    batch_pending: RefCell<VecDeque<TrackedOperation>>,
    batch_active: Cell<Option<JobId>>,
}

impl ApplicationState {
    pub fn new() -> Result<Self, ApplicationStateSpawnError> {
        let jobs = Arc::new(Mutex::new(ApplicationJobManager::new()));
        let copy_executor = CopyExecutor::spawn(Arc::clone(&jobs))?;
        let move_executor = MoveExecutor::spawn(Arc::clone(&jobs))?;
        let trash_executor = TrashExecutor::spawn(Arc::clone(&jobs))?;
        Ok(Self {
            jobs,
            copy_executor,
            move_executor,
            trash_executor,
            transfer_buffer: RefCell::new(TransferBuffer::default()),
            operation_requests: RefCell::new(HashMap::new()),
            terminal_history: RefCell::new(VecDeque::new()),
            resolved_conflicts: RefCell::new(HashSet::new()),
            batch_pending: RefCell::new(VecDeque::new()),
            batch_active: Cell::new(None),
        })
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
        let queued = operations.len();
        self.batch_pending.borrow_mut().extend(operations);
        if intent == TransferIntent::Move {
            self.transfer_buffer.borrow_mut().clear_move();
        }
        self.pump_batch();
        Ok(BatchSubmission { queued })
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
        let queued = operations.len();
        self.batch_pending.borrow_mut().extend(operations);
        self.pump_batch();
        Ok(BatchSubmission { queued })
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
            }
        }
    }

    pub fn submit_rename(
        &self,
        source: PathBuf,
        new_name: OsString,
    ) -> Result<MoveSubmission, CopyInteractionError> {
        validate_rename_name(&new_name)?;
        let request = RenameRequest::new(source, new_name, ConflictPolicy::FailIfExists);
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
    }

    pub fn operation_request(&self, job_id: JobId) -> Option<TrackedOperation> {
        self.operation_requests.borrow().get(&job_id).cloned()
    }

    pub fn submit_trash(&self, source: PathBuf) -> Result<TrashSubmission, CopyInteractionError> {
        let request = TrashRequest::new(source)?;
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
    }

    fn pump_batch(&self) {
        if self.batch_active.get().is_some() {
            return;
        }
        while let Some(operation) = self.batch_pending.borrow_mut().pop_front() {
            match self.submit_batch_operation(operation) {
                Ok(job_id) => {
                    self.batch_active.set(Some(job_id));
                    return;
                }
                Err(error) => {
                    tracing::error!(%error, "could not dispatch queued batch operation");
                }
            }
        }
    }

    fn submit_batch_operation(
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
            TrackedOperation::Rename(_) => {
                unreachable!("rename is never queued as a multi-selection batch")
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
        if outcome == TerminalOutcome::Completed
            && let Some(TrackedOperation::Move(request)) = operation.as_ref()
        {
            self.transfer_buffer
                .borrow_mut()
                .clear_completed_move(request.source());
        }
        if let (Some(operation_id), Some(operation)) = (operation_id, operation.as_ref()) {
            let mut history = self.terminal_history.borrow_mut();
            if history.len() == MAX_TERMINAL_HISTORY {
                if let Some(evicted) = history.pop_front() {
                    self.resolved_conflicts
                        .borrow_mut()
                        .remove(&evicted.job_id());
                    lock(&self.jobs).forget_terminal(evicted.job_id());
                }
            }
            history.push_back(TerminalOperation {
                job_id,
                operation_id,
                outcome,
                operation: operation.clone(),
            });
        }
        if self.batch_active.get() == Some(job_id) {
            self.batch_active.set(None);
            self.pump_batch();
        }
        operation
    }

    pub fn terminal_history(&self) -> Vec<TerminalOperation> {
        self.terminal_history.borrow().iter().cloned().collect()
    }

    pub fn retry_operation(
        &self,
        failed_job_id: JobId,
    ) -> Result<RetrySubmission, CopyInteractionError> {
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

        match terminal.operation().clone() {
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
        }
    }

    pub fn pending_conflict(&self, job_id: JobId) -> Result<PendingConflict, CopyInteractionError> {
        let terminal = self.pending_conflict_operation(job_id)?;
        let destination = match terminal.operation() {
            TrackedOperation::Copy(request) => request.destination().to_path_buf(),
            TrackedOperation::Move(request) => request.destination().to_path_buf(),
            TrackedOperation::Rename(request) => request
                .source()
                .parent()
                .ok_or(CopyInteractionError::ConflictUnsupported(job_id))?
                .join(request.new_name()),
            TrackedOperation::Trash(_) => {
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
        let terminal = self.pending_conflict_operation(job_id)?;
        if matches!(terminal.operation(), TrackedOperation::Trash(_)) {
            return Err(CopyInteractionError::ConflictUnsupported(job_id));
        }

        match decision {
            ConflictDecision::KeepExisting => {
                self.resolved_conflicts.borrow_mut().insert(job_id);
                Ok(ConflictResolution::KeptExisting)
            }
            ConflictDecision::RetryWithName(new_name) => {
                validate_rename_name(&new_name)?;
                let submission =
                    self.submit_conflict_retry(job_id, terminal.operation(), new_name)?;
                self.resolved_conflicts.borrow_mut().insert(job_id);
                Ok(ConflictResolution::Retried(submission))
            }
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
            TrackedOperation::Move(request) => {
                let destination =
                    retry_destination(request.destination(), &new_name, failed_job_id)?;
                let revised =
                    MoveRequest::new(request.source(), destination, ConflictPolicy::FailIfExists);
                match self
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
                }
            }
            TrackedOperation::Rename(request) => {
                let revised =
                    RenameRequest::new(request.source(), new_name, ConflictPolicy::FailIfExists);
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
            TrackedOperation::Trash(_) => {
                Err(CopyInteractionError::ConflictUnsupported(failed_job_id))
            }
        }
    }

    pub fn cancel_operation(&self, job_id: JobId) -> Result<(), CopyInteractionError> {
        match self.operation_request(job_id) {
            Some(TrackedOperation::Copy(_)) => self.copy_executor.cancel(job_id)?,
            Some(TrackedOperation::Move(_) | TrackedOperation::Rename(_)) => {
                self.move_executor.cancel(job_id)?;
            }
            Some(TrackedOperation::Trash(_)) => self.trash_executor.cancel(job_id)?,
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

    #[cfg(test)]
    pub(crate) fn new_with_trash_backend(
        backend: Arc<dyn TrashBackend>,
    ) -> Result<Self, ApplicationStateSpawnError> {
        let jobs = Arc::new(Mutex::new(ApplicationJobManager::new()));
        let copy_executor = CopyExecutor::spawn(Arc::clone(&jobs))?;
        let move_executor = MoveExecutor::spawn(Arc::clone(&jobs))?;
        let trash_executor = TrashExecutor::spawn_with_backend(Arc::clone(&jobs), 8, backend)?;
        Ok(Self {
            jobs,
            copy_executor,
            move_executor,
            trash_executor,
            transfer_buffer: RefCell::new(TransferBuffer::default()),
            operation_requests: RefCell::new(HashMap::new()),
            terminal_history: RefCell::new(VecDeque::new()),
            resolved_conflicts: RefCell::new(HashSet::new()),
            batch_pending: RefCell::new(VecDeque::new()),
            batch_active: Cell::new(None),
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

fn transfer_destination(
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
        os::unix::ffi::{OsStrExt, OsStringExt},
        sync::atomic::{AtomicUsize, Ordering},
        thread,
        time::{Duration, Instant},
    };

    use floe_core::{CopyCancellation, JobEventKind, JobFailureKind, JobState};
    use tempfile::tempdir;

    use super::*;

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

        let moved = state
            .submit_paste(&destination_directory)
            .expect("move paste should be submitted");
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
        let renamed = state
            .submit_rename(moved_path.clone(), renamed_name.clone())
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
        let submission = state
            .submit_trash(source.clone())
            .expect("trash request should be submitted");

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
        let retried_copy = state
            .retry_operation(cancelled_copy.job_id())
            .expect("cancelled copy should retry");
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
        let failed_move = state
            .submit_paste(&move_destination_dir)
            .expect("move should submit");
        assert_eq!(
            wait_for_terminal(&state, failed_move.job_id()),
            JobState::Failed
        );
        state.finish_operation(failed_move.job_id(), TerminalOutcome::Failed);
        fs::remove_file(&move_conflict).expect("move conflict should be removable");
        let retried_move = state
            .retry_operation(failed_move.job_id())
            .expect("failed move should retry");
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
        let failed_rename = state
            .submit_rename(rename_source, OsString::from("rename-target"))
            .expect("rename should submit");
        assert_eq!(
            wait_for_terminal(&state, failed_rename.job_id()),
            JobState::Failed
        );
        state.finish_operation(failed_rename.job_id(), TerminalOutcome::Failed);
        fs::remove_file(&rename_conflict).expect("rename conflict should be removable");
        let retried_rename = state
            .retry_operation(failed_rename.job_id())
            .expect("failed rename should retry");
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
        let failed = state
            .submit_trash(source.clone())
            .expect("trash should submit");
        assert_eq!(wait_for_terminal(&state, failed.job_id()), JobState::Failed);
        state.finish_operation(failed.job_id(), TerminalOutcome::Failed);

        let retried = state
            .retry_operation(failed.job_id())
            .expect("failed trash should retry");
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
            let submission = state
                .submit_trash(PathBuf::from(format!("/virtual/history-{index}")))
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
            state.retry_operation(first_job.expect("first job")),
            Err(CopyInteractionError::RetryNotFound(_))
        ));
        assert!(matches!(
            state.retry_operation(last_job.expect("last job")),
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
            state.retry_operation(failed.job_id()),
            Err(CopyInteractionError::ConflictDecisionRequired(job_id))
                if job_id == failed.job_id()
        ));

        assert!(matches!(
            state.resolve_conflict(
                failed.job_id(),
                ConflictDecision::RetryWithName(OsString::from("nested/name")),
            ),
            Err(CopyInteractionError::InvalidRenameName)
        ));
        assert!(state.pending_conflict(failed.job_id()).is_ok());

        let revised_name = OsString::from_vec(b"item-\xff".to_vec());
        let resolution = state
            .resolve_conflict(
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
            state.resolve_conflict(failed.job_id(), ConflictDecision::KeepExisting),
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
        let failed_move = state
            .submit_paste(&move_destination_directory)
            .expect("conflicting move should submit");
        assert_eq!(
            wait_for_terminal(&state, failed_move.job_id()),
            JobState::Failed
        );
        state.finish_operation(failed_move.job_id(), TerminalOutcome::Conflict);
        let moved_name = OsString::from("moved-item");
        let ConflictResolution::Retried(retried_move) = state
            .resolve_conflict(
                failed_move.job_id(),
                ConflictDecision::RetryWithName(moved_name.clone()),
            )
            .expect("revised move should submit")
        else {
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
        let failed_rename = state
            .submit_rename(rename_source.clone(), OsString::from("rename-target"))
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
        let ConflictResolution::Retried(retried_rename) = state
            .resolve_conflict(
                failed_rename.job_id(),
                ConflictDecision::RetryWithName(OsString::from("renamed-item")),
            )
            .expect("revised rename should submit")
        else {
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
        let submission = state
            .submit_trash(PathBuf::from("/virtual/conflict"))
            .expect("trash should submit");
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
        let batch = state
            .submit_paste_batch(&destination_directory)
            .expect("batch should be accepted");
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
        let batch = state
            .submit_paste_batch(&destination_directory)
            .expect("move batch should queue");
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
        let batch = state
            .submit_trash_batch(sources.clone())
            .expect("trash batch should queue");
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
            let submission = state
                .submit_trash(PathBuf::from(format!("/virtual/evict-{index}")))
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
}
