use std::{
    ffi::{OsStr, OsString},
    fs, io,
    os::unix::ffi::OsStrExt,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use rustix::{
    fs::{CWD, RenameFlags, renameat_with},
    io::Errno,
};
use thiserror::Error;

use crate::ConflictPolicy;

/// An exact-source, exact-destination move request.
///
/// Floe currently supports only fail-if-exists moves. The explicit policy is
/// retained so overwrite cannot appear accidentally when more policies arrive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoveRequest {
    source: PathBuf,
    destination: PathBuf,
    conflict_policy: ConflictPolicy,
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

/// Cooperative cancellation checked before the atomic rename syscall.
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
pub struct MoveOutcome;

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
    #[error("cross-filesystem move is not supported safely yet")]
    CrossFilesystem,
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
        matches!(self, Self::SamePath(_) | Self::DestinationExists(_))
    }

    pub const fn is_unsupported(&self) -> bool {
        matches!(self, Self::CrossFilesystem)
    }

    pub fn io_kind(&self) -> Option<io::ErrorKind> {
        match self {
            Self::Io { source, .. } => Some(source.kind()),
            _ => None,
        }
    }
}

/// Execute a same-filesystem move synchronously outside GTK's main loop.
///
/// Linux `renameat2(RENAME_NOREPLACE)` supplies the atomic conflict guarantee:
/// an existing destination is never replaced, including under races.
pub fn execute_move(
    request: &MoveRequest,
    cancellation: &MoveCancellation,
) -> Result<MoveOutcome, MoveError> {
    check_cancelled(cancellation)?;
    validate_move(request)?;
    check_cancelled(cancellation)?;

    match request.conflict_policy() {
        ConflictPolicy::FailIfExists => renameat_with(
            CWD,
            request.source(),
            CWD,
            request.destination(),
            RenameFlags::NOREPLACE,
        )
        .map_err(|error| map_rename_error(request, error))?,
    }

    Ok(MoveOutcome)
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

fn validate_move(request: &MoveRequest) -> Result<(), MoveError> {
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

    if metadata.file_type().is_dir() && destination.starts_with(&source) {
        return Err(MoveError::DestinationInsideSource(destination));
    }

    Ok(())
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
    } else if error == Errno::XDEV {
        MoveError::CrossFilesystem
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
        },
    };

    use tempfile::tempdir;

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
}
