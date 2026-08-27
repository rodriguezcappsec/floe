//! Private, versioned storage for one explicit integrity-monitor baseline.

use std::{
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
    INTEGRITY_MONITOR_ENTRY_CAPACITY, INTEGRITY_MONITOR_PATH_BYTES, IntegrityBaseline,
    IntegrityBaselineEntry,
};
use thiserror::Error;

const MAGIC: &[u8; 8] = b"FLOEIMON";
const VERSION: u16 = 1;
const MAX_STORE_BYTES: u64 = 2 * 1_024 * 1_024;
static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrityBaselineStoragePolicy {
    Persist,
    #[cfg(test)]
    SuppressPrivateState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrityBaselineStoreOutcome {
    Persisted,
    Suppressed,
}

pub struct IntegrityBaselineStore;

impl IntegrityBaselineStore {
    pub fn load(path: &Path) -> Result<Option<IntegrityBaseline>, IntegrityBaselineStoreError> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(IntegrityBaselineStoreError::Io {
                    operation: "inspect",
                    source,
                });
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(IntegrityBaselineStoreError::UnsafePath);
        }
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(IntegrityBaselineStoreError::InsecurePermissions);
        }
        if metadata.len() > MAX_STORE_BYTES {
            return Err(IntegrityBaselineStoreError::FileTooLarge);
        }

        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(path)
            .map_err(|source| IntegrityBaselineStoreError::Io {
                operation: "open",
                source,
            })?;
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.read_to_end(&mut bytes)
            .map_err(|source| IntegrityBaselineStoreError::Io {
                operation: "read",
                source,
            })?;
        decode(&bytes).map(Some)
    }

    pub fn persist(
        path: &Path,
        baseline: &IntegrityBaseline,
        policy: IntegrityBaselineStoragePolicy,
    ) -> Result<IntegrityBaselineStoreOutcome, IntegrityBaselineStoreError> {
        match policy {
            IntegrityBaselineStoragePolicy::Persist => {
                let parent = path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .ok_or(IntegrityBaselineStoreError::InvalidPath)?;
                ensure_private_directory(parent)?;
                reject_unsafe_existing_file(path)?;
                let bytes = encode(baseline)?;
                atomic_write(path, &bytes)?;
                Ok(IntegrityBaselineStoreOutcome::Persisted)
            }
            #[cfg(test)]
            IntegrityBaselineStoragePolicy::SuppressPrivateState => {
                remove_owned_store(path)?;
                Ok(IntegrityBaselineStoreOutcome::Suppressed)
            }
        }
    }

    pub fn remove_private_state(
        path: &Path,
    ) -> Result<IntegrityBaselineStoreOutcome, IntegrityBaselineStoreError> {
        remove_owned_store(path)?;
        Ok(IntegrityBaselineStoreOutcome::Suppressed)
    }
}

#[derive(Debug, Error)]
pub enum IntegrityBaselineStoreError {
    #[error("integrity baseline store path is invalid")]
    InvalidPath,
    #[error("integrity baseline store path is not a regular non-symlink file")]
    UnsafePath,
    #[error("integrity baseline store permissions are not private")]
    InsecurePermissions,
    #[error("integrity baseline store has an unsupported version")]
    UnsupportedVersion,
    #[error("integrity baseline store is malformed")]
    Malformed,
    #[error("integrity baseline store exceeds its bounded size")]
    FileTooLarge,
    #[error("could not {operation} integrity baseline store")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
}

fn encode(baseline: &IntegrityBaseline) -> Result<Vec<u8>, IntegrityBaselineStoreError> {
    if baseline.entries().len() > INTEGRITY_MONITOR_ENTRY_CAPACITY {
        return Err(IntegrityBaselineStoreError::Malformed);
    }
    let root = baseline.root().as_os_str().as_bytes();
    if root.len() > INTEGRITY_MONITOR_PATH_BYTES {
        return Err(IntegrityBaselineStoreError::Malformed);
    }
    let mut bytes = Vec::with_capacity(32 + root.len() + baseline.entries().len() * 100);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    push_bytes(&mut bytes, root)?;
    bytes.extend_from_slice(
        &u32::try_from(baseline.entries().len())
            .map_err(|_| IntegrityBaselineStoreError::Malformed)?
            .to_le_bytes(),
    );
    for entry in baseline.entries() {
        push_bytes(&mut bytes, entry.path().as_os_str().as_bytes())?;
        bytes.extend_from_slice(entry.sha256().as_bytes());
    }
    if bytes.len() as u64 > MAX_STORE_BYTES {
        return Err(IntegrityBaselineStoreError::FileTooLarge);
    }
    Ok(bytes)
}

fn decode(bytes: &[u8]) -> Result<IntegrityBaseline, IntegrityBaselineStoreError> {
    if bytes.len() as u64 > MAX_STORE_BYTES {
        return Err(IntegrityBaselineStoreError::FileTooLarge);
    }
    let mut cursor = Cursor::new(bytes);
    if cursor.take(MAGIC.len())? != MAGIC {
        return Err(IntegrityBaselineStoreError::Malformed);
    }
    if cursor.u16()? != VERSION {
        return Err(IntegrityBaselineStoreError::UnsupportedVersion);
    }
    let root = PathBuf::from(std::ffi::OsString::from_vec(cursor.path_bytes()?));
    let count =
        usize::try_from(cursor.u32()?).map_err(|_| IntegrityBaselineStoreError::Malformed)?;
    if count > INTEGRITY_MONITOR_ENTRY_CAPACITY {
        return Err(IntegrityBaselineStoreError::Malformed);
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let path = PathBuf::from(std::ffi::OsString::from_vec(cursor.path_bytes()?));
        let digest = std::str::from_utf8(cursor.take(64)?)
            .map_err(|_| IntegrityBaselineStoreError::Malformed)?
            .to_owned();
        entries.push(
            IntegrityBaselineEntry::new(path, digest)
                .map_err(|_| IntegrityBaselineStoreError::Malformed)?,
        );
    }
    if !cursor.remaining().is_empty() {
        return Err(IntegrityBaselineStoreError::Malformed);
    }
    IntegrityBaseline::new(root, entries).map_err(|_| IntegrityBaselineStoreError::Malformed)
}

fn push_bytes(output: &mut Vec<u8>, value: &[u8]) -> Result<(), IntegrityBaselineStoreError> {
    if value.len() > INTEGRITY_MONITOR_PATH_BYTES {
        return Err(IntegrityBaselineStoreError::Malformed);
    }
    output.extend_from_slice(
        &u32::try_from(value.len())
            .map_err(|_| IntegrityBaselineStoreError::Malformed)?
            .to_le_bytes(),
    );
    output.extend_from_slice(value);
    Ok(())
}

fn ensure_private_directory(path: &Path) -> Result<(), IntegrityBaselineStoreError> {
    fs::create_dir_all(path).map_err(|source| IntegrityBaselineStoreError::Io {
        operation: "create directory",
        source,
    })?;
    let metadata =
        fs::symlink_metadata(path).map_err(|source| IntegrityBaselineStoreError::Io {
            operation: "inspect directory",
            source,
        })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(IntegrityBaselineStoreError::UnsafePath);
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
        IntegrityBaselineStoreError::Io {
            operation: "secure directory",
            source,
        }
    })
}

fn reject_unsafe_existing_file(path: &Path) -> Result<(), IntegrityBaselineStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(IntegrityBaselineStoreError::UnsafePath)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(IntegrityBaselineStoreError::Io {
            operation: "inspect destination",
            source,
        }),
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), IntegrityBaselineStoreError> {
    let parent = path
        .parent()
        .ok_or(IntegrityBaselineStoreError::InvalidPath)?;
    let temporary = parent.join(format!(
        ".integrity-monitor-{}-{}.tmp",
        std::process::id(),
        TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|source| IntegrityBaselineStoreError::Io {
                operation: "create temporary file",
                source,
            })?;
        file.write_all(bytes)
            .map_err(|source| IntegrityBaselineStoreError::Io {
                operation: "write temporary file",
                source,
            })?;
        file.sync_all()
            .map_err(|source| IntegrityBaselineStoreError::Io {
                operation: "synchronize temporary file",
                source,
            })?;
        fs::rename(&temporary, path).map_err(|source| IntegrityBaselineStoreError::Io {
            operation: "publish baseline",
            source,
        })?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|source| IntegrityBaselineStoreError::Io {
                operation: "synchronize directory",
                source,
            })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn remove_owned_store(path: &Path) -> Result<(), IntegrityBaselineStoreError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(IntegrityBaselineStoreError::Io {
            operation: "inspect suppressed store",
            source,
        }),
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(IntegrityBaselineStoreError::UnsafePath)
        }
        Ok(_) => fs::remove_file(path).map_err(|source| IntegrityBaselineStoreError::Io {
            operation: "remove suppressed store",
            source,
        }),
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], IntegrityBaselineStoreError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(IntegrityBaselineStoreError::Malformed)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(IntegrityBaselineStoreError::Malformed)?;
        self.offset = end;
        Ok(value)
    }

    fn u16(&mut self) -> Result<u16, IntegrityBaselineStoreError> {
        Ok(u16::from_le_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| IntegrityBaselineStoreError::Malformed)?,
        ))
    }

    fn u32(&mut self) -> Result<u32, IntegrityBaselineStoreError> {
        Ok(u32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| IntegrityBaselineStoreError::Malformed)?,
        ))
    }

    fn path_bytes(&mut self) -> Result<Vec<u8>, IntegrityBaselineStoreError> {
        let length =
            usize::try_from(self.u32()?).map_err(|_| IntegrityBaselineStoreError::Malformed)?;
        if length > INTEGRITY_MONITOR_PATH_BYTES {
            return Err(IntegrityBaselineStoreError::Malformed);
        }
        Ok(self.take(length)?.to_vec())
    }

    fn remaining(&self) -> &'a [u8] {
        &self.bytes[self.offset..]
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    };

    use tempfile::tempdir;

    use super::*;

    fn baseline(root: PathBuf) -> IntegrityBaseline {
        IntegrityBaseline::new(
            root,
            vec![
                IntegrityBaselineEntry::new(
                    PathBuf::from(std::ffi::OsString::from_vec(b"raw-\xff".to_vec())),
                    "11".repeat(32),
                )
                .expect("entry"),
            ],
        )
        .expect("baseline")
    }

    #[test]
    fn phase_18u_storage_round_trips_private_raw_path_baseline_atomically() {
        let fixture = tempdir().expect("fixture");
        let store = fixture.path().join("private").join("baseline-v1");
        let expected = baseline(fixture.path().join("watched"));
        assert_eq!(
            IntegrityBaselineStore::persist(
                &store,
                &expected,
                IntegrityBaselineStoragePolicy::Persist
            )
            .expect("persist"),
            IntegrityBaselineStoreOutcome::Persisted
        );
        assert_eq!(
            fs::metadata(store.parent().expect("parent"))
                .expect("directory metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&store)
                .expect("file metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            IntegrityBaselineStore::load(&store).expect("load"),
            Some(expected)
        );
        assert_eq!(
            fs::read_dir(store.parent().expect("parent"))
                .expect("read directory")
                .count(),
            1
        );
    }

    #[test]
    fn phase_18u_storage_rejects_corrupt_insecure_and_symlink_records() {
        let fixture = tempdir().expect("fixture");
        let corrupt = fixture.path().join("corrupt");
        fs::write(&corrupt, b"not a baseline").expect("corrupt fixture");
        fs::set_permissions(&corrupt, fs::Permissions::from_mode(0o600)).expect("permissions");
        assert!(matches!(
            IntegrityBaselineStore::load(&corrupt),
            Err(IntegrityBaselineStoreError::Malformed)
        ));

        fs::set_permissions(&corrupt, fs::Permissions::from_mode(0o644)).expect("permissions");
        assert!(matches!(
            IntegrityBaselineStore::load(&corrupt),
            Err(IntegrityBaselineStoreError::InsecurePermissions)
        ));

        let link = fixture.path().join("link");
        std::os::unix::fs::symlink(&corrupt, &link).expect("symlink");
        assert!(matches!(
            IntegrityBaselineStore::load(&link),
            Err(IntegrityBaselineStoreError::UnsafePath)
        ));
    }

    #[test]
    fn phase_18u_storage_rejects_truncated_tampered_and_unsupported_versions() {
        let fixture = tempdir().expect("fixture");
        let store = fixture.path().join("private").join("baseline-v1");
        let expected = baseline(fixture.path().join("watched"));
        IntegrityBaselineStore::persist(&store, &expected, IntegrityBaselineStoragePolicy::Persist)
            .expect("persist");
        let valid = fs::read(&store).expect("read");

        fs::write(&store, &valid[..valid.len() - 1]).expect("truncate");
        assert!(matches!(
            IntegrityBaselineStore::load(&store),
            Err(IntegrityBaselineStoreError::Malformed)
        ));

        let mut unsupported = valid.clone();
        unsupported[MAGIC.len()..MAGIC.len() + 2].copy_from_slice(&2u16.to_le_bytes());
        fs::write(&store, unsupported).expect("unsupported version");
        assert!(matches!(
            IntegrityBaselineStore::load(&store),
            Err(IntegrityBaselineStoreError::UnsupportedVersion)
        ));

        let mut tampered = valid;
        let last = tampered.last_mut().expect("non-empty store");
        *last = b'z';
        fs::write(&store, tampered).expect("tamper digest");
        assert!(matches!(
            IntegrityBaselineStore::load(&store),
            Err(IntegrityBaselineStoreError::Malformed)
        ));
    }

    #[test]
    fn phase_18u_storage_private_policy_removes_only_regular_owned_store() {
        let fixture = tempdir().expect("fixture");
        let store = fixture.path().join("private").join("baseline-v1");
        let expected = baseline(fixture.path().join("watched"));
        IntegrityBaselineStore::persist(&store, &expected, IntegrityBaselineStoragePolicy::Persist)
            .expect("persist");
        assert_eq!(
            IntegrityBaselineStore::persist(
                &store,
                &expected,
                IntegrityBaselineStoragePolicy::SuppressPrivateState
            )
            .expect("suppress"),
            IntegrityBaselineStoreOutcome::Suppressed
        );
        assert!(!store.exists());
    }
}
