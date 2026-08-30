use std::{
    fs, io,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

use rustix::fs::{CWD, RenameFlags, renameat_with};
use thiserror::Error;

use crate::{
    ConflictPolicy, CopyCancellation, CopyError, CopyRequest, FileIdentity, MoveCancellation,
    MoveError, MoveRequest, SymlinkPolicy, execute_copy, execute_move,
};

pub const REPLACE_BACKUP_DIRECTORY: &str = ".floe-replace-backups";
pub const REPLACE_BACKUP_CAPACITY: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplaceMode {
    Copy,
    Move,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplaceRequest {
    source: PathBuf,
    destination: PathBuf,
    backup: PathBuf,
    mode: ReplaceMode,
    symlink_policy: SymlinkPolicy,
    expected_source: FileIdentity,
    expected_destination: FileIdentity,
}

impl ReplaceRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: impl Into<PathBuf>,
        destination: impl Into<PathBuf>,
        backup: impl Into<PathBuf>,
        mode: ReplaceMode,
        symlink_policy: SymlinkPolicy,
        expected_source: FileIdentity,
        expected_destination: FileIdentity,
    ) -> Self {
        Self {
            source: source.into(),
            destination: destination.into(),
            backup: backup.into(),
            mode,
            symlink_policy,
            expected_source,
            expected_destination,
        }
    }

    pub fn source(&self) -> &Path {
        &self.source
    }

    pub fn destination(&self) -> &Path {
        &self.destination
    }

    pub fn backup(&self) -> &Path {
        &self.backup
    }

    pub const fn mode(&self) -> ReplaceMode {
        self.mode
    }

    pub const fn symlink_policy(&self) -> SymlinkPolicy {
        self.symlink_policy
    }

    pub const fn expected_source(&self) -> FileIdentity {
        self.expected_source
    }

    pub const fn expected_destination(&self) -> FileIdentity {
        self.expected_destination
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplaceOutcome {
    destination_identity: FileIdentity,
    backup_identity: FileIdentity,
}

impl ReplaceOutcome {
    pub const fn destination_identity(self) -> FileIdentity {
        self.destination_identity
    }

    pub const fn backup_identity(self) -> FileIdentity {
        self.backup_identity
    }
}

#[derive(Clone, Debug, Default)]
pub struct ReplaceCancellation {
    copy: CopyCancellation,
    move_item: MoveCancellation,
}

impl ReplaceCancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.copy.cancel();
        self.move_item.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.copy.is_cancelled() || self.move_item.is_cancelled()
    }
}

#[derive(Debug, Error)]
pub enum ReplaceError {
    #[error("replacement was cancelled before its atomic commit")]
    Cancelled,
    #[error("replacement source changed before commit: {}", .0.display())]
    SourceChanged(PathBuf),
    #[error("replacement destination changed before commit: {}", .0.display())]
    DestinationChanged(PathBuf),
    #[error("replacement backup path is invalid or outside the private staging root: {}", .0.display())]
    InvalidBackup(PathBuf),
    #[error("private replacement backup capacity ({REPLACE_BACKUP_CAPACITY}) is full beside {}", .0.display())]
    BackupCapacity(PathBuf),
    #[error("cannot {action} replacement path {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot stage replacement copy: {0}")]
    Copy(#[from] CopyError),
    #[error("cannot stage or roll back replacement move: {0}")]
    Move(#[from] MoveError),
    #[error("replacement did not commit and staging rollback failed: {message}")]
    RollbackFailed { message: String },
    #[error("replacement committed but exact post-commit evidence is incomplete: {message}")]
    Partial { message: String },
}

impl ReplaceError {
    pub const fn is_cancelled(&self) -> bool {
        match self {
            Self::Cancelled => true,
            Self::Copy(error) => error.is_cancelled(),
            Self::Move(error) => error.is_cancelled(),
            _ => false,
        }
    }

    pub const fn is_partial(&self) -> bool {
        match self {
            Self::RollbackFailed { .. } | Self::Partial { .. } => true,
            Self::Copy(error) => error.is_partial(),
            Self::Move(error) => error.is_partial(),
            _ => false,
        }
    }

    pub const fn is_conflict(&self) -> bool {
        match self {
            Self::SourceChanged(_) | Self::DestinationChanged(_) => true,
            Self::Copy(error) => error.is_conflict(),
            Self::Move(error) => error.is_conflict(),
            _ => false,
        }
    }

    pub const fn is_unsupported(&self) -> bool {
        match self {
            Self::Copy(error) => error.is_unsupported(),
            Self::Move(error) => error.is_unsupported(),
            _ => false,
        }
    }

    pub fn io_kind(&self) -> Option<io::ErrorKind> {
        match self {
            Self::Io { source, .. } => Some(source.kind()),
            Self::Copy(error) => error.io_kind(),
            Self::Move(error) => error.io_kind(),
            _ => None,
        }
    }
}

/// Allocate one exact backup path inside an owner-only directory on the same
/// filesystem as the destination. The caller must persist the returned raw path
/// before submitting mutation work.
pub fn allocate_replace_backup(
    destination: &Path,
    operation_id: u64,
) -> Result<PathBuf, ReplaceError> {
    let destination_parent = destination
        .parent()
        .ok_or_else(|| ReplaceError::InvalidBackup(destination.to_path_buf()))?;
    let root = destination_parent.join(REPLACE_BACKUP_DIRECTORY);
    match fs::symlink_metadata(&root) {
        Ok(metadata) => validate_backup_root(&root, &metadata)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&root)
                .map_err(|source| ReplaceError::Io {
                    action: "create private backup directory",
                    path: root.clone(),
                    source,
                })?;
            let metadata = fs::symlink_metadata(&root).map_err(|source| ReplaceError::Io {
                action: "inspect private backup directory",
                path: root.clone(),
                source,
            })?;
            validate_backup_root(&root, &metadata)?;
        }
        Err(source) => {
            return Err(ReplaceError::Io {
                action: "inspect private backup directory",
                path: root,
                source,
            });
        }
    }

    let count = fs::read_dir(&root)
        .map_err(|source| ReplaceError::Io {
            action: "enumerate private backups",
            path: root.clone(),
            source,
        })?
        .take(REPLACE_BACKUP_CAPACITY + 1)
        .count();
    if count >= REPLACE_BACKUP_CAPACITY {
        return Err(ReplaceError::BackupCapacity(root));
    }

    let backup = root.join(format!("{operation_id:016x}.backup"));
    match fs::symlink_metadata(&backup) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(backup),
        Ok(_) => Err(ReplaceError::InvalidBackup(backup)),
        Err(source) => Err(ReplaceError::Io {
            action: "inspect backup destination",
            path: backup,
            source,
        }),
    }
}

/// Remove a replacement backup only when its private root and exact no-follow
/// identity still match the evidence retained by durable Undo history.
///
/// This never follows a symbolic-link backup root and never removes an object
/// whose identity changed after the replacement committed.
pub fn remove_replace_backup(
    backup: &Path,
    expected_identity: FileIdentity,
) -> Result<(), ReplaceError> {
    let root = backup
        .parent()
        .ok_or_else(|| ReplaceError::InvalidBackup(backup.to_path_buf()))?;
    if root
        .file_name()
        .is_none_or(|name| name != REPLACE_BACKUP_DIRECTORY)
        || backup.file_name().is_none()
    {
        return Err(ReplaceError::InvalidBackup(backup.to_path_buf()));
    }
    let root_metadata = fs::symlink_metadata(root).map_err(|source| ReplaceError::Io {
        action: "inspect private backup directory",
        path: root.to_path_buf(),
        source,
    })?;
    validate_backup_root(root, &root_metadata)?;
    remove_owned_stage(backup, expected_identity)?;

    // Best-effort removal of the Floe-owned container. A non-empty directory
    // belongs to other still-live replacement records and must remain.
    let root_identity = FileIdentity::from_metadata(&root_metadata);
    let empty = fs::read_dir(root)
        .map_err(|source| ReplaceError::Io {
            action: "inspect private backup directory",
            path: root.to_path_buf(),
            source,
        })?
        .next()
        .is_none();
    if empty {
        revalidate(root, root_identity, ReplaceError::InvalidBackup)?;
        match fs::remove_dir(root) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::DirectoryNotEmpty | io::ErrorKind::NotFound
                ) => {}
            Err(source) => {
                return Err(ReplaceError::Io {
                    action: "remove empty private backup directory",
                    path: root.to_path_buf(),
                    source,
                });
            }
        }
    }
    Ok(())
}

/// Stage the incoming object beside the destination and atomically exchange it
/// with the reviewed existing destination. After success `backup` owns the old
/// destination and must remain until durable Undo history expires or is
/// explicitly resolved.
pub fn execute_replace(
    request: &ReplaceRequest,
    cancellation: &ReplaceCancellation,
) -> Result<ReplaceOutcome, ReplaceError> {
    validate_request(request)?;
    revalidate(
        request.source(),
        request.expected_source(),
        ReplaceError::SourceChanged,
    )?;
    revalidate(
        request.destination(),
        request.expected_destination(),
        ReplaceError::DestinationChanged,
    )?;
    check_cancelled(cancellation)?;

    match request.mode() {
        ReplaceMode::Copy => {
            execute_copy(
                &CopyRequest::new(
                    request.source(),
                    request.backup(),
                    ConflictPolicy::FailIfExists,
                    request.symlink_policy(),
                ),
                &cancellation.copy,
                |_| {},
            )?;
        }
        ReplaceMode::Move => {
            execute_move(
                &MoveRequest::new(
                    request.source(),
                    request.backup(),
                    ConflictPolicy::FailIfExists,
                )
                .with_expected_source_identity(request.expected_source()),
                &cancellation.move_item,
            )?;
        }
    };

    let staged_identity = capture(request.backup(), "capture staged replacement")?;
    let ready = (|| {
        if request.mode() == ReplaceMode::Copy {
            revalidate(
                request.source(),
                request.expected_source(),
                ReplaceError::SourceChanged,
            )?;
        }
        revalidate(
            request.destination(),
            request.expected_destination(),
            ReplaceError::DestinationChanged,
        )?;
        check_cancelled(cancellation)
    })();
    if let Err(error) = ready {
        rollback_stage(request, staged_identity).map_err(|rollback| {
            ReplaceError::RollbackFailed {
                message: format!("{error}; {rollback}"),
            }
        })?;
        return Err(error);
    }

    if let Err(error) = exchange(request.backup(), request.destination()) {
        rollback_stage(request, staged_identity).map_err(|rollback| {
            ReplaceError::RollbackFailed {
                message: format!("{error}; {rollback}"),
            }
        })?;
        return Err(error);
    }

    let destination_identity = capture(request.destination(), "capture replacement result")
        .map_err(|error| ReplaceError::Partial {
            message: error.to_string(),
        })?;
    let backup_identity =
        capture(request.backup(), "capture replacement backup").map_err(|error| {
            ReplaceError::Partial {
                message: error.to_string(),
            }
        })?;
    if destination_identity != staged_identity || backup_identity != request.expected_destination()
    {
        return Err(ReplaceError::Partial {
            message: "atomic exchange completed but post-commit identities did not match the reviewed versions"
                .to_owned(),
        });
    }
    Ok(ReplaceOutcome {
        destination_identity,
        backup_identity,
    })
}

/// Atomically swap the reviewed replacement and backup. This is the durable
/// Undo/Redo primitive; both versions remain present after every successful
/// action.
pub fn exchange_replace_versions(
    destination: &Path,
    backup: &Path,
    expected_destination: FileIdentity,
    expected_backup: FileIdentity,
) -> Result<ReplaceOutcome, ReplaceError> {
    validate_backup_location(destination, backup)?;
    revalidate(
        destination,
        expected_destination,
        ReplaceError::DestinationChanged,
    )?;
    revalidate(backup, expected_backup, ReplaceError::SourceChanged)?;
    exchange(destination, backup)?;
    let destination_identity =
        capture(destination, "capture exchanged destination").map_err(|error| {
            ReplaceError::Partial {
                message: error.to_string(),
            }
        })?;
    let backup_identity =
        capture(backup, "capture exchanged backup").map_err(|error| ReplaceError::Partial {
            message: error.to_string(),
        })?;
    if destination_identity != expected_backup || backup_identity != expected_destination {
        return Err(ReplaceError::Partial {
            message: "version exchange completed but post-commit identities were unexpected"
                .to_owned(),
        });
    }
    Ok(ReplaceOutcome {
        destination_identity,
        backup_identity,
    })
}

fn validate_request(request: &ReplaceRequest) -> Result<(), ReplaceError> {
    if request.source() == request.destination()
        || request.source() == request.backup()
        || request.destination() == request.backup()
    {
        return Err(ReplaceError::InvalidBackup(request.backup().to_path_buf()));
    }
    validate_backup_location(request.destination(), request.backup())
}

fn validate_backup_location(destination: &Path, backup: &Path) -> Result<(), ReplaceError> {
    let destination_parent = destination
        .parent()
        .ok_or_else(|| ReplaceError::InvalidBackup(backup.to_path_buf()))?
        .canonicalize()
        .map_err(|source| ReplaceError::Io {
            action: "resolve destination parent",
            path: destination.to_path_buf(),
            source,
        })?;
    let root = backup
        .parent()
        .ok_or_else(|| ReplaceError::InvalidBackup(backup.to_path_buf()))?;
    if root.file_name().and_then(|name| name.to_str()) != Some(REPLACE_BACKUP_DIRECTORY) {
        return Err(ReplaceError::InvalidBackup(backup.to_path_buf()));
    }
    let metadata = fs::symlink_metadata(root).map_err(|source| ReplaceError::Io {
        action: "inspect private backup directory",
        path: root.to_path_buf(),
        source,
    })?;
    validate_backup_root(root, &metadata)?;
    let root_parent = root
        .parent()
        .ok_or_else(|| ReplaceError::InvalidBackup(backup.to_path_buf()))?
        .canonicalize()
        .map_err(|source| ReplaceError::Io {
            action: "resolve backup parent",
            path: root.to_path_buf(),
            source,
        })?;
    if root_parent != destination_parent {
        return Err(ReplaceError::InvalidBackup(backup.to_path_buf()));
    }
    Ok(())
}

fn validate_backup_root(path: &Path, metadata: &fs::Metadata) -> Result<(), ReplaceError> {
    if !metadata.file_type().is_dir()
        || metadata.uid() != rustix::process::getuid().as_raw()
        || metadata.permissions().mode() & 0o777 != 0o700
    {
        return Err(ReplaceError::InvalidBackup(path.to_path_buf()));
    }
    Ok(())
}

fn rollback_stage(
    request: &ReplaceRequest,
    staged_identity: FileIdentity,
) -> Result<(), ReplaceError> {
    match request.mode() {
        ReplaceMode::Copy => remove_owned_stage(request.backup(), staged_identity),
        ReplaceMode::Move => execute_move(
            &MoveRequest::new(
                request.backup(),
                request.source(),
                ConflictPolicy::FailIfExists,
            )
            .with_expected_source_identity(staged_identity),
            &MoveCancellation::new(),
        )
        .map(|_| ())
        .map_err(ReplaceError::Move),
    }
}

fn remove_owned_stage(path: &Path, expected: FileIdentity) -> Result<(), ReplaceError> {
    revalidate(path, expected, ReplaceError::SourceChanged)?;
    let metadata = fs::symlink_metadata(path).map_err(|source| ReplaceError::Io {
        action: "inspect staged replacement",
        path: path.to_path_buf(),
        source,
    })?;
    let result = if metadata.file_type().is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    result.map_err(|source| ReplaceError::Io {
        action: "remove staged replacement",
        path: path.to_path_buf(),
        source,
    })
}

fn revalidate(
    path: &Path,
    expected: FileIdentity,
    changed: impl FnOnce(PathBuf) -> ReplaceError,
) -> Result<(), ReplaceError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if expected.matches(&metadata) => Ok(()),
        Ok(_) => Err(changed(path.to_path_buf())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(changed(path.to_path_buf())),
        Err(source) => Err(ReplaceError::Io {
            action: "revalidate replacement identity",
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn exchange(left: &Path, right: &Path) -> Result<(), ReplaceError> {
    renameat_with(CWD, left, CWD, right, RenameFlags::EXCHANGE).map_err(|error| ReplaceError::Io {
        action: "atomically exchange replacement versions",
        path: right.to_path_buf(),
        source: io::Error::from_raw_os_error(error.raw_os_error()),
    })
}

fn capture(path: &Path, action: &'static str) -> Result<FileIdentity, ReplaceError> {
    FileIdentity::capture(path).map_err(|source| ReplaceError::Io {
        action,
        path: path.to_path_buf(),
        source,
    })
}

fn check_cancelled(cancellation: &ReplaceCancellation) -> Result<(), ReplaceError> {
    if cancellation.is_cancelled() {
        Err(ReplaceError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt};

    use tempfile::tempdir;

    use super::*;

    fn request(
        source: &Path,
        destination: &Path,
        backup: &Path,
        mode: ReplaceMode,
    ) -> ReplaceRequest {
        ReplaceRequest::new(
            source,
            destination,
            backup,
            mode,
            SymlinkPolicy::Preserve,
            FileIdentity::capture(source).expect("source identity"),
            FileIdentity::capture(destination).expect("destination identity"),
        )
    }

    #[test]
    fn phase_6u_engine_copy_replace_preserves_both_versions_and_exchanges_back() {
        let fixture = tempdir().expect("fixture");
        let source = fixture.path().join("incoming");
        let destination = fixture.path().join("item");
        fs::write(&source, b"new").expect("source");
        fs::write(&destination, b"old").expect("destination");
        let backup = allocate_replace_backup(&destination, 1).expect("backup");
        let outcome = execute_replace(
            &request(&source, &destination, &backup, ReplaceMode::Copy),
            &ReplaceCancellation::new(),
        )
        .expect("replace");
        assert_eq!(fs::read(&destination).expect("new"), b"new");
        assert_eq!(fs::read(&backup).expect("old"), b"old");
        let undone = exchange_replace_versions(
            &destination,
            &backup,
            outcome.destination_identity(),
            outcome.backup_identity(),
        )
        .expect("undo exchange");
        assert_eq!(fs::read(&destination).expect("old restored"), b"old");
        assert_eq!(fs::read(&backup).expect("new retained"), b"new");
        assert_eq!(undone.destination_identity(), outcome.backup_identity());
    }

    #[test]
    fn phase_6u_engine_move_replace_removes_source_and_undo_retains_new_version() {
        let fixture = tempdir().expect("fixture");
        let source = fixture.path().join("incoming");
        let destination = fixture.path().join("item");
        fs::write(&source, b"new").expect("source");
        fs::write(&destination, b"old").expect("destination");
        let backup = allocate_replace_backup(&destination, 2).expect("backup");
        let outcome = execute_replace(
            &request(&source, &destination, &backup, ReplaceMode::Move),
            &ReplaceCancellation::new(),
        )
        .expect("replace");
        assert!(!source.exists());
        assert_eq!(fs::read(&destination).expect("new"), b"new");
        assert_eq!(fs::read(&backup).expect("old"), b"old");
        exchange_replace_versions(
            &destination,
            &backup,
            outcome.destination_identity(),
            outcome.backup_identity(),
        )
        .expect("undo exchange");
        assert_eq!(fs::read(&destination).expect("old restored"), b"old");
        assert_eq!(fs::read(&backup).expect("new retained"), b"new");
    }

    #[test]
    fn phase_6u_engine_changed_destination_rolls_move_stage_back() {
        let fixture = tempdir().expect("fixture");
        let source = fixture.path().join("incoming");
        let destination = fixture.path().join("item");
        fs::write(&source, b"new").expect("source");
        fs::write(&destination, b"old").expect("destination");
        let backup = allocate_replace_backup(&destination, 3).expect("backup");
        let replace = request(&source, &destination, &backup, ReplaceMode::Move);
        fs::write(&destination, b"changed").expect("race");
        // Retain the originally reviewed identity in the request.
        assert!(matches!(
            execute_replace(&replace, &ReplaceCancellation::new()),
            Err(ReplaceError::DestinationChanged(_))
        ));
        assert_eq!(fs::read(&source).expect("source retained"), b"new");
        assert_eq!(
            fs::read(&destination).expect("occupant retained"),
            b"changed"
        );
        assert!(!backup.exists());
    }

    #[test]
    fn phase_6u_recovery_backup_root_is_owner_only_and_rejects_insecure_reuse() {
        let fixture = tempdir().expect("fixture");
        let destination = fixture.path().join("item");
        fs::write(&destination, b"old").expect("destination");
        let backup = allocate_replace_backup(&destination, 4).expect("backup");
        let root = backup.parent().expect("root");
        assert_eq!(
            fs::symlink_metadata(root)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        fs::set_permissions(root, fs::Permissions::from_mode(0o755)).expect("weaken root");
        assert!(matches!(
            allocate_replace_backup(&destination, 5),
            Err(ReplaceError::InvalidBackup(_))
        ));
    }

    #[test]
    fn phase_6u_engine_precommit_cancellation_preserves_both_items() {
        let fixture = tempdir().expect("fixture");
        let source = fixture.path().join("incoming");
        let destination = fixture.path().join("item");
        fs::write(&source, b"new").expect("source");
        fs::write(&destination, b"old").expect("destination");
        let backup = allocate_replace_backup(&destination, 6).expect("backup");
        let cancellation = ReplaceCancellation::new();
        cancellation.cancel();

        assert!(matches!(
            execute_replace(
                &request(&source, &destination, &backup, ReplaceMode::Copy),
                &cancellation,
            ),
            Err(ReplaceError::Cancelled)
        ));
        assert_eq!(fs::read(&source).expect("source retained"), b"new");
        assert_eq!(
            fs::read(&destination).expect("destination retained"),
            b"old"
        );
        assert!(!backup.exists());
    }

    #[test]
    fn phase_6u_engine_backup_collision_is_never_reused() {
        let fixture = tempdir().expect("fixture");
        let destination = fixture.path().join("item");
        fs::write(&destination, b"old").expect("destination");
        let backup = allocate_replace_backup(&destination, 7).expect("backup path");
        fs::write(&backup, b"occupied").expect("collision occupant");
        assert!(matches!(
            allocate_replace_backup(&destination, 7),
            Err(ReplaceError::InvalidBackup(path)) if path == backup
        ));
        assert_eq!(fs::read(&backup).expect("occupant retained"), b"occupied");
    }

    #[test]
    fn phase_6u_engine_cleanup_rejects_changed_backup_identity() {
        let fixture = tempdir().expect("fixture");
        let source = fixture.path().join("incoming");
        let destination = fixture.path().join("item");
        fs::write(&source, b"new").expect("source");
        fs::write(&destination, b"old").expect("destination");
        let backup = allocate_replace_backup(&destination, 8).expect("backup");
        let outcome = execute_replace(
            &request(&source, &destination, &backup, ReplaceMode::Copy),
            &ReplaceCancellation::new(),
        )
        .expect("replace");
        fs::remove_file(&backup).expect("remove old backup fixture");
        fs::write(&backup, b"changed occupant").expect("changed occupant");

        assert!(matches!(
            remove_replace_backup(&backup, outcome.backup_identity()),
            Err(ReplaceError::SourceChanged(path)) if path == backup
        ));
        assert_eq!(
            fs::read(&backup).expect("changed occupant retained"),
            b"changed occupant"
        );
    }
}
