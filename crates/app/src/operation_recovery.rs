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
};

use rustix::fs::OFlags;
use thiserror::Error;

const MAGIC: &[u8; 8] = b"FLOEOR01";
const VERSION: u16 = 1;
const MAX_RECORDS: usize = 1_024;
const MAX_PATH_BYTES: usize = 16 * 1_024;
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TEMP_ATTEMPTS: u32 = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RecoveryOperationKind {
    Copy = 1,
    Move = 2,
    Rename = 3,
    Create = 4,
}

impl RecoveryOperationKind {
    fn decode(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Copy),
            2 => Some(Self::Move),
            3 => Some(Self::Rename),
            4 => Some(Self::Create),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Move => "Move",
            Self::Rename => "Rename",
            Self::Create => "Create",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RecoveryRecordState {
    InProgress = 1,
    NeedsReview = 2,
}

impl RecoveryRecordState {
    fn decode(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::InProgress),
            2 => Some(Self::NeedsReview),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryRecord {
    id: u64,
    process_id: u32,
    kind: RecoveryOperationKind,
    state: RecoveryRecordState,
    source: Option<PathBuf>,
    destination: PathBuf,
}

impl RecoveryRecord {
    pub const fn id(&self) -> u64 {
        self.id
    }

    pub const fn process_id(&self) -> u32 {
        self.process_id
    }

    pub const fn kind(&self) -> RecoveryOperationKind {
        self.kind
    }

    pub const fn state(&self) -> RecoveryRecordState {
        self.state
    }

    pub fn source(&self) -> Option<&Path> {
        self.source.as_deref()
    }

    pub fn destination(&self) -> &Path {
        &self.destination
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryTicket(u64);

#[derive(Clone, Debug)]
pub struct RecoveryJournal {
    inner: Arc<Mutex<RecoveryJournalInner>>,
}

/// Shared recovery capability used by operation workers. A corrupt or insecure
/// store blocks new journaled mutations without preventing read-only browsing.
#[derive(Clone, Debug)]
pub struct RecoveryCoordinator {
    inner: Arc<Mutex<RecoveryCoordinatorInner>>,
}

#[derive(Debug)]
struct RecoveryCoordinatorInner {
    path: PathBuf,
    journal: Option<RecoveryJournal>,
    blocked_reason: Option<String>,
    startup_max_id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryStoreHealth {
    Ready { pending_records: usize },
    Blocked { reason: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryPathStatus {
    Missing,
    Present,
    Inaccessible,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryReview {
    record: RecoveryRecord,
    source_status: Option<RecoveryPathStatus>,
    destination_status: RecoveryPathStatus,
    interrupted: bool,
}

impl RecoveryReview {
    pub fn from_record(record: RecoveryRecord, interrupted: bool) -> Self {
        let source_status = record.source().map(path_status);
        let destination_status = path_status(record.destination());
        Self {
            record,
            source_status,
            destination_status,
            interrupted,
        }
    }

    pub fn record(&self) -> &RecoveryRecord {
        &self.record
    }

    pub const fn source_status(&self) -> Option<RecoveryPathStatus> {
        self.source_status
    }

    pub const fn destination_status(&self) -> RecoveryPathStatus {
        self.destination_status
    }

    pub fn can_retry(&self) -> bool {
        self.interrupted
            && self.record.kind() != RecoveryOperationKind::Create
            && self.source_status == Some(RecoveryPathStatus::Present)
            && self.destination_status == RecoveryPathStatus::Missing
    }

    pub fn needs_urgent_review(&self) -> bool {
        self.interrupted
            && (self.destination_status != RecoveryPathStatus::Missing
                || self.source_status == Some(RecoveryPathStatus::Missing))
    }

    pub const fn can_resolve(&self) -> bool {
        self.interrupted
    }
}

#[derive(Debug)]
struct RecoveryJournalInner {
    path: PathBuf,
    next_id: u64,
    records: Vec<RecoveryRecord>,
}

impl RecoveryJournal {
    pub fn open_at(path: PathBuf) -> Result<Self, RecoveryJournalError> {
        prepare_private_parent(&path)?;
        let records = load_records(&path)?;
        let next_id = records
            .iter()
            .map(RecoveryRecord::id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(RecoveryJournalError::IdentifierExhausted)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(RecoveryJournalInner {
                path,
                next_id,
                records,
            })),
        })
    }

    pub fn begin(
        &self,
        kind: RecoveryOperationKind,
        source: Option<&Path>,
        destination: &Path,
    ) -> Result<RecoveryTicket, RecoveryJournalError> {
        validate_path(destination)?;
        if let Some(source) = source {
            validate_path(source)?;
        }
        let mut inner = lock(&self.inner);
        if inner.records.len() >= MAX_RECORDS {
            return Err(RecoveryJournalError::CapacityExceeded);
        }
        let id = inner.next_id;
        inner.next_id = inner
            .next_id
            .checked_add(1)
            .ok_or(RecoveryJournalError::IdentifierExhausted)?;
        inner.records.push(RecoveryRecord {
            id,
            process_id: std::process::id(),
            kind,
            state: RecoveryRecordState::InProgress,
            source: source.map(Path::to_path_buf),
            destination: destination.to_path_buf(),
        });
        if let Err(error) = persist(&inner.path, &inner.records) {
            inner.records.pop();
            inner.next_id = id;
            return Err(error);
        }
        Ok(RecoveryTicket(id))
    }

    pub fn mark_needs_review(&self, ticket: RecoveryTicket) -> Result<(), RecoveryJournalError> {
        let mut inner = lock(&self.inner);
        let record = inner
            .records
            .iter_mut()
            .find(|record| record.id == ticket.0)
            .ok_or(RecoveryJournalError::UnknownRecord(ticket.0))?;
        let previous = record.state;
        record.state = RecoveryRecordState::NeedsReview;
        if let Err(error) = persist(&inner.path, &inner.records) {
            if let Some(record) = inner
                .records
                .iter_mut()
                .find(|record| record.id == ticket.0)
            {
                record.state = previous;
            }
            return Err(error);
        }
        Ok(())
    }

    pub fn finish(&self, ticket: RecoveryTicket) -> Result<(), RecoveryJournalError> {
        self.resolve(ticket.0)
    }

    pub fn resolve(&self, id: u64) -> Result<(), RecoveryJournalError> {
        let mut inner = lock(&self.inner);
        let position = inner
            .records
            .iter()
            .position(|record| record.id == id)
            .ok_or(RecoveryJournalError::UnknownRecord(id))?;
        let removed = inner.records.remove(position);
        if let Err(error) = persist(&inner.path, &inner.records) {
            inner.records.insert(position, removed);
            return Err(error);
        }
        Ok(())
    }

    pub fn pending(&self) -> Vec<RecoveryRecord> {
        lock(&self.inner).records.clone()
    }

    pub fn retain_if_destination_exists(
        &self,
        ticket: RecoveryTicket,
        destination: &Path,
    ) -> Result<(), RecoveryJournalError> {
        match fs::symlink_metadata(destination) {
            Ok(_) => self.mark_needs_review(ticket),
            Err(error) if error.kind() == io::ErrorKind::NotFound => self.finish(ticket),
            Err(_) => self.mark_needs_review(ticket),
        }
    }
}

impl RecoveryCoordinator {
    pub fn load_at(path: PathBuf) -> Self {
        let (journal, blocked_reason) = match RecoveryJournal::open_at(path.clone()) {
            Ok(journal) => (Some(journal), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let startup_max_id = journal
            .as_ref()
            .and_then(|journal| journal.pending().iter().map(RecoveryRecord::id).max())
            .unwrap_or(0);
        Self {
            inner: Arc::new(Mutex::new(RecoveryCoordinatorInner {
                path,
                journal,
                blocked_reason,
                startup_max_id,
            })),
        }
    }

    pub fn from_journal(journal: RecoveryJournal) -> Self {
        let path = lock(&journal.inner).path.clone();
        let startup_max_id = journal
            .pending()
            .iter()
            .map(RecoveryRecord::id)
            .max()
            .unwrap_or(0);
        Self {
            inner: Arc::new(Mutex::new(RecoveryCoordinatorInner {
                path,
                journal: Some(journal),
                blocked_reason: None,
                startup_max_id,
            })),
        }
    }

    pub fn health(&self) -> RecoveryStoreHealth {
        let inner = lock(&self.inner);
        match (&inner.journal, &inner.blocked_reason) {
            (Some(journal), _) => RecoveryStoreHealth::Ready {
                pending_records: journal.pending().len(),
            },
            (None, Some(reason)) => RecoveryStoreHealth::Blocked {
                reason: reason.clone(),
            },
            (None, None) => RecoveryStoreHealth::Blocked {
                reason: "operation recovery is unavailable".to_owned(),
            },
        }
    }

    pub fn begin(
        &self,
        kind: RecoveryOperationKind,
        source: Option<&Path>,
        destination: &Path,
    ) -> Result<RecoveryTicket, RecoveryJournalError> {
        let journal = self.ready_journal()?;
        journal.begin(kind, source, destination)
    }

    pub fn finish(&self, ticket: RecoveryTicket) -> Result<(), RecoveryJournalError> {
        self.ready_journal()?.finish(ticket)
    }

    pub fn retain_if_destination_exists(
        &self,
        ticket: RecoveryTicket,
        destination: &Path,
    ) -> Result<(), RecoveryJournalError> {
        self.ready_journal()?
            .retain_if_destination_exists(ticket, destination)
    }

    pub fn pending(&self) -> Result<Vec<RecoveryRecord>, RecoveryJournalError> {
        Ok(self.ready_journal()?.pending())
    }

    pub fn reviews(&self) -> Result<Vec<RecoveryReview>, RecoveryJournalError> {
        let (journal, startup_max_id) = {
            let inner = lock(&self.inner);
            let journal = inner.journal.clone().ok_or_else(|| {
                RecoveryJournalError::Blocked(
                    inner
                        .blocked_reason
                        .clone()
                        .unwrap_or_else(|| "operation recovery is unavailable".to_owned()),
                )
            })?;
            (journal, inner.startup_max_id)
        };
        Ok(journal
            .pending()
            .into_iter()
            .map(|record| {
                let interrupted = record.id() <= startup_max_id;
                RecoveryReview::from_record(record, interrupted)
            })
            .collect())
    }

    pub fn resolve(&self, id: u64) -> Result<(), RecoveryJournalError> {
        let journal = {
            let inner = lock(&self.inner);
            if id > inner.startup_max_id {
                return Err(RecoveryJournalError::ActiveRecord(id));
            }
            inner.journal.clone().ok_or_else(|| {
                RecoveryJournalError::Blocked(
                    inner
                        .blocked_reason
                        .clone()
                        .unwrap_or_else(|| "operation recovery is unavailable".to_owned()),
                )
            })?
        };
        journal.resolve(id)
    }

    /// Explicitly discards an unreadable recovery store and immediately
    /// re-enables journaling. Callers must obtain deliberate user consent.
    pub fn reset_blocked(&self) -> Result<(), RecoveryJournalError> {
        let path = {
            let inner = lock(&self.inner);
            if inner.journal.is_some() {
                return Ok(());
            }
            inner.path.clone()
        };
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_dir() => {
                return Err(RecoveryJournalError::Insecure(
                    "recovery store path is a directory",
                ));
            }
            Ok(_) => fs::remove_file(&path)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let journal = RecoveryJournal::open_at(path)?;
        let mut inner = lock(&self.inner);
        inner.journal = Some(journal);
        inner.blocked_reason = None;
        inner.startup_max_id = 0;
        Ok(())
    }

    fn ready_journal(&self) -> Result<RecoveryJournal, RecoveryJournalError> {
        let inner = lock(&self.inner);
        inner.journal.clone().ok_or_else(|| {
            RecoveryJournalError::Blocked(
                inner
                    .blocked_reason
                    .clone()
                    .unwrap_or_else(|| "operation recovery is unavailable".to_owned()),
            )
        })
    }
}

#[derive(Debug, Error)]
pub enum RecoveryJournalError {
    #[error("operation recovery path must be absolute and bounded")]
    InvalidPath,
    #[error("operation recovery journal reached its {MAX_RECORDS}-record limit")]
    CapacityExceeded,
    #[error("operation recovery identifier space was exhausted")]
    IdentifierExhausted,
    #[error("operation recovery record {0} is unavailable")]
    UnknownRecord(u64),
    #[error("operation recovery record {0} belongs to a running operation")]
    ActiveRecord(u64),
    #[error("operation recovery journal is corrupt: {0}")]
    Corrupt(&'static str),
    #[error("operation recovery storage is insecure: {0}")]
    Insecure(&'static str),
    #[error("operation recovery storage failed: {0}")]
    Io(#[from] io::Error),
    #[error("operation recovery is blocked: {0}")]
    Blocked(String),
}

fn validate_path(path: &Path) -> Result<(), RecoveryJournalError> {
    if !path.is_absolute() || path.as_os_str().as_bytes().len() > MAX_PATH_BYTES {
        return Err(RecoveryJournalError::InvalidPath);
    }
    Ok(())
}

fn path_status(path: &Path) -> RecoveryPathStatus {
    match fs::symlink_metadata(path) {
        Ok(_) => RecoveryPathStatus::Present,
        Err(error) if error.kind() == io::ErrorKind::NotFound => RecoveryPathStatus::Missing,
        Err(_) => RecoveryPathStatus::Inaccessible,
    }
}

fn prepare_private_parent(path: &Path) -> Result<(), RecoveryJournalError> {
    let parent = path
        .parent()
        .ok_or(RecoveryJournalError::Insecure("journal has no parent"))?;
    fs::create_dir_all(parent)?;
    let metadata = fs::symlink_metadata(parent)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(RecoveryJournalError::Insecure(
            "journal parent is not a real directory",
        ));
    }
    if metadata.uid() != rustix::process::geteuid().as_raw() {
        return Err(RecoveryJournalError::Insecure(
            "journal parent is owned by another user",
        ));
    }
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn load_records(path: &Path) -> Result<Vec<RecoveryRecord>, RecoveryJournalError> {
    let mut options = OpenOptions::new();
    options.read(true);
    options.custom_flags(OFlags::NOFOLLOW.bits() as i32);
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
        return Err(RecoveryJournalError::Insecure(
            "journal file type, owner, or permissions are unsafe",
        ));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(RecoveryJournalError::Corrupt("journal is oversized"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    decode(&bytes)
}

fn persist(path: &Path, records: &[RecoveryRecord]) -> Result<(), RecoveryJournalError> {
    prepare_private_parent(path)?;
    if records.is_empty() {
        return remove_file_if_present(path);
    }
    let bytes = encode(records)?;
    let parent = path
        .parent()
        .ok_or(RecoveryJournalError::Insecure("journal has no parent"))?;
    let name = path
        .file_name()
        .ok_or(RecoveryJournalError::Insecure("journal has no file name"))?;
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
    let (temporary, mut file) = temporary.ok_or(RecoveryJournalError::Io(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no recovery temp name available",
    )))?;
    let result = (|| -> io::Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(RecoveryJournalError::Io)
}

fn remove_file_if_present(path: &Path) -> Result<(), RecoveryJournalError> {
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

fn encode(records: &[RecoveryRecord]) -> Result<Vec<u8>, RecoveryJournalError> {
    if records.len() > MAX_RECORDS {
        return Err(RecoveryJournalError::CapacityExceeded);
    }
    let mut output = Vec::new();
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&VERSION.to_le_bytes());
    output.extend_from_slice(&(records.len() as u32).to_le_bytes());
    for record in records {
        output.extend_from_slice(&record.id.to_le_bytes());
        output.extend_from_slice(&record.process_id.to_le_bytes());
        output.push(record.kind as u8);
        output.push(record.state as u8);
        match &record.source {
            Some(source) => {
                output.push(1);
                encode_path(&mut output, source)?;
            }
            None => output.push(0),
        }
        encode_path(&mut output, &record.destination)?;
    }
    if output.len() as u64 > MAX_FILE_BYTES {
        return Err(RecoveryJournalError::Corrupt(
            "encoded journal is oversized",
        ));
    }
    Ok(output)
}

fn encode_path(output: &mut Vec<u8>, path: &Path) -> Result<(), RecoveryJournalError> {
    validate_path(path)?;
    let bytes = path.as_os_str().as_bytes();
    output.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

fn decode(bytes: &[u8]) -> Result<Vec<RecoveryRecord>, RecoveryJournalError> {
    let mut cursor = Cursor::new(bytes);
    if cursor.take(8)? != MAGIC {
        return Err(RecoveryJournalError::Corrupt("invalid journal magic"));
    }
    if cursor.u16()? != VERSION {
        return Err(RecoveryJournalError::Corrupt("unsupported journal version"));
    }
    let count = cursor.u32()? as usize;
    if count > MAX_RECORDS {
        return Err(RecoveryJournalError::Corrupt("too many journal records"));
    }
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let id = cursor.u64()?;
        if id == 0
            || records
                .iter()
                .any(|record: &RecoveryRecord| record.id == id)
        {
            return Err(RecoveryJournalError::Corrupt(
                "invalid or duplicate record id",
            ));
        }
        let process_id = cursor.u32()?;
        let kind = RecoveryOperationKind::decode(cursor.u8()?)
            .ok_or(RecoveryJournalError::Corrupt("invalid operation kind"))?;
        let state = RecoveryRecordState::decode(cursor.u8()?)
            .ok_or(RecoveryJournalError::Corrupt("invalid record state"))?;
        let source = match cursor.u8()? {
            0 => None,
            1 => Some(cursor.path()?),
            _ => return Err(RecoveryJournalError::Corrupt("invalid source marker")),
        };
        let destination = cursor.path()?;
        records.push(RecoveryRecord {
            id,
            process_id,
            kind,
            state,
            source,
            destination,
        });
    }
    if !cursor.remaining().is_empty() {
        return Err(RecoveryJournalError::Corrupt("trailing journal bytes"));
    }
    Ok(records)
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], RecoveryJournalError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(RecoveryJournalError::Corrupt("journal offset overflow"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(RecoveryJournalError::Corrupt("truncated journal"))?;
        self.offset = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, RecoveryJournalError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, RecoveryJournalError> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| RecoveryJournalError::Corrupt("invalid u16"))?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32, RecoveryJournalError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| RecoveryJournalError::Corrupt("invalid u32"))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, RecoveryJournalError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| RecoveryJournalError::Corrupt("invalid u64"))?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn path(&mut self) -> Result<PathBuf, RecoveryJournalError> {
        let length = self.u32()? as usize;
        if length == 0 || length > MAX_PATH_BYTES {
            return Err(RecoveryJournalError::Corrupt("invalid path length"));
        }
        let path = PathBuf::from(OsString::from_vec(self.take(length)?.to_vec()));
        validate_path(&path).map_err(|_| RecoveryJournalError::Corrupt("invalid path"))?;
        Ok(path)
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
    use std::{os::unix::ffi::OsStringExt, os::unix::fs::PermissionsExt};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn phase_18y_journal_round_trips_raw_paths_privately_and_atomically() {
        let fixture = tempdir().expect("temporary recovery root");
        let path = fixture.path().join("state/floe/operation-recovery-v1.bin");
        let source = fixture
            .path()
            .join(OsString::from_vec(b"source-\xff".to_vec()));
        let destination = fixture
            .path()
            .join(OsString::from_vec(b"destination-\xfe".to_vec()));
        let journal = RecoveryJournal::open_at(path.clone()).expect("open journal");
        let ticket = journal
            .begin(RecoveryOperationKind::Copy, Some(&source), &destination)
            .expect("begin recovery record");
        journal
            .mark_needs_review(ticket)
            .expect("retain recovery record");

        let restored = RecoveryJournal::open_at(path.clone()).expect("restore journal");
        assert_eq!(restored.pending(), journal.pending());
        assert_eq!(restored.pending()[0].source(), Some(source.as_path()));
        assert_eq!(restored.pending()[0].destination(), destination);
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
                .expect("journal metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn phase_18y_success_removes_record_and_empty_journal() {
        let fixture = tempdir().expect("temporary recovery root");
        let path = fixture.path().join("operation-recovery-v1.bin");
        let journal = RecoveryJournal::open_at(path.clone()).expect("open journal");
        let ticket = journal
            .begin(
                RecoveryOperationKind::Create,
                None,
                &fixture.path().join("created"),
            )
            .expect("begin recovery record");
        journal.finish(ticket).expect("finish record");
        assert!(journal.pending().is_empty());
        assert!(!path.exists());
    }

    #[test]
    fn phase_18y_failed_output_is_retained_only_when_destination_may_exist() {
        let fixture = tempdir().expect("temporary recovery root");
        let journal =
            RecoveryJournal::open_at(fixture.path().join("journal")).expect("open journal");
        let absent = fixture.path().join("absent");
        let first = journal
            .begin(RecoveryOperationKind::Copy, None, &absent)
            .expect("first record");
        journal
            .retain_if_destination_exists(first, &absent)
            .expect("remove safe record");
        assert!(journal.pending().is_empty());

        let present = fixture.path().join("present");
        fs::write(&present, b"partial").expect("partial output");
        let second = journal
            .begin(RecoveryOperationKind::Copy, None, &present)
            .expect("second record");
        journal
            .retain_if_destination_exists(second, &present)
            .expect("retain ambiguous output");
        assert_eq!(
            journal.pending()[0].state(),
            RecoveryRecordState::NeedsReview
        );
    }

    #[test]
    fn phase_18y_corrupt_symlinked_and_insecure_journals_fail_closed() {
        let fixture = tempdir().expect("temporary recovery root");
        let corrupt = fixture.path().join("corrupt");
        fs::write(&corrupt, b"not a journal").expect("corrupt fixture");
        assert!(matches!(
            RecoveryJournal::open_at(corrupt),
            Err(RecoveryJournalError::Insecure(_)) | Err(RecoveryJournalError::Corrupt(_))
        ));

        let target = fixture.path().join("target");
        fs::write(&target, b"target").expect("target fixture");
        let link = fixture.path().join("link");
        std::os::unix::fs::symlink(&target, &link).expect("symlink fixture");
        assert!(RecoveryJournal::open_at(link).is_err());

        let insecure = fixture.path().join("insecure");
        fs::write(&insecure, encode(&[]).expect("empty codec")).expect("insecure file");
        fs::set_permissions(&insecure, fs::Permissions::from_mode(0o644)).expect("insecure mode");
        assert!(matches!(
            RecoveryJournal::open_at(insecure),
            Err(RecoveryJournalError::Insecure(_))
        ));
    }

    #[test]
    fn phase_18y_blocked_coordinator_requires_explicit_reset() {
        let fixture = tempdir().expect("temporary recovery root");
        let path = fixture.path().join("operation-recovery-v1.bin");
        fs::write(&path, b"corrupt recovery data").expect("corrupt fixture");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("private mode");
        let coordinator = RecoveryCoordinator::load_at(path.clone());
        assert!(matches!(
            coordinator.health(),
            RecoveryStoreHealth::Blocked { .. }
        ));
        let destination = fixture.path().join("destination");
        assert!(matches!(
            coordinator.begin(RecoveryOperationKind::Create, None, &destination),
            Err(RecoveryJournalError::Blocked(_))
        ));
        assert!(
            path.exists(),
            "blocked data is retained until explicit reset"
        );

        coordinator
            .reset_blocked()
            .expect("explicit reset should restore private journal capability");
        assert_eq!(
            coordinator.health(),
            RecoveryStoreHealth::Ready { pending_records: 0 }
        );
        assert!(!path.exists(), "reset removes only the corrupt journal");
    }

    #[test]
    fn phase_18y_review_retry_requires_old_record_intact_source_and_absent_destination() {
        let fixture = tempdir().expect("temporary recovery root");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"source").expect("source fixture");
        let mut record = RecoveryRecord {
            id: 41,
            process_id: std::process::id().wrapping_add(1),
            kind: RecoveryOperationKind::Copy,
            state: RecoveryRecordState::NeedsReview,
            source: Some(source.clone()),
            destination: destination.clone(),
        };
        let review = RecoveryReview::from_record(record.clone(), true);
        assert!(review.can_retry());
        fs::write(&destination, b"uncertain output").expect("destination fixture");
        assert!(!RecoveryReview::from_record(record.clone(), true).can_retry());
        fs::remove_file(&destination).expect("remove destination fixture");
        fs::remove_file(&source).expect("remove source fixture");
        assert!(RecoveryReview::from_record(record.clone(), true).needs_urgent_review());
        record.kind = RecoveryOperationKind::Create;
        assert!(!RecoveryReview::from_record(record, true).can_retry());
    }

    #[test]
    fn phase_18y_startup_boundary_not_process_id_controls_resolution() {
        let fixture = tempdir().expect("temporary recovery root");
        let path = fixture.path().join("recovery.bin");
        let journal = RecoveryJournal::open_at(path.clone()).expect("journal");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"source").expect("source fixture");
        let _ticket = journal
            .begin(RecoveryOperationKind::Copy, Some(&source), &destination)
            .expect("startup record");

        let restarted = RecoveryCoordinator::load_at(path);
        let old = restarted.reviews().expect("restart reviews");
        assert_eq!(old.len(), 1);
        assert!(old[0].can_resolve());
        assert_eq!(old[0].record().process_id(), std::process::id());
        restarted
            .resolve(old[0].record().id())
            .expect("startup record can be explicitly resolved despite PID reuse");

        let current = RecoveryCoordinator::from_journal(
            RecoveryJournal::open_at(fixture.path().join("current.bin")).expect("current journal"),
        );
        let current_ticket = current
            .begin(RecoveryOperationKind::Copy, Some(&source), &destination)
            .expect("current record");
        let current_review = current.reviews().expect("current reviews");
        assert!(!current_review[0].can_resolve());
        assert!(matches!(
            current.resolve(current_ticket.0),
            Err(RecoveryJournalError::ActiveRecord(_))
        ));
    }
}
