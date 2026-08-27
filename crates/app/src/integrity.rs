//! GTK-independent integrity fingerprint and checksum-manifest support.
//!
//! The manifest profile is the strict GNU `sha256sum` text format: each record
//! has a lower-case digest, a two-space text marker, and a relative filename.
//! GNU's leading-backslash record marker and `\\`/`\n` escapes are required for
//! filenames containing a backslash or newline. Other filename bytes, including
//! non-UTF-8 bytes, remain raw. Paths are never reconstructed from display text.

use std::{
    collections::HashSet,
    ffi::OsString,
    fs,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::MetadataExt,
    },
    path::{Component, Path, PathBuf},
};

use floe_core::{CHECKSUM_TARGET_CAPACITY, ChecksumAlgorithm, ChecksumRequest, ExpectedDigest};
use thiserror::Error;

use crate::checksum_executor::{ChecksumError, ChecksumVerification, execute_checksum};

const MAX_MANIFEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 4 * 1024;
const MAX_ENCODED_PATH_BYTES: usize = MAX_PATH_BYTES * 2;
const MAX_DISCOVERED_FILES: usize = CHECKSUM_TARGET_CAPACITY;
const MAX_DISCOVERED_DIRECTORIES: usize = 4_096;
const MAX_DISCOVERY_DEPTH: usize = 32;
const MAX_DISCOVERED_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const FINGERPRINT_RECORD_HEADER: &[u8] = b"Floe fingerprint v1\n";

#[derive(Clone, Debug, Eq, PartialEq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

impl FileIdentity {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
        }
    }
}

/// An in-memory fingerprint bound to one exact absolute local path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedFingerprint {
    path: PathBuf,
    digest: String,
    identity: FileIdentity,
}

impl SavedFingerprint {
    #[allow(dead_code)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[allow(dead_code)]
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Versioned binary-safe record for the app-owned persistence layer.
    /// Filesystem atomicity and permissions remain the caller's responsibility.
    pub fn encode_record(&self) -> Vec<u8> {
        let path = self.path.as_os_str().as_bytes();
        let mut output = Vec::with_capacity(FINGERPRINT_RECORD_HEADER.len() + 128 + path.len());
        output.extend_from_slice(FINGERPRINT_RECORD_HEADER);
        output.extend_from_slice(self.digest.as_bytes());
        output.push(b'\n');
        for value in [
            self.identity.device,
            self.identity.inode,
            self.identity.size,
            self.identity.modified_seconds as u64,
            self.identity.modified_nanoseconds as u64,
            self.identity.changed_seconds as u64,
            self.identity.changed_nanoseconds as u64,
        ] {
            output.extend_from_slice(&value.to_be_bytes());
        }
        output.extend_from_slice(&(u32::try_from(path.len()).unwrap_or(u32::MAX)).to_be_bytes());
        output.extend_from_slice(path);
        output
    }

    pub fn decode_record(input: &[u8]) -> Result<Self, IntegrityError> {
        const FIXED_BYTES: usize = 65 + (7 * 8) + 4;
        if !input.starts_with(FINGERPRINT_RECORD_HEADER)
            || input.len() < FINGERPRINT_RECORD_HEADER.len() + FIXED_BYTES
        {
            return Err(IntegrityError::MalformedFingerprintRecord);
        }
        let body = &input[FINGERPRINT_RECORD_HEADER.len()..];
        let (digest, rest) = body.split_at(64);
        if rest.first() != Some(&b'\n') || !is_lower_hex(digest) {
            return Err(IntegrityError::MalformedFingerprintRecord);
        }
        let digest =
            std::str::from_utf8(digest).map_err(|_| IntegrityError::MalformedFingerprintRecord)?;
        ExpectedDigest::parse(ChecksumAlgorithm::Sha256, digest)
            .map_err(|_| IntegrityError::MalformedFingerprintRecord)?;
        let values = &rest[1..57];
        let path_length = u32::from_be_bytes(
            rest[57..61]
                .try_into()
                .map_err(|_| IntegrityError::MalformedFingerprintRecord)?,
        ) as usize;
        let path = &rest[61..];
        if path_length != path.len() || path_length > MAX_PATH_BYTES || path.contains(&0) {
            return Err(IntegrityError::MalformedFingerprintRecord);
        }
        let read_u64 = |index: usize| -> u64 {
            u64::from_be_bytes(
                values[index..index + 8]
                    .try_into()
                    .expect("fixed record width"),
            )
        };
        let path = PathBuf::from(OsString::from_vec(path.to_vec()));
        validate_absolute_file_path(&path)
            .map_err(|_| IntegrityError::MalformedFingerprintRecord)?;
        Ok(Self {
            path,
            digest: digest.to_owned(),
            identity: FileIdentity {
                device: read_u64(0),
                inode: read_u64(8),
                size: read_u64(16),
                modified_seconds: read_u64(24) as i64,
                modified_nanoseconds: read_u64(32) as i64,
                changed_seconds: read_u64(40) as i64,
                changed_nanoseconds: read_u64(48) as i64,
            },
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FingerprintVerification {
    Match,
    Changed {
        expected: String,
        actual: String,
    },
    /// The exact path now names a different filesystem object or metadata.
    StaleIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestEntry {
    path: PathBuf,
    digest: String,
}

impl ManifestEntry {
    #[allow(dead_code)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[allow(dead_code)]
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sha256SumsManifest {
    entries: Vec<ManifestEntry>,
}

impl Sha256SumsManifest {
    #[allow(dead_code)]
    pub fn entries(&self) -> &[ManifestEntry] {
        &self.entries
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(self.entries.len() * 80);
        for entry in &self.entries {
            let raw_path = entry.path.as_os_str().as_bytes();
            let escaped = raw_path.contains(&b'\\') || raw_path.contains(&b'\n');
            if escaped {
                output.push(b'\\');
            }
            output.extend_from_slice(entry.digest.as_bytes());
            output.extend_from_slice(b"  ");
            if escaped {
                encode_gnu_path(raw_path, &mut output);
            } else {
                output.extend_from_slice(raw_path);
            }
            output.push(b'\n');
        }
        output
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManifestEntryVerification {
    Match,
    Changed { expected: String, actual: String },
    Missing,
    StaleIdentity,
    New,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestVerification {
    pub entries: Vec<(PathBuf, ManifestEntryVerification)>,
}

#[derive(Debug, Error)]
pub enum IntegrityError {
    #[error("integrity operation cancelled")]
    Cancelled,
    #[error("integrity root must be an absolute normalized directory: {0}")]
    InvalidRoot(PathBuf),
    #[error("integrity path must be an absolute normalized child of its root: {0}")]
    InvalidPath(PathBuf),
    #[error("integrity path is a symlink or has a symlink ancestor: {0}")]
    Symlink(PathBuf),
    #[error("too many manifest entries")]
    TooManyEntries,
    #[error("folder discovery exceeds its directory limit")]
    TooManyDirectories,
    #[error("folder discovery exceeds its depth limit")]
    DiscoveryTooDeep,
    #[error("folder discovery exceeds its byte limit")]
    DiscoveryTooLarge,
    #[error("integrity discovery does not cross filesystem devices: {0}")]
    CrossDevice(PathBuf),
    #[error("source changed while its integrity record was created: {0}")]
    SourceChanged(PathBuf),
    #[error("manifest exceeds its bounded input size")]
    ManifestTooLarge,
    #[error("malformed Floe SHA256SUMS manifest")]
    MalformedManifest,
    #[error("malformed versioned fingerprint record")]
    MalformedFingerprintRecord,
    #[error("duplicate manifest path: {0}")]
    DuplicateManifestPath(PathBuf),
    #[error("checksum request invalid: {0}")]
    Request(#[from] floe_core::ChecksumRequestError),
    #[error(transparent)]
    Checksum(#[from] ChecksumError),
    #[error("filesystem access failed at {path}: {message}")]
    Io {
        path: PathBuf,
        message: String,
        not_found: bool,
    },
}

/// Calculates a SHA-256 fingerprint using the reviewed streaming executor.
pub fn save_fingerprint(
    path: PathBuf,
    cancelled: impl Fn() -> bool,
    on_progress: impl FnMut(u64, u64),
) -> Result<SavedFingerprint, IntegrityError> {
    if cancelled() {
        return Err(IntegrityError::Cancelled);
    }
    validate_absolute_file_path(&path)?;
    let identity = no_follow_file_identity(&path)?;
    let request = ChecksumRequest::new(vec![path.clone()], ChecksumAlgorithm::Sha256, None)?;
    let outcome = execute_checksum(&request, cancelled, on_progress).map_err(map_checksum_error)?;
    let item = &outcome.items[0];
    if no_follow_file_identity(&path)? != identity {
        return Err(IntegrityError::SourceChanged(path));
    }
    Ok(SavedFingerprint {
        path,
        digest: item.digest.clone(),
        identity,
    })
}

/// Verifies a fingerprint without following a terminal symlink.
pub fn verify_fingerprint(
    fingerprint: &SavedFingerprint,
    cancelled: impl Fn() -> bool,
    on_progress: impl FnMut(u64, u64),
) -> Result<FingerprintVerification, IntegrityError> {
    if cancelled() {
        return Err(IntegrityError::Cancelled);
    }
    let current = no_follow_file_identity(&fingerprint.path)?;
    if current != fingerprint.identity {
        return Ok(FingerprintVerification::StaleIdentity);
    }
    let expected = ExpectedDigest::parse(ChecksumAlgorithm::Sha256, &fingerprint.digest)?;
    let request = ChecksumRequest::new(
        vec![fingerprint.path.clone()],
        ChecksumAlgorithm::Sha256,
        Some(expected),
    )?;
    let outcome = execute_checksum(&request, cancelled, on_progress).map_err(map_checksum_error)?;
    let item = &outcome.items[0];
    match &item.verification {
        ChecksumVerification::Match => Ok(FingerprintVerification::Match),
        ChecksumVerification::Mismatch { expected } => Ok(FingerprintVerification::Changed {
            expected: expected.clone(),
            actual: item.digest.clone(),
        }),
        ChecksumVerification::NotRequested => unreachable!("expected digest was supplied"),
    }
}

/// Generates the strict Floe SHA256SUMS profile for exact children of `root`.
pub fn generate_sha256sums(
    root: &Path,
    targets: &[PathBuf],
    cancelled: impl Fn() -> bool,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<Sha256SumsManifest, IntegrityError> {
    validate_root(root)?;
    if targets.is_empty() || targets.len() > CHECKSUM_TARGET_CAPACITY {
        return Err(IntegrityError::TooManyEntries);
    }

    let discovered = discover_manifest_targets(root, targets)?;
    let mut entries = Vec::with_capacity(discovered.len());
    for (target, relative) in discovered {
        if cancelled() {
            return Err(IntegrityError::Cancelled);
        }
        let request = ChecksumRequest::new(vec![target], ChecksumAlgorithm::Sha256, None)?;
        let outcome = execute_checksum(&request, &cancelled, |completed, total| {
            on_progress(completed, total)
        })
        .map_err(map_checksum_error)?;
        entries.push(ManifestEntry {
            path: relative,
            digest: outcome.items[0].digest.clone(),
        });
    }
    Ok(Sha256SumsManifest { entries })
}

/// Parses only a strict, portable GNU SHA256SUMS text profile.
pub fn parse_sha256sums(input: &[u8]) -> Result<Sha256SumsManifest, IntegrityError> {
    if input.len() > MAX_MANIFEST_BYTES {
        return Err(IntegrityError::ManifestTooLarge);
    }
    if input.is_empty() {
        return Ok(Sha256SumsManifest {
            entries: Vec::new(),
        });
    }

    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    let mut body = input;
    while !body.is_empty() {
        let Some(newline) = body.iter().position(|byte| *byte == b'\n') else {
            return Err(IntegrityError::MalformedManifest);
        };
        let line = &body[..newline];
        body = &body[newline + 1..];
        let (digest, filename, escaped) = if line.first() == Some(&b'\\') {
            if line.len() <= 67 || line[65..67] != *b"  " || !is_lower_hex(&line[1..65]) {
                return Err(IntegrityError::MalformedManifest);
            }
            (&line[1..65], &line[67..], true)
        } else {
            if line.len() <= 66 || line[64..66] != *b"  " || !is_lower_hex(&line[..64]) {
                return Err(IntegrityError::MalformedManifest);
            }
            (&line[..64], &line[66..], false)
        };
        if filename.is_empty() || filename.len() > MAX_ENCODED_PATH_BYTES {
            return Err(IntegrityError::MalformedManifest);
        }
        let raw = if escaped {
            decode_gnu_path(filename)?
        } else {
            if filename.contains(&b'\\') || filename.contains(&0) {
                return Err(IntegrityError::MalformedManifest);
            }
            filename.to_vec()
        };
        if raw.len() > MAX_PATH_BYTES || raw.contains(&0) {
            return Err(IntegrityError::MalformedManifest);
        }
        let path = PathBuf::from(OsString::from_vec(raw));
        validate_relative_path(&path).map_err(|_| IntegrityError::MalformedManifest)?;
        if !seen.insert(path.clone()) {
            return Err(IntegrityError::DuplicateManifestPath(path));
        }
        if entries.len() == CHECKSUM_TARGET_CAPACITY {
            return Err(IntegrityError::TooManyEntries);
        }
        let digest =
            String::from_utf8(digest.to_vec()).map_err(|_| IntegrityError::MalformedManifest)?;
        entries.push(ManifestEntry { path, digest });
    }
    Ok(Sha256SumsManifest { entries })
}

/// Verifies every manifest member below `root`; names absent from the manifest
/// are deliberately not scanned or treated as an integrity finding.
pub fn verify_sha256sums(
    root: &Path,
    manifest: &Sha256SumsManifest,
    cancelled: impl Fn() -> bool,
    on_progress: impl FnMut(u64, u64),
) -> Result<ManifestVerification, IntegrityError> {
    verify_sha256sums_excluding(root, manifest, None, cancelled, on_progress)
}

/// Verifies a manifest and reports ordinary files under `root` that are not
/// listed. `excluded` is explicit so callers can omit the manifest file itself
/// without relying on a magic filename.
pub fn verify_sha256sums_excluding(
    root: &Path,
    manifest: &Sha256SumsManifest,
    excluded: Option<&Path>,
    cancelled: impl Fn() -> bool,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<ManifestVerification, IntegrityError> {
    validate_root(root)?;
    let excluded = excluded
        .map(|path| path.strip_prefix(root).map(Path::to_path_buf))
        .transpose()
        .map_err(|_| IntegrityError::InvalidPath(root.to_path_buf()))?;
    if let Some(path) = &excluded {
        validate_relative_path(path)?;
    }
    let mut results = Vec::with_capacity(manifest.entries.len());
    for entry in &manifest.entries {
        if cancelled() {
            return Err(IntegrityError::Cancelled);
        }
        validate_relative_path(&entry.path)?;
        let target = root.join(&entry.path);
        if let Err(error) = ensure_no_symlink_path(root, &entry.path) {
            results.push((
                entry.path.clone(),
                match error {
                    IntegrityError::Io {
                        not_found: true, ..
                    } => ManifestEntryVerification::Missing,
                    _ => return Err(error),
                },
            ));
            continue;
        }
        let expected = ExpectedDigest::parse(ChecksumAlgorithm::Sha256, &entry.digest)?;
        let request = ChecksumRequest::new(
            vec![target.clone()],
            ChecksumAlgorithm::Sha256,
            Some(expected),
        )?;
        match execute_checksum(&request, &cancelled, |completed, total| {
            on_progress(completed, total)
        }) {
            Ok(outcome) => match &outcome.items[0].verification {
                ChecksumVerification::Match => {
                    results.push((entry.path.clone(), ManifestEntryVerification::Match))
                }
                ChecksumVerification::Mismatch { expected } => results.push((
                    entry.path.clone(),
                    ManifestEntryVerification::Changed {
                        expected: expected.clone(),
                        actual: outcome.items[0].digest.clone(),
                    },
                )),
                ChecksumVerification::NotRequested => unreachable!("expected digest was supplied"),
            },
            Err(ChecksumError::SourceChanged(_)) => {
                results.push((entry.path.clone(), ManifestEntryVerification::StaleIdentity));
            }
            Err(error @ ChecksumError::Io { .. }) => {
                if is_missing(&target) {
                    results.push((entry.path.clone(), ManifestEntryVerification::Missing));
                } else {
                    return Err(map_checksum_error(error));
                }
            }
            Err(error) => return Err(map_checksum_error(error)),
        }
    }
    let listed = manifest
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<HashSet<_>>();
    for path in discover_unlisted_regular_files(root, excluded.as_deref())? {
        if !listed.contains(&path) {
            results.push((path, ManifestEntryVerification::New));
        }
    }
    Ok(ManifestVerification { entries: results })
}

fn discover_manifest_targets(
    root: &Path,
    targets: &[PathBuf],
) -> Result<Vec<(PathBuf, PathBuf)>, IntegrityError> {
    let root_metadata =
        fs::symlink_metadata(root).map_err(|error| io_error(root.to_path_buf(), error))?;
    let root_device = root_metadata.dev();
    let mut discovered = Vec::new();
    let mut directories = 0usize;
    for target in targets {
        if !target.is_absolute() || !is_lexically_normal(target) {
            return Err(IntegrityError::InvalidPath(target.clone()));
        }
        let relative = target
            .strip_prefix(root)
            .map_err(|_| IntegrityError::InvalidPath(target.clone()))?
            .to_path_buf();
        validate_relative_path(&relative)?;
        discover_path(
            root,
            target,
            relative,
            root_device,
            0,
            &mut directories,
            &mut discovered,
        )?;
    }
    discovered.sort_by(|(_, left), (_, right)| {
        left.as_os_str()
            .as_bytes()
            .cmp(right.as_os_str().as_bytes())
    });
    let mut seen = HashSet::with_capacity(discovered.len());
    for (_, relative) in &discovered {
        if !seen.insert(relative.clone()) {
            return Err(IntegrityError::DuplicateManifestPath(relative.clone()));
        }
    }
    Ok(discovered)
}

fn discover_path(
    root: &Path,
    path: &Path,
    relative: PathBuf,
    root_device: u64,
    depth: usize,
    directories: &mut usize,
    discovered: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<(), IntegrityError> {
    if relative.as_os_str().as_bytes().len() > MAX_PATH_BYTES {
        return Err(IntegrityError::InvalidPath(path.to_path_buf()));
    }
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error(path.to_path_buf(), error))?;
    if metadata.file_type().is_symlink() {
        return Err(IntegrityError::Symlink(path.to_path_buf()));
    }
    if metadata.dev() != root_device {
        return Err(IntegrityError::CrossDevice(path.to_path_buf()));
    }
    if metadata.is_file() {
        if discovered.len() == MAX_DISCOVERED_FILES {
            return Err(IntegrityError::TooManyEntries);
        }
        let bytes = discovered
            .iter()
            .try_fold(0u64, |total, (file, _)| {
                fs::symlink_metadata(file)
                    .ok()
                    .and_then(|entry| total.checked_add(entry.len()))
            })
            .ok_or(IntegrityError::DiscoveryTooLarge)?;
        if bytes
            .checked_add(metadata.len())
            .ok_or(IntegrityError::DiscoveryTooLarge)?
            > MAX_DISCOVERED_BYTES
        {
            return Err(IntegrityError::DiscoveryTooLarge);
        }
        discovered.push((path.to_path_buf(), relative));
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(IntegrityError::InvalidPath(path.to_path_buf()));
    }
    if depth == MAX_DISCOVERY_DEPTH {
        return Err(IntegrityError::DiscoveryTooDeep);
    }
    *directories = directories
        .checked_add(1)
        .ok_or(IntegrityError::TooManyDirectories)?;
    if *directories > MAX_DISCOVERED_DIRECTORIES {
        return Err(IntegrityError::TooManyDirectories);
    }
    let mut children = fs::read_dir(path)
        .map_err(|error| io_error(path.to_path_buf(), error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| io_error(path.to_path_buf(), error))?;
    children.sort_by(|left, right| {
        left.file_name()
            .as_encoded_bytes()
            .cmp(right.file_name().as_encoded_bytes())
    });
    for child in children {
        let name = child.file_name();
        let child_path = child.path();
        let child_relative = relative.join(name);
        discover_path(
            root,
            &child_path,
            child_relative,
            root_device,
            depth + 1,
            directories,
            discovered,
        )?;
    }
    let _ = root;
    Ok(())
}

fn discover_unlisted_regular_files(
    root: &Path,
    excluded: Option<&Path>,
) -> Result<Vec<PathBuf>, IntegrityError> {
    let root_device = fs::symlink_metadata(root)
        .map_err(|error| io_error(root.to_path_buf(), error))?
        .dev();
    let mut files = Vec::new();
    scan_unlisted_directory(root, Path::new(""), root_device, 0, excluded, &mut files)?;
    files.sort_by(|left, right| {
        left.as_os_str()
            .as_bytes()
            .cmp(right.as_os_str().as_bytes())
    });
    Ok(files)
}

fn scan_unlisted_directory(
    directory: &Path,
    relative: &Path,
    root_device: u64,
    depth: usize,
    excluded: Option<&Path>,
    files: &mut Vec<PathBuf>,
) -> Result<(), IntegrityError> {
    if depth > MAX_DISCOVERY_DEPTH {
        return Err(IntegrityError::DiscoveryTooDeep);
    }
    let mut children = fs::read_dir(directory)
        .map_err(|error| io_error(directory.to_path_buf(), error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| io_error(directory.to_path_buf(), error))?;
    children.sort_by(|left, right| {
        left.file_name()
            .as_encoded_bytes()
            .cmp(right.file_name().as_encoded_bytes())
    });
    for child in children {
        let name = child.file_name();
        let path = child.path();
        let child_relative = relative.join(name);
        if excluded == Some(child_relative.as_path()) {
            continue;
        }
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| io_error(path.clone(), error))?;
        if metadata.file_type().is_symlink() || metadata.dev() != root_device {
            continue;
        }
        if metadata.is_file() {
            if files.len() == MAX_DISCOVERED_FILES {
                return Err(IntegrityError::TooManyEntries);
            }
            files.push(child_relative);
        } else if metadata.is_dir() {
            scan_unlisted_directory(
                &path,
                &child_relative,
                root_device,
                depth + 1,
                excluded,
                files,
            )?;
        }
    }
    Ok(())
}

fn is_missing(path: &Path) -> bool {
    matches!(fs::symlink_metadata(path), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
}

fn validate_root(root: &Path) -> Result<(), IntegrityError> {
    if !root.is_absolute() || !is_lexically_normal(root) {
        return Err(IntegrityError::InvalidRoot(root.to_path_buf()));
    }
    let metadata =
        fs::symlink_metadata(root).map_err(|error| io_error(root.to_path_buf(), error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(IntegrityError::InvalidRoot(root.to_path_buf()));
    }
    Ok(())
}

fn validate_absolute_file_path(path: &Path) -> Result<(), IntegrityError> {
    if !path.is_absolute() || path.file_name().is_none() || !is_lexically_normal(path) {
        return Err(IntegrityError::InvalidPath(path.to_path_buf()));
    }
    Ok(())
}

fn validate_relative_path(path: &Path) -> Result<(), IntegrityError> {
    if path.as_os_str().is_empty() || path.is_absolute() || !is_lexically_normal(path) {
        return Err(IntegrityError::InvalidPath(path.to_path_buf()));
    }
    Ok(())
}

fn is_lexically_normal(path: &Path) -> bool {
    !path.components().any(|component| {
        matches!(
            component,
            Component::CurDir | Component::ParentDir | Component::Prefix(_)
        )
    })
}

fn ensure_no_symlink_path(root: &Path, relative: &Path) -> Result<(), IntegrityError> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(IntegrityError::InvalidPath(relative.to_path_buf()));
        };
        current.push(name);
        let metadata =
            fs::symlink_metadata(&current).map_err(|error| io_error(current.clone(), error))?;
        if metadata.file_type().is_symlink() {
            return Err(IntegrityError::Symlink(current));
        }
    }
    Ok(())
}

fn no_follow_file_identity(path: &Path) -> Result<FileIdentity, IntegrityError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error(path.to_path_buf(), error))?;
    if metadata.file_type().is_symlink() {
        return Err(IntegrityError::Symlink(path.to_path_buf()));
    }
    if !metadata.is_file() {
        return Err(IntegrityError::InvalidPath(path.to_path_buf()));
    }
    Ok(FileIdentity::from_metadata(&metadata))
}

fn map_checksum_error(error: ChecksumError) -> IntegrityError {
    match error {
        ChecksumError::Cancelled => IntegrityError::Cancelled,
        error => IntegrityError::Checksum(error),
    }
}

fn io_error(path: PathBuf, error: std::io::Error) -> IntegrityError {
    IntegrityError::Io {
        path,
        message: error.to_string(),
        not_found: error.kind() == std::io::ErrorKind::NotFound,
    }
}

fn encode_gnu_path(path: &[u8], output: &mut Vec<u8>) {
    for byte in path {
        match byte {
            b'\\' => output.extend_from_slice(b"\\\\"),
            b'\n' => output.extend_from_slice(b"\\n"),
            byte => output.push(*byte),
        }
    }
}

fn decode_gnu_path(encoded: &[u8]) -> Result<Vec<u8>, IntegrityError> {
    let mut output = Vec::with_capacity(encoded.len());
    let mut index = 0;
    while index < encoded.len() {
        if encoded[index] != b'\\' {
            output.push(encoded[index]);
            index += 1;
            continue;
        }
        let Some(escaped) = encoded.get(index + 1) else {
            return Err(IntegrityError::MalformedManifest);
        };
        match escaped {
            b'\\' => output.push(b'\\'),
            b'n' => output.push(b'\n'),
            _ => return Err(IntegrityError::MalformedManifest),
        }
        index += 2;
    }
    Ok(output)
}

fn is_lower_hex(input: &[u8]) -> bool {
    input
        .iter()
        .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        fs,
        os::unix::{ffi::OsStringExt, fs::symlink},
        process::Command,
    };

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn phase_18t_fingerprint_matches_then_rejects_stale_identity_and_cancellation() {
        let root = tempdir().expect("root");
        let path = root.path().join("report");
        fs::write(&path, b"original").expect("write");

        let fingerprint = save_fingerprint(path.clone(), || false, |_, _| {}).expect("save");
        assert_eq!(
            verify_fingerprint(&fingerprint, || false, |_, _| {}).expect("verify"),
            FingerprintVerification::Match
        );

        fs::write(&path, b"replacement").expect("replace");
        assert_eq!(
            verify_fingerprint(&fingerprint, || false, |_, _| {}).expect("verify stale"),
            FingerprintVerification::StaleIdentity
        );
        assert!(matches!(
            verify_fingerprint(&fingerprint, || true, |_, _| {}),
            Err(IntegrityError::Cancelled)
        ));
        fs::remove_file(&path).expect("remove");
        assert!(matches!(
            verify_fingerprint(&fingerprint, || false, |_, _| {}),
            Err(IntegrityError::Io { .. })
        ));
    }

    #[test]
    fn phase_18t_manifest_round_trips_non_utf8_and_verifies_changed_missing_and_links() {
        let root = tempdir().expect("root");
        let regular = root.path().join("ordinary file");
        let non_utf8 = root
            .path()
            .join(OsString::from_vec(b"hostile-\xff\nname".to_vec()));
        let backslash = root.path().join("back\\slash");
        fs::write(&regular, b"one").expect("regular");
        fs::write(&non_utf8, b"two").expect("non utf8");
        fs::write(&backslash, b"three").expect("backslash");

        let manifest = generate_sha256sums(
            root.path(),
            &[regular.clone(), non_utf8.clone(), backslash.clone()],
            || false,
            |_, _| {},
        )
        .expect("generate");
        let bytes = manifest.encode();
        assert!(!bytes.starts_with(b"#"));
        assert!(bytes.windows(2).any(|window| window == b"\\n"));
        let parsed = parse_sha256sums(&bytes).expect("parse");
        assert_eq!(parsed, manifest);
        assert_eq!(
            verify_sha256sums(root.path(), &parsed, || false, |_, _| {})
                .expect("verify")
                .entries,
            vec![
                (
                    PathBuf::from("back\\slash"),
                    ManifestEntryVerification::Match
                ),
                (
                    PathBuf::from(OsString::from_vec(b"hostile-\xff\nname".to_vec())),
                    ManifestEntryVerification::Match,
                ),
                (
                    PathBuf::from("ordinary file"),
                    ManifestEntryVerification::Match
                ),
            ]
        );

        fs::write(&regular, b"changed").expect("change");
        fs::remove_file(&non_utf8).expect("remove");
        let result = verify_sha256sums(root.path(), &parsed, || false, |_, _| {}).expect("verify");
        assert!(matches!(
            result.entries[2].1,
            ManifestEntryVerification::Changed { .. }
        ));
        assert_eq!(result.entries[1].1, ManifestEntryVerification::Missing);
        fs::write(&regular, b"one").expect("restore");
        fs::write(&non_utf8, b"two").expect("restore non utf8");

        let link = root.path().join("link");
        symlink(&regular, &link).expect("link");
        assert!(matches!(
            generate_sha256sums(root.path(), &[link], || false, |_, _| {}),
            Err(IntegrityError::Symlink(_))
        ));

        fs::write(root.path().join("SHA256SUMS"), &bytes).expect("manifest file");
        match Command::new("sha256sum")
            .arg("-c")
            .arg("SHA256SUMS")
            .current_dir(root.path())
            .output()
        {
            Ok(output) => assert!(
                output.status.success(),
                "sha256sum -c failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("could not run sha256sum: {error}"),
        }
    }

    #[test]
    fn phase_18t_manifest_rejects_malformed_duplicate_and_escaping_records() {
        let digest = b"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let mut duplicate = Vec::new();
        duplicate.extend_from_slice(digest);
        duplicate.extend_from_slice(b"  a\n");
        duplicate.extend_from_slice(digest);
        duplicate.extend_from_slice(b"  a\n");
        assert!(matches!(
            parse_sha256sums(&duplicate),
            Err(IntegrityError::DuplicateManifestPath(_))
        ));

        let mut escape = Vec::new();
        escape.extend_from_slice(digest);
        escape.extend_from_slice(b"  ../secret\n");
        assert!(matches!(
            parse_sha256sums(&escape),
            Err(IntegrityError::MalformedManifest)
        ));
        assert!(matches!(
            parse_sha256sums(b"not a manifest"),
            Err(IntegrityError::MalformedManifest)
        ));
        let mut bad_escape = Vec::new();
        bad_escape.push(b'\\');
        bad_escape.extend_from_slice(digest);
        bad_escape.extend_from_slice(b"  bad\\q\n");
        assert!(matches!(
            parse_sha256sums(&bad_escape),
            Err(IntegrityError::MalformedManifest)
        ));
    }

    #[test]
    fn phase_18t_manifest_cancellation_is_cooperative() {
        let root = tempdir().expect("root");
        let path = root.path().join("one");
        fs::write(&path, b"one").expect("write");
        let cancelled = Cell::new(false);
        let result =
            generate_sha256sums(root.path(), &[path], || cancelled.replace(true), |_, _| {});
        assert!(matches!(result, Err(IntegrityError::Cancelled)));
    }

    #[test]
    fn phase_18t_manifest_recurses_and_reports_unlisted_files() {
        let root = tempdir().expect("root");
        let folder = root.path().join("selected");
        fs::create_dir_all(folder.join("nested")).expect("folders");
        fs::write(folder.join("nested/known"), b"known").expect("known");
        let manifest = generate_sha256sums(root.path(), &[folder], || false, |_, _| {})
            .expect("recursive manifest");
        assert_eq!(manifest.entries().len(), 1);
        fs::write(root.path().join("unexpected"), b"new").expect("new");
        let verified =
            verify_sha256sums(root.path(), &manifest, || false, |_, _| {}).expect("verify");
        assert!(
            verified
                .entries
                .iter()
                .any(|(path, state)| path == Path::new("unexpected")
                    && *state == ManifestEntryVerification::New)
        );
    }

    #[test]
    fn phase_18t_fingerprint_record_is_versioned_binary_safe_and_strict() {
        let root = tempdir().expect("root");
        let path = root
            .path()
            .join(OsString::from_vec(b"record-\xff".to_vec()));
        fs::write(&path, b"record").expect("write");
        let saved = save_fingerprint(path, || false, |_, _| {}).expect("save");
        assert_eq!(
            SavedFingerprint::decode_record(&saved.encode_record()).expect("decode"),
            saved
        );
        assert!(matches!(
            SavedFingerprint::decode_record(b"Floe fingerprint v0\n"),
            Err(IntegrityError::MalformedFingerprintRecord)
        ));
    }

    #[test]
    fn phase_18t_manifest_enforces_recursive_depth_and_file_capacity() {
        let root = tempdir().expect("root");
        let deep = root.path().join("deep");
        let mut cursor = deep.clone();
        for _ in 0..=MAX_DISCOVERY_DEPTH {
            cursor.push("d");
        }
        fs::create_dir_all(&cursor).expect("deep folders");
        fs::write(cursor.join("file"), b"deep").expect("deep file");
        assert!(matches!(
            generate_sha256sums(root.path(), &[deep], || false, |_, _| {}),
            Err(IntegrityError::DiscoveryTooDeep)
        ));

        let many = root.path().join("many");
        fs::create_dir(&many).expect("many folder");
        for index in 0..=MAX_DISCOVERED_FILES {
            fs::write(many.join(index.to_string()), b"x").expect("file");
        }
        assert!(matches!(
            generate_sha256sums(root.path(), &[many], || false, |_, _| {}),
            Err(IntegrityError::TooManyEntries)
        ));
    }
}
