use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use floe_core::{
    CreateKind, CreateRequest, FileIdentity, REPLACE_BACKUP_DIRECTORY, ReplaceMode, SymlinkPolicy,
    remove_replace_backup,
};
use rustix::fs::OFlags;
use thiserror::Error;

const MAGIC: &[u8; 8] = b"FLOEUH01";
const VERSION: u16 = 2;
pub const MAX_UNDO_HISTORY_RECORDS: usize = 256;
const MAX_PATH_BYTES: usize = 16 * 1024;
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TEMP_ATTEMPTS: u32 = 64;
pub const UNDO_HISTORY_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum UndoHistoryState {
    InProgress = 1,
    Applied = 2,
    Undone = 3,
    Undoing = 4,
    Redoing = 5,
    NeedsReview = 6,
}

impl UndoHistoryState {
    fn decode(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::InProgress),
            2 => Some(Self::Applied),
            3 => Some(Self::Undone),
            4 => Some(Self::Undoing),
            5 => Some(Self::Redoing),
            6 => Some(Self::NeedsReview),
            _ => None,
        }
    }

    pub const fn is_history(self) -> bool {
        matches!(self, Self::Applied | Self::Undone)
    }

    pub const fn needs_review(self) -> bool {
        matches!(
            self,
            Self::InProgress | Self::Undoing | Self::Redoing | Self::NeedsReview
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UndoHistoryAction {
    Undo,
    Redo,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UndoRecipe {
    Copy {
        source: PathBuf,
        destination: PathBuf,
        symlink_policy: SymlinkPolicy,
    },
    Move {
        source: PathBuf,
        destination: PathBuf,
    },
    Rename {
        source: PathBuf,
        destination: PathBuf,
    },
    Create(CreateRequest),
    Replace {
        source: PathBuf,
        destination: PathBuf,
        backup: PathBuf,
        mode: ReplaceMode,
        symlink_policy: SymlinkPolicy,
    },
}

impl UndoRecipe {
    pub fn copy(
        source: impl Into<PathBuf>,
        destination: impl Into<PathBuf>,
        symlink_policy: SymlinkPolicy,
    ) -> Self {
        Self::Copy {
            source: source.into(),
            destination: destination.into(),
            symlink_policy,
        }
    }

    pub fn move_item(source: impl Into<PathBuf>, destination: impl Into<PathBuf>) -> Self {
        Self::Move {
            source: source.into(),
            destination: destination.into(),
        }
    }

    pub fn rename(source: impl Into<PathBuf>, destination: impl Into<PathBuf>) -> Self {
        Self::Rename {
            source: source.into(),
            destination: destination.into(),
        }
    }

    pub fn create(request: CreateRequest) -> Self {
        Self::Create(request)
    }

    pub fn replace(
        source: impl Into<PathBuf>,
        destination: impl Into<PathBuf>,
        backup: impl Into<PathBuf>,
        mode: ReplaceMode,
        symlink_policy: SymlinkPolicy,
    ) -> Self {
        Self::Replace {
            source: source.into(),
            destination: destination.into(),
            backup: backup.into(),
            mode,
            symlink_policy,
        }
    }

    pub fn source(&self) -> Option<&Path> {
        match self {
            Self::Copy { source, .. } | Self::Move { source, .. } | Self::Rename { source, .. } => {
                Some(source)
            }
            Self::Create(request) => request.source(),
            Self::Replace { source, .. } => Some(source),
        }
    }

    pub fn destination(&self) -> &Path {
        match self {
            Self::Copy { destination, .. }
            | Self::Move { destination, .. }
            | Self::Rename { destination, .. } => destination,
            Self::Create(request) => request.destination(),
            Self::Replace { destination, .. } => destination,
        }
    }

    pub const fn label(&self) -> &'static str {
        match self {
            Self::Copy { .. } => "Copy",
            Self::Move { .. } => "Move",
            Self::Rename { .. } => "Rename",
            Self::Create(_) => "Create",
            Self::Replace { .. } => "Replace",
        }
    }

    pub const fn require_empty_directory_on_undo(&self) -> bool {
        matches!(
            self,
            Self::Create(request) if matches!(request.kind(), CreateKind::Directory)
        )
    }

    pub fn replace_backup(&self) -> Option<&Path> {
        match self {
            Self::Replace { backup, .. } => Some(backup),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UndoHistoryRecord {
    id: u64,
    created_at: u64,
    updated_at: u64,
    state: UndoHistoryState,
    recipe: UndoRecipe,
    current_identity: Option<FileIdentity>,
    alternate_identity: Option<FileIdentity>,
}

impl UndoHistoryRecord {
    pub const fn id(&self) -> u64 {
        self.id
    }

    pub const fn created_at(&self) -> u64 {
        self.created_at
    }

    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }

    pub const fn state(&self) -> UndoHistoryState {
        self.state
    }

    pub const fn recipe(&self) -> &UndoRecipe {
        &self.recipe
    }

    pub const fn current_identity(&self) -> Option<FileIdentity> {
        self.current_identity
    }

    pub const fn alternate_identity(&self) -> Option<FileIdentity> {
        self.alternate_identity
    }

    pub const fn can_undo(&self) -> bool {
        matches!(self.state, UndoHistoryState::Applied)
    }

    pub const fn can_redo(&self) -> bool {
        matches!(self.state, UndoHistoryState::Undone)
    }

    pub fn expires_at(&self) -> u64 {
        self.updated_at.saturating_add(UNDO_HISTORY_TTL.as_secs())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UndoHistoryTicket(u64);

impl UndoHistoryTicket {
    pub const fn id(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug)]
pub struct UndoHistoryStore {
    inner: Arc<Mutex<UndoHistoryStoreInner>>,
}

#[derive(Debug)]
struct UndoHistoryStoreInner {
    path: PathBuf,
    next_id: u64,
    records: Vec<UndoHistoryRecord>,
}

impl UndoHistoryStore {
    pub fn open_at(path: PathBuf) -> Result<Self, UndoHistoryError> {
        Self::open_at_time(path, unix_now()?)
    }

    fn open_at_time(path: PathBuf, now: u64) -> Result<Self, UndoHistoryError> {
        prepare_private_parent(&path)?;
        let mut records = load_records(&path)?;
        let before = records.clone();
        expire_records(&mut records, now);
        let next_id = records
            .iter()
            .map(UndoHistoryRecord::id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(UndoHistoryError::IdentifierExhausted)?;
        if records != before {
            persist(&path, &records)?;
        }
        Ok(Self {
            inner: Arc::new(Mutex::new(UndoHistoryStoreInner {
                path,
                next_id,
                records,
            })),
        })
    }

    pub fn begin(&self, recipe: UndoRecipe) -> Result<UndoHistoryTicket, UndoHistoryError> {
        self.begin_at(recipe, unix_now()?)
    }

    fn begin_at(
        &self,
        recipe: UndoRecipe,
        now: u64,
    ) -> Result<UndoHistoryTicket, UndoHistoryError> {
        validate_recipe(&recipe)?;
        let mut inner = lock(&self.inner);
        expire_records(&mut inner.records, now);
        make_capacity(&mut inner.records)?;
        let id = inner.next_id;
        inner.next_id = inner
            .next_id
            .checked_add(1)
            .ok_or(UndoHistoryError::IdentifierExhausted)?;
        inner.records.push(UndoHistoryRecord {
            id,
            created_at: now,
            updated_at: now,
            state: UndoHistoryState::InProgress,
            recipe,
            current_identity: None,
            alternate_identity: None,
        });
        if let Err(error) = persist(&inner.path, &inner.records) {
            inner.records.pop();
            inner.next_id = id;
            return Err(error);
        }
        Ok(UndoHistoryTicket(id))
    }

    pub fn complete(
        &self,
        ticket: UndoHistoryTicket,
        identity: FileIdentity,
    ) -> Result<(), UndoHistoryError> {
        self.complete_at(ticket, identity, unix_now()?)
    }

    pub fn complete_replace(
        &self,
        ticket: UndoHistoryTicket,
        destination_identity: FileIdentity,
        backup_identity: FileIdentity,
    ) -> Result<(), UndoHistoryError> {
        self.update_record(ticket.0, |record, now| {
            if record.state != UndoHistoryState::InProgress
                || !matches!(record.recipe, UndoRecipe::Replace { .. })
            {
                return Err(UndoHistoryError::InvalidTransition(record.id));
            }
            record.state = UndoHistoryState::Applied;
            record.updated_at = now;
            record.current_identity = Some(destination_identity);
            record.alternate_identity = Some(backup_identity);
            Ok(())
        })
    }

    fn complete_at(
        &self,
        ticket: UndoHistoryTicket,
        identity: FileIdentity,
        now: u64,
    ) -> Result<(), UndoHistoryError> {
        self.update_record_at(ticket.0, now, |record, now| {
            if record.state != UndoHistoryState::InProgress {
                return Err(UndoHistoryError::InvalidTransition(record.id));
            }
            record.state = UndoHistoryState::Applied;
            record.updated_at = now;
            record.current_identity = Some(identity);
            Ok(())
        })
    }

    pub fn retain_if_destination_exists(
        &self,
        ticket: UndoHistoryTicket,
        destination: &Path,
    ) -> Result<(), UndoHistoryError> {
        match fs::symlink_metadata(destination) {
            Ok(_) => self.mark_needs_review(ticket.0),
            Err(error) if error.kind() == io::ErrorKind::NotFound => self.resolve(ticket.0),
            Err(_) => self.mark_needs_review(ticket.0),
        }
    }

    pub fn prepare_action(
        &self,
        id: u64,
        action: UndoHistoryAction,
    ) -> Result<UndoHistoryRecord, UndoHistoryError> {
        let mut prepared = None;
        self.update_record(id, |record, now| {
            let expected = match action {
                UndoHistoryAction::Undo => UndoHistoryState::Applied,
                UndoHistoryAction::Redo => UndoHistoryState::Undone,
            };
            if record.state != expected {
                return Err(UndoHistoryError::ActionUnavailable(id));
            }
            record.state = match action {
                UndoHistoryAction::Undo => UndoHistoryState::Undoing,
                UndoHistoryAction::Redo => UndoHistoryState::Redoing,
            };
            record.updated_at = now;
            prepared = Some(record.clone());
            Ok(())
        })?;
        prepared.ok_or(UndoHistoryError::UnknownRecord(id))
    }

    pub fn complete_action(
        &self,
        id: u64,
        action: UndoHistoryAction,
        identity: Option<FileIdentity>,
    ) -> Result<(), UndoHistoryError> {
        self.update_record(id, |record, now| {
            let expected = match action {
                UndoHistoryAction::Undo => UndoHistoryState::Undoing,
                UndoHistoryAction::Redo => UndoHistoryState::Redoing,
            };
            if record.state != expected {
                return Err(UndoHistoryError::InvalidTransition(id));
            }
            let is_replace = matches!(record.recipe, UndoRecipe::Replace { .. });
            if (action == UndoHistoryAction::Redo || is_replace) && identity.is_none() {
                return Err(UndoHistoryError::MissingIdentity(id));
            }
            if is_replace && record.alternate_identity.is_none() {
                return Err(UndoHistoryError::MissingIdentity(id));
            }
            record.state = match action {
                UndoHistoryAction::Undo => UndoHistoryState::Undone,
                UndoHistoryAction::Redo => UndoHistoryState::Applied,
            };
            record.updated_at = now;
            if is_replace {
                let previous_current = record.current_identity;
                record.current_identity = identity;
                record.alternate_identity = previous_current;
            } else {
                record.current_identity = identity;
            }
            Ok(())
        })
    }

    pub fn mark_action_uncertain(&self, id: u64) -> Result<(), UndoHistoryError> {
        self.mark_needs_review(id)
    }

    pub fn cancel_action(
        &self,
        id: u64,
        action: UndoHistoryAction,
    ) -> Result<(), UndoHistoryError> {
        self.update_record(id, |record, now| {
            let (expected, restored) = match action {
                UndoHistoryAction::Undo => (UndoHistoryState::Undoing, UndoHistoryState::Applied),
                UndoHistoryAction::Redo => (UndoHistoryState::Redoing, UndoHistoryState::Undone),
            };
            if record.state != expected {
                return Err(UndoHistoryError::InvalidTransition(id));
            }
            record.state = restored;
            record.updated_at = now;
            Ok(())
        })
    }

    pub fn resolve(&self, id: u64) -> Result<(), UndoHistoryError> {
        let mut inner = lock(&self.inner);
        let position = inner
            .records
            .iter()
            .position(|record| record.id == id)
            .ok_or(UndoHistoryError::UnknownRecord(id))?;
        if let Err(error) = cleanup_replace_record(&inner.records[position]) {
            inner.records[position].state = UndoHistoryState::NeedsReview;
            inner.records[position].updated_at = unix_now()?;
            persist(&inner.path, &inner.records)?;
            return Err(error);
        }
        let removed = inner.records.remove(position);
        if let Err(error) = persist(&inner.path, &inner.records) {
            inner.records.insert(position, removed);
            return Err(error);
        }
        Ok(())
    }

    pub fn records(&self) -> Vec<UndoHistoryRecord> {
        let mut records = lock(&self.inner).records.clone();
        records.sort_by_key(|record| std::cmp::Reverse(record.updated_at));
        records
    }

    pub fn history(&self) -> Vec<UndoHistoryRecord> {
        self.records()
            .into_iter()
            .filter(|record| record.state.is_history())
            .collect()
    }

    pub fn reviews(&self) -> Vec<UndoHistoryRecord> {
        self.records()
            .into_iter()
            .filter(|record| record.state.needs_review())
            .collect()
    }

    fn mark_needs_review(&self, id: u64) -> Result<(), UndoHistoryError> {
        self.update_record(id, |record, now| {
            record.state = UndoHistoryState::NeedsReview;
            record.updated_at = now;
            Ok(())
        })
    }

    fn update_record(
        &self,
        id: u64,
        update: impl FnOnce(&mut UndoHistoryRecord, u64) -> Result<(), UndoHistoryError>,
    ) -> Result<(), UndoHistoryError> {
        self.update_record_at(id, unix_now()?, update)
    }

    fn update_record_at(
        &self,
        id: u64,
        now: u64,
        update: impl FnOnce(&mut UndoHistoryRecord, u64) -> Result<(), UndoHistoryError>,
    ) -> Result<(), UndoHistoryError> {
        let mut inner = lock(&self.inner);
        let position = inner
            .records
            .iter()
            .position(|record| record.id == id)
            .ok_or(UndoHistoryError::UnknownRecord(id))?;
        let previous = inner.records[position].clone();
        update(&mut inner.records[position], now)?;
        if let Err(error) = persist(&inner.path, &inner.records) {
            inner.records[position] = previous;
            return Err(error);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct UndoHistoryCoordinator {
    inner: Arc<Mutex<UndoHistoryCoordinatorInner>>,
}

#[derive(Debug)]
struct UndoHistoryCoordinatorInner {
    path: PathBuf,
    store: Option<UndoHistoryStore>,
    blocked_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UndoHistoryHealth {
    Ready { history: usize, review: usize },
    Blocked { reason: String },
}

impl UndoHistoryCoordinator {
    pub fn load_at(path: PathBuf) -> Self {
        let (store, blocked_reason) = match UndoHistoryStore::open_at(path.clone()) {
            Ok(store) => (Some(store), None),
            Err(error) => (None, Some(error.to_string())),
        };
        Self {
            inner: Arc::new(Mutex::new(UndoHistoryCoordinatorInner {
                path,
                store,
                blocked_reason,
            })),
        }
    }

    pub fn from_store(store: UndoHistoryStore) -> Self {
        let path = lock(&store.inner).path.clone();
        Self {
            inner: Arc::new(Mutex::new(UndoHistoryCoordinatorInner {
                path,
                store: Some(store),
                blocked_reason: None,
            })),
        }
    }

    pub fn health(&self) -> UndoHistoryHealth {
        let inner = lock(&self.inner);
        match (&inner.store, &inner.blocked_reason) {
            (Some(store), _) => UndoHistoryHealth::Ready {
                history: store.history().len(),
                review: store.reviews().len(),
            },
            (None, Some(reason)) => UndoHistoryHealth::Blocked {
                reason: reason.clone(),
            },
            (None, None) => UndoHistoryHealth::Blocked {
                reason: "operation Undo history is unavailable".to_owned(),
            },
        }
    }

    pub fn begin(&self, recipe: UndoRecipe) -> Result<UndoHistoryTicket, UndoHistoryError> {
        self.ready_store()?.begin(recipe)
    }

    pub fn complete(
        &self,
        ticket: UndoHistoryTicket,
        identity: FileIdentity,
    ) -> Result<(), UndoHistoryError> {
        self.ready_store()?.complete(ticket, identity)
    }

    pub fn complete_replace(
        &self,
        ticket: UndoHistoryTicket,
        destination_identity: FileIdentity,
        backup_identity: FileIdentity,
    ) -> Result<(), UndoHistoryError> {
        self.ready_store()?
            .complete_replace(ticket, destination_identity, backup_identity)
    }

    pub fn retain_if_destination_exists(
        &self,
        ticket: UndoHistoryTicket,
        destination: &Path,
    ) -> Result<(), UndoHistoryError> {
        self.ready_store()?
            .retain_if_destination_exists(ticket, destination)
    }

    pub fn history(&self) -> Result<Vec<UndoHistoryRecord>, UndoHistoryError> {
        Ok(self.ready_store()?.history())
    }

    pub fn reviews(&self) -> Result<Vec<UndoHistoryRecord>, UndoHistoryError> {
        Ok(self.ready_store()?.reviews())
    }

    pub fn prepare_action(
        &self,
        id: u64,
        action: UndoHistoryAction,
    ) -> Result<UndoHistoryRecord, UndoHistoryError> {
        self.ready_store()?.prepare_action(id, action)
    }

    pub fn complete_action(
        &self,
        id: u64,
        action: UndoHistoryAction,
        identity: Option<FileIdentity>,
    ) -> Result<(), UndoHistoryError> {
        self.ready_store()?.complete_action(id, action, identity)
    }

    pub fn mark_action_uncertain(&self, id: u64) -> Result<(), UndoHistoryError> {
        self.ready_store()?.mark_action_uncertain(id)
    }

    pub fn cancel_action(
        &self,
        id: u64,
        action: UndoHistoryAction,
    ) -> Result<(), UndoHistoryError> {
        self.ready_store()?.cancel_action(id, action)
    }

    pub fn resolve(&self, id: u64) -> Result<(), UndoHistoryError> {
        self.ready_store()?.resolve(id)
    }

    pub fn reset_blocked(&self) -> Result<(), UndoHistoryError> {
        let path = {
            let inner = lock(&self.inner);
            if inner.store.is_some() {
                return Ok(());
            }
            inner.path.clone()
        };
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_dir() => {
                return Err(UndoHistoryError::Insecure(
                    "Undo history path is a directory",
                ));
            }
            Ok(_) => fs::remove_file(&path)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let store = UndoHistoryStore::open_at(path)?;
        let mut inner = lock(&self.inner);
        inner.store = Some(store);
        inner.blocked_reason = None;
        Ok(())
    }

    fn ready_store(&self) -> Result<UndoHistoryStore, UndoHistoryError> {
        let inner = lock(&self.inner);
        inner.store.clone().ok_or_else(|| {
            UndoHistoryError::Blocked(
                inner
                    .blocked_reason
                    .clone()
                    .unwrap_or_else(|| "operation Undo history is unavailable".to_owned()),
            )
        })
    }
}

#[derive(Debug, Error)]
pub enum UndoHistoryError {
    #[error("Undo history path or recipe contains an invalid unbounded path")]
    InvalidPath,
    #[error("Undo history is full of records requiring review")]
    CapacityExceeded,
    #[error("Undo history identifier space is exhausted")]
    IdentifierExhausted,
    #[error("Undo history record {0} is unavailable")]
    UnknownRecord(u64),
    #[error("Undo history action is unavailable for record {0}")]
    ActionUnavailable(u64),
    #[error("Undo history record {0} has an invalid state transition")]
    InvalidTransition(u64),
    #[error("Undo history record {0} is missing a committed identity")]
    MissingIdentity(u64),
    #[error("Undo history is corrupt: {0}")]
    Corrupt(&'static str),
    #[error("Undo history storage is insecure: {0}")]
    Insecure(&'static str),
    #[error("Undo history is blocked: {0}")]
    Blocked(String),
    #[error("Undo history I/O failed: {0}")]
    Io(#[from] io::Error),
}

fn validate_recipe(recipe: &UndoRecipe) -> Result<(), UndoHistoryError> {
    if recipe.destination().file_name().is_none() {
        return Err(UndoHistoryError::InvalidPath);
    }
    for path in recipe.source().into_iter().chain([recipe.destination()]) {
        if path.as_os_str().as_bytes().is_empty()
            || path.as_os_str().as_bytes().len() > MAX_PATH_BYTES
        {
            return Err(UndoHistoryError::InvalidPath);
        }
    }
    if let Some(backup) = recipe.replace_backup() {
        if backup.as_os_str().as_bytes().is_empty()
            || backup.as_os_str().as_bytes().len() > MAX_PATH_BYTES
            || backup.file_name().is_none()
            || backup
                .parent()
                .and_then(Path::file_name)
                .is_none_or(|name| name != REPLACE_BACKUP_DIRECTORY)
        {
            return Err(UndoHistoryError::InvalidPath);
        }
    }
    Ok(())
}

fn make_capacity(records: &mut Vec<UndoHistoryRecord>) -> Result<(), UndoHistoryError> {
    while records.len() >= MAX_UNDO_HISTORY_RECORDS {
        let positions = records
            .iter()
            .enumerate()
            .filter(|(_, record)| record.state.is_history())
            .map(|(position, record)| (position, record.updated_at))
            .collect::<Vec<_>>();
        if positions.is_empty() {
            return Err(UndoHistoryError::CapacityExceeded);
        }
        let mut positions = positions;
        positions.sort_by_key(|(_, updated_at)| *updated_at);
        let mut removed = false;
        for (position, _) in positions {
            match cleanup_replace_record(&records[position]) {
                Ok(()) => {
                    records.remove(position);
                    removed = true;
                    break;
                }
                Err(_) => records[position].state = UndoHistoryState::NeedsReview,
            }
        }
        if !removed {
            return Err(UndoHistoryError::CapacityExceeded);
        }
    }
    Ok(())
}

fn expire_records(records: &mut Vec<UndoHistoryRecord>, now: u64) {
    let mut position = records.len();
    while position > 0 {
        position -= 1;
        let expired = records[position].state.is_history()
            && now.saturating_sub(records[position].updated_at) > UNDO_HISTORY_TTL.as_secs();
        if !expired {
            continue;
        }
        match cleanup_replace_record(&records[position]) {
            Ok(()) => {
                records.remove(position);
            }
            Err(_) => {
                records[position].state = UndoHistoryState::NeedsReview;
                records[position].updated_at = now;
            }
        }
    }
}

fn cleanup_replace_record(record: &UndoHistoryRecord) -> Result<(), UndoHistoryError> {
    let Some(backup) = record.recipe.replace_backup() else {
        return Ok(());
    };
    let Some(identity) = record.alternate_identity else {
        return match fs::symlink_metadata(backup) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Ok(_) => Err(UndoHistoryError::Blocked(format!(
                "replacement backup {} exists without exact identity evidence",
                backup.display()
            ))),
            Err(error) => Err(error.into()),
        };
    };
    remove_replace_backup(backup, identity).map_err(|error| {
        UndoHistoryError::Blocked(format!(
            "replacement backup requires review before cleanup: {error}"
        ))
    })
}

fn unix_now() -> Result<u64, UndoHistoryError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| UndoHistoryError::Corrupt("system clock predates Unix epoch"))
}

fn prepare_private_parent(path: &Path) -> Result<(), UndoHistoryError> {
    if !path.is_absolute() || path.as_os_str().as_bytes().len() > MAX_PATH_BYTES {
        return Err(UndoHistoryError::InvalidPath);
    }
    let parent = path
        .parent()
        .ok_or(UndoHistoryError::Insecure("Undo history has no parent"))?;
    fs::create_dir_all(parent)?;
    let metadata = fs::symlink_metadata(parent)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(UndoHistoryError::Insecure(
            "Undo history parent is not a real directory",
        ));
    }
    if metadata.uid() != rustix::process::geteuid().as_raw() {
        return Err(UndoHistoryError::Insecure(
            "Undo history parent has the wrong owner",
        ));
    }
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(parent, permissions)?;
    Ok(())
}

fn load_records(path: &Path) -> Result<Vec<UndoHistoryRecord>, UndoHistoryError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(OFlags::NOFOLLOW.bits() as i32);
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(UndoHistoryError::Insecure(
            "Undo history file type, owner, or permissions are unsafe",
        ));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(UndoHistoryError::Corrupt("Undo history is oversized"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    decode(&bytes)
}

fn persist(path: &Path, records: &[UndoHistoryRecord]) -> Result<(), UndoHistoryError> {
    prepare_private_parent(path)?;
    if records.is_empty() {
        return remove_file_if_present(path);
    }
    let bytes = encode(records)?;
    let parent = path
        .parent()
        .ok_or(UndoHistoryError::Insecure("Undo history has no parent"))?;
    let name = path
        .file_name()
        .ok_or(UndoHistoryError::Insecure("Undo history has no file name"))?;
    let mut temporary = None;
    for attempt in 0..MAX_TEMP_ATTEMPTS {
        let mut temporary_name = OsString::from(".");
        temporary_name.push(name);
        temporary_name.push(format!(".tmp-{}-{attempt}", std::process::id()));
        let candidate = parent.join(temporary_name);
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(OFlags::NOFOLLOW.bits() as i32);
        match options.open(&candidate) {
            Ok(file) => {
                temporary = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    let (temporary_path, mut file) = temporary.ok_or_else(|| {
        UndoHistoryError::Io(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate private Undo history temporary file",
        ))
    })?;
    let result = (|| -> Result<(), io::Error> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary_path, path)?;
        File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result.map_err(UndoHistoryError::Io)
}

fn remove_file_if_present(path: &Path) -> Result<(), UndoHistoryError> {
    match fs::remove_file(path) {
        Ok(()) => {
            if let Some(parent) = path.parent() {
                File::open(parent)?.sync_all()?;
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn encode(records: &[UndoHistoryRecord]) -> Result<Vec<u8>, UndoHistoryError> {
    if records.len() > MAX_UNDO_HISTORY_RECORDS {
        return Err(UndoHistoryError::CapacityExceeded);
    }
    let mut output = Vec::new();
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&VERSION.to_le_bytes());
    output.extend_from_slice(&(records.len() as u32).to_le_bytes());
    for record in records {
        output.extend_from_slice(&record.id.to_le_bytes());
        output.extend_from_slice(&record.created_at.to_le_bytes());
        output.extend_from_slice(&record.updated_at.to_le_bytes());
        output.push(record.state as u8);
        encode_recipe(&mut output, &record.recipe)?;
        match record.current_identity {
            Some(identity) => {
                output.push(1);
                encode_identity(&mut output, identity);
            }
            None => output.push(0),
        }
        match record.alternate_identity {
            Some(identity) => {
                output.push(1);
                encode_identity(&mut output, identity);
            }
            None => output.push(0),
        }
    }
    if output.len() as u64 > MAX_FILE_BYTES {
        return Err(UndoHistoryError::Corrupt(
            "encoded Undo history is oversized",
        ));
    }
    Ok(output)
}

fn encode_recipe(output: &mut Vec<u8>, recipe: &UndoRecipe) -> Result<(), UndoHistoryError> {
    match recipe {
        UndoRecipe::Copy {
            source,
            destination,
            symlink_policy,
        } => {
            output.push(1);
            encode_path(output, source)?;
            encode_path(output, destination)?;
            output.push(match symlink_policy {
                SymlinkPolicy::Preserve => 1,
                SymlinkPolicy::Reject => 2,
            });
        }
        UndoRecipe::Move {
            source,
            destination,
        } => {
            output.push(2);
            encode_path(output, source)?;
            encode_path(output, destination)?;
        }
        UndoRecipe::Rename {
            source,
            destination,
        } => {
            output.push(3);
            encode_path(output, source)?;
            encode_path(output, destination)?;
        }
        UndoRecipe::Create(request) => {
            output.push(4);
            encode_path(output, request.destination())?;
            match request.kind() {
                CreateKind::Directory => output.push(1),
                CreateKind::EmptyFile => output.push(2),
                CreateKind::Template { source } => {
                    output.push(3);
                    encode_path(output, source)?;
                }
                CreateKind::Duplicate { source } => {
                    output.push(4);
                    encode_path(output, source)?;
                }
                CreateKind::SymbolicLink { target } => {
                    output.push(5);
                    encode_path(output, target)?;
                }
                CreateKind::HardLink { source } => {
                    output.push(6);
                    encode_path(output, source)?;
                }
            }
        }
        UndoRecipe::Replace {
            source,
            destination,
            backup,
            mode,
            symlink_policy,
        } => {
            output.push(5);
            encode_path(output, source)?;
            encode_path(output, destination)?;
            encode_path(output, backup)?;
            output.push(match mode {
                ReplaceMode::Copy => 1,
                ReplaceMode::Move => 2,
            });
            output.push(match symlink_policy {
                SymlinkPolicy::Preserve => 1,
                SymlinkPolicy::Reject => 2,
            });
        }
    }
    Ok(())
}

fn encode_identity(output: &mut Vec<u8>, identity: FileIdentity) {
    output.extend_from_slice(&identity.device().to_le_bytes());
    output.extend_from_slice(&identity.inode().to_le_bytes());
    output.extend_from_slice(&identity.mode().to_le_bytes());
    output.extend_from_slice(&identity.length().to_le_bytes());
    output.extend_from_slice(&identity.modified_seconds().to_le_bytes());
    output.extend_from_slice(&identity.modified_nanoseconds().to_le_bytes());
}

fn encode_path(output: &mut Vec<u8>, path: &Path) -> Result<(), UndoHistoryError> {
    let bytes = path.as_os_str().as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_PATH_BYTES {
        return Err(UndoHistoryError::InvalidPath);
    }
    output.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

fn decode(bytes: &[u8]) -> Result<Vec<UndoHistoryRecord>, UndoHistoryError> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let mut cursor = Cursor::new(bytes);
    if cursor.take(8)? != MAGIC {
        return Err(UndoHistoryError::Corrupt("invalid Undo history magic"));
    }
    let version = cursor.u16()?;
    if !(1..=VERSION).contains(&version) {
        return Err(UndoHistoryError::Corrupt(
            "unsupported Undo history version",
        ));
    }
    let count = cursor.u32()? as usize;
    if count > MAX_UNDO_HISTORY_RECORDS {
        return Err(UndoHistoryError::Corrupt("too many Undo history records"));
    }
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let id = cursor.u64()?;
        if id == 0
            || records
                .iter()
                .any(|record: &UndoHistoryRecord| record.id == id)
        {
            return Err(UndoHistoryError::Corrupt(
                "invalid or duplicate Undo history identifier",
            ));
        }
        let created_at = cursor.u64()?;
        let updated_at = cursor.u64()?;
        if updated_at < created_at {
            return Err(UndoHistoryError::Corrupt(
                "Undo history timestamp moved backwards",
            ));
        }
        let state = UndoHistoryState::decode(cursor.u8()?)
            .ok_or(UndoHistoryError::Corrupt("invalid Undo history state"))?;
        let recipe = decode_recipe(&mut cursor)?;
        let current_identity = match cursor.u8()? {
            0 => None,
            1 => Some(decode_identity(&mut cursor)?),
            _ => {
                return Err(UndoHistoryError::Corrupt(
                    "invalid Undo history identity marker",
                ));
            }
        };
        let alternate_identity = if version >= 2 {
            match cursor.u8()? {
                0 => None,
                1 => Some(decode_identity(&mut cursor)?),
                _ => {
                    return Err(UndoHistoryError::Corrupt(
                        "invalid alternate Undo history identity marker",
                    ));
                }
            }
        } else {
            None
        };
        if state == UndoHistoryState::Applied && current_identity.is_none() {
            return Err(UndoHistoryError::Corrupt(
                "applied Undo history record has no identity",
            ));
        }
        if matches!(recipe, UndoRecipe::Replace { .. })
            && (current_identity.is_none() || alternate_identity.is_none())
            && state.is_history()
        {
            return Err(UndoHistoryError::Corrupt(
                "completed replacement history lacks both version identities",
            ));
        }
        records.push(UndoHistoryRecord {
            id,
            created_at,
            updated_at,
            state,
            recipe,
            current_identity,
            alternate_identity,
        });
    }
    if !cursor.remaining().is_empty() {
        return Err(UndoHistoryError::Corrupt("trailing Undo history bytes"));
    }
    Ok(records)
}

fn decode_recipe(cursor: &mut Cursor<'_>) -> Result<UndoRecipe, UndoHistoryError> {
    match cursor.u8()? {
        1 => {
            let source = cursor.path()?;
            let destination = cursor.path()?;
            let symlink_policy = match cursor.u8()? {
                1 => SymlinkPolicy::Preserve,
                2 => SymlinkPolicy::Reject,
                _ => {
                    return Err(UndoHistoryError::Corrupt("invalid copy symlink policy"));
                }
            };
            Ok(UndoRecipe::copy(source, destination, symlink_policy))
        }
        2 => Ok(UndoRecipe::move_item(cursor.path()?, cursor.path()?)),
        3 => Ok(UndoRecipe::rename(cursor.path()?, cursor.path()?)),
        4 => {
            let destination = cursor.path()?;
            let request = match cursor.u8()? {
                1 => CreateRequest::directory(destination),
                2 => CreateRequest::empty_file(destination),
                3 => CreateRequest::template(cursor.path()?, destination),
                4 => CreateRequest::duplicate(cursor.path()?, destination),
                5 => CreateRequest::symbolic_link(cursor.path()?, destination),
                6 => CreateRequest::hard_link(cursor.path()?, destination),
                _ => {
                    return Err(UndoHistoryError::Corrupt("invalid create recipe kind"));
                }
            }
            .map_err(|_| UndoHistoryError::Corrupt("invalid create recipe"))?;
            Ok(UndoRecipe::create(request))
        }
        5 => {
            let source = cursor.path()?;
            let destination = cursor.path()?;
            let backup = cursor.path()?;
            let mode = match cursor.u8()? {
                1 => ReplaceMode::Copy,
                2 => ReplaceMode::Move,
                _ => return Err(UndoHistoryError::Corrupt("invalid replacement mode")),
            };
            let symlink_policy = match cursor.u8()? {
                1 => SymlinkPolicy::Preserve,
                2 => SymlinkPolicy::Reject,
                _ => {
                    return Err(UndoHistoryError::Corrupt(
                        "invalid replacement symlink policy",
                    ));
                }
            };
            Ok(UndoRecipe::replace(
                source,
                destination,
                backup,
                mode,
                symlink_policy,
            ))
        }
        _ => Err(UndoHistoryError::Corrupt("invalid Undo recipe kind")),
    }
}

fn decode_identity(cursor: &mut Cursor<'_>) -> Result<FileIdentity, UndoHistoryError> {
    Ok(FileIdentity::from_components(
        cursor.u64()?,
        cursor.u64()?,
        cursor.u32()?,
        cursor.u64()?,
        cursor.i64()?,
        cursor.i64()?,
    ))
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], UndoHistoryError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(UndoHistoryError::Corrupt("Undo history offset overflow"))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(UndoHistoryError::Corrupt("truncated Undo history"))?;
        self.offset = end;
        Ok(bytes)
    }

    fn u8(&mut self) -> Result<u8, UndoHistoryError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, UndoHistoryError> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| UndoHistoryError::Corrupt("invalid u16"))?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32, UndoHistoryError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| UndoHistoryError::Corrupt("invalid u32"))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, UndoHistoryError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| UndoHistoryError::Corrupt("invalid u64"))?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn i64(&mut self) -> Result<i64, UndoHistoryError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| UndoHistoryError::Corrupt("invalid i64"))?;
        Ok(i64::from_le_bytes(bytes))
    }

    fn path(&mut self) -> Result<PathBuf, UndoHistoryError> {
        let length = self.u32()? as usize;
        if length == 0 || length > MAX_PATH_BYTES {
            return Err(UndoHistoryError::Corrupt("invalid path length"));
        }
        Ok(PathBuf::from(OsString::from_vec(
            self.take(length)?.to_vec(),
        )))
    }

    fn remaining(&self) -> &'a [u8] {
        &self.bytes[self.offset..]
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::{
        os::unix::{ffi::OsStringExt, fs::PermissionsExt},
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };

    use floe_core::{
        ConflictPolicy, JobId, JobState, RenameRequest, ReplaceCancellation, ReplaceRequest,
        allocate_replace_backup, execute_replace,
    };
    use tempfile::tempdir;

    use crate::{
        copy_executor::CopyExecutor,
        create_executor::CreateExecutor,
        job_manager::{ApplicationJobManager, SharedJobManager},
        move_executor::MoveExecutor,
        operation_recovery::RecoveryCoordinator,
    };

    use super::*;

    fn identity(seed: u64) -> FileIdentity {
        FileIdentity::from_components(seed, seed + 1, 0o100644, seed + 2, 7, 8)
    }

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
            assert!(
                Instant::now() < deadline,
                "journaled operation did not finish"
            );
            std::thread::yield_now();
        }
    }

    #[test]
    fn phase_18y2_store_round_trips_raw_paths_identity_and_private_permissions() {
        let fixture = tempdir().expect("temporary history root");
        let path = fixture.path().join("state/floe/operation-undo-v1.bin");
        let source = fixture
            .path()
            .join(OsString::from_vec(b"source-\xff".to_vec()));
        let destination = fixture
            .path()
            .join(OsString::from_vec(b"destination-\xfe".to_vec()));
        let store = UndoHistoryStore::open_at_time(path.clone(), 100).expect("open store");
        let ticket = store
            .begin_at(
                UndoRecipe::copy(&source, &destination, SymlinkPolicy::Preserve),
                100,
            )
            .expect("begin record");
        store
            .complete(ticket, identity(11))
            .expect("complete record");

        let restored = UndoHistoryStore::open_at_time(path.clone(), 101).expect("restore store");
        let records = restored.history();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].recipe().source(), Some(source.as_path()));
        assert_eq!(records[0].recipe().destination(), destination);
        assert_eq!(records[0].current_identity(), Some(identity(11)));
        assert_eq!(
            fs::metadata(path.parent().expect("state parent"))
                .expect("parent metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(path)
                .expect("history metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn phase_18y2_store_expires_only_completed_history_and_preserves_review() {
        let fixture = tempdir().expect("temporary history root");
        let path = fixture.path().join("operation-undo-v1.bin");
        let store = UndoHistoryStore::open_at_time(path.clone(), 10).expect("open store");
        let applied = store
            .begin_at(
                UndoRecipe::move_item(
                    fixture.path().join("source"),
                    fixture.path().join("destination"),
                ),
                10,
            )
            .expect("begin applied");
        store
            .complete_at(applied, identity(1), 10)
            .expect("complete applied");
        let review = store
            .begin_at(
                UndoRecipe::move_item(
                    fixture.path().join("source-2"),
                    fixture.path().join("destination-2"),
                ),
                10,
            )
            .expect("begin review");
        store.mark_needs_review(review.id()).expect("mark review");

        let future = 10 + UNDO_HISTORY_TTL.as_secs() + 1;
        let restored = UndoHistoryStore::open_at_time(path, future).expect("restore store");
        assert!(restored.history().is_empty());
        assert_eq!(restored.reviews().len(), 1);
        assert_eq!(restored.reviews()[0].id(), review.id());
    }

    #[test]
    fn phase_18y2_store_action_state_machine_is_persisted() {
        let fixture = tempdir().expect("temporary history root");
        let store = UndoHistoryStore::open_at(fixture.path().join("history")).expect("store");
        let ticket = store
            .begin(UndoRecipe::create(
                CreateRequest::empty_file(fixture.path().join("created")).expect("request"),
            ))
            .expect("begin");
        store.complete(ticket, identity(5)).expect("complete");
        assert!(
            store
                .prepare_action(ticket.id(), UndoHistoryAction::Redo)
                .is_err()
        );
        store
            .prepare_action(ticket.id(), UndoHistoryAction::Undo)
            .expect("prepare undo");
        store
            .complete_action(ticket.id(), UndoHistoryAction::Undo, None)
            .expect("complete undo");
        assert!(store.history()[0].can_redo());
        store
            .prepare_action(ticket.id(), UndoHistoryAction::Redo)
            .expect("prepare redo");
        store
            .complete_action(ticket.id(), UndoHistoryAction::Redo, Some(identity(9)))
            .expect("complete redo");
        assert!(store.history()[0].can_undo());
        assert_eq!(store.history()[0].current_identity(), Some(identity(9)));
    }

    #[test]
    fn phase_18y2_local_copy_move_rename_create_prepare_and_commit_durable_recipes() {
        let fixture = tempdir().expect("fixture");
        let store = UndoHistoryStore::open_at(fixture.path().join("undo.bin")).expect("store");
        let history = UndoHistoryCoordinator::from_store(store.clone());
        let recovery = RecoveryCoordinator::load_at(fixture.path().join("recovery.bin"));
        let jobs = jobs();
        let copy = CopyExecutor::spawn_with_recovery_and_undo(
            Arc::clone(&jobs),
            recovery.clone(),
            history.clone(),
        )
        .expect("copy executor");
        let create = CreateExecutor::spawn_with_recovery_and_undo(
            Arc::clone(&jobs),
            recovery.clone(),
            history.clone(),
        )
        .expect("create executor");
        let move_item =
            MoveExecutor::spawn_with_recovery_and_undo(Arc::clone(&jobs), recovery, history)
                .expect("move executor");

        let copy_source = fixture.path().join("copy-source");
        let copy_destination = fixture.path().join("copy-destination");
        fs::write(&copy_source, b"copy").expect("copy source");
        let submission = copy
            .submit_copy(floe_core::CopyRequest::new(
                &copy_source,
                &copy_destination,
                ConflictPolicy::FailIfExists,
                SymlinkPolicy::Preserve,
            ))
            .expect("copy submit");
        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Completed
        );

        let created = fixture.path().join("created");
        let submission = create
            .submit(CreateRequest::empty_file(&created).expect("create request"))
            .expect("create submit");
        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Completed
        );

        let move_source = fixture.path().join("move-source");
        let move_destination = fixture.path().join("move-destination");
        fs::write(&move_source, b"move").expect("move source");
        let submission = move_item
            .submit_move(floe_core::MoveRequest::new(
                &move_source,
                &move_destination,
                ConflictPolicy::FailIfExists,
            ))
            .expect("move submit");
        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Completed
        );

        let rename_source = fixture.path().join("rename-source");
        fs::write(&rename_source, b"rename").expect("rename source");
        let submission = move_item
            .submit_rename(RenameRequest::new(
                &rename_source,
                "renamed",
                ConflictPolicy::FailIfExists,
            ))
            .expect("rename submit");
        assert_eq!(
            wait_for_terminal(&jobs, submission.job_id()),
            JobState::Completed
        );

        let records = store.history();
        assert_eq!(records.len(), 4);
        assert!(records.iter().all(UndoHistoryRecord::can_undo));
        assert!(
            records
                .iter()
                .all(|record| record.current_identity().is_some())
        );
        assert!(
            records
                .iter()
                .any(|record| matches!(record.recipe(), UndoRecipe::Copy { .. }))
        );
        assert!(
            records
                .iter()
                .any(|record| matches!(record.recipe(), UndoRecipe::Create(_)))
        );
        assert!(
            records
                .iter()
                .any(|record| matches!(record.recipe(), UndoRecipe::Move { .. }))
        );
        assert!(
            records
                .iter()
                .any(|record| matches!(record.recipe(), UndoRecipe::Rename { .. }))
        );
    }

    #[test]
    fn phase_18y2_store_rejects_corrupt_symlinked_and_insecure_files() {
        let fixture = tempdir().expect("temporary history root");
        let corrupt = fixture.path().join("corrupt");
        fs::write(&corrupt, b"not history").expect("corrupt fixture");
        assert!(UndoHistoryStore::open_at(corrupt).is_err());

        let target = fixture.path().join("target");
        fs::write(&target, b"target").expect("target fixture");
        let link = fixture.path().join("link");
        std::os::unix::fs::symlink(&target, &link).expect("link fixture");
        assert!(UndoHistoryStore::open_at(link).is_err());

        let insecure = fixture.path().join("insecure");
        fs::write(&insecure, encode(&[]).expect("empty codec")).expect("insecure fixture");
        fs::set_permissions(&insecure, fs::Permissions::from_mode(0o644))
            .expect("insecure permissions");
        assert!(matches!(
            UndoHistoryStore::open_at(insecure),
            Err(UndoHistoryError::Insecure(_))
        ));
    }

    #[test]
    fn phase_6u_recovery_restart_retains_both_replace_identities() {
        let fixture = tempdir().expect("temporary history root");
        let history_path = fixture.path().join("state/undo.bin");
        let source = fixture.path().join("incoming");
        let destination = fixture.path().join("item");
        fs::write(&source, b"new").expect("incoming version");
        fs::write(&destination, b"old").expect("existing version");
        let backup = allocate_replace_backup(&destination, 90).expect("private backup");
        let request = ReplaceRequest::new(
            &source,
            &destination,
            &backup,
            ReplaceMode::Copy,
            SymlinkPolicy::Preserve,
            FileIdentity::capture(&source).expect("source identity"),
            FileIdentity::capture(&destination).expect("destination identity"),
        );
        let store = UndoHistoryStore::open_at(history_path.clone()).expect("history store");
        let ticket = store
            .begin(UndoRecipe::replace(
                &source,
                &destination,
                &backup,
                ReplaceMode::Copy,
                SymlinkPolicy::Preserve,
            ))
            .expect("begin replacement history");
        let outcome = execute_replace(&request, &ReplaceCancellation::new()).expect("replace");
        store
            .complete_replace(
                ticket,
                outcome.destination_identity(),
                outcome.backup_identity(),
            )
            .expect("complete replacement history");

        let restored = UndoHistoryStore::open_at(history_path).expect("restart history");
        let records = restored.history();
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].current_identity(),
            Some(outcome.destination_identity())
        );
        assert_eq!(
            records[0].alternate_identity(),
            Some(outcome.backup_identity())
        );
        assert_eq!(records[0].recipe().replace_backup(), Some(backup.as_path()));
    }

    #[test]
    fn phase_6u_recovery_expiry_cleans_only_identity_owned_backup() {
        let fixture = tempdir().expect("temporary history root");
        let history_path = fixture.path().join("state/undo.bin");
        let source = fixture.path().join("incoming");
        let destination = fixture.path().join("item");
        fs::write(&source, b"new").expect("incoming version");
        fs::write(&destination, b"old").expect("existing version");
        let backup = allocate_replace_backup(&destination, 91).expect("private backup");
        let request = ReplaceRequest::new(
            &source,
            &destination,
            &backup,
            ReplaceMode::Copy,
            SymlinkPolicy::Preserve,
            FileIdentity::capture(&source).expect("source identity"),
            FileIdentity::capture(&destination).expect("destination identity"),
        );
        let store = UndoHistoryStore::open_at(history_path.clone()).expect("history store");
        let ticket = store
            .begin(UndoRecipe::replace(
                &source,
                &destination,
                &backup,
                ReplaceMode::Copy,
                SymlinkPolicy::Preserve,
            ))
            .expect("begin replacement history");
        let outcome = execute_replace(&request, &ReplaceCancellation::new()).expect("replace");
        store
            .complete_replace(
                ticket,
                outcome.destination_identity(),
                outcome.backup_identity(),
            )
            .expect("complete replacement history");
        let expired_at = unix_now()
            .expect("clock")
            .saturating_add(UNDO_HISTORY_TTL.as_secs())
            .saturating_add(1);

        let restored = UndoHistoryStore::open_at_time(history_path, expired_at).expect("expire");
        assert!(restored.history().is_empty());
        assert!(!backup.exists());
    }

    #[test]
    fn phase_6u_recovery_changed_backup_remains_review_required() {
        let fixture = tempdir().expect("temporary history root");
        let history_path = fixture.path().join("state/undo.bin");
        let source = fixture.path().join("incoming");
        let destination = fixture.path().join("item");
        fs::write(&source, b"new").expect("incoming version");
        fs::write(&destination, b"old").expect("existing version");
        let backup = allocate_replace_backup(&destination, 92).expect("private backup");
        let request = ReplaceRequest::new(
            &source,
            &destination,
            &backup,
            ReplaceMode::Copy,
            SymlinkPolicy::Preserve,
            FileIdentity::capture(&source).expect("source identity"),
            FileIdentity::capture(&destination).expect("destination identity"),
        );
        let store = UndoHistoryStore::open_at(history_path).expect("history store");
        let ticket = store
            .begin(UndoRecipe::replace(
                &source,
                &destination,
                &backup,
                ReplaceMode::Copy,
                SymlinkPolicy::Preserve,
            ))
            .expect("begin replacement history");
        let outcome = execute_replace(&request, &ReplaceCancellation::new()).expect("replace");
        store
            .complete_replace(
                ticket,
                outcome.destination_identity(),
                outcome.backup_identity(),
            )
            .expect("complete replacement history");
        fs::remove_file(&backup).expect("remove owned backup fixture");
        fs::write(&backup, b"unrelated occupant").expect("changed backup occupant");

        assert!(store.resolve(ticket.id()).is_err());
        let reviews = store.reviews();
        assert_eq!(reviews.len(), 1);
        assert_eq!(reviews[0].id(), ticket.id());
        assert_eq!(
            fs::read(&backup).expect("occupant retained"),
            b"unrelated occupant"
        );
    }
}
