use std::{
    fs::{self, File, FileTimes, OpenOptions, Permissions},
    io::{self, Read, Write},
    os::unix::fs::{MetadataExt, symlink},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::SystemTime,
};

use rustix::fs::{CWD, Mode, OFlags, RenameFlags, open, renameat_with, statvfs};
use thiserror::Error;

const COPY_BUFFER_SIZE: usize = 128 * 1024;
const MAX_CLEANUP_QUARANTINE_ATTEMPTS: u64 = 128;
static CLEANUP_QUARANTINE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Defines what happens when the exact destination path already exists.
///
/// Phase 4A deliberately supports no overwrite mode. Callers must resolve a
/// conflict explicitly before submitting another request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictPolicy {
    FailIfExists,
}

/// Defines how symbolic links found anywhere in the source tree are handled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymlinkPolicy {
    /// Recreate the link with the same stored target; never follow it.
    Preserve,
    /// Fail the whole request before creating the destination.
    Reject,
}

/// A path-safe request to copy one source to one exact destination path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CopyRequest {
    source: PathBuf,
    destination: PathBuf,
    conflict_policy: ConflictPolicy,
    symlink_policy: SymlinkPolicy,
}

impl CopyRequest {
    pub fn new(
        source: impl Into<PathBuf>,
        destination: impl Into<PathBuf>,
        conflict_policy: ConflictPolicy,
        symlink_policy: SymlinkPolicy,
    ) -> Self {
        Self {
            source: source.into(),
            destination: destination.into(),
            conflict_policy,
            symlink_policy,
        }
    }

    pub fn source(&self) -> &Path {
        &self.source
    }

    pub fn destination(&self) -> &Path {
        &self.destination
    }

    pub const fn conflict_policy(&self) -> ConflictPolicy {
        self.conflict_policy
    }

    pub const fn symlink_policy(&self) -> SymlinkPolicy {
        self.symlink_policy
    }
}

/// Cooperative cancellation shared between a submitter and copy execution.
#[derive(Clone, Debug, Default)]
pub struct CopyCancellation(Arc<AtomicBool>);

impl CopyCancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub(crate) fn from_shared(flag: Arc<AtomicBool>) -> Self {
        Self(flag)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CopyProgress {
    entries_copied: u64,
    total_entries: u64,
    bytes_copied: u64,
    total_bytes: u64,
}

impl CopyProgress {
    pub const fn entries_copied(self) -> u64 {
        self.entries_copied
    }

    pub const fn total_entries(self) -> u64 {
        self.total_entries
    }

    pub const fn bytes_copied(self) -> u64 {
        self.bytes_copied
    }

    pub const fn total_bytes(self) -> u64 {
        self.total_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CopyOutcome {
    entries_copied: u64,
    bytes_copied: u64,
    metadata_preserved: u64,
    metadata_not_preserved: u64,
}

impl CopyOutcome {
    pub const fn entries_copied(self) -> u64 {
        self.entries_copied
    }

    pub const fn bytes_copied(self) -> u64 {
        self.bytes_copied
    }

    /// Entries whose supported POSIX mode and timestamps were applied.
    pub const fn metadata_preserved(self) -> u64 {
        self.metadata_preserved
    }

    /// Entries, currently symbolic links, for which Floe makes no metadata
    /// preservation claim. Link targets are still preserved without following.
    pub const fn metadata_not_preserved(self) -> u64 {
        self.metadata_not_preserved
    }
}

#[derive(Debug, Error)]
pub enum CopyError {
    #[error("copy was cancelled")]
    Cancelled,
    #[error("source path has no usable final component: {}", .0.display())]
    InvalidSource(PathBuf),
    #[error("destination path has no usable final component: {}", .0.display())]
    InvalidDestination(PathBuf),
    #[error("source and destination resolve to the same path: {}", .0.display())]
    SamePath(PathBuf),
    #[error("destination is inside the source directory: {}", .0.display())]
    DestinationInsideSource(PathBuf),
    #[error("destination already exists: {}", .0.display())]
    DestinationExists(PathBuf),
    #[error(
        "destination filesystem has insufficient available space: required {required} bytes, available {available} bytes"
    )]
    InsufficientSpace { required: u64, available: u64 },
    #[error("symbolic links are rejected by this request: {}", .0.display())]
    SymlinkRejected(PathBuf),
    #[error("unsupported filesystem object: {}", .0.display())]
    UnsupportedFileType(PathBuf),
    #[error("source changed after copy planning: {}", .0.display())]
    SourceChanged(PathBuf),
    #[error("cannot {action} {path}: {source}", path = path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("copy failed ({original}) and cleanup also failed for {path}: {cleanup}", path = path.display())]
    CleanupFailed {
        original: Box<CopyError>,
        path: PathBuf,
        #[source]
        cleanup: io::Error,
    },
}

impl CopyError {
    pub const fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }

    pub const fn is_partial(&self) -> bool {
        matches!(self, Self::CleanupFailed { .. })
    }

    pub fn io_kind(&self) -> Option<io::ErrorKind> {
        match self {
            Self::Io { source, .. } => Some(source.kind()),
            Self::CleanupFailed { cleanup, .. } => Some(cleanup.kind()),
            _ => None,
        }
    }

    pub const fn is_conflict(&self) -> bool {
        matches!(
            self,
            Self::SamePath(_) | Self::DestinationInsideSource(_) | Self::DestinationExists(_)
        )
    }

    pub const fn is_unsupported(&self) -> bool {
        matches!(
            self,
            Self::SymlinkRejected(_) | Self::UnsupportedFileType(_)
        )
    }
}

#[derive(Debug)]
struct CopyPlan {
    source: PathBuf,
    destination: PathBuf,
    kind: PlannedKind,
    total_entries: u64,
    total_bytes: u64,
}

#[derive(Debug)]
enum PlannedKind {
    File {
        metadata: BasicMetadata,
        length: u64,
        identity: ObjectIdentity,
    },
    Directory {
        metadata: BasicMetadata,
        children: Vec<CopyPlan>,
    },
    Symlink {
        target: PathBuf,
    },
}

#[derive(Default)]
struct CopyState {
    entries_copied: u64,
    bytes_copied: u64,
    created_paths: Vec<CreatedPath>,
    metadata_preserved: u64,
    metadata_not_preserved: u64,
}

#[derive(Debug)]
struct BasicMetadata {
    permissions: Permissions,
    accessed: Option<SystemTime>,
    modified: Option<SystemTime>,
}

impl BasicMetadata {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            permissions: metadata.permissions(),
            accessed: metadata.accessed().ok(),
            modified: metadata.modified().ok(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CreatedKind {
    File,
    Directory,
    Symlink,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ObjectIdentity {
    device: u64,
    inode: u64,
    kind: CreatedKind,
}

#[derive(Debug)]
struct CreatedPath {
    path: PathBuf,
    identity: ObjectIdentity,
}

impl ObjectIdentity {
    fn from_metadata(metadata: &fs::Metadata) -> Option<Self> {
        let file_type = metadata.file_type();
        let kind = if file_type.is_dir() {
            CreatedKind::Directory
        } else if file_type.is_file() {
            CreatedKind::File
        } else if file_type.is_symlink() {
            CreatedKind::Symlink
        } else {
            return None;
        };
        Some(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            kind,
        })
    }

    fn matches(self, metadata: &fs::Metadata) -> bool {
        Self::from_metadata(metadata).is_some_and(|current| current == self)
    }
}

/// Execute a copy synchronously. Callers must run this outside GTK's main loop.
///
/// The source tree is inspected before the destination is created. Progress is
/// reported after each entry and after each copied file chunk. On failure, only
/// paths created by this attempt are removed, in reverse order. Cleanup
/// revalidates each created object's no-follow identity immediately before
/// removal and reports a partial failure instead of removing a changed path.
pub fn execute_copy<F>(
    request: &CopyRequest,
    cancellation: &CopyCancellation,
    mut report_progress: F,
) -> Result<CopyOutcome, CopyError>
where
    F: FnMut(CopyProgress),
{
    check_cancelled(cancellation)?;
    let destination = validate_request(request)?;
    let plan = build_plan(
        request.source(),
        &destination,
        request.symlink_policy(),
        cancellation,
    )?;
    check_destination_absent(&destination, request.conflict_policy())?;
    check_destination_space(&destination, plan.total_bytes)?;

    let mut state = CopyState::default();
    let result = execute_plan(
        &plan,
        cancellation,
        &mut state,
        &mut report_progress,
        plan.total_entries,
        plan.total_bytes,
    );

    if let Err(error) = result {
        return match cleanup_created(&state.created_paths) {
            Ok(()) => Err(error),
            Err((path, cleanup)) => Err(CopyError::CleanupFailed {
                original: Box::new(error),
                path,
                cleanup,
            }),
        };
    }

    Ok(CopyOutcome {
        entries_copied: state.entries_copied,
        bytes_copied: state.bytes_copied,
        metadata_preserved: state.metadata_preserved,
        metadata_not_preserved: state.metadata_not_preserved,
    })
}

fn validate_request(request: &CopyRequest) -> Result<PathBuf, CopyError> {
    let source_name = request
        .source()
        .file_name()
        .ok_or_else(|| CopyError::InvalidSource(request.source().to_path_buf()))?;
    let destination_name = request
        .destination()
        .file_name()
        .ok_or_else(|| CopyError::InvalidDestination(request.destination().to_path_buf()))?;

    let source_parent = request.source().parent().unwrap_or_else(|| Path::new("."));
    let destination_parent = request
        .destination()
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let source_parent = canonicalize(source_parent, "resolve source parent")?;
    let destination_parent = canonicalize(destination_parent, "resolve destination parent")?;
    let source = source_parent.join(source_name);
    let destination = destination_parent.join(destination_name);

    if source == destination {
        return Err(CopyError::SamePath(request.source().to_path_buf()));
    }

    let source_metadata = symlink_metadata(&source, "inspect source")?;
    if source_metadata.file_type().is_dir() && destination.starts_with(&source) {
        return Err(CopyError::DestinationInsideSource(destination));
    }

    Ok(destination)
}

fn build_plan(
    source: &Path,
    destination: &Path,
    symlink_policy: SymlinkPolicy,
    cancellation: &CopyCancellation,
) -> Result<CopyPlan, CopyError> {
    check_cancelled(cancellation)?;
    let metadata = symlink_metadata(source, "inspect source")?;
    let file_type = metadata.file_type();

    let (kind, total_entries, total_bytes) = if file_type.is_file() {
        let length = metadata.len();
        (
            PlannedKind::File {
                metadata: BasicMetadata::from_metadata(&metadata),
                length,
                identity: ObjectIdentity {
                    device: metadata.dev(),
                    inode: metadata.ino(),
                    kind: CreatedKind::File,
                },
            },
            1,
            length,
        )
    } else if file_type.is_dir() {
        let mut entries = read_directory(source)?;
        entries.sort_by_key(|entry| entry.file_name());

        let mut children = Vec::with_capacity(entries.len());
        let mut total_entries = 1u64;
        let mut total_bytes = 0u64;
        for entry in entries {
            check_cancelled(cancellation)?;
            let child = build_plan(
                &entry.path(),
                &destination.join(entry.file_name()),
                symlink_policy,
                cancellation,
            )?;
            total_entries = total_entries.saturating_add(child.total_entries);
            total_bytes = total_bytes.saturating_add(child.total_bytes);
            children.push(child);
        }
        (
            PlannedKind::Directory {
                metadata: BasicMetadata::from_metadata(&metadata),
                children,
            },
            total_entries,
            total_bytes,
        )
    } else if file_type.is_symlink() {
        if symlink_policy == SymlinkPolicy::Reject {
            return Err(CopyError::SymlinkRejected(source.to_path_buf()));
        }
        let target = fs::read_link(source).map_err(|source_error| CopyError::Io {
            action: "read symbolic link",
            path: source.to_path_buf(),
            source: source_error,
        })?;
        (PlannedKind::Symlink { target }, 1, 0)
    } else {
        return Err(CopyError::UnsupportedFileType(source.to_path_buf()));
    };

    Ok(CopyPlan {
        source: source.to_path_buf(),
        destination: destination.to_path_buf(),
        kind,
        total_entries,
        total_bytes,
    })
}

fn execute_plan<F>(
    plan: &CopyPlan,
    cancellation: &CopyCancellation,
    state: &mut CopyState,
    report_progress: &mut F,
    total_entries: u64,
    total_bytes: u64,
) -> Result<(), CopyError>
where
    F: FnMut(CopyProgress),
{
    check_cancelled(cancellation)?;
    match &plan.kind {
        PlannedKind::File {
            metadata,
            length,
            identity,
        } => {
            copy_file(
                &plan.source,
                &plan.destination,
                *length,
                *identity,
                metadata,
                cancellation,
                state,
                report_progress,
                total_entries,
                total_bytes,
            )?;
        }
        PlannedKind::Directory { metadata, children } => {
            create_directory(&plan.destination, state)?;
            for child in children {
                execute_plan(
                    child,
                    cancellation,
                    state,
                    report_progress,
                    total_entries,
                    total_bytes,
                )?;
            }
            check_cancelled(cancellation)?;
            apply_basic_metadata(&plan.destination, metadata, true)?;
            state.metadata_preserved = state.metadata_preserved.saturating_add(1);
        }
        PlannedKind::Symlink { target } => {
            symlink(target, &plan.destination).map_err(|source| {
                destination_create_error("create symbolic link at", &plan.destination, source)
            })?;
            state.created_paths.push(capture_created_path(
                &plan.destination,
                CreatedKind::Symlink,
            )?);
            state.metadata_not_preserved = state.metadata_not_preserved.saturating_add(1);
        }
    }

    state.entries_copied = state.entries_copied.saturating_add(1);
    report_progress(CopyProgress {
        entries_copied: state.entries_copied,
        total_entries,
        bytes_copied: state.bytes_copied,
        total_bytes,
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn copy_file<F>(
    source_path: &Path,
    destination_path: &Path,
    expected_length: u64,
    expected_identity: ObjectIdentity,
    metadata: &BasicMetadata,
    cancellation: &CopyCancellation,
    state: &mut CopyState,
    report_progress: &mut F,
    total_entries: u64,
    total_bytes: u64,
) -> Result<(), CopyError>
where
    F: FnMut(CopyProgress),
{
    let mut source = open_planned_source(source_path, expected_identity)?;
    let mut destination = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination_path)
        .map_err(|source| {
            destination_create_error("create destination file", destination_path, source)
        })?;
    state.created_paths.push(capture_created_file(
        destination_path,
        &destination,
        CreatedKind::File,
    )?);

    let mut copied_for_file = 0u64;
    let mut buffer = vec![0u8; COPY_BUFFER_SIZE];
    loop {
        check_cancelled(cancellation)?;
        let read = source.read(&mut buffer).map_err(|source| CopyError::Io {
            action: "read source file",
            path: source_path.to_path_buf(),
            source,
        })?;
        if read == 0 {
            break;
        }
        destination
            .write_all(&buffer[..read])
            .map_err(|source| CopyError::Io {
                action: "write destination file",
                path: destination_path.to_path_buf(),
                source,
            })?;
        let read = read as u64;
        copied_for_file = copied_for_file.saturating_add(read);
        state.bytes_copied = state.bytes_copied.saturating_add(read);
        report_progress(CopyProgress {
            entries_copied: state.entries_copied,
            total_entries,
            bytes_copied: state.bytes_copied,
            total_bytes,
        });
    }

    if copied_for_file != expected_length {
        return Err(CopyError::Io {
            action: "copy source file that changed size while being read",
            path: source_path.to_path_buf(),
            source: io::Error::new(
                io::ErrorKind::InvalidData,
                format!("expected {expected_length} bytes but copied {copied_for_file}"),
            ),
        });
    }

    destination.sync_all().map_err(|source| CopyError::Io {
        action: "synchronize destination file",
        path: destination_path.to_path_buf(),
        source,
    })?;
    apply_basic_metadata_to_file(&destination, destination_path, metadata)?;
    destination.sync_all().map_err(|source| CopyError::Io {
        action: "synchronize destination file metadata",
        path: destination_path.to_path_buf(),
        source,
    })?;
    state.metadata_preserved = state.metadata_preserved.saturating_add(1);
    Ok(())
}

fn open_planned_source(
    source_path: &Path,
    expected_identity: ObjectIdentity,
) -> Result<File, CopyError> {
    let inspection = open(
        source_path,
        OFlags::PATH | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|source| source_open_error(source_path, source))?;
    let inspection = File::from(inspection);
    let inspected = inspection.metadata().map_err(|source| CopyError::Io {
        action: "inspect source file without following links",
        path: source_path.to_path_buf(),
        source,
    })?;
    if !expected_identity.matches(&inspected) || !inspected.file_type().is_file() {
        return Err(CopyError::SourceChanged(source_path.to_path_buf()));
    }

    let descriptor = open(
        source_path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(|source| source_open_error(source_path, source))?;
    let source = File::from(descriptor);
    let opened = source.metadata().map_err(|source| CopyError::Io {
        action: "inspect opened source file",
        path: source_path.to_path_buf(),
        source,
    })?;
    if !expected_identity.matches(&opened) || !opened.file_type().is_file() {
        return Err(CopyError::SourceChanged(source_path.to_path_buf()));
    }
    Ok(source)
}

fn source_open_error(source_path: &Path, source: rustix::io::Errno) -> CopyError {
    if matches!(source, rustix::io::Errno::LOOP | rustix::io::Errno::NOENT) {
        CopyError::SourceChanged(source_path.to_path_buf())
    } else {
        CopyError::Io {
            action: "open source file without following links",
            path: source_path.to_path_buf(),
            source: io::Error::from_raw_os_error(source.raw_os_error()),
        }
    }
}

fn capture_created_file(
    path: &Path,
    file: &File,
    expected_kind: CreatedKind,
) -> Result<CreatedPath, CopyError> {
    let metadata = file.metadata().map_err(|source| CopyError::Io {
        action: "capture created destination identity for",
        path: path.to_path_buf(),
        source,
    })?;
    created_path_from_metadata(path, &metadata, expected_kind)
}

fn capture_created_path(path: &Path, expected_kind: CreatedKind) -> Result<CreatedPath, CopyError> {
    let metadata = symlink_metadata(path, "capture created destination identity for")?;
    created_path_from_metadata(path, &metadata, expected_kind)
}

fn created_path_from_metadata(
    path: &Path,
    metadata: &fs::Metadata,
    expected_kind: CreatedKind,
) -> Result<CreatedPath, CopyError> {
    let identity =
        ObjectIdentity::from_metadata(metadata).filter(|identity| identity.kind == expected_kind);
    identity
        .map(|identity| CreatedPath {
            path: path.to_path_buf(),
            identity,
        })
        .ok_or_else(|| CopyError::Io {
            action: "revalidate created destination",
            path: path.to_path_buf(),
            source: io::Error::other("created destination kind changed before ownership capture"),
        })
}

fn apply_basic_metadata(
    destination_path: &Path,
    metadata: &BasicMetadata,
    synchronize: bool,
) -> Result<(), CopyError> {
    let destination = File::open(destination_path).map_err(|source| CopyError::Io {
        action: "open destination for metadata",
        path: destination_path.to_path_buf(),
        source,
    })?;
    apply_basic_metadata_to_file(&destination, destination_path, metadata)?;
    if synchronize {
        destination.sync_all().map_err(|source| CopyError::Io {
            action: "synchronize destination directory metadata",
            path: destination_path.to_path_buf(),
            source,
        })?;
    }
    Ok(())
}

fn apply_basic_metadata_to_file(
    destination: &File,
    destination_path: &Path,
    metadata: &BasicMetadata,
) -> Result<(), CopyError> {
    destination
        .set_permissions(metadata.permissions.clone())
        .map_err(|source| CopyError::Io {
            action: "set destination permissions on",
            path: destination_path.to_path_buf(),
            source,
        })?;

    let mut times = FileTimes::new();
    if let Some(accessed) = metadata.accessed {
        times = times.set_accessed(accessed);
    }
    if let Some(modified) = metadata.modified {
        times = times.set_modified(modified);
    }
    destination
        .set_times(times)
        .map_err(|source| CopyError::Io {
            action: "set destination timestamps on",
            path: destination_path.to_path_buf(),
            source,
        })
}

fn check_destination_space(destination: &Path, required: u64) -> Result<(), CopyError> {
    check_destination_space_with(destination, required, |parent| {
        let status = statvfs(parent).map_err(io::Error::from)?;
        Ok(status.f_bavail.saturating_mul(status.f_frsize))
    })
}

fn check_destination_space_with<F>(
    destination: &Path,
    required: u64,
    query_available: F,
) -> Result<(), CopyError>
where
    F: FnOnce(&Path) -> io::Result<u64>,
{
    if required == 0 {
        return Ok(());
    }
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let available = query_available(parent).map_err(|source| CopyError::Io {
        action: "check destination filesystem space for",
        path: parent.to_path_buf(),
        source,
    })?;
    ensure_available_space(required, available)
}

fn ensure_available_space(required: u64, available: u64) -> Result<(), CopyError> {
    if required > available {
        Err(CopyError::InsufficientSpace {
            required,
            available,
        })
    } else {
        Ok(())
    }
}

fn create_directory(path: &Path, state: &mut CopyState) -> Result<(), CopyError> {
    fs::create_dir(path)
        .map_err(|source| destination_create_error("create destination directory", path, source))?;
    state
        .created_paths
        .push(capture_created_path(path, CreatedKind::Directory)?);
    Ok(())
}

fn check_destination_absent(
    destination: &Path,
    conflict_policy: ConflictPolicy,
) -> Result<(), CopyError> {
    match conflict_policy {
        ConflictPolicy::FailIfExists => match fs::symlink_metadata(destination) {
            Ok(_) => Err(CopyError::DestinationExists(destination.to_path_buf())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(CopyError::Io {
                action: "inspect destination",
                path: destination.to_path_buf(),
                source,
            }),
        },
    }
}

fn destination_create_error(action: &'static str, path: &Path, source: io::Error) -> CopyError {
    if source.kind() == io::ErrorKind::AlreadyExists {
        CopyError::DestinationExists(path.to_path_buf())
    } else {
        CopyError::Io {
            action,
            path: path.to_path_buf(),
            source,
        }
    }
}

fn check_cancelled(cancellation: &CopyCancellation) -> Result<(), CopyError> {
    if cancellation.is_cancelled() {
        Err(CopyError::Cancelled)
    } else {
        Ok(())
    }
}

fn canonicalize(path: &Path, action: &'static str) -> Result<PathBuf, CopyError> {
    path.canonicalize().map_err(|source| CopyError::Io {
        action,
        path: path.to_path_buf(),
        source,
    })
}

fn symlink_metadata(path: &Path, action: &'static str) -> Result<fs::Metadata, CopyError> {
    fs::symlink_metadata(path).map_err(|source| CopyError::Io {
        action,
        path: path.to_path_buf(),
        source,
    })
}

fn read_directory(path: &Path) -> Result<Vec<fs::DirEntry>, CopyError> {
    let entries = fs::read_dir(path).map_err(|source| CopyError::Io {
        action: "read source directory",
        path: path.to_path_buf(),
        source,
    })?;
    entries
        .map(|entry| {
            entry.map_err(|source| CopyError::Io {
                action: "read source directory entry in",
                path: path.to_path_buf(),
                source,
            })
        })
        .collect()
}

fn cleanup_created(paths: &[CreatedPath]) -> Result<(), (PathBuf, io::Error)> {
    cleanup_created_with(paths, |_| {})
}

fn cleanup_created_with<F>(
    paths: &[CreatedPath],
    mut before_quarantine: F,
) -> Result<(), (PathBuf, io::Error)>
where
    F: FnMut(&CreatedPath),
{
    for created in paths.iter().rev() {
        let current = match fs::symlink_metadata(&created.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err((created.path.clone(), error)),
        };
        if !created.identity.matches(&current) {
            return Err((
                created.path.clone(),
                io::Error::other("cleanup refused because the destination object identity changed"),
            ));
        }
        before_quarantine(created);
        let Some(quarantine) = quarantine_created_path(created)? else {
            continue;
        };
        let quarantined =
            fs::symlink_metadata(&quarantine).map_err(|error| (created.path.clone(), error))?;
        if !created.identity.matches(&quarantined) {
            restore_quarantined_path(&quarantine, &created.path)
                .map_err(|error| (created.path.clone(), error))?;
            return Err((
                created.path.clone(),
                io::Error::other(
                    "cleanup refused because the destination object identity changed during cleanup",
                ),
            ));
        }
        let result = match created.identity.kind {
            CreatedKind::File | CreatedKind::Symlink => fs::remove_file(&quarantine),
            CreatedKind::Directory => fs::remove_dir(&quarantine),
        };
        if let Err(error) = result
            && error.kind() != io::ErrorKind::NotFound
        {
            return Err((created.path.clone(), error));
        }
    }
    Ok(())
}

/// Linux has no pathname operation that unlinks only when a device/inode pair
/// matches. Move the checked entry atomically to an internal no-overwrite
/// sibling first, then revalidate the object that actually moved. A concurrent
/// replacement at the public destination is restored and never passed to a
/// removal operation. A malicious same-authority process that discovers and
/// races the internal quarantine name remains outside what pathname APIs can
/// exclude; ordinary concurrent destination replacement cannot reach that
/// final removal name.
fn quarantine_created_path(created: &CreatedPath) -> Result<Option<PathBuf>, (PathBuf, io::Error)> {
    let parent = created.path.parent().unwrap_or_else(|| Path::new("."));
    for _ in 0..MAX_CLEANUP_QUARANTINE_ATTEMPTS {
        let sequence = CLEANUP_QUARANTINE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let quarantine = parent.join(format!(
            ".floe-copy-cleanup-{}-{sequence:016x}",
            std::process::id()
        ));
        match renameat_with(CWD, &created.path, CWD, &quarantine, RenameFlags::NOREPLACE) {
            Ok(()) => return Ok(Some(quarantine)),
            Err(rustix::io::Errno::EXIST) => continue,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(error) => {
                return Err((
                    created.path.clone(),
                    io::Error::from_raw_os_error(error.raw_os_error()),
                ));
            }
        }
    }
    Err((
        created.path.clone(),
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not reserve an internal no-overwrite cleanup path",
        ),
    ))
}

fn restore_quarantined_path(quarantine: &Path, destination: &Path) -> io::Result<()> {
    renameat_with(CWD, quarantine, CWD, destination, RenameFlags::NOREPLACE)
        .map_err(|error| io::Error::from_raw_os_error(error.raw_os_error()))
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        os::unix::ffi::{OsStrExt, OsStringExt},
    };

    use tempfile::tempdir;

    use super::*;

    fn request(source: &Path, destination: &Path) -> CopyRequest {
        CopyRequest::new(
            source,
            destination,
            ConflictPolicy::FailIfExists,
            SymlinkPolicy::Preserve,
        )
    }

    #[test]
    fn copy_file_preserves_content_and_reports_progress() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source.bin");
        let destination = fixture.path().join("destination.bin");
        let content = vec![0x5a; COPY_BUFFER_SIZE + 19];
        fs::write(&source, &content).expect("fixture file should be writable");
        let mut progress = Vec::new();

        let outcome = execute_copy(
            &request(&source, &destination),
            &CopyCancellation::new(),
            |p| {
                progress.push(p);
            },
        )
        .expect("file copy should succeed");

        assert_eq!(
            fs::read(destination).expect("copy should be readable"),
            content
        );
        assert_eq!(outcome.entries_copied(), 1);
        assert_eq!(outcome.bytes_copied(), (COPY_BUFFER_SIZE + 19) as u64);
        let final_progress = progress
            .last()
            .copied()
            .expect("progress should be emitted");
        assert_eq!(final_progress.entries_copied(), 1);
        assert_eq!(final_progress.total_entries(), 1);
        assert_eq!(final_progress.bytes_copied(), outcome.bytes_copied());
    }

    #[test]
    fn copy_directory_is_recursive_and_preserves_symlinks_without_following_them() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::create_dir(&source).expect("source directory should be creatable");
        fs::create_dir(source.join("nested")).expect("nested directory should be creatable");
        fs::write(source.join("nested/file.txt"), b"floe").expect("fixture should be writable");
        symlink(Path::new("nested/file.txt"), source.join("link"))
            .expect("fixture symlink should be creatable");

        let outcome = execute_copy(
            &request(&source, &destination),
            &CopyCancellation::new(),
            |_| {},
        )
        .expect("directory copy should succeed");

        assert_eq!(
            fs::read(destination.join("nested/file.txt")).expect("copied file should be readable"),
            b"floe"
        );
        assert_eq!(
            fs::read_link(destination.join("link")).expect("link should be preserved"),
            PathBuf::from("nested/file.txt")
        );
        assert_eq!(outcome.entries_copied(), 4);
        assert_eq!(outcome.bytes_copied(), 4);
    }

    #[test]
    fn copy_conflict_never_changes_existing_destination() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"new").expect("source fixture should be writable");
        fs::write(&destination, b"keep").expect("destination fixture should be writable");

        let error = execute_copy(
            &request(&source, &destination),
            &CopyCancellation::new(),
            |_| {},
        )
        .expect_err("existing destination must conflict");

        assert!(matches!(error, CopyError::DestinationExists(_)));
        assert_eq!(
            fs::read(destination).expect("existing destination should remain readable"),
            b"keep"
        );
    }

    #[test]
    fn copy_rejects_destination_inside_source_before_creating_it() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = source.join("nested-copy");
        fs::create_dir(&source).expect("source directory should be creatable");

        let error = execute_copy(
            &request(&source, &destination),
            &CopyCancellation::new(),
            |_| {},
        )
        .expect_err("recursive self-copy must be rejected");

        assert!(matches!(error, CopyError::DestinationInsideSource(_)));
        assert!(!destination.exists());
    }

    #[test]
    fn copy_rejects_same_source_and_destination_without_changing_source() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        fs::write(&source, b"keep").expect("source fixture should be writable");

        let error = execute_copy(&request(&source, &source), &CopyCancellation::new(), |_| {})
            .expect_err("copying a path onto itself must be rejected");

        assert!(matches!(error, CopyError::SamePath(_)));
        assert_eq!(
            fs::read(source).expect("source should remain readable"),
            b"keep"
        );
    }

    #[test]
    fn copy_rejects_symlink_tree_before_creating_destination() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::create_dir(&source).expect("source directory should be creatable");
        symlink(Path::new("missing-target"), source.join("link"))
            .expect("fixture symlink should be creatable");
        let request = CopyRequest::new(
            &source,
            &destination,
            ConflictPolicy::FailIfExists,
            SymlinkPolicy::Reject,
        );

        let error = execute_copy(&request, &CopyCancellation::new(), |_| {})
            .expect_err("reject policy must fail");

        assert!(matches!(error, CopyError::SymlinkRejected(_)));
        assert!(!destination.exists());
    }

    #[test]
    fn copy_cancellation_before_execution_creates_nothing() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"content").expect("source fixture should be writable");
        let cancellation = CopyCancellation::new();
        cancellation.cancel();

        let error = execute_copy(&request(&source, &destination), &cancellation, |_| {})
            .expect_err("cancelled copy must stop");

        assert!(matches!(error, CopyError::Cancelled));
        assert!(!destination.exists());
    }

    #[test]
    fn copy_cancellation_during_file_copy_removes_partial_destination() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, vec![0x33; COPY_BUFFER_SIZE * 2])
            .expect("source fixture should be writable");
        let cancellation = CopyCancellation::new();
        let progress_cancellation = cancellation.clone();

        let error = execute_copy(&request(&source, &destination), &cancellation, |progress| {
            if progress.bytes_copied() > 0 {
                progress_cancellation.cancel();
            }
        })
        .expect_err("copy should observe cancellation between chunks");

        assert!(matches!(error, CopyError::Cancelled));
        assert!(!destination.exists());
    }

    #[test]
    fn adversarial_copy_source_identity_rejects_same_size_regular_file_substitution() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let retained_source = fixture.path().join("retained-source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"safe").expect("source fixture should be writable");
        let cancellation = CopyCancellation::new();
        let plan = build_plan(
            &source,
            &destination,
            SymlinkPolicy::Preserve,
            &cancellation,
        )
        .expect("source should be plannable");

        fs::rename(&source, &retained_source).expect("planned source should remain retained");
        fs::write(&source, b"evil").expect("same-size replacement should be writable");
        let mut state = CopyState::default();
        let error = execute_plan(
            &plan,
            &cancellation,
            &mut state,
            &mut |_| {},
            plan.total_entries,
            plan.total_bytes,
        )
        .expect_err("same-size replacement identity must be rejected");

        assert!(matches!(error, CopyError::SourceChanged(path) if path == source));
        assert!(!destination.exists());
        assert_eq!(
            fs::read(retained_source).expect("original source should remain readable"),
            b"safe"
        );
    }

    #[test]
    fn adversarial_copy_source_identity_never_follows_substituted_symlink() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let retained_source = fixture.path().join("retained-source");
        let link_target = fixture.path().join("link-target");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"safe").expect("source fixture should be writable");
        fs::write(&link_target, b"evil").expect("link target fixture should be writable");
        let cancellation = CopyCancellation::new();
        let plan = build_plan(
            &source,
            &destination,
            SymlinkPolicy::Preserve,
            &cancellation,
        )
        .expect("source should be plannable");

        fs::rename(&source, &retained_source).expect("planned source should remain retained");
        symlink(&link_target, &source).expect("source should be replaceable by a symlink");
        let mut state = CopyState::default();
        let error = execute_plan(
            &plan,
            &cancellation,
            &mut state,
            &mut |_| {},
            plan.total_entries,
            plan.total_bytes,
        )
        .expect_err("substituted symlink must not be followed");

        assert!(matches!(error, CopyError::SourceChanged(path) if path == source));
        assert!(!destination.exists());
        assert_eq!(
            fs::read(link_target).expect("link target should remain readable"),
            b"evil"
        );
    }

    #[test]
    fn adversarial_copy_cleanup_identity_preserves_replacement_file() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let raw_destination = OsString::from_vec(b"destination-\xff".to_vec());
        let destination = fixture.path().join(raw_destination);
        fs::write(&source, vec![0x33; COPY_BUFFER_SIZE * 2])
            .expect("source fixture should be writable");
        let cancellation = CopyCancellation::new();
        let progress_cancellation = cancellation.clone();
        let replacement_path = destination.clone();
        let mut replaced = false;

        let error = execute_copy(&request(&source, &destination), &cancellation, |progress| {
            if !replaced && progress.bytes_copied() > 0 {
                fs::remove_file(&replacement_path)
                    .expect("partial destination should be removable");
                fs::write(&replacement_path, b"replacement")
                    .expect("replacement destination should be writable");
                replaced = true;
                progress_cancellation.cancel();
            }
        })
        .expect_err("ownership loss must make cancellation cleanup partial");

        match error {
            CopyError::CleanupFailed {
                original,
                path,
                cleanup,
            } => {
                assert!(original.is_cancelled());
                assert_eq!(path, destination);
                assert!(cleanup.to_string().contains("object identity changed"));
            }
            other => panic!("expected explicit cleanup ownership failure, got {other:?}"),
        }
        assert_eq!(
            fs::read(&destination).expect("replacement must remain at its exact path"),
            b"replacement"
        );
    }

    #[test]
    fn adversarial_copy_cleanup_identity_closes_check_remove_swap_window() {
        let fixture = tempdir().expect("temporary directory should be available");
        let destination = fixture.path().join("destination");
        let retained_owned_file = fixture.path().join("retained-owned-file");
        fs::write(&destination, b"owned").expect("owned fixture should be writable");
        let created = capture_created_path(&destination, CreatedKind::File)
            .expect("owned fixture identity should be capturable");

        let error = cleanup_created_with(&[created], |_| {
            fs::rename(&destination, &retained_owned_file)
                .expect("owned fixture should be replaceable after the first identity check");
            fs::write(&destination, b"replacement")
                .expect("concurrent replacement should be writable");
        })
        .expect_err("the quarantined replacement identity must be rejected");

        assert_eq!(error.0, destination);
        assert!(
            error
                .1
                .to_string()
                .contains("identity changed during cleanup")
        );
        assert_eq!(
            fs::read(&destination).expect("replacement must be restored to its exact path"),
            b"replacement"
        );
        assert_eq!(
            fs::read(retained_owned_file).expect("owned file should remain where it was displaced"),
            b"owned"
        );
        assert!(
            fs::read_dir(fixture.path())
                .expect("fixture should remain readable")
                .all(|entry| !entry
                    .expect("fixture entry should be readable")
                    .file_name()
                    .as_bytes()
                    .starts_with(b".floe-copy-cleanup-"))
        );
    }

    #[test]
    fn adversarial_copy_cleanup_identity_preserves_replacement_directory() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        let displaced = fixture.path().join("displaced-owned-copy");
        fs::create_dir(&source).expect("source directory should be creatable");
        fs::write(source.join("child"), vec![0x33; COPY_BUFFER_SIZE * 2])
            .expect("source fixture should be writable");
        let cancellation = CopyCancellation::new();
        let progress_cancellation = cancellation.clone();
        let replacement_path = destination.clone();
        let displaced_path = displaced.clone();
        let mut replaced = false;

        let error = execute_copy(&request(&source, &destination), &cancellation, |progress| {
            if !replaced && progress.bytes_copied() > 0 {
                fs::rename(&replacement_path, &displaced_path)
                    .expect("owned destination tree should be displaceable");
                fs::create_dir(&replacement_path)
                    .expect("replacement directory should be creatable");
                fs::write(replacement_path.join("marker"), b"replacement")
                    .expect("replacement marker should be writable");
                replaced = true;
                progress_cancellation.cancel();
            }
        })
        .expect_err("directory ownership loss must make cleanup partial");

        assert!(matches!(
            error,
            CopyError::CleanupFailed {
                original,
                path,
                cleanup,
            } if original.is_cancelled()
                && path == destination
                && cleanup.to_string().contains("object identity changed")
        ));
        assert_eq!(
            fs::read(destination.join("marker")).expect("replacement directory must remain intact"),
            b"replacement"
        );
        assert!(displaced.exists());
    }

    #[test]
    fn copy_preserves_non_utf8_file_names() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::create_dir(&source).expect("source directory should be creatable");
        let raw_name = OsString::from_vec(b"file-\xff".to_vec());
        fs::write(source.join(&raw_name), b"bytes").expect("non-UTF-8 fixture should be writable");

        execute_copy(
            &request(&source, &destination),
            &CopyCancellation::new(),
            |_| {},
        )
        .expect("non-UTF-8 path copy should succeed");

        let copied_name = fs::read_dir(destination)
            .expect("copied directory should be readable")
            .next()
            .expect("copied directory should contain one entry")
            .expect("copied entry should be readable")
            .file_name();
        assert_eq!(copied_name.as_bytes(), raw_name.as_bytes());
    }

    #[test]
    fn phase_6o_space_preflight_reports_required_and_available_bytes() {
        assert!(ensure_available_space(4096, 4096).is_ok());
        let error = ensure_available_space(4097, 4096)
            .expect_err("insufficient destination space must reject copy");
        assert!(matches!(
            error,
            CopyError::InsufficientSpace {
                required: 4097,
                available: 4096
            }
        ));
        assert!(ensure_available_space(0, 0).is_ok());

        let fixture = tempdir().expect("temporary directory should be available");
        let destination = fixture.path().join("destination");
        let error = check_destination_space_with(&destination, 4097, |_| Ok(4096))
            .expect_err("injected insufficient space must reject preflight");
        assert!(matches!(
            error,
            CopyError::InsufficientSpace {
                required: 4097,
                available: 4096
            }
        ));
        assert!(!destination.exists());

        let error = check_destination_space_with(&destination, 1, |_| {
            Err(io::Error::other("injected space query failure"))
        })
        .expect_err("space query failure must remain structured");
        assert!(matches!(
            error,
            CopyError::Io {
                action: "check destination filesystem space for",
                path,
                source,
            } if path == fixture.path() && source.kind() == io::ErrorKind::Other
        ));

        let mut queried = false;
        check_destination_space_with(&destination, 0, |_| {
            queried = true;
            Ok(0)
        })
        .expect("zero-byte copy should not require a filesystem query");
        assert!(!queried);
    }

    #[test]
    fn phase_6o_metadata_preserves_file_and_directory_mode_and_timestamps() {
        use std::{
            os::unix::fs::{MetadataExt, PermissionsExt},
            time::{Duration, UNIX_EPOCH},
        };

        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::create_dir(&source).expect("source directory should be creatable");
        let child = source.join("child");
        fs::write(&child, b"metadata").expect("source child should be writable");

        fs::set_permissions(&source, fs::Permissions::from_mode(0o750))
            .expect("directory mode should be settable");
        fs::set_permissions(&child, fs::Permissions::from_mode(0o640))
            .expect("file mode should be settable");
        let timestamp = UNIX_EPOCH + Duration::from_secs(1_700_000_123);
        File::open(&source)
            .expect("source directory should open")
            .set_times(
                FileTimes::new()
                    .set_accessed(timestamp)
                    .set_modified(timestamp),
            )
            .expect("directory timestamps should be settable");
        File::open(&child)
            .expect("source file should open")
            .set_times(
                FileTimes::new()
                    .set_accessed(timestamp)
                    .set_modified(timestamp),
            )
            .expect("file timestamps should be settable");

        let outcome = execute_copy(
            &request(&source, &destination),
            &CopyCancellation::new(),
            |_| {},
        )
        .expect("metadata-aware copy should succeed");
        let copied_directory = fs::symlink_metadata(&destination)
            .expect("copied directory metadata should be readable");
        let copied_file = fs::symlink_metadata(destination.join("child"))
            .expect("copied file metadata should be readable");

        assert_eq!(copied_directory.mode() & 0o777, 0o750);
        assert_eq!(copied_file.mode() & 0o777, 0o640);
        assert_eq!(copied_directory.mtime(), 1_700_000_123);
        assert_eq!(copied_file.mtime(), 1_700_000_123);
        assert_eq!(outcome.metadata_preserved(), 2);
        assert_eq!(outcome.metadata_not_preserved(), 0);
    }

    #[test]
    fn phase_6o_metadata_reports_symlink_metadata_as_not_preserved() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source-link");
        let destination = fixture.path().join("destination-link");
        symlink("missing-target", &source).expect("source symlink should be creatable");

        let outcome = execute_copy(
            &request(&source, &destination),
            &CopyCancellation::new(),
            |_| {},
        )
        .expect("symlink copy should succeed without following target");

        assert_eq!(outcome.metadata_preserved(), 0);
        assert_eq!(outcome.metadata_not_preserved(), 1);
        assert_eq!(
            fs::read_link(destination).expect("copied symlink should remain a link"),
            PathBuf::from("missing-target")
        );
    }
}
