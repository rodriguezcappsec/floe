//! Strict private persistence for Phase 18X protected-folder policy.
//!
//! Missing storage is an empty policy. Every other load failure is explicit so
//! integration can block destructive operations until the user acknowledges
//! the problem; corrupt or unsafe storage must never silently become empty.

use std::{
    collections::HashSet,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use floe_core::{
    GUARDRAIL_PATH_BYTE_CAPACITY, PROTECTED_ROOT_CAPACITY, ProtectedRoots, ProtectedRootsError,
};
use thiserror::Error;

const GUARDRAIL_STORE_MAGIC: &[u8; 8] = b"FLOEGRDX";
const GUARDRAIL_STORE_VERSION: u16 = 1;
const GUARDRAIL_STORE_BYTE_CAPACITY: u64 = 1_024 * 1_024;
static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct GuardrailStore;

impl GuardrailStore {
    /// Load policy while preserving the fail-closed distinction between a
    /// genuinely missing store and an unreadable, unsafe, or corrupt store.
    pub fn load_fail_closed(path: &Path) -> GuardrailPolicyLoad {
        match Self::load(path) {
            Ok(Some(policy)) => GuardrailPolicyLoad::Ready(policy),
            Ok(None) => GuardrailPolicyLoad::Missing,
            Err(error) => GuardrailPolicyLoad::Blocked(error),
        }
    }

    pub fn load(path: &Path) -> Result<Option<ProtectedRoots>, GuardrailStoreError> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(GuardrailStoreError::Io {
                    operation: "inspect policy store",
                    source,
                });
            }
        };
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| GuardrailStoreError::InvalidPath(path.to_path_buf()))?;
        validate_private_directory(parent)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(GuardrailStoreError::UnsafePath(path.to_path_buf()));
        }
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(GuardrailStoreError::InsecurePermissions(path.to_path_buf()));
        }
        if metadata.len() > GUARDRAIL_STORE_BYTE_CAPACITY {
            return Err(GuardrailStoreError::FileTooLarge(metadata.len()));
        }

        let file = OpenOptions::new()
            .read(true)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(path)
            .map_err(|source| GuardrailStoreError::Io {
                operation: "open policy store",
                source,
            })?;
        let mut bytes = Vec::with_capacity(
            usize::try_from(metadata.len()).unwrap_or(GUARDRAIL_STORE_BYTE_CAPACITY as usize),
        );
        file.take(GUARDRAIL_STORE_BYTE_CAPACITY.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|source| GuardrailStoreError::Io {
                operation: "read policy store",
                source,
            })?;
        if bytes.len() as u64 > GUARDRAIL_STORE_BYTE_CAPACITY {
            return Err(GuardrailStoreError::FileTooLarge(bytes.len() as u64));
        }
        decode(&bytes).map(Some)
    }

    /// Atomically replace storage only after the complete new policy is
    /// encoded, written, synchronized, and validated for safe destination use.
    /// A failure before rename leaves the previous policy file intact.
    pub fn persist(path: &Path, policy: &ProtectedRoots) -> Result<(), GuardrailStoreError> {
        let bytes = encode(policy)?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| GuardrailStoreError::InvalidPath(path.to_path_buf()))?;
        ensure_private_directory(parent)?;
        validate_existing_destination(path)?;
        atomic_write(path, &bytes)
    }
}

#[derive(Debug)]
pub enum GuardrailPolicyLoad {
    Missing,
    Ready(ProtectedRoots),
    Blocked(GuardrailStoreError),
}

impl GuardrailPolicyLoad {
    pub const fn destructive_actions_blocked(&self) -> bool {
        matches!(self, Self::Blocked(_))
    }

    pub fn policy(&self) -> Option<&ProtectedRoots> {
        match self {
            Self::Ready(policy) => Some(policy),
            Self::Missing | Self::Blocked(_) => None,
        }
    }

    pub fn error(&self) -> Option<&GuardrailStoreError> {
        match self {
            Self::Blocked(error) => Some(error),
            Self::Missing | Self::Ready(_) => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum GuardrailStoreError {
    #[error("guardrail policy store path is invalid: {}", .0.display())]
    InvalidPath(PathBuf),
    #[error("guardrail policy store path is unsafe: {}", .0.display())]
    UnsafePath(PathBuf),
    #[error("guardrail policy store is accessible by group or others: {}", .0.display())]
    InsecurePermissions(PathBuf),
    #[error("guardrail policy store version {0} is unsupported")]
    UnsupportedVersion(u16),
    #[error("guardrail policy store is malformed or truncated")]
    Malformed,
    #[error("guardrail policy store has trailing data")]
    TrailingData,
    #[error("guardrail policy store is {0} bytes, exceeding its capacity")]
    FileTooLarge(u64),
    #[error("guardrail policy store contains a relative root: {}", .0.display())]
    RelativeRoot(PathBuf),
    #[error("guardrail policy store contains an unnormalized root: {}", .0.display())]
    UnnormalizedRoot(PathBuf),
    #[error("guardrail policy store contains an overlong root: {}", .0.display())]
    PathTooLong(PathBuf),
    #[error("guardrail policy store root length {length} exceeds capacity {capacity}")]
    EncodedPathTooLong { length: usize, capacity: usize },
    #[error("guardrail policy store repeats a root: {}", .0.display())]
    DuplicateRoot(PathBuf),
    #[error("guardrail policy store has {count} roots, exceeding capacity {capacity}")]
    TooManyRoots { count: usize, capacity: usize },
    #[error("could not {operation} guardrail policy store")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
}

fn encode(policy: &ProtectedRoots) -> Result<Vec<u8>, GuardrailStoreError> {
    if policy.roots().len() > PROTECTED_ROOT_CAPACITY {
        return Err(GuardrailStoreError::TooManyRoots {
            count: policy.roots().len(),
            capacity: PROTECTED_ROOT_CAPACITY,
        });
    }
    let mut bytes = Vec::with_capacity(32 + policy.roots().len().saturating_mul(64));
    bytes.extend_from_slice(GUARDRAIL_STORE_MAGIC);
    bytes.extend_from_slice(&GUARDRAIL_STORE_VERSION.to_le_bytes());
    bytes.extend_from_slice(&policy.generation().to_le_bytes());
    bytes.extend_from_slice(
        &u32::try_from(policy.roots().len())
            .map_err(|_| GuardrailStoreError::TooManyRoots {
                count: policy.roots().len(),
                capacity: PROTECTED_ROOT_CAPACITY,
            })?
            .to_le_bytes(),
    );
    for root in policy.roots() {
        let raw = root.as_os_str().as_bytes();
        if raw.len() > GUARDRAIL_PATH_BYTE_CAPACITY {
            return Err(GuardrailStoreError::PathTooLong(root.clone()));
        }
        bytes.extend_from_slice(
            &u32::try_from(raw.len())
                .map_err(|_| GuardrailStoreError::PathTooLong(root.clone()))?
                .to_le_bytes(),
        );
        bytes.extend_from_slice(raw);
    }
    if bytes.len() as u64 > GUARDRAIL_STORE_BYTE_CAPACITY {
        return Err(GuardrailStoreError::FileTooLarge(bytes.len() as u64));
    }
    Ok(bytes)
}

fn decode(bytes: &[u8]) -> Result<ProtectedRoots, GuardrailStoreError> {
    if bytes.len() as u64 > GUARDRAIL_STORE_BYTE_CAPACITY {
        return Err(GuardrailStoreError::FileTooLarge(bytes.len() as u64));
    }
    let mut cursor = Cursor::new(bytes);
    if cursor.take(GUARDRAIL_STORE_MAGIC.len())? != GUARDRAIL_STORE_MAGIC {
        return Err(GuardrailStoreError::Malformed);
    }
    let version = cursor.u16()?;
    if version != GUARDRAIL_STORE_VERSION {
        return Err(GuardrailStoreError::UnsupportedVersion(version));
    }
    let generation = cursor.u64()?;
    let count = usize::try_from(cursor.u32()?).map_err(|_| GuardrailStoreError::Malformed)?;
    if count > PROTECTED_ROOT_CAPACITY {
        return Err(GuardrailStoreError::TooManyRoots {
            count,
            capacity: PROTECTED_ROOT_CAPACITY,
        });
    }

    let mut roots = Vec::with_capacity(count);
    let mut seen = HashSet::with_capacity(count);
    for _ in 0..count {
        let length = usize::try_from(cursor.u32()?).map_err(|_| GuardrailStoreError::Malformed)?;
        if length > GUARDRAIL_PATH_BYTE_CAPACITY {
            return Err(GuardrailStoreError::EncodedPathTooLong {
                length,
                capacity: GUARDRAIL_PATH_BYTE_CAPACITY,
            });
        }
        let root = PathBuf::from(OsString::from_vec(cursor.take(length)?.to_vec()));
        if !seen.insert(root.clone()) {
            return Err(GuardrailStoreError::DuplicateRoot(root));
        }
        roots.push(root);
    }
    if !cursor.remaining().is_empty() {
        return Err(GuardrailStoreError::TrailingData);
    }

    ProtectedRoots::with_generation(generation, roots).map_err(map_policy_error)
}

fn map_policy_error(error: ProtectedRootsError) -> GuardrailStoreError {
    match error {
        ProtectedRootsError::Relative(path) => GuardrailStoreError::RelativeRoot(path),
        ProtectedRootsError::Unnormalized(path) => GuardrailStoreError::UnnormalizedRoot(path),
        ProtectedRootsError::PathTooLong(path) => GuardrailStoreError::PathTooLong(path),
        ProtectedRootsError::Duplicate(path) => GuardrailStoreError::DuplicateRoot(path),
        ProtectedRootsError::CapacityExceeded { count, capacity } => {
            GuardrailStoreError::TooManyRoots { count, capacity }
        }
        ProtectedRootsError::GenerationExhausted => GuardrailStoreError::Malformed,
    }
}

fn ensure_private_directory(path: &Path) -> Result<(), GuardrailStoreError> {
    fs::create_dir_all(path).map_err(|source| GuardrailStoreError::Io {
        operation: "create policy directory",
        source,
    })?;
    let metadata = fs::symlink_metadata(path).map_err(|source| GuardrailStoreError::Io {
        operation: "inspect policy directory",
        source,
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(GuardrailStoreError::UnsafePath(path.to_path_buf()));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
        GuardrailStoreError::Io {
            operation: "restrict policy directory",
            source,
        }
    })
}

fn validate_private_directory(path: &Path) -> Result<(), GuardrailStoreError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| GuardrailStoreError::Io {
        operation: "inspect policy directory",
        source,
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(GuardrailStoreError::UnsafePath(path.to_path_buf()));
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(GuardrailStoreError::InsecurePermissions(path.to_path_buf()));
    }
    Ok(())
}

fn validate_existing_destination(path: &Path) -> Result<(), GuardrailStoreError> {
    match fs::symlink_metadata(path) {
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(GuardrailStoreError::Io {
            operation: "inspect existing policy store",
            source,
        }),
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(GuardrailStoreError::UnsafePath(path.to_path_buf()))
        }
        Ok(metadata) if metadata.permissions().mode() & 0o077 != 0 => {
            Err(GuardrailStoreError::InsecurePermissions(path.to_path_buf()))
        }
        Ok(_) => Ok(()),
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), GuardrailStoreError> {
    let parent = path
        .parent()
        .ok_or_else(|| GuardrailStoreError::InvalidPath(path.to_path_buf()))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| GuardrailStoreError::InvalidPath(path.to_path_buf()))?;
    let temporary = parent.join(format!(
        ".{}.{}-{}.tmp",
        file_name.to_string_lossy(),
        std::process::id(),
        TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(&temporary)
            .map_err(|source| GuardrailStoreError::Io {
                operation: "create temporary policy store",
                source,
            })?;
        file.write_all(bytes)
            .map_err(|source| GuardrailStoreError::Io {
                operation: "write temporary policy store",
                source,
            })?;
        file.sync_all().map_err(|source| GuardrailStoreError::Io {
            operation: "synchronize temporary policy store",
            source,
        })?;
        fs::rename(&temporary, path).map_err(|source| GuardrailStoreError::Io {
            operation: "publish policy store",
            source,
        })?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|source| GuardrailStoreError::Io {
                operation: "synchronize policy directory",
                source,
            })
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], GuardrailStoreError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(GuardrailStoreError::Malformed)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(GuardrailStoreError::Malformed)?;
        self.offset = end;
        Ok(value)
    }

    fn u16(&mut self) -> Result<u16, GuardrailStoreError> {
        Ok(u16::from_le_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| GuardrailStoreError::Malformed)?,
        ))
    }

    fn u32(&mut self) -> Result<u32, GuardrailStoreError> {
        Ok(u32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| GuardrailStoreError::Malformed)?,
        ))
    }

    fn u64(&mut self) -> Result<u64, GuardrailStoreError> {
        Ok(u64::from_le_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| GuardrailStoreError::Malformed)?,
        ))
    }

    fn remaining(&self) -> &'a [u8] {
        &self.bytes[self.offset..]
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::{ffi::OsStringExt, fs::symlink};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn phase_18x_protected_store_round_trips_raw_policy_privately_and_atomically() {
        let fixture = tempdir().expect("fixture");
        let raw = PathBuf::from(OsString::from_vec(b"/protected/raw-\xff".to_vec()));
        let policy = ProtectedRoots::with_generation(41, vec![raw]).expect("policy");
        let path = fixture.path().join("private/guardrails-v1.bin");

        GuardrailStore::persist(&path, &policy).expect("persist");
        assert_eq!(
            fs::metadata(path.parent().expect("parent"))
                .expect("parent metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path)
                .expect("file metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(GuardrailStore::load(&path).expect("load"), Some(policy));
        assert_eq!(
            fs::read_dir(path.parent().expect("parent"))
                .expect("read")
                .count(),
            1
        );
    }

    #[test]
    fn phase_18x_protected_store_rejects_corrupt_relative_duplicate_and_over_capacity_data() {
        let fixture = tempdir().expect("fixture");
        let private = fixture.path().join("private");
        fs::create_dir(&private).expect("private directory");
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700))
            .expect("private directory mode");
        let path = private.join("guardrails.bin");
        fs::write(&path, b"corrupt").expect("corrupt");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("mode");
        assert!(matches!(
            GuardrailStore::load(&path),
            Err(GuardrailStoreError::Malformed)
        ));

        let mut relative = Vec::new();
        relative.extend_from_slice(GUARDRAIL_STORE_MAGIC);
        relative.extend_from_slice(&GUARDRAIL_STORE_VERSION.to_le_bytes());
        relative.extend_from_slice(&0u64.to_le_bytes());
        relative.extend_from_slice(&1u32.to_le_bytes());
        relative.extend_from_slice(&8u32.to_le_bytes());
        relative.extend_from_slice(b"relative");
        assert!(matches!(
            decode(&relative),
            Err(GuardrailStoreError::RelativeRoot(_))
        ));

        let mut duplicate = relative[..22].to_vec();
        duplicate[18..22].copy_from_slice(&2u32.to_le_bytes());
        duplicate.extend_from_slice(&4u32.to_le_bytes());
        duplicate.extend_from_slice(b"/one");
        duplicate.extend_from_slice(&4u32.to_le_bytes());
        duplicate.extend_from_slice(b"/one");
        assert!(matches!(
            decode(&duplicate),
            Err(GuardrailStoreError::DuplicateRoot(_))
        ));

        let mut over_capacity = relative[..18].to_vec();
        over_capacity.extend_from_slice(
            &u32::try_from(PROTECTED_ROOT_CAPACITY + 1)
                .expect("count")
                .to_le_bytes(),
        );
        assert!(matches!(
            decode(&over_capacity),
            Err(GuardrailStoreError::TooManyRoots { .. })
        ));
    }

    #[test]
    fn phase_18x_store_rejects_symlink_and_preserves_prior_policy_on_save_failure() {
        let fixture = tempdir().expect("fixture");
        let private = fixture.path().join("links");
        fs::create_dir(&private).expect("private directory");
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700))
            .expect("private directory mode");
        let target = private.join("target");
        fs::write(&target, b"not policy").expect("target");
        let link = private.join("link");
        symlink(&target, &link).expect("symlink");
        assert!(matches!(
            GuardrailStore::load(&link),
            Err(GuardrailStoreError::UnsafePath(_))
        ));

        let path = fixture.path().join("private/guardrails-v1.bin");
        let policy = ProtectedRoots::new(vec![PathBuf::from("/one")]).expect("policy");
        GuardrailStore::persist(&path, &policy).expect("initial save");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("insecure");
        let replacement = ProtectedRoots::new(vec![PathBuf::from("/two")]).expect("replacement");
        assert!(matches!(
            GuardrailStore::persist(&path, &replacement),
            Err(GuardrailStoreError::InsecurePermissions(_))
        ));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("restore mode");
        assert_eq!(
            GuardrailStore::load(&path).expect("prior remains"),
            Some(policy)
        );
    }

    #[test]
    fn phase_18x_store_missing_file_is_the_only_empty_policy_fallback() {
        let fixture = tempdir().expect("fixture");
        let missing = fixture.path().join("absent/private/guardrails-v1.bin");
        assert_eq!(GuardrailStore::load(&missing).expect("missing"), None);
        assert!(matches!(
            GuardrailStore::load_fail_closed(&missing),
            GuardrailPolicyLoad::Missing
        ));
    }

    #[test]
    fn phase_18x_store_corruption_is_an_explicit_fail_closed_state() {
        let fixture = tempdir().expect("fixture");
        let private = fixture.path().join("private");
        fs::create_dir(&private).expect("private directory");
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700))
            .expect("private directory mode");
        let path = private.join("guardrails-v1.bin");
        fs::write(&path, b"truncated policy").expect("corrupt store");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("private file");

        let load = GuardrailStore::load_fail_closed(&path);
        assert!(load.destructive_actions_blocked());
        assert!(load.policy().is_none());
        assert!(matches!(load.error(), Some(GuardrailStoreError::Malformed)));

        let policy = ProtectedRoots::new(vec![PathBuf::from("/protected")]).expect("policy");
        GuardrailStore::persist(&path, &policy).expect("repair store");
        let repaired = GuardrailStore::load_fail_closed(&path);
        assert!(!repaired.destructive_actions_blocked());
        assert_eq!(repaired.policy(), Some(&policy));
    }

    #[test]
    fn phase_18x_store_rejects_unsupported_trailing_and_insecure_parent_state() {
        let fixture = tempdir().expect("fixture");
        let private = fixture.path().join("private");
        fs::create_dir(&private).expect("private directory");
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700))
            .expect("private directory mode");
        let path = private.join("guardrails-v1.bin");
        let policy = ProtectedRoots::new(vec![PathBuf::from("/protected")]).expect("policy");
        GuardrailStore::persist(&path, &policy).expect("persist");
        let valid = fs::read(&path).expect("encoded policy");

        let mut unsupported = valid.clone();
        unsupported[GUARDRAIL_STORE_MAGIC.len()..GUARDRAIL_STORE_MAGIC.len() + 2]
            .copy_from_slice(&2u16.to_le_bytes());
        fs::write(&path, unsupported).expect("unsupported version");
        assert!(matches!(
            GuardrailStore::load_fail_closed(&path),
            GuardrailPolicyLoad::Blocked(GuardrailStoreError::UnsupportedVersion(2))
        ));

        let mut trailing = valid.clone();
        trailing.push(0);
        fs::write(&path, trailing).expect("trailing data");
        assert!(matches!(
            GuardrailStore::load_fail_closed(&path),
            GuardrailPolicyLoad::Blocked(GuardrailStoreError::TrailingData)
        ));

        fs::write(&path, valid).expect("restore valid data");
        fs::set_permissions(&private, fs::Permissions::from_mode(0o755))
            .expect("insecure directory");
        assert!(matches!(
            GuardrailStore::load_fail_closed(&path),
            GuardrailPolicyLoad::Blocked(GuardrailStoreError::InsecurePermissions(_))
        ));
    }
}
