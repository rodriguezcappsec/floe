use std::{
    ffi::{OsStr, OsString},
    fs, io,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use rustix::{
    fs::{CWD, RenameFlags, renameat_with},
    io::Errno,
};
use thiserror::Error;

use crate::{
    ConflictPolicy, CopyCancellation, CopyError, CopyProgress, CopyRequest, SymlinkPolicy,
    execute_copy,
};

const MAX_STAGING_ATTEMPTS: u64 = 128;
static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// An exact-source, exact-destination move request.
///
/// Floe currently supports only fail-if-exists moves. The explicit policy is
/// retained so overwrite cannot appear accidentally when more policies arrive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoveRequest {
    source: PathBuf,
    destination: PathBuf,
    conflict_policy: ConflictPolicy,
    expected_source_identity: Option<FileIdentity>,
}

impl MoveRequest {
    pub fn new(
        source: impl Into<PathBuf>,
        destination: impl Into<PathBuf>,
        conflict_policy: ConflictPolicy,
    ) -> Self {
        Self {
            source: source.into(),
            destination: destination.into(),
            conflict_policy,
            expected_source_identity: None,
        }
    }

    pub fn with_expected_source_identity(mut self, identity: FileIdentity) -> Self {
        self.expected_source_identity = Some(identity);
        self
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

    pub const fn expected_source_identity(&self) -> Option<FileIdentity> {
        self.expected_source_identity
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileIdentity {
    device: u64,
    inode: u64,
    mode: u32,
    length: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
}

impl FileIdentity {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            mode: metadata.mode(),
            length: metadata.len(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
        }
    }

    fn matches(self, metadata: &fs::Metadata) -> bool {
        self == Self::from_metadata(metadata)
    }
}

/// A rename request whose destination must remain in the source parent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenameRequest {
    source: PathBuf,
    new_name: OsString,
    conflict_policy: ConflictPolicy,
}

impl RenameRequest {
    pub fn new(
        source: impl Into<PathBuf>,
        new_name: impl Into<OsString>,
        conflict_policy: ConflictPolicy,
    ) -> Self {
        Self {
            source: source.into(),
            new_name: new_name.into(),
            conflict_policy,
        }
    }

    pub fn source(&self) -> &Path {
        &self.source
    }

    pub fn new_name(&self) -> &OsStr {
        &self.new_name
    }

    pub const fn conflict_policy(&self) -> ConflictPolicy {
        self.conflict_policy
    }
}

/// Cooperative cancellation checked throughout same- and cross-filesystem moves.
#[derive(Clone, Debug, Default)]
pub struct MoveCancellation(Arc<AtomicBool>);

impl MoveCancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MoveOutcome {
    destination_identity: FileIdentity,
}

impl MoveOutcome {
    pub const fn destination_identity(self) -> FileIdentity {
        self.destination_identity
    }
}

#[derive(Debug, Error)]
pub enum MoveError {
    #[error("move was cancelled before it changed the filesystem")]
    Cancelled,
    #[error("source path has no usable final component: {}", .0.display())]
    InvalidSource(PathBuf),
    #[error("destination path has no usable final component: {}", .0.display())]
    InvalidDestination(PathBuf),
    #[error("destination parent does not exist or is not a directory: {}", .0.display())]
    DestinationParentMissing(PathBuf),
    #[error("rename requires exactly one valid filename component: {}", .0.display())]
    InvalidName(PathBuf),
    #[error("source does not exist: {}", .0.display())]
    SourceMissing(PathBuf),
    #[error("source and destination are the same path: {}", .0.display())]
    SamePath(PathBuf),
    #[error("cannot move a directory inside itself: {}", .0.display())]
    DestinationInsideSource(PathBuf),
    #[error("destination already exists: {}", .0.display())]
    DestinationExists(PathBuf),
    #[error(transparent)]
    Copy(#[from] CopyError),
    #[error("source changed before the move could commit: {}", .0.display())]
    SourceChanged(PathBuf),
    #[error(
        "destination was committed but source was retained after {reason}: source {}, destination {}",
        source_path.display(),
        destination_path.display()
    )]
    Partial {
        source_path: PathBuf,
        destination_path: PathBuf,
        reason: String,
    },
    #[error("could not {action} {}: {source}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl MoveError {
    pub const fn is_conflict(&self) -> bool {
        match self {
            Self::SamePath(_) | Self::DestinationExists(_) => true,
            Self::Copy(error) => error.is_conflict(),
            _ => false,
        }
    }

    pub const fn is_unsupported(&self) -> bool {
        matches!(self, Self::Copy(error) if error.is_unsupported())
    }

    pub const fn is_partial(&self) -> bool {
        matches!(self, Self::Partial { .. })
    }

    pub fn io_kind(&self) -> Option<io::ErrorKind> {
        match self {
            Self::Io { source, .. } => Some(source.kind()),
            Self::Copy(error) => error.io_kind(),
            _ => None,
        }
    }
}

/// Execute a move synchronously outside GTK's main loop.
///
/// Linux `renameat2(RENAME_NOREPLACE)` supplies the atomic conflict guarantee:
/// an existing destination is never replaced, including under races. `EXDEV`
/// falls back to a synchronized hidden staging copy, atomic publication, and
/// identity-checked no-follow source removal.
pub fn execute_move(
    request: &MoveRequest,
    cancellation: &MoveCancellation,
) -> Result<MoveOutcome, MoveError> {
    execute_move_with_progress(request, cancellation, |_| {})
}

/// Execute a move while reporting copy progress when an `EXDEV` fallback is
/// required. Same-filesystem renames complete without intermediate progress.
pub fn execute_move_with_progress<F>(
    request: &MoveRequest,
    cancellation: &MoveCancellation,
    mut report_progress: F,
) -> Result<MoveOutcome, MoveError>
where
    F: FnMut(CopyProgress),
{
    check_cancelled(cancellation)?;
    let source_identity = validate_move(request)?;
    check_cancelled(cancellation)?;

    let rename_result = match request.conflict_policy() {
        ConflictPolicy::FailIfExists => renameat_with(
            CWD,
            request.source(),
            CWD,
            request.destination(),
            RenameFlags::NOREPLACE,
        ),
    };

    match rename_result {
        Ok(()) => Ok(MoveOutcome {
            destination_identity: source_identity,
        }),
        Err(Errno::XDEV) => {
            execute_cross_filesystem_move(request, cancellation, &mut report_progress)
        }
        Err(error) => Err(map_rename_error(request, error)),
    }
}

/// Execute a same-directory rename using the same atomic move primitive.
pub fn execute_rename(
    request: &RenameRequest,
    cancellation: &MoveCancellation,
) -> Result<MoveOutcome, MoveError> {
    let destination = rename_destination(request)?;
    execute_move(
        &MoveRequest::new(
            request.source().to_path_buf(),
            destination,
            request.conflict_policy(),
        ),
        cancellation,
    )
}

#[derive(Debug)]
struct TreeSnapshot {
    path: PathBuf,
    device: u64,
    inode: u64,
    mode: u32,
    length: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    kind: SnapshotKind,
    children: Vec<TreeSnapshot>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SnapshotKind {
    File,
    Directory,
    Symlink,
}

fn execute_cross_filesystem_move<F>(
    request: &MoveRequest,
    cancellation: &MoveCancellation,
    report_progress: &mut F,
) -> Result<MoveOutcome, MoveError>
where
    F: FnMut(CopyProgress),
{
    check_cancelled(cancellation)?;
    let source_snapshot = snapshot_tree(request.source())?;
    let staging = available_staging_path(request.destination())?;
    let copy_request = CopyRequest::new(
        request.source(),
        &staging,
        ConflictPolicy::FailIfExists,
        SymlinkPolicy::Preserve,
    );
    let copy_cancellation = CopyCancellation::from_shared(Arc::clone(&cancellation.0));

    if let Err(error) = execute_copy(&copy_request, &copy_cancellation, report_progress) {
        return Err(error.into());
    }

    if cancellation.is_cancelled() {
        cleanup_staging(&staging)?;
        return Err(MoveError::Cancelled);
    }
    if !snapshot_matches(&source_snapshot)? {
        cleanup_staging(&staging)?;
        return Err(MoveError::SourceChanged(request.source().to_path_buf()));
    }

    match renameat_with(
        CWD,
        &staging,
        CWD,
        request.destination(),
        RenameFlags::NOREPLACE,
    ) {
        Ok(()) => {}
        Err(error) => {
            cleanup_staging(&staging)?;
            return Err(map_rename_error(request, error));
        }
    }

    if let Err(error) = synchronize_destination_parent(request.destination()) {
        return Err(committed_partial(request, error.to_string()));
    }

    if cancellation.is_cancelled() {
        return Err(committed_partial(request, "cancellation"));
    }
    let destination_identity = capture_identity(request.destination()).map_err(|error| {
        committed_partial(
            request,
            format!("could not capture destination identity: {error}"),
        )
    })?;
    if let Err(reason) = remove_snapshot_tree(&source_snapshot, cancellation) {
        return Err(committed_partial(request, reason));
    }

    Ok(MoveOutcome {
        destination_identity,
    })
}

fn synchronize_destination_parent(destination: &Path) -> Result<(), MoveError> {
    let parent = effective_parent(destination)
        .ok_or_else(|| MoveError::InvalidDestination(destination.to_path_buf()))?;
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| MoveError::Io {
            action: "synchronize destination directory after move",
            path: parent.to_path_buf(),
            source,
        })
}

fn available_staging_path(destination: &Path) -> Result<PathBuf, MoveError> {
    let parent = effective_parent(destination)
        .ok_or_else(|| MoveError::InvalidDestination(destination.to_path_buf()))?;
    for _ in 0..MAX_STAGING_ATTEMPTS {
        let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".floe-transfer-{}-{sequence}.partial",
            std::process::id()
        ));
        match fs::symlink_metadata(&candidate) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(candidate),
            Ok(_) => {}
            Err(source) => {
                return Err(MoveError::Io {
                    action: "inspect transfer staging path",
                    path: candidate,
                    source,
                });
            }
        }
    }
    Err(MoveError::Io {
        action: "allocate transfer staging path",
        path: parent.to_path_buf(),
        source: io::Error::new(
            io::ErrorKind::AlreadyExists,
            "all bounded staging names were occupied",
        ),
    })
}

fn snapshot_tree(path: &Path) -> Result<TreeSnapshot, MoveError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| MoveError::Io {
        action: "snapshot move source",
        path: path.to_path_buf(),
        source,
    })?;
    let file_type = metadata.file_type();
    let kind = if file_type.is_file() {
        SnapshotKind::File
    } else if file_type.is_dir() {
        SnapshotKind::Directory
    } else if file_type.is_symlink() {
        SnapshotKind::Symlink
    } else {
        return Err(CopyError::UnsupportedFileType(path.to_path_buf()).into());
    };
    let mut children = Vec::new();
    if kind == SnapshotKind::Directory {
        let entries = fs::read_dir(path).map_err(|source| MoveError::Io {
            action: "read move source directory",
            path: path.to_path_buf(),
            source,
        })?;
        let mut paths = entries
            .map(|entry| {
                entry
                    .map(|entry| entry.path())
                    .map_err(|source| MoveError::Io {
                        action: "read move source directory entry",
                        path: path.to_path_buf(),
                        source,
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        paths.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
        for child in paths {
            children.push(snapshot_tree(&child)?);
        }
    }
    Ok(TreeSnapshot {
        path: path.to_path_buf(),
        device: metadata.dev(),
        inode: metadata.ino(),
        mode: metadata.mode(),
        length: metadata.len(),
        modified_seconds: metadata.mtime(),
        modified_nanoseconds: metadata.mtime_nsec(),
        kind,
        children,
    })
}

fn snapshot_matches(snapshot: &TreeSnapshot) -> Result<bool, MoveError> {
    let metadata = match fs::symlink_metadata(&snapshot.path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(source) => {
            return Err(MoveError::Io {
                action: "revalidate move source",
                path: snapshot.path.clone(),
                source,
            });
        }
    };
    if metadata.dev() != snapshot.device
        || metadata.ino() != snapshot.inode
        || metadata.mode() != snapshot.mode
        || metadata.len() != snapshot.length
        || metadata.mtime() != snapshot.modified_seconds
        || metadata.mtime_nsec() != snapshot.modified_nanoseconds
    {
        return Ok(false);
    }
    if snapshot.kind != SnapshotKind::Directory {
        return Ok(true);
    }

    let entries = fs::read_dir(&snapshot.path).map_err(|source| MoveError::Io {
        action: "revalidate move source directory",
        path: snapshot.path.clone(),
        source,
    })?;
    let mut names = entries
        .map(|entry| {
            entry
                .map(|entry| entry.file_name())
                .map_err(|source| MoveError::Io {
                    action: "revalidate move source directory entry",
                    path: snapshot.path.clone(),
                    source,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    names.sort();
    let expected = snapshot
        .children
        .iter()
        .filter_map(|child| child.path.file_name().map(OsStr::to_os_string))
        .collect::<Vec<_>>();
    if names != expected {
        return Ok(false);
    }
    for child in &snapshot.children {
        if !snapshot_matches(child)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn remove_snapshot_tree(
    snapshot: &TreeSnapshot,
    cancellation: &MoveCancellation,
) -> Result<(), String> {
    if cancellation.is_cancelled() {
        return Err("cancellation during source cleanup".to_owned());
    }
    if !snapshot_matches(snapshot).map_err(|error| error.to_string())? {
        return Err(format!(
            "source identity changed before cleanup: {}",
            snapshot.path.display()
        ));
    }
    for child in &snapshot.children {
        remove_snapshot_tree(child, cancellation)?;
    }
    if !snapshot_matches_shallow(snapshot).map_err(|error| error.to_string())? {
        return Err(format!(
            "source identity changed during cleanup: {}",
            snapshot.path.display()
        ));
    }
    let result = match snapshot.kind {
        SnapshotKind::Directory => fs::remove_dir(&snapshot.path),
        SnapshotKind::File | SnapshotKind::Symlink => fs::remove_file(&snapshot.path),
    };
    result.map_err(|error| {
        format!(
            "source cleanup failed at {}: {error}",
            snapshot.path.display()
        )
    })
}

fn snapshot_matches_shallow(snapshot: &TreeSnapshot) -> Result<bool, MoveError> {
    let metadata = match fs::symlink_metadata(&snapshot.path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(source) => {
            return Err(MoveError::Io {
                action: "revalidate move source during cleanup",
                path: snapshot.path.clone(),
                source,
            });
        }
    };
    let identity_matches = metadata.dev() == snapshot.device
        && metadata.ino() == snapshot.inode
        && metadata.mode() == snapshot.mode;
    if snapshot.kind == SnapshotKind::Directory {
        Ok(identity_matches)
    } else {
        Ok(identity_matches
            && metadata.len() == snapshot.length
            && metadata.mtime() == snapshot.modified_seconds
            && metadata.mtime_nsec() == snapshot.modified_nanoseconds)
    }
}

fn cleanup_staging(staging: &Path) -> Result<(), MoveError> {
    let cleanup = match fs::symlink_metadata(staging) {
        Ok(metadata) if metadata.file_type().is_dir() => fs::remove_dir_all(staging),
        Ok(_) => fs::remove_file(staging),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => Err(error),
    };
    cleanup.map_err(|source| MoveError::Io {
        action: "clean cross-filesystem staging path",
        path: staging.to_path_buf(),
        source,
    })
}

fn committed_partial(request: &MoveRequest, reason: impl Into<String>) -> MoveError {
    MoveError::Partial {
        source_path: request.source().to_path_buf(),
        destination_path: request.destination().to_path_buf(),
        reason: reason.into(),
    }
}

fn validate_move(request: &MoveRequest) -> Result<FileIdentity, MoveError> {
    if request.source().file_name().is_none() {
        return Err(MoveError::InvalidSource(request.source().to_path_buf()));
    }
    if request.destination().file_name().is_none() {
        return Err(MoveError::InvalidDestination(
            request.destination().to_path_buf(),
        ));
    }

    let destination_parent = effective_parent(request.destination())
        .ok_or_else(|| MoveError::InvalidDestination(request.destination().to_path_buf()))?;
    match fs::metadata(destination_parent) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            return Err(MoveError::DestinationParentMissing(
                destination_parent.to_path_buf(),
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(MoveError::DestinationParentMissing(
                destination_parent.to_path_buf(),
            ));
        }
        Err(source) => {
            return Err(MoveError::Io {
                action: "inspect destination parent",
                path: destination_parent.to_path_buf(),
                source,
            });
        }
    }

    let source = lexically_normalized(request.source());
    let destination = lexically_normalized(request.destination());
    if source == destination {
        return Err(MoveError::SamePath(request.source().to_path_buf()));
    }

    let metadata = match fs::symlink_metadata(request.source()) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(MoveError::SourceMissing(request.source().to_path_buf()));
        }
        Err(source) => {
            return Err(MoveError::Io {
                action: "inspect source",
                path: request.source().to_path_buf(),
                source,
            });
        }
    };

    let identity = FileIdentity::from_metadata(&metadata);
    if request
        .expected_source_identity()
        .is_some_and(|expected| !expected.matches(&metadata))
    {
        return Err(MoveError::SourceChanged(request.source().to_path_buf()));
    }
    if metadata.file_type().is_dir() && destination.starts_with(&source) {
        return Err(MoveError::DestinationInsideSource(destination));
    }

    Ok(identity)
}

fn capture_identity(path: &Path) -> Result<FileIdentity, MoveError> {
    fs::symlink_metadata(path)
        .map(|metadata| FileIdentity::from_metadata(&metadata))
        .map_err(|source| MoveError::Io {
            action: "capture move destination identity",
            path: path.to_path_buf(),
            source,
        })
}

fn rename_destination(request: &RenameRequest) -> Result<PathBuf, MoveError> {
    let source_parent = request
        .source()
        .parent()
        .ok_or_else(|| MoveError::InvalidSource(request.source().to_path_buf()))?;
    let name_path = Path::new(request.new_name());
    let mut components = name_path.components();
    let valid_component =
        matches!(components.next(), Some(Component::Normal(name)) if name == request.new_name());
    if !valid_component || components.next().is_some() || request.new_name().as_bytes().contains(&0)
    {
        return Err(MoveError::InvalidName(name_path.to_path_buf()));
    }
    Ok(source_parent.join(request.new_name()))
}

fn check_cancelled(cancellation: &MoveCancellation) -> Result<(), MoveError> {
    if cancellation.is_cancelled() {
        Err(MoveError::Cancelled)
    } else {
        Ok(())
    }
}

fn map_rename_error(request: &MoveRequest, error: Errno) -> MoveError {
    if error == Errno::EXIST || error == Errno::NOTEMPTY {
        MoveError::DestinationExists(request.destination().to_path_buf())
    } else if error == Errno::NOENT {
        match fs::symlink_metadata(request.source()) {
            Err(source_error) if source_error.kind() == io::ErrorKind::NotFound => {
                MoveError::SourceMissing(request.source().to_path_buf())
            }
            _ => MoveError::DestinationParentMissing(
                effective_parent(request.destination())
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf(),
            ),
        }
    } else {
        MoveError::Io {
            action: "move item",
            path: request.source().to_path_buf(),
            source: error.into(),
        }
    }
}

fn effective_parent(path: &Path) -> Option<&Path> {
    path.parent().map(|parent| {
        if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        }
    })
}

fn lexically_normalized(path: &Path) -> PathBuf {
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
            fs as unix_fs,
            fs::{MetadataExt, PermissionsExt},
        },
    };

    use tempfile::{Builder, tempdir};

    use super::*;

    fn move_request(source: &Path, destination: &Path) -> MoveRequest {
        MoveRequest::new(source, destination, ConflictPolicy::FailIfExists)
    }

    #[test]
    fn move_operation_moves_regular_file_without_copying() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"floe").expect("source fixture should be writable");

        execute_move(
            &move_request(&source, &destination),
            &MoveCancellation::new(),
        )
        .expect("file move should succeed");

        assert!(!source.exists());
        assert_eq!(
            fs::read(destination).expect("destination should be readable"),
            b"floe"
        );
    }

    #[test]
    fn move_operation_moves_directory_tree() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::create_dir(&source).expect("source directory should be creatable");
        fs::write(source.join("child"), b"tree").expect("child should be writable");

        let nested_error = execute_move(
            &move_request(&source, &source.join("nested")),
            &MoveCancellation::new(),
        )
        .expect_err("directory must not move inside itself");
        assert!(matches!(
            nested_error,
            MoveError::DestinationInsideSource(_)
        ));

        execute_move(
            &move_request(&source, &destination),
            &MoveCancellation::new(),
        )
        .expect("directory move should succeed");

        assert!(!source.exists());
        assert_eq!(
            fs::read(destination.join("child")).expect("moved child should be readable"),
            b"tree"
        );
    }

    #[test]
    fn move_operation_preserves_symlink_without_following_target() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source-link");
        let destination = fixture.path().join("destination-link");
        unix_fs::symlink("missing-target", &source).expect("symlink fixture should be creatable");

        execute_move(
            &move_request(&source, &destination),
            &MoveCancellation::new(),
        )
        .expect("symlink move should succeed");

        assert!(fs::symlink_metadata(&source).is_err());
        assert!(
            fs::symlink_metadata(&destination)
                .expect("destination symlink should exist")
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_link(destination).expect("symlink target should be readable"),
            PathBuf::from("missing-target")
        );
    }

    #[test]
    fn move_operation_conflict_leaves_source_and_destination_unchanged() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"source").expect("source fixture should be writable");
        fs::write(&destination, b"destination").expect("destination fixture should be writable");

        let error = execute_move(
            &move_request(&source, &destination),
            &MoveCancellation::new(),
        )
        .expect_err("existing destination must reject move");

        assert!(matches!(error, MoveError::DestinationExists(path) if path == destination));
        assert_eq!(fs::read(source).expect("source should remain"), b"source");
        assert_eq!(
            fs::read(destination).expect("destination should remain"),
            b"destination"
        );
    }

    #[test]
    fn move_operation_rejects_invalid_rename_name() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        fs::write(&source, b"source").expect("source fixture should be writable");
        let invalid_names = [
            OsString::new(),
            OsString::from("."),
            OsString::from(".."),
            OsString::from("nested/name"),
            OsString::from_vec(b"nul\0name".to_vec()),
        ];
        for invalid_name in invalid_names {
            let request = RenameRequest::new(&source, invalid_name, ConflictPolicy::FailIfExists);
            let error = execute_rename(&request, &MoveCancellation::new())
                .expect_err("invalid name must be rejected");
            assert!(matches!(error, MoveError::InvalidName(_)));
        }
        let relative_request = RenameRequest::new("before", "after", ConflictPolicy::FailIfExists);
        assert_eq!(
            rename_destination(&relative_request).expect("relative rename should be valid"),
            PathBuf::from("after")
        );
        assert!(source.exists());
    }

    #[test]
    fn move_operation_rejects_missing_source() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("missing");
        let destination = fixture.path().join("destination");

        let error = execute_move(
            &move_request(&source, &destination),
            &MoveCancellation::new(),
        )
        .expect_err("missing source must be rejected");

        assert!(matches!(error, MoveError::SourceMissing(path) if path == source));
        assert!(!destination.exists());
    }

    #[test]
    fn move_operation_honors_pre_cancel_before_irreversible_boundary() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"source").expect("source fixture should be writable");
        let cancellation = MoveCancellation::new();
        cancellation.cancel();

        let error = execute_move(&move_request(&source, &destination), &cancellation)
            .expect_err("pre-cancelled move must be rejected");

        assert!(matches!(error, MoveError::Cancelled));
        assert!(source.exists());
        assert!(!destination.exists());
    }

    #[test]
    fn move_operation_preserves_non_utf8_rename_name() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        fs::write(&source, b"source").expect("source fixture should be writable");
        let new_name = OsString::from_vec(b"renamed-\xff".to_vec());
        let request = RenameRequest::new(&source, new_name.clone(), ConflictPolicy::FailIfExists);

        execute_rename(&request, &MoveCancellation::new())
            .expect("non-UTF-8 rename should succeed");

        let renamed = fs::read_dir(fixture.path())
            .expect("fixture should be readable")
            .next()
            .expect("renamed item should exist")
            .expect("renamed entry should be readable");
        assert_eq!(renamed.file_name().as_bytes(), new_name.as_bytes());
    }

    #[test]
    fn phase_6o_cross_filesystem_preserves_tree_symlink_and_non_utf8_identity() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::create_dir(&source).expect("source directory should be creatable");
        let raw_name = OsString::from_vec(b"item-\xff".to_vec());
        fs::write(source.join(&raw_name), b"payload").expect("non-UTF-8 source should be writable");
        unix_fs::symlink(&raw_name, source.join("link"))
            .expect("relative source symlink should be creatable");

        execute_cross_filesystem_move(
            &move_request(&source, &destination),
            &MoveCancellation::new(),
            &mut |_| {},
        )
        .expect("injected cross-filesystem fallback should complete");

        assert!(!source.exists());
        assert_eq!(
            fs::read(destination.join(&raw_name)).expect("moved file should be readable"),
            b"payload"
        );
        assert_eq!(
            fs::read_link(destination.join("link")).expect("moved link should be readable"),
            PathBuf::from(raw_name)
        );
    }

    #[test]
    fn phase_6o_cross_filesystem_real_exdev_uses_fallback_when_devices_differ() {
        let source_fixture = tempdir().expect("temporary source should be available");
        let destination_fixture = Builder::new()
            .prefix("floe-6o-xdev-")
            .tempdir_in(env!("CARGO_MANIFEST_DIR"))
            .expect("workspace-device destination should be available");
        if fs::metadata(source_fixture.path())
            .expect("source device should be inspectable")
            .dev()
            == fs::metadata(destination_fixture.path())
                .expect("destination device should be inspectable")
                .dev()
        {
            return;
        }

        let source = source_fixture.path().join("source");
        let destination = destination_fixture.path().join("destination");
        fs::write(&source, b"real EXDEV fallback").expect("cross-device source should be writable");
        execute_move(
            &move_request(&source, &destination),
            &MoveCancellation::new(),
        )
        .expect("real EXDEV move should use copy-delete fallback");

        assert!(!source.exists());
        assert_eq!(
            fs::read(destination).expect("cross-device destination should be readable"),
            b"real EXDEV fallback"
        );
    }

    #[test]
    fn phase_6o_cross_filesystem_conflict_never_overwrites_and_cleans_staging() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"source").expect("source fixture should be writable");
        fs::write(&destination, b"keep").expect("destination fixture should be writable");

        let error = execute_cross_filesystem_move(
            &move_request(&source, &destination),
            &MoveCancellation::new(),
            &mut |_| {},
        )
        .expect_err("existing destination must reject staged publication");

        assert!(matches!(error, MoveError::DestinationExists(path) if path == destination));
        assert_eq!(fs::read(&source).expect("source should remain"), b"source");
        assert_eq!(
            fs::read(&destination).expect("destination should remain"),
            b"keep"
        );
        assert!(
            fs::read_dir(fixture.path())
                .expect("fixture should be readable")
                .all(|entry| !entry
                    .expect("fixture entry should be readable")
                    .file_name()
                    .as_bytes()
                    .starts_with(b".floe-transfer-"))
        );
    }

    #[test]
    fn phase_6o_recovery_source_change_removes_staging_and_retains_source() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        let original = vec![0x31; 128 * 1024 * 2];
        fs::write(&source, &original).expect("source fixture should be writable");
        let mut changed = false;

        let error = execute_cross_filesystem_move(
            &move_request(&source, &destination),
            &MoveCancellation::new(),
            &mut |progress| {
                if !changed && progress.bytes_copied() > 0 {
                    changed = true;
                    fs::write(&source, vec![0x32; original.len()])
                        .expect("source should be changeable during injected copy");
                }
            },
        )
        .expect_err("changed source must not be removed or published");

        assert!(matches!(error, MoveError::SourceChanged(path) if path == source));
        assert!(source.exists());
        assert!(!destination.exists());
    }

    #[test]
    fn phase_6o_recovery_post_commit_delete_failure_is_partial() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source_parent = fixture.path().join("locked-source");
        let destination_parent = fixture.path().join("destination-parent");
        fs::create_dir(&source_parent).expect("source parent should be creatable");
        fs::create_dir(&destination_parent).expect("destination parent should be creatable");
        let source = source_parent.join("source");
        let destination = destination_parent.join("destination");
        fs::write(&source, b"payload").expect("source fixture should be writable");
        fs::set_permissions(&source_parent, fs::Permissions::from_mode(0o500))
            .expect("source parent should become non-writable");

        let result = execute_cross_filesystem_move(
            &move_request(&source, &destination),
            &MoveCancellation::new(),
            &mut |_| {},
        );
        fs::set_permissions(&source_parent, fs::Permissions::from_mode(0o700))
            .expect("source parent permissions should be restored");
        let error = result.expect_err("source removal should fail after destination commit");

        assert!(error.is_partial());
        assert!(source.exists());
        assert_eq!(
            fs::read(destination).expect("committed destination should be complete"),
            b"payload"
        );
    }

    #[test]
    fn phase_6p_undo_revalidates_identity_and_preserves_non_utf8_paths() {
        let fixture = tempdir().expect("temporary directory should be available");
        let raw_source = OsString::from_vec(b"source-\xff".to_vec());
        let raw_destination = OsString::from_vec(b"destination-\xfe".to_vec());
        let source = fixture.path().join(raw_source);
        let destination = fixture.path().join(raw_destination);
        fs::write(&source, b"payload").expect("source fixture should be writable");

        let outcome = execute_move(
            &move_request(&source, &destination),
            &MoveCancellation::new(),
        )
        .expect("initial move should succeed");
        execute_move(
            &MoveRequest::new(&destination, &source, ConflictPolicy::FailIfExists)
                .with_expected_source_identity(outcome.destination_identity()),
            &MoveCancellation::new(),
        )
        .expect("identity-matching undo should succeed");

        assert_eq!(
            fs::read(&source).expect("source should be restored"),
            b"payload"
        );
        assert!(!destination.exists());
        assert_eq!(
            source
                .file_name()
                .expect("source should retain a name")
                .as_bytes(),
            b"source-\xff"
        );
    }

    #[test]
    fn phase_6p_undo_rejects_conflict_and_changed_destination_identity() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"payload").expect("source fixture should be writable");
        let outcome = execute_move(
            &move_request(&source, &destination),
            &MoveCancellation::new(),
        )
        .expect("initial move should succeed");
        let undo = MoveRequest::new(&destination, &source, ConflictPolicy::FailIfExists)
            .with_expected_source_identity(outcome.destination_identity());

        fs::write(&source, b"blocker").expect("original path blocker should be writable");
        assert!(matches!(
            execute_move(&undo, &MoveCancellation::new()),
            Err(MoveError::DestinationExists(path)) if path == source
        ));
        fs::remove_file(&source).expect("blocker should be removable");
        fs::write(&destination, b"changed payload")
            .expect("moved destination should be changeable");
        assert!(matches!(
            execute_move(&undo, &MoveCancellation::new()),
            Err(MoveError::SourceChanged(path)) if path == destination
        ));
        assert!(!source.exists());
        assert_eq!(
            fs::read(destination).expect("changed destination must remain"),
            b"changed payload"
        );
    }
}
