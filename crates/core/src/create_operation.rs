use std::{
    fs::{self, OpenOptions},
    io,
    os::unix::fs as unix_fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

use rustix::io::Errno;
use thiserror::Error;

use crate::{
    ConflictPolicy, CopyCancellation, CopyError, CopyProgress, CopyRequest, SymlinkPolicy,
    execute_copy,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CreateKind {
    Directory,
    EmptyFile,
    Template { source: PathBuf },
    Duplicate { source: PathBuf },
    SymbolicLink { target: PathBuf },
    HardLink { source: PathBuf },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateRequest {
    kind: CreateKind,
    destination: PathBuf,
}

impl CreateRequest {
    pub fn directory(destination: impl Into<PathBuf>) -> Result<Self, CreateRequestError> {
        Self::new(CreateKind::Directory, destination.into())
    }

    pub fn empty_file(destination: impl Into<PathBuf>) -> Result<Self, CreateRequestError> {
        Self::new(CreateKind::EmptyFile, destination.into())
    }

    pub fn template(
        source: impl Into<PathBuf>,
        destination: impl Into<PathBuf>,
    ) -> Result<Self, CreateRequestError> {
        let source = source.into();
        if source.file_name().is_none() {
            return Err(CreateRequestError::InvalidSource(source));
        }
        Self::new(CreateKind::Template { source }, destination.into())
    }

    pub fn duplicate(
        source: impl Into<PathBuf>,
        destination: impl Into<PathBuf>,
    ) -> Result<Self, CreateRequestError> {
        let source = source.into();
        if source.file_name().is_none() {
            return Err(CreateRequestError::InvalidSource(source));
        }
        Self::new(CreateKind::Duplicate { source }, destination.into())
    }

    pub fn symbolic_link(
        target: impl Into<PathBuf>,
        destination: impl Into<PathBuf>,
    ) -> Result<Self, CreateRequestError> {
        Self::new(
            CreateKind::SymbolicLink {
                target: target.into(),
            },
            destination.into(),
        )
    }

    pub fn hard_link(
        source: impl Into<PathBuf>,
        destination: impl Into<PathBuf>,
    ) -> Result<Self, CreateRequestError> {
        let source = source.into();
        if source.file_name().is_none() {
            return Err(CreateRequestError::InvalidSource(source));
        }
        Self::new(CreateKind::HardLink { source }, destination.into())
    }

    fn new(kind: CreateKind, destination: PathBuf) -> Result<Self, CreateRequestError> {
        validate_destination(&destination)?;
        Ok(Self { kind, destination })
    }

    pub fn with_destination(
        &self,
        destination: impl Into<PathBuf>,
    ) -> Result<Self, CreateRequestError> {
        Self::new(self.kind.clone(), destination.into())
    }

    pub const fn kind(&self) -> &CreateKind {
        &self.kind
    }

    pub fn destination(&self) -> &Path {
        &self.destination
    }

    pub fn source(&self) -> Option<&Path> {
        match &self.kind {
            CreateKind::Template { source }
            | CreateKind::Duplicate { source }
            | CreateKind::HardLink { source } => Some(source),
            CreateKind::SymbolicLink { target } => Some(target),
            CreateKind::Directory | CreateKind::EmptyFile => None,
        }
    }
}

fn validate_destination(destination: &Path) -> Result<(), CreateRequestError> {
    if destination.file_name().is_none() || destination.parent().is_none() {
        return Err(CreateRequestError::InvalidDestination(
            destination.to_path_buf(),
        ));
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum CreateRequestError {
    #[error("creation source has no usable final component: {}", .0.display())]
    InvalidSource(PathBuf),
    #[error("creation destination has no usable final component: {}", .0.display())]
    InvalidDestination(PathBuf),
}

#[derive(Clone, Debug, Default)]
pub struct CreateCancellation(CopyCancellation);

impl CreateCancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreateProgress {
    Item { completed: u64, total: u64 },
    Copy(CopyProgress),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateOutcome {
    destination: PathBuf,
}

impl CreateOutcome {
    pub fn destination(&self) -> &Path {
        &self.destination
    }
}

#[derive(Debug, Error)]
pub enum CreateError {
    #[error("creation was cancelled before commit")]
    Cancelled,
    #[error("creation destination parent is unavailable: {}", .0.display())]
    InvalidDestinationParent(PathBuf),
    #[error("creation destination already exists: {}", .0.display())]
    DestinationExists(PathBuf),
    #[error("hard-link source is not a regular non-symbolic file: {}", .0.display())]
    UnsupportedHardLinkSource(PathBuf),
    #[error("template source is not a regular non-symbolic file: {}", .0.display())]
    UnsupportedTemplateSource(PathBuf),
    #[error("template output changed before permissions could be secured: {}", .0.display())]
    TemplateOutputChanged(PathBuf),
    #[error("hard links require source and destination on the same filesystem")]
    CrossFilesystemHardLink,
    #[error(transparent)]
    Copy(#[from] CopyError),
    #[error("could not {action} at {}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl CreateError {
    pub fn io_kind(&self) -> Option<io::ErrorKind> {
        match self {
            Self::Io { source, .. } => Some(source.kind()),
            Self::Copy(error) => error.io_kind(),
            _ => None,
        }
    }

    pub const fn is_conflict(&self) -> bool {
        match self {
            Self::DestinationExists(_) => true,
            Self::Copy(error) => error.is_conflict(),
            _ => false,
        }
    }

    pub const fn is_unsupported(&self) -> bool {
        match self {
            Self::UnsupportedHardLinkSource(_)
            | Self::UnsupportedTemplateSource(_)
            | Self::CrossFilesystemHardLink => true,
            Self::Copy(error) => error.is_unsupported(),
            _ => false,
        }
    }
}

pub fn execute_create<F>(
    request: &CreateRequest,
    cancellation: &CreateCancellation,
    mut report_progress: F,
) -> Result<CreateOutcome, CreateError>
where
    F: FnMut(CreateProgress),
{
    if cancellation.is_cancelled() {
        return Err(CreateError::Cancelled);
    }
    validate_parent(request.destination())?;

    match request.kind() {
        CreateKind::Directory => {
            fs::create_dir(request.destination()).map_err(|error| {
                map_destination_error("create directory", request.destination(), error)
            })?;
            report_progress(CreateProgress::Item {
                completed: 1,
                total: 1,
            });
        }
        CreateKind::EmptyFile => {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(request.destination())
                .map_err(|error| {
                    map_destination_error("create empty file", request.destination(), error)
                })?;
            report_progress(CreateProgress::Item {
                completed: 1,
                total: 1,
            });
        }
        CreateKind::Template { source } => {
            let source_metadata =
                fs::symlink_metadata(source).map_err(|error| CreateError::Io {
                    action: "inspect template source",
                    path: source.clone(),
                    source: error,
                })?;
            if !source_metadata.file_type().is_file() || source_metadata.file_type().is_symlink() {
                return Err(CreateError::UnsupportedTemplateSource(source.clone()));
            }
            execute_copy(
                &CopyRequest::new(
                    source,
                    request.destination(),
                    ConflictPolicy::FailIfExists,
                    SymlinkPolicy::Preserve,
                ),
                &cancellation.0,
                |progress| report_progress(CreateProgress::Copy(progress)),
            )?;
            secure_template_output(request.destination())?;
        }
        CreateKind::Duplicate { source } => {
            execute_copy(
                &CopyRequest::new(
                    source,
                    request.destination(),
                    ConflictPolicy::FailIfExists,
                    SymlinkPolicy::Preserve,
                ),
                &cancellation.0,
                |progress| report_progress(CreateProgress::Copy(progress)),
            )?;
        }
        CreateKind::SymbolicLink { target } => {
            unix_fs::symlink(target, request.destination()).map_err(|error| {
                map_destination_error("create symbolic link", request.destination(), error)
            })?;
            report_progress(CreateProgress::Item {
                completed: 1,
                total: 1,
            });
        }
        CreateKind::HardLink { source } => {
            let metadata = fs::symlink_metadata(source).map_err(|error| CreateError::Io {
                action: "inspect hard-link source",
                path: source.clone(),
                source: error,
            })?;
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err(CreateError::UnsupportedHardLinkSource(source.clone()));
            }
            fs::hard_link(source, request.destination()).map_err(|error| {
                if error.raw_os_error() == Some(Errno::XDEV.raw_os_error()) {
                    CreateError::CrossFilesystemHardLink
                } else {
                    map_destination_error("create hard link", request.destination(), error)
                }
            })?;
            report_progress(CreateProgress::Item {
                completed: 1,
                total: 1,
            });
        }
    }

    Ok(CreateOutcome {
        destination: request.destination().to_path_buf(),
    })
}

fn secure_template_output(destination: &Path) -> Result<(), CreateError> {
    let before = fs::symlink_metadata(destination).map_err(|error| CreateError::Io {
        action: "inspect created template",
        path: destination.to_path_buf(),
        source: error,
    })?;
    if !before.file_type().is_file() || before.file_type().is_symlink() {
        return Err(CreateError::TemplateOutputChanged(
            destination.to_path_buf(),
        ));
    }

    let descriptor = rustix::fs::open(
        destination,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::CLOEXEC | rustix::fs::OFlags::NOFOLLOW,
        rustix::fs::Mode::empty(),
    )
    .map_err(|error| CreateError::Io {
        action: "open created template safely",
        path: destination.to_path_buf(),
        source: io::Error::from_raw_os_error(error.raw_os_error()),
    })?;
    let opened = fs::File::from(descriptor);
    let current = opened.metadata().map_err(|error| CreateError::Io {
        action: "revalidate created template",
        path: destination.to_path_buf(),
        source: error,
    })?;
    if (before.dev(), before.ino()) != (current.dev(), current.ino()) || !current.is_file() {
        return Err(CreateError::TemplateOutputChanged(
            destination.to_path_buf(),
        ));
    }

    let safe_mode = current.permissions().mode() & !0o111;
    let descriptor_path = PathBuf::from(format!(
        "/proc/self/fd/{}",
        std::os::fd::AsRawFd::as_raw_fd(&opened)
    ));
    rustix::fs::chmod(&descriptor_path, rustix::fs::Mode::from_raw_mode(safe_mode)).map_err(
        |error| CreateError::Io {
            action: "remove execute permissions from created template",
            path: destination.to_path_buf(),
            source: io::Error::from_raw_os_error(error.raw_os_error()),
        },
    )?;
    Ok(())
}

fn validate_parent(destination: &Path) -> Result<(), CreateError> {
    let Some(parent) = destination.parent() else {
        return Err(CreateError::InvalidDestinationParent(
            destination.to_path_buf(),
        ));
    };
    let metadata = fs::metadata(parent).map_err(|error| CreateError::Io {
        action: "inspect creation destination parent",
        path: parent.to_path_buf(),
        source: error,
    })?;
    if !metadata.is_dir() {
        return Err(CreateError::InvalidDestinationParent(parent.to_path_buf()));
    }
    Ok(())
}

fn map_destination_error(action: &'static str, path: &Path, error: io::Error) -> CreateError {
    if error.kind() == io::ErrorKind::AlreadyExists {
        CreateError::DestinationExists(path.to_path_buf())
    } else {
        CreateError::Io {
            action,
            path: path.to_path_buf(),
            source: error,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        fs,
        os::unix::{ffi::OsStringExt, fs::MetadataExt},
    };

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn phase_6q_create_directory_empty_file_template_and_collision_are_safe() {
        let fixture = tempdir().expect("temporary fixture");
        let raw_name = OsString::from_vec(b"folder-\xff".to_vec());
        let directory = fixture.path().join(raw_name);
        execute_create(
            &CreateRequest::directory(&directory).expect("directory request"),
            &CreateCancellation::new(),
            |_| {},
        )
        .expect("directory creation");
        assert!(directory.is_dir());

        let empty = directory.join("empty");
        execute_create(
            &CreateRequest::empty_file(&empty).expect("empty request"),
            &CreateCancellation::new(),
            |_| {},
        )
        .expect("empty-file creation");
        assert_eq!(fs::metadata(&empty).expect("empty metadata").len(), 0);
        assert!(matches!(
            execute_create(
                &CreateRequest::empty_file(&empty).expect("collision request"),
                &CreateCancellation::new(),
                |_| {}
            ),
            Err(CreateError::DestinationExists(path)) if path == empty
        ));

        let template = fixture.path().join("template.txt");
        let created = fixture.path().join("created.txt");
        fs::write(&template, b"template payload").expect("template payload");
        execute_create(
            &CreateRequest::template(&template, &created).expect("template request"),
            &CreateCancellation::new(),
            |_| {},
        )
        .expect("template creation");
        assert_eq!(
            fs::read(created).expect("created payload"),
            b"template payload"
        );
    }

    #[test]
    fn phase_12d_template_create_strips_execute_bits_and_rejects_links() {
        let fixture = tempdir().expect("temporary fixture");
        let source = fixture.path().join("executable-template");
        fs::write(&source, b"#!/bin/false\n").expect("template payload");
        fs::set_permissions(&source, fs::Permissions::from_mode(0o775))
            .expect("executable template permissions");

        let destination = fixture.path().join("safe-copy");
        execute_create(
            &CreateRequest::template(&source, &destination).expect("template request"),
            &CreateCancellation::new(),
            |_| {},
        )
        .expect("safe template creation");

        let source_mode = fs::metadata(&source).expect("source metadata").mode() & 0o777;
        let destination_mode = fs::metadata(&destination)
            .expect("destination metadata")
            .mode()
            & 0o777;
        assert_eq!(source_mode, 0o775, "source template must not be modified");
        assert_eq!(destination_mode & 0o111, 0);
        assert_eq!(
            fs::read(&destination).expect("destination payload"),
            b"#!/bin/false\n"
        );

        let link = fixture.path().join("template-link");
        unix_fs::symlink(&source, &link).expect("template symlink");
        let linked_destination = fixture.path().join("linked-copy");
        assert!(matches!(
            execute_create(
                &CreateRequest::template(&link, &linked_destination).expect("linked request"),
                &CreateCancellation::new(),
                |_| {},
            ),
            Err(CreateError::UnsupportedTemplateSource(path)) if path == link
        ));
        assert!(!linked_destination.exists());
    }

    #[test]
    fn phase_6q_create_pre_cancelled_request_commits_nothing() {
        let fixture = tempdir().expect("temporary fixture");
        let destination = fixture.path().join("cancelled");
        let cancellation = CreateCancellation::new();
        cancellation.cancel();
        assert!(matches!(
            execute_create(
                &CreateRequest::empty_file(&destination).expect("request"),
                &cancellation,
                |_| {}
            ),
            Err(CreateError::Cancelled)
        ));
        assert!(!destination.exists());
    }

    #[test]
    fn phase_6q_links_preserve_broken_raw_target_and_hard_link_identity() {
        let fixture = tempdir().expect("temporary fixture");
        let raw_target = PathBuf::from(OsString::from_vec(b"missing-\xfe".to_vec()));
        let symbolic = fixture
            .path()
            .join(OsString::from_vec(b"link-\xff".to_vec()));
        execute_create(
            &CreateRequest::symbolic_link(&raw_target, &symbolic).expect("symbolic request"),
            &CreateCancellation::new(),
            |_| {},
        )
        .expect("broken symbolic link should be intentional");
        assert_eq!(fs::read_link(&symbolic).expect("stored target"), raw_target);
        assert!(fs::metadata(&symbolic).is_err());

        let source = fixture.path().join("source");
        let hard = fixture.path().join("hard");
        fs::write(&source, b"payload").expect("hard-link source");
        execute_create(
            &CreateRequest::hard_link(&source, &hard).expect("hard-link request"),
            &CreateCancellation::new(),
            |_| {},
        )
        .expect("hard link");
        let source_meta = fs::metadata(&source).expect("source metadata");
        let hard_meta = fs::metadata(&hard).expect("hard metadata");
        assert_eq!(
            (source_meta.dev(), source_meta.ino()),
            (hard_meta.dev(), hard_meta.ino())
        );
    }

    #[test]
    fn phase_6q_links_reject_hard_linking_a_symlink_and_never_overwrite() {
        let fixture = tempdir().expect("temporary fixture");
        let source = fixture.path().join("source");
        let source_link = fixture.path().join("source-link");
        let destination = fixture.path().join("destination");
        fs::write(&source, b"payload").expect("source");
        unix_fs::symlink(&source, &source_link).expect("source symlink");
        assert!(matches!(
            execute_create(
                &CreateRequest::hard_link(&source_link, &destination).expect("request"),
                &CreateCancellation::new(),
                |_| {}
            ),
            Err(CreateError::UnsupportedHardLinkSource(path)) if path == source_link
        ));
        fs::write(&destination, b"existing").expect("existing destination");
        assert!(matches!(
            execute_create(
                &CreateRequest::symbolic_link("target", &destination).expect("request"),
                &CreateCancellation::new(),
                |_| {}
            ),
            Err(CreateError::DestinationExists(path)) if path == destination
        ));
        assert_eq!(
            fs::read(destination).expect("existing survives"),
            b"existing"
        );
    }
}
