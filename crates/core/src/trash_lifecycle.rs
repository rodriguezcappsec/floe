use std::{
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::{self, Read},
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    path::{Component, Path, PathBuf},
};

use rustix::fs::{Mode, OFlags};
use thiserror::Error;

use crate::{
    ConflictPolicy, DirectoryEntry, DirectoryError, MoveCancellation, MoveError, MoveRequest,
    TrashMetadata, enumerate_directory_with_cancel, execute_move,
};

const MAX_TRASHINFO_BYTES: u64 = 64 * 1024;
const TRASHINFO_HEADER: &[u8] = b"[Trash Info]";

/// One supported local freedesktop Trash root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrashRoot {
    base: PathBuf,
    files: PathBuf,
    info: PathBuf,
    relative_original_base: Option<PathBuf>,
    guarded_ancestor: Option<PathBuf>,
}

impl TrashRoot {
    /// Build the user's home Trash below an XDG data directory.
    pub fn for_data_home(data_home: impl Into<PathBuf>) -> Self {
        let base = data_home.into().join("Trash");
        Self::new(base, None)
    }

    /// Build a root whose relative `Path=` values resolve below `original_base`.
    pub fn new(base: impl Into<PathBuf>, original_base: Option<PathBuf>) -> Self {
        let base = base.into();
        Self {
            files: base.join("files"),
            info: base.join("info"),
            base,
            relative_original_base: original_base,
            guarded_ancestor: None,
        }
    }

    /// Candidate freedesktop Trash roots for one mounted filesystem.
    pub fn for_mount_top(top: &Path, uid: u32) -> [Self; 2] {
        let shared = top.join(".Trash");
        let mut shared_user = Self::new(shared.join(uid.to_string()), Some(top.to_path_buf()));
        shared_user.guarded_ancestor = Some(shared);
        let private = Self::new(top.join(format!(".Trash-{uid}")), Some(top.to_path_buf()));
        [shared_user, private]
    }

    pub fn base(&self) -> &Path {
        &self.base
    }

    pub fn files(&self) -> &Path {
        &self.files
    }

    pub fn info(&self) -> &Path {
        &self.info
    }
}

#[derive(Debug, Error)]
pub enum TrashEnumerateError {
    #[error(transparent)]
    Directory(#[from] DirectoryError),
    #[error("Trash {kind} path is not a real directory: {path}")]
    UnsafeRoot { kind: &'static str, path: PathBuf },
    #[error("could not inspect Trash {kind} path {path}: {source}")]
    Inspect {
        kind: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Enumerate a local Trash root without following a symlinked `files` or `info`
/// directory. Missing roots represent an empty Trash.
pub fn enumerate_trash_with_cancel(
    root: &TrashRoot,
    is_cancelled: impl Fn() -> bool,
) -> Result<Vec<DirectoryEntry>, TrashEnumerateError> {
    if let Some(ancestor) = root.guarded_ancestor.as_deref()
        && !validate_shared_root(ancestor)?
    {
        return Ok(Vec::new());
    }
    if !validate_root_directory(root.base(), "root")? {
        return Ok(Vec::new());
    }
    if !validate_root_directory(root.files(), "files")? {
        return Ok(Vec::new());
    }

    let info_available = validate_root_directory(root.info(), "info")?;
    let listing = enumerate_directory_with_cancel(root.files(), &is_cancelled)?;
    let mut entries = Vec::with_capacity(listing.entries().len());

    for entry in listing.into_entries() {
        if is_cancelled() {
            return Err(TrashEnumerateError::Directory(DirectoryError::Cancelled));
        }

        let info_path = info_available.then(|| trash_info_path(root.info(), entry.display_name()));
        let parsed = info_path
            .as_deref()
            .and_then(|path| parse_trash_info(path, root).ok());
        let metadata = TrashMetadata::new(
            parsed.as_ref().and_then(|info| info.original_path.clone()),
            parsed.and_then(|info| info.deletion_date),
            info_path.filter(|path| {
                fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
            }),
        );
        entries.push(entry.with_trash_metadata(metadata));
    }

    Ok(entries)
}

fn validate_shared_root(path: &Path) -> Result<bool, TrashEnumerateError> {
    if !validate_root_directory(path, "shared root")? {
        return Ok(false);
    }
    let metadata = fs::symlink_metadata(path).map_err(|source| TrashEnumerateError::Inspect {
        kind: "shared root",
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.permissions().mode() & 0o1000 == 0 {
        return Err(TrashEnumerateError::UnsafeRoot {
            kind: "shared root without sticky bit",
            path: path.to_path_buf(),
        });
    }
    Ok(true)
}

fn validate_root_directory(path: &Path, kind: &'static str) -> Result<bool, TrashEnumerateError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(true),
        Ok(_) => Err(TrashEnumerateError::UnsafeRoot {
            kind,
            path: path.to_path_buf(),
        }),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(TrashEnumerateError::Inspect {
            kind,
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn trash_info_path(info_directory: &Path, name: &OsStr) -> PathBuf {
    let mut filename = name.to_os_string();
    filename.push(".trashinfo");
    info_directory.join(filename)
}

#[derive(Debug)]
struct ParsedTrashInfo {
    original_path: Option<PathBuf>,
    deletion_date: Option<String>,
}

fn parse_trash_info(path: &Path, root: &TrashRoot) -> io::Result<ParsedTrashInfo> {
    let descriptor = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(io::Error::from)?;
    let file = File::from(descriptor);
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Trash metadata is not a regular file",
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_TRASHINFO_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_TRASHINFO_BYTES || bytes.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Trash metadata exceeds the safe limit or contains NUL",
        ));
    }

    let mut lines = bytes.split(|byte| *byte == b'\n');
    let header = lines
        .next()
        .map(trim_ascii)
        .filter(|line| *line == TRASHINFO_HEADER)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Trash Info header"))?;
    let _ = header;

    let mut original_path = None;
    let mut deletion_date = None;
    for line in lines {
        let line = trim_ascii(line);
        if let Some(value) = line.strip_prefix(b"Path=") {
            original_path = decode_original_path(value, root);
        } else if let Some(value) = line.strip_prefix(b"DeletionDate=") {
            deletion_date = validate_deletion_date(value);
        }
    }

    Ok(ParsedTrashInfo {
        original_path,
        deletion_date,
    })
}

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while bytes
        .last()
        .is_some_and(|byte| matches!(byte, b'\r' | b' ' | b'\t'))
    {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn decode_original_path(encoded: &[u8], root: &TrashRoot) -> Option<PathBuf> {
    let bytes = percent_decode(encoded)?;
    let decoded = PathBuf::from(OsString::from_vec(bytes));
    let path = if decoded.is_absolute() {
        decoded
    } else {
        root.relative_original_base.as_ref()?.join(decoded)
    };
    is_normalized_absolute(&path).then_some(path)
}

fn percent_decode(encoded: &[u8]) -> Option<Vec<u8>> {
    let mut decoded = Vec::with_capacity(encoded.len());
    let mut index = 0;
    while index < encoded.len() {
        if encoded[index] == b'%' {
            let high = *encoded.get(index + 1)?;
            let low = *encoded.get(index + 2)?;
            let byte = hex(high)?.checked_mul(16)?.checked_add(hex(low)?)?;
            if byte == 0 {
                return None;
            }
            decoded.push(byte);
            index += 3;
        } else {
            decoded.push(encoded[index]);
            index += 1;
        }
    }
    Some(decoded)
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn validate_deletion_date(bytes: &[u8]) -> Option<String> {
    let valid = bytes.len() == 19
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            10 => *byte == b'T',
            13 | 16 => *byte == b':',
            _ => byte.is_ascii_digit(),
        });
    valid.then(|| String::from_utf8(bytes.to_vec()).expect("validated ASCII date"))
}

fn is_normalized_absolute(path: &Path) -> bool {
    path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::RootDir | Component::Normal(_)))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreRequest {
    backing_path: PathBuf,
    info_path: PathBuf,
    destination: PathBuf,
}

impl RestoreRequest {
    pub fn new(
        backing_path: impl Into<PathBuf>,
        info_path: impl Into<PathBuf>,
        destination: impl Into<PathBuf>,
    ) -> Result<Self, RestoreRequestError> {
        let request = Self {
            backing_path: backing_path.into(),
            info_path: info_path.into(),
            destination: destination.into(),
        };
        if request.backing_path.file_name().is_none() {
            return Err(RestoreRequestError::InvalidBacking(request.backing_path));
        }
        if request.info_path.file_name().is_none() {
            return Err(RestoreRequestError::InvalidInfo(request.info_path));
        }
        if !is_normalized_absolute(&request.backing_path)
            || !is_normalized_absolute(&request.info_path)
            || !matching_trash_paths(&request.backing_path, &request.info_path)
        {
            return Err(RestoreRequestError::MismatchedMetadata {
                backing: request.backing_path,
                info: request.info_path,
            });
        }
        if !is_normalized_absolute(&request.destination)
            || request.destination.file_name().is_none()
        {
            return Err(RestoreRequestError::InvalidDestination(request.destination));
        }
        Ok(request)
    }

    pub fn from_entry(entry: &DirectoryEntry) -> Result<Self, RestoreRequestError> {
        let metadata = entry
            .trash_metadata()
            .ok_or_else(|| RestoreRequestError::MetadataUnavailable(entry.path().to_path_buf()))?;
        let info = metadata
            .info_path()
            .ok_or_else(|| RestoreRequestError::MetadataUnavailable(entry.path().to_path_buf()))?;
        let destination = metadata
            .original_path()
            .ok_or_else(|| RestoreRequestError::MetadataUnavailable(entry.path().to_path_buf()))?;
        Self::new(entry.path(), info, destination)
    }

    pub fn with_destination(
        &self,
        destination: impl Into<PathBuf>,
    ) -> Result<Self, RestoreRequestError> {
        Self::new(&self.backing_path, &self.info_path, destination)
    }

    pub fn backing_path(&self) -> &Path {
        &self.backing_path
    }

    pub fn info_path(&self) -> &Path {
        &self.info_path
    }

    pub fn destination(&self) -> &Path {
        &self.destination
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum RestoreRequestError {
    #[error("invalid Trash backing path: {0:?}")]
    InvalidBacking(PathBuf),
    #[error("invalid Trash metadata path: {0:?}")]
    InvalidInfo(PathBuf),
    #[error("Trash payload and metadata paths do not form a matching entry: {backing:?}, {info:?}")]
    MismatchedMetadata { backing: PathBuf, info: PathBuf },
    #[error("invalid restore destination: {0:?}")]
    InvalidDestination(PathBuf),
    #[error("restore metadata is unavailable for Trash item: {0:?}")]
    MetadataUnavailable(PathBuf),
}

fn matching_trash_paths(backing: &Path, info: &Path) -> bool {
    let (Some(backing_parent), Some(info_parent), Some(backing_name), Some(info_name)) = (
        backing.parent(),
        info.parent(),
        backing.file_name(),
        info.file_name(),
    ) else {
        return false;
    };
    let mut expected_info_name = backing_name.to_os_string();
    expected_info_name.push(".trashinfo");
    backing_parent.file_name() == Some(OsStr::new("files"))
        && info_parent.file_name() == Some(OsStr::new("info"))
        && backing_parent.parent() == info_parent.parent()
        && info_name == expected_info_name
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RestoreOutcome;

#[derive(Debug, Error)]
pub enum RestoreError {
    #[error(transparent)]
    Move(#[from] MoveError),
    #[error("Trash metadata is missing: {0:?}")]
    MetadataMissing(PathBuf),
    #[error("could not inspect Trash metadata {path}: {source}")]
    MetadataInspect {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("item was restored, but Trash metadata cleanup failed for {path}: {source}")]
    MetadataCleanup {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl RestoreError {
    pub fn is_conflict(&self) -> bool {
        matches!(self, Self::Move(error) if error.is_conflict())
    }

    pub const fn is_partial(&self) -> bool {
        matches!(self, Self::MetadataCleanup { .. })
    }

    pub fn io_kind(&self) -> Option<io::ErrorKind> {
        match self {
            Self::Move(error) => error.io_kind(),
            Self::MetadataInspect { source, .. } | Self::MetadataCleanup { source, .. } => {
                Some(source.kind())
            }
            Self::MetadataMissing(_) => Some(io::ErrorKind::NotFound),
        }
    }
}

pub fn execute_restore(
    request: &RestoreRequest,
    cancellation: &MoveCancellation,
) -> Result<RestoreOutcome, RestoreError> {
    match fs::symlink_metadata(request.info_path()) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => return Err(RestoreError::MetadataMissing(request.info_path.clone())),
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return Err(RestoreError::MetadataMissing(request.info_path.clone()));
        }
        Err(source) => {
            return Err(RestoreError::MetadataInspect {
                path: request.info_path.clone(),
                source,
            });
        }
    }

    execute_move(
        &MoveRequest::new(
            request.backing_path(),
            request.destination(),
            ConflictPolicy::FailIfExists,
        ),
        cancellation,
    )?;

    fs::remove_file(request.info_path()).map_err(|source| RestoreError::MetadataCleanup {
        path: request.info_path.clone(),
        source,
    })?;
    Ok(RestoreOutcome)
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::ffi::OsStrExt};

    use tempfile::tempdir;

    use super::*;

    fn trash_fixture() -> (tempfile::TempDir, TrashRoot) {
        let fixture = tempdir().expect("temporary Trash fixture");
        let root = TrashRoot::new(fixture.path().join("Trash"), None);
        fs::create_dir_all(root.files()).expect("files directory");
        fs::create_dir_all(root.info()).expect("info directory");
        (fixture, root)
    }

    #[test]
    fn phase_6n_trash_metadata_preserves_encoded_raw_paths_and_dates() {
        let (_fixture, root) = trash_fixture();
        let payload = root.files().join("report");
        fs::write(&payload, b"content").expect("payload");
        fs::write(
            root.info().join("report.trashinfo"),
            b"[Trash Info]\nPath=/tmp/raw-%FF-name\nDeletionDate=2026-08-24T12:34:56\n",
        )
        .expect("metadata");

        let entries = enumerate_trash_with_cancel(&root, || false).expect("enumeration");
        let metadata = entries[0].trash_metadata().expect("Trash metadata");
        assert_eq!(
            metadata
                .original_path()
                .expect("original")
                .as_os_str()
                .as_bytes(),
            b"/tmp/raw-\xff-name"
        );
        assert_eq!(metadata.deletion_date(), Some("2026-08-24T12:34:56"));
        assert_eq!(
            metadata.info_path(),
            Some(root.info().join("report.trashinfo").as_path())
        );
    }

    #[test]
    fn phase_6n_trash_metadata_keeps_orphan_and_malformed_payloads_visible() {
        let (_fixture, root) = trash_fixture();
        fs::write(root.files().join("orphan"), b"one").expect("orphan");
        fs::write(root.files().join("malformed"), b"two").expect("malformed");
        fs::write(root.info().join("malformed.trashinfo"), b"not valid").expect("metadata");

        let entries = enumerate_trash_with_cancel(&root, || false).expect("enumeration");
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|entry| entry.trash_metadata().is_some()));
        assert!(entries.iter().all(|entry| {
            entry
                .trash_metadata()
                .expect("metadata marker")
                .original_path()
                .is_none()
        }));
    }

    #[test]
    fn phase_6n_trash_metadata_rejects_symlinked_roots() {
        let fixture = tempdir().expect("temporary fixture");
        let target = fixture.path().join("target");
        fs::create_dir(&target).expect("target");
        let root = TrashRoot::new(fixture.path().join("Trash"), None);
        fs::create_dir_all(root.base()).expect("base");
        std::os::unix::fs::symlink(&target, root.files()).expect("symlink");
        assert!(matches!(
            enumerate_trash_with_cancel(&root, || false),
            Err(TrashEnumerateError::UnsafeRoot { kind: "files", .. })
        ));
    }

    #[test]
    fn phase_6n_trash_metadata_resolves_mounted_relative_paths_below_mount() {
        let fixture = tempdir().expect("temporary mount fixture");
        let mount = fixture.path().join("mount");
        fs::create_dir(&mount).expect("mount directory");
        let [_, root] = TrashRoot::for_mount_top(&mount, 1000);
        fs::create_dir_all(root.files()).expect("files directory");
        fs::create_dir_all(root.info()).expect("info directory");
        fs::write(root.files().join("item"), b"payload").expect("payload");
        fs::write(
            root.info().join("item.trashinfo"),
            b"[Trash Info]\nPath=Documents/item\nDeletionDate=2026-08-24T12:00:00\n",
        )
        .expect("metadata");

        let entries = enumerate_trash_with_cancel(&root, || false).expect("enumeration");
        assert_eq!(
            entries[0]
                .trash_metadata()
                .and_then(TrashMetadata::original_path),
            Some(mount.join("Documents/item").as_path())
        );
    }

    #[test]
    fn phase_6n_restore_moves_without_overwrite_then_removes_metadata() {
        let (fixture, root) = trash_fixture();
        let payload = root.files().join("item");
        let info = root.info().join("item.trashinfo");
        let destination = fixture.path().join("restored/item");
        fs::create_dir(destination.parent().expect("parent")).expect("destination parent");
        fs::write(&payload, b"restore me").expect("payload");
        fs::write(&info, b"metadata").expect("metadata");
        let request = RestoreRequest::new(&payload, &info, &destination).expect("request");

        execute_restore(&request, &MoveCancellation::default()).expect("restore");
        assert_eq!(
            fs::read(&destination).expect("restored payload"),
            b"restore me"
        );
        assert!(!payload.exists());
        assert!(!info.exists());
    }

    #[test]
    fn phase_6n_restore_conflict_keeps_payload_metadata_and_destination() {
        let (fixture, root) = trash_fixture();
        let payload = root.files().join("item");
        let info = root.info().join("item.trashinfo");
        let destination = fixture.path().join("restored");
        fs::write(&payload, b"trashed").expect("payload");
        fs::write(&info, b"metadata").expect("metadata");
        fs::write(&destination, b"existing").expect("destination");
        let request = RestoreRequest::new(&payload, &info, &destination).expect("request");

        let error = execute_restore(&request, &MoveCancellation::default())
            .expect_err("restore must not overwrite");
        assert!(error.is_conflict());
        assert_eq!(fs::read(&destination).expect("existing"), b"existing");
        assert!(payload.exists());
        assert!(info.exists());
    }

    #[test]
    fn phase_6n_restore_requires_metadata_before_moving_payload() {
        let (fixture, root) = trash_fixture();
        let payload = root.files().join("item");
        let destination = fixture.path().join("restored");
        fs::write(&payload, b"trashed").expect("payload");
        let request =
            RestoreRequest::new(&payload, root.info().join("item.trashinfo"), &destination)
                .expect("request");

        assert!(matches!(
            execute_restore(&request, &MoveCancellation::default()),
            Err(RestoreError::MetadataMissing(_))
        ));
        assert!(payload.exists());
        assert!(!destination.exists());
    }

    #[test]
    fn phase_6n_restore_rejects_mismatched_metadata_pair() {
        let (fixture, root) = trash_fixture();
        let payload = root.files().join("item");
        let mismatched = root.info().join("other.trashinfo");
        let destination = fixture.path().join("restored");
        assert!(matches!(
            RestoreRequest::new(payload, mismatched, destination),
            Err(RestoreRequestError::MismatchedMetadata { .. })
        ));
    }
}
