//! Private, versioned persistence for Floe-owned integrity fingerprints.
//!
//! This deliberately stores raw Unix path bytes rather than display strings.  A malformed,
//! insecure, or unsupported file is an error: callers must never treat it as an empty store.

use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use thiserror::Error;

use crate::integrity::SavedFingerprint;

const MAGIC: &[u8; 8] = b"FLOEFPNT";
const VERSION: u16 = 1;
const MAX_RECORDS: usize = 2_048;
const MAX_PATH_BYTES: usize = 4 * 1024;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FingerprintStore {
    records: HashMap<PathBuf, SavedFingerprint>,
}

impl FingerprintStore {
    pub fn load(path: &Path) -> Result<Self, FingerprintStoreError> {
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => {
                return Err(FingerprintStoreError::Io {
                    operation: "inspect",
                    source: error,
                });
            }
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(FingerprintStoreError::UnsafeFile(path.to_path_buf()));
                }
                if metadata.len() > MAX_FILE_BYTES {
                    return Err(FingerprintStoreError::FileTooLarge(metadata.len()));
                }
                if metadata.permissions().mode() & 0o077 != 0 {
                    return Err(FingerprintStoreError::InsecurePermissions(
                        path.to_path_buf(),
                    ));
                }
            }
        }

        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as _)
            .open(path)
            .map_err(|source| FingerprintStoreError::Io {
                operation: "open",
                source,
            })?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|source| FingerprintStoreError::Io {
                operation: "read",
                source,
            })?;
        decode(&bytes)
    }

    pub fn get(&self, path: &Path) -> Option<&SavedFingerprint> {
        self.records.get(path)
    }

    pub fn insert(&mut self, fingerprint: SavedFingerprint) -> Result<(), FingerprintStoreError> {
        if self.records.len() >= MAX_RECORDS && !self.records.contains_key(fingerprint.path()) {
            return Err(FingerprintStoreError::TooManyRecords);
        }
        self.records
            .insert(fingerprint.path().to_path_buf(), fingerprint);
        Ok(())
    }

    pub fn persist(&self, path: &Path) -> Result<(), FingerprintStoreError> {
        let parent = path
            .parent()
            .ok_or_else(|| FingerprintStoreError::InvalidPath(path.to_path_buf()))?;
        ensure_private_directory(parent)?;
        if let Ok(metadata) = fs::symlink_metadata(path) {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(FingerprintStoreError::UnsafeFile(path.to_path_buf()));
            }
        }
        let bytes = encode(self)?;
        atomic_write(path, &bytes)
    }
}

#[derive(Debug, Error)]
pub enum FingerprintStoreError {
    #[error("fingerprint store path is invalid: {0:?}")]
    InvalidPath(PathBuf),
    #[error("fingerprint store file is not a regular non-symlink file: {0:?}")]
    UnsafeFile(PathBuf),
    #[error("fingerprint store permissions are not private: {0:?}")]
    InsecurePermissions(PathBuf),
    #[error("fingerprint store contains unsupported version {0}")]
    UnsupportedVersion(u16),
    #[error("fingerprint store format is malformed")]
    Malformed,
    #[error("fingerprint store has trailing data")]
    TrailingData,
    #[error("fingerprint store declares too many records")]
    TooManyRecords,
    #[error("fingerprint path exceeds the bounded persistence format")]
    PathTooLong,
    #[error("fingerprint store exceeds its bounded size ({0} bytes)")]
    FileTooLarge(u64),
    #[error("could not {operation} fingerprint store")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
}

fn encode(store: &FingerprintStore) -> Result<Vec<u8>, FingerprintStoreError> {
    if store.records.len() > MAX_RECORDS {
        return Err(FingerprintStoreError::TooManyRecords);
    }
    let mut records: Vec<_> = store.records.values().collect();
    records.sort_by(|left, right| {
        left.path()
            .as_os_str()
            .as_bytes()
            .cmp(right.path().as_os_str().as_bytes())
    });
    let mut output = Vec::with_capacity(16 + records.len() * 128);
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&VERSION.to_le_bytes());
    output.extend_from_slice(
        &(u32::try_from(records.len()).map_err(|_| FingerprintStoreError::TooManyRecords)?)
            .to_le_bytes(),
    );
    for fingerprint in records {
        let raw_path = fingerprint.path().as_os_str().as_bytes();
        if raw_path.len() > MAX_PATH_BYTES {
            return Err(FingerprintStoreError::PathTooLong);
        }
        let record = fingerprint.encode_record();
        output.extend_from_slice(
            &(u32::try_from(record.len()).map_err(|_| FingerprintStoreError::Malformed)?)
                .to_le_bytes(),
        );
        output.extend_from_slice(&record);
    }
    Ok(output)
}

fn decode(bytes: &[u8]) -> Result<FingerprintStore, FingerprintStoreError> {
    let mut cursor = Cursor::new(bytes);
    if cursor.take(MAGIC.len())? != MAGIC {
        return Err(FingerprintStoreError::Malformed);
    }
    if cursor.u16()? != VERSION {
        return Err(FingerprintStoreError::UnsupportedVersion(cursor.last_u16));
    }
    let count =
        usize::try_from(cursor.u32()?).map_err(|_| FingerprintStoreError::TooManyRecords)?;
    if count > MAX_RECORDS {
        return Err(FingerprintStoreError::TooManyRecords);
    }
    let mut records = HashMap::with_capacity(count);
    for _ in 0..count {
        let record_length =
            usize::try_from(cursor.u32()?).map_err(|_| FingerprintStoreError::Malformed)?;
        if record_length > MAX_PATH_BYTES.saturating_add(256) {
            return Err(FingerprintStoreError::Malformed);
        }
        let fingerprint = SavedFingerprint::decode_record(cursor.take(record_length)?)
            .map_err(|_| FingerprintStoreError::Malformed)?;
        let path = fingerprint.path().to_path_buf();
        if records.insert(path, fingerprint).is_some() {
            return Err(FingerprintStoreError::Malformed);
        }
    }
    if !cursor.remaining().is_empty() {
        return Err(FingerprintStoreError::TrailingData);
    }
    Ok(FingerprintStore { records })
}

fn ensure_private_directory(path: &Path) -> Result<(), FingerprintStoreError> {
    fs::create_dir_all(path).map_err(|source| FingerprintStoreError::Io {
        operation: "create directory",
        source,
    })?;
    let metadata = fs::symlink_metadata(path).map_err(|source| FingerprintStoreError::Io {
        operation: "inspect directory",
        source,
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(FingerprintStoreError::UnsafeFile(path.to_path_buf()));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
        FingerprintStoreError::Io {
            operation: "secure directory",
            source,
        }
    })
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), FingerprintStoreError> {
    let parent = path
        .parent()
        .ok_or_else(|| FingerprintStoreError::InvalidPath(path.to_path_buf()))?;
    let name = path
        .file_name()
        .ok_or_else(|| FingerprintStoreError::InvalidPath(path.to_path_buf()))?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        name.to_string_lossy(),
        TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|source| FingerprintStoreError::Io {
            operation: "create temporary store",
            source,
        })?;
    let result = (|| {
        file.write_all(bytes)
            .map_err(|source| FingerprintStoreError::Io {
                operation: "write temporary store",
                source,
            })?;
        file.sync_all()
            .map_err(|source| FingerprintStoreError::Io {
                operation: "sync temporary store",
                source,
            })?;
        fs::rename(&temporary, path).map_err(|source| FingerprintStoreError::Io {
            operation: "replace store",
            source,
        })?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|source| FingerprintStoreError::Io {
                operation: "sync store directory",
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
    last_u16: u16,
}
impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            offset: 0,
            last_u16: 0,
        }
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8], FingerprintStoreError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(FingerprintStoreError::Malformed)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(FingerprintStoreError::Malformed)?;
        self.offset = end;
        Ok(value)
    }
    fn u16(&mut self) -> Result<u16, FingerprintStoreError> {
        let value = u16::from_le_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| FingerprintStoreError::Malformed)?,
        );
        self.last_u16 = value;
        Ok(value)
    }
    fn u32(&mut self) -> Result<u32, FingerprintStoreError> {
        Ok(u32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| FingerprintStoreError::Malformed)?,
        ))
    }
    fn remaining(&self) -> &'a [u8] {
        &self.bytes[self.offset..]
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::{
            ffi::OsStringExt,
            fs::{PermissionsExt, symlink},
        },
    };

    use tempfile::tempdir;

    use super::*;
    use crate::integrity::save_fingerprint;

    #[test]
    fn phase_18t_fingerprint_store_round_trips_raw_paths_atomically_and_privately() {
        let fixture = tempdir().expect("temporary root");
        let raw_name = std::ffi::OsString::from_vec(b"\xff-document".to_vec());
        let target = fixture.path().join(raw_name);
        fs::write(&target, b"integrity fixture").expect("fixture");
        let fingerprint =
            save_fingerprint(target.clone(), || false, |_, _| {}).expect("fingerprint");
        let store_path = fixture.path().join("private").join("fingerprints.bin");
        let mut store = FingerprintStore::default();
        store.insert(fingerprint).expect("record");
        store.persist(&store_path).expect("atomic write");

        assert_eq!(
            fs::metadata(store_path.parent().expect("parent"))
                .expect("parent metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&store_path)
                .expect("store metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(
            FingerprintStore::load(&store_path)
                .expect("reload")
                .get(&target)
                .is_some()
        );
        assert_eq!(
            fs::read_dir(store_path.parent().expect("parent"))
                .expect("private directory")
                .count(),
            1
        );
    }

    #[test]
    fn phase_18t_fingerprint_store_rejects_corruption_insecurity_and_symlinks() {
        let fixture = tempdir().expect("temporary root");
        let path = fixture.path().join("fingerprints.bin");
        fs::write(&path, b"not a Floe fingerprint store").expect("corrupt fixture");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("private fixture");
        assert!(matches!(
            FingerprintStore::load(&path),
            Err(FingerprintStoreError::Malformed)
        ));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("insecure fixture");
        assert!(matches!(
            FingerprintStore::load(&path),
            Err(FingerprintStoreError::InsecurePermissions(_))
        ));

        let target = fixture.path().join("target");
        fs::write(&target, b"fixture").expect("target");
        let link = fixture.path().join("link");
        symlink(&target, &link).expect("symlink");
        assert!(matches!(
            FingerprintStore::load(&link),
            Err(FingerprintStoreError::UnsafeFile(_))
        ));
    }
}
