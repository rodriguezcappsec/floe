use std::{
    fs::{self, File, OpenOptions, Permissions},
    io::{self, Read, Write},
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use thiserror::Error;

const COPY_BUFFER_SIZE: usize = 128 * 1024;

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
}

impl CopyOutcome {
    pub const fn entries_copied(self) -> u64 {
        self.entries_copied
    }

    pub const fn bytes_copied(self) -> u64 {
        self.bytes_copied
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
    #[error("symbolic links are rejected by this request: {}", .0.display())]
    SymlinkRejected(PathBuf),
    #[error("unsupported filesystem object: {}", .0.display())]
    UnsupportedFileType(PathBuf),
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
        permissions: Permissions,
        length: u64,
    },
    Directory {
        permissions: Permissions,
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
    created_paths: Vec<(PathBuf, CreatedKind)>,
}

#[derive(Clone, Copy)]
enum CreatedKind {
    FileLike,
    Directory,
}

/// Execute a copy synchronously. Callers must run this outside GTK's main loop.
///
/// The source tree is inspected before the destination is created. Progress is
/// reported after each entry and after each copied file chunk. On failure, only
/// paths created by this attempt are removed, in reverse order.
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
                permissions: metadata.permissions(),
                length,
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
                permissions: metadata.permissions(),
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
            permissions,
            length,
        } => {
            copy_file(
                &plan.source,
                &plan.destination,
                *length,
                permissions,
                cancellation,
                state,
                report_progress,
                total_entries,
                total_bytes,
            )?;
        }
        PlannedKind::Directory {
            permissions,
            children,
        } => {
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
            fs::set_permissions(&plan.destination, permissions.clone()).map_err(|source| {
                CopyError::Io {
                    action: "set directory permissions on",
                    path: plan.destination.clone(),
                    source,
                }
            })?;
        }
        PlannedKind::Symlink { target } => {
            symlink(target, &plan.destination).map_err(|source| {
                destination_create_error("create symbolic link at", &plan.destination, source)
            })?;
            state
                .created_paths
                .push((plan.destination.clone(), CreatedKind::FileLike));
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
    permissions: &Permissions,
    cancellation: &CopyCancellation,
    state: &mut CopyState,
    report_progress: &mut F,
    total_entries: u64,
    total_bytes: u64,
) -> Result<(), CopyError>
where
    F: FnMut(CopyProgress),
{
    let mut source = File::open(source_path).map_err(|source| CopyError::Io {
        action: "open source file",
        path: source_path.to_path_buf(),
        source,
    })?;
    let mut destination = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination_path)
        .map_err(|source| {
            destination_create_error("create destination file", destination_path, source)
        })?;
    state
        .created_paths
        .push((destination_path.to_path_buf(), CreatedKind::FileLike));

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

    destination
        .set_permissions(permissions.clone())
        .map_err(|source| CopyError::Io {
            action: "set destination file permissions on",
            path: destination_path.to_path_buf(),
            source,
        })?;
    Ok(())
}

fn create_directory(path: &Path, state: &mut CopyState) -> Result<(), CopyError> {
    fs::create_dir(path)
        .map_err(|source| destination_create_error("create destination directory", path, source))?;
    state
        .created_paths
        .push((path.to_path_buf(), CreatedKind::Directory));
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

fn cleanup_created(paths: &[(PathBuf, CreatedKind)]) -> Result<(), (PathBuf, io::Error)> {
    for (path, kind) in paths.iter().rev() {
        let result = match kind {
            CreatedKind::FileLike => fs::remove_file(path),
            CreatedKind::Directory => fs::remove_dir(path),
        };
        if let Err(error) = result
            && error.kind() != io::ErrorKind::NotFound
        {
            return Err((path.clone(), error));
        }
    }
    Ok(())
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
}
