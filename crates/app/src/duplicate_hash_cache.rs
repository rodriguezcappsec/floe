//! Private, fingerprint-validated derived SHA-256 cache for duplicate discovery.

use std::{
    collections::HashMap,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Component, Path, PathBuf},
};

use rustix::fs::{FileType, Mode, OFlags};
use thiserror::Error;

const MAGIC: &[u8; 9] = b"FLOEDHC01";
const ENTRY_CAPACITY: usize = 200_000;
const EVICTION_BATCH: usize = ENTRY_CAPACITY / 10;
const ENCODED_CAPACITY: u64 = 64 * 1024 * 1024;
const PATH_CAPACITY: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FileStamp {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

impl FileStamp {
    pub(crate) fn current(path: &Path) -> Result<Self, DuplicateHashCacheError> {
        let metadata = fs::symlink_metadata(path).map_err(DuplicateHashCacheError::Io)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(DuplicateHashCacheError::UnsafeSource);
        }
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CacheEntry {
    stamp: FileStamp,
    digest: [u8; 32],
    last_used: u64,
}

#[derive(Debug, Default)]
pub struct DuplicateHashCache {
    entries: HashMap<PathBuf, CacheEntry>,
    tick: u64,
    dirty: bool,
}

impl DuplicateHashCache {
    pub fn source_stamp(path: &Path) -> Result<FileStamp, DuplicateHashCacheError> {
        validate_source_path(path)?;
        FileStamp::current(path)
    }

    pub fn rebuilding_empty() -> Self {
        Self {
            dirty: true,
            ..Self::default()
        }
    }

    pub fn load(path: &Path) -> Result<Self, DuplicateHashCacheError> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(DuplicateHashCacheError::Io(error)),
        };
        validate_private_file(path, &metadata)?;
        if metadata.len() > ENCODED_CAPACITY {
            return Err(DuplicateHashCacheError::Capacity);
        }
        let descriptor = rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::empty(),
        )
        .map_err(|error| DuplicateHashCacheError::Io(io::Error::from(error)))?;
        let stat = rustix::fs::fstat(&descriptor)
            .map_err(|error| DuplicateHashCacheError::Io(io::Error::from(error)))?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
            || stat.st_mode & 0o077 != 0
            || stat.st_uid != rustix::process::getuid().as_raw()
            || stat.st_dev != metadata.dev()
            || stat.st_ino != metadata.ino()
        {
            return Err(DuplicateHashCacheError::UnsafeStorage);
        }
        let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
        File::from(descriptor)
            .read_to_end(&mut bytes)
            .map_err(DuplicateHashCacheError::Io)?;
        Self::decode(&bytes)
    }

    pub fn lookup(&mut self, path: &Path) -> Option<[u8; 32]> {
        let current = match FileStamp::current(path) {
            Ok(current) => current,
            Err(_) => {
                self.dirty |= self.entries.remove(path).is_some();
                return None;
            }
        };
        let entry = self.entries.get_mut(path)?;
        if entry.stamp != current {
            self.entries.remove(path);
            self.dirty = true;
            return None;
        }
        self.tick = self.tick.wrapping_add(1).max(1);
        entry.last_used = self.tick;
        Some(entry.digest)
    }

    #[cfg(test)]
    pub fn insert_current(
        &mut self,
        path: PathBuf,
        digest: [u8; 32],
    ) -> Result<(), DuplicateHashCacheError> {
        let stamp = FileStamp::current(&path)?;
        self.insert_with_stamp(path, digest, stamp)?;
        Ok(())
    }

    pub fn insert_if_unchanged(
        &mut self,
        path: PathBuf,
        digest: [u8; 32],
        expected: FileStamp,
    ) -> Result<bool, DuplicateHashCacheError> {
        validate_source_path(&path)?;
        let current = FileStamp::current(&path)?;
        if current != expected {
            self.dirty |= self.entries.remove(&path).is_some();
            return Ok(false);
        }
        self.insert_with_stamp(path, digest, current)?;
        Ok(true)
    }

    fn insert_with_stamp(
        &mut self,
        path: PathBuf,
        digest: [u8; 32],
        stamp: FileStamp,
    ) -> Result<(), DuplicateHashCacheError> {
        validate_source_path(&path)?;
        self.tick = self.tick.wrapping_add(1).max(1);
        if !self.entries.contains_key(&path) && self.entries.len() >= ENTRY_CAPACITY {
            let mut oldest = self
                .entries
                .iter()
                .map(|(path, entry)| (entry.last_used, path.clone()))
                .collect::<Vec<_>>();
            oldest.sort_unstable_by(|(left_used, left_path), (right_used, right_path)| {
                left_used
                    .cmp(right_used)
                    .then_with(|| left_path.cmp(right_path))
            });
            for (_, old_path) in oldest.into_iter().take(EVICTION_BATCH.max(1)) {
                self.entries.remove(&old_path);
            }
        }
        self.entries.insert(
            path,
            CacheEntry {
                stamp,
                digest,
                last_used: self.tick,
            },
        );
        self.dirty = true;
        Ok(())
    }

    pub fn invalidate_paths(&mut self, paths: &[PathBuf]) -> usize {
        if paths.is_empty() {
            return 0;
        }
        let before = self.entries.len();
        self.entries
            .retain(|cached, _| !paths.iter().any(|changed| cached.starts_with(changed)));
        let removed = before.saturating_sub(self.entries.len());
        self.dirty |= removed > 0;
        removed
    }

    pub fn clear(&mut self) {
        self.dirty |= !self.entries.is_empty();
        self.entries.clear();
    }

    pub fn persist(&mut self, path: &Path) -> Result<(), DuplicateHashCacheError> {
        if !self.dirty {
            return Ok(());
        }
        let parent = path
            .parent()
            .ok_or(DuplicateHashCacheError::UnsafeStorage)?;
        create_private_parent(parent)?;
        let bytes = self.encode()?;
        let temporary = path.with_extension(format!(
            "tmp-{}-{}",
            std::process::id(),
            self.tick.wrapping_add(1)
        ));
        let result = (|| {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true).mode(0o600);
            let mut file = options
                .open(&temporary)
                .map_err(DuplicateHashCacheError::Io)?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(DuplicateHashCacheError::Io)?;
            fs::rename(&temporary, path).map_err(DuplicateHashCacheError::Io)?;
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(DuplicateHashCacheError::Io)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        } else {
            self.dirty = false;
        }
        result
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    fn encode(&self) -> Result<Vec<u8>, DuplicateHashCacheError> {
        let count =
            u32::try_from(self.entries.len()).map_err(|_| DuplicateHashCacheError::Capacity)?;
        let mut ordered = self.entries.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|(path, _)| *path);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&count.to_le_bytes());
        for (path, entry) in ordered {
            let raw = path.as_os_str().as_bytes();
            if raw.is_empty() || raw.len() > PATH_CAPACITY {
                return Err(DuplicateHashCacheError::Capacity);
            }
            let length = u32::try_from(raw.len()).map_err(|_| DuplicateHashCacheError::Capacity)?;
            bytes.extend_from_slice(&length.to_le_bytes());
            bytes.extend_from_slice(raw);
            bytes.extend_from_slice(&entry.stamp.device.to_le_bytes());
            bytes.extend_from_slice(&entry.stamp.inode.to_le_bytes());
            bytes.extend_from_slice(&entry.stamp.size.to_le_bytes());
            bytes.extend_from_slice(&entry.stamp.modified_seconds.to_le_bytes());
            bytes.extend_from_slice(&entry.stamp.modified_nanoseconds.to_le_bytes());
            bytes.extend_from_slice(&entry.stamp.changed_seconds.to_le_bytes());
            bytes.extend_from_slice(&entry.stamp.changed_nanoseconds.to_le_bytes());
            bytes.extend_from_slice(&entry.digest);
            bytes.extend_from_slice(&entry.last_used.to_le_bytes());
            if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > ENCODED_CAPACITY {
                return Err(DuplicateHashCacheError::Capacity);
            }
        }
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self, DuplicateHashCacheError> {
        let mut cursor = Cursor::new(bytes);
        if cursor.take(MAGIC.len())? != MAGIC {
            return Err(DuplicateHashCacheError::Corrupt);
        }
        let count =
            usize::try_from(cursor.u32()?).map_err(|_| DuplicateHashCacheError::Capacity)?;
        if count > ENTRY_CAPACITY {
            return Err(DuplicateHashCacheError::Capacity);
        }
        let mut cache = Self::default();
        for _ in 0..count {
            let path_length =
                usize::try_from(cursor.u32()?).map_err(|_| DuplicateHashCacheError::Capacity)?;
            if path_length == 0 || path_length > PATH_CAPACITY {
                return Err(DuplicateHashCacheError::Capacity);
            }
            let path = PathBuf::from(OsString::from_vec(cursor.take(path_length)?.to_vec()));
            validate_source_path(&path)?;
            let stamp = FileStamp {
                device: cursor.u64()?,
                inode: cursor.u64()?,
                size: cursor.u64()?,
                modified_seconds: cursor.i64()?,
                modified_nanoseconds: cursor.i64()?,
                changed_seconds: cursor.i64()?,
                changed_nanoseconds: cursor.i64()?,
            };
            let digest: [u8; 32] = cursor
                .take(32)?
                .try_into()
                .map_err(|_| DuplicateHashCacheError::Corrupt)?;
            let last_used = cursor.u64()?;
            if cache
                .entries
                .insert(
                    path,
                    CacheEntry {
                        stamp,
                        digest,
                        last_used,
                    },
                )
                .is_some()
            {
                return Err(DuplicateHashCacheError::Corrupt);
            }
            cache.tick = cache.tick.max(last_used);
        }
        if !cursor.is_empty() {
            return Err(DuplicateHashCacheError::Corrupt);
        }
        Ok(cache)
    }
}

#[derive(Debug, Error)]
pub enum DuplicateHashCacheError {
    #[error("duplicate hash cache I/O failed: {0}")]
    Io(io::Error),
    #[error("duplicate hash cache is corrupt")]
    Corrupt,
    #[error("duplicate hash cache exceeds its capacity")]
    Capacity,
    #[error("duplicate hash cache storage is not private and regular")]
    UnsafeStorage,
    #[error("duplicate hash source path is unsafe or not a regular file")]
    UnsafeSource,
}

fn validate_source_path(path: &Path) -> Result<(), DuplicateHashCacheError> {
    if !path.is_absolute()
        || path.file_name().is_none()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(DuplicateHashCacheError::UnsafeSource);
    }
    Ok(())
}

fn validate_private_file(
    path: &Path,
    metadata: &fs::Metadata,
) -> Result<(), DuplicateHashCacheError> {
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != rustix::process::getuid().as_raw()
    {
        return Err(DuplicateHashCacheError::UnsafeStorage);
    }
    let parent = path
        .parent()
        .ok_or(DuplicateHashCacheError::UnsafeStorage)?;
    validate_private_directory(parent)
}

fn create_private_parent(path: &Path) -> Result<(), DuplicateHashCacheError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder.create(path).map_err(DuplicateHashCacheError::Io)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(DuplicateHashCacheError::Io)?;
    validate_private_directory(path)
}

fn validate_private_directory(path: &Path) -> Result<(), DuplicateHashCacheError> {
    let metadata = fs::symlink_metadata(path).map_err(DuplicateHashCacheError::Io)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != rustix::process::getuid().as_raw()
    {
        return Err(DuplicateHashCacheError::UnsafeStorage);
    }
    Ok(())
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], DuplicateHashCacheError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(DuplicateHashCacheError::Corrupt)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(DuplicateHashCacheError::Corrupt)?;
        self.offset = end;
        Ok(value)
    }

    fn u32(&mut self) -> Result<u32, DuplicateHashCacheError> {
        Ok(u32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| DuplicateHashCacheError::Corrupt)?,
        ))
    }

    fn u64(&mut self) -> Result<u64, DuplicateHashCacheError> {
        Ok(u64::from_le_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| DuplicateHashCacheError::Corrupt)?,
        ))
    }

    fn i64(&mut self) -> Result<i64, DuplicateHashCacheError> {
        Ok(i64::from_le_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| DuplicateHashCacheError::Corrupt)?,
        ))
    }

    fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use std::{os::unix::fs::symlink, thread, time::Duration};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn phase_13g3_hash_cache_round_trips_privately_atomically_and_preserves_raw_paths() {
        let fixture = tempdir().expect("fixture");
        let raw = OsString::from_vec(vec![b'f', b'i', b'l', b'e', 0xff]);
        let source = fixture.path().join(raw);
        fs::write(&source, b"content").expect("source");
        let cache_path = fixture.path().join("private/duplicate-hashes-v1");
        let digest = [7_u8; 32];
        let mut cache = DuplicateHashCache::default();
        cache
            .insert_current(source.clone(), digest)
            .expect("insert");
        cache.persist(&cache_path).expect("persist");

        assert_eq!(
            fs::metadata(&cache_path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let mut restored = DuplicateHashCache::load(&cache_path).expect("load");
        assert_eq!(restored.lookup(&source), Some(digest));
        assert_eq!(restored.len(), 1);
        assert!(!cache_path.with_extension("tmp").exists());
    }

    #[test]
    fn phase_13g3_hash_cache_rejects_corrupt_insecure_and_symlinked_storage() {
        let fixture = tempdir().expect("fixture");
        let private = fixture.path().join("private");
        fs::create_dir(&private).expect("private");
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700)).expect("private mode");
        let cache_path = private.join("cache");
        fs::write(&cache_path, b"not a cache").expect("corrupt");
        fs::set_permissions(&cache_path, fs::Permissions::from_mode(0o600)).expect("cache mode");
        assert!(matches!(
            DuplicateHashCache::load(&cache_path),
            Err(DuplicateHashCacheError::Corrupt)
        ));

        fs::set_permissions(&cache_path, fs::Permissions::from_mode(0o644)).expect("insecure");
        assert!(matches!(
            DuplicateHashCache::load(&cache_path),
            Err(DuplicateHashCacheError::UnsafeStorage)
        ));
        fs::remove_file(&cache_path).expect("remove");
        let target = private.join("target");
        fs::write(&target, MAGIC).expect("target");
        symlink(&target, &cache_path).expect("symlink");
        assert!(matches!(
            DuplicateHashCache::load(&cache_path),
            Err(DuplicateHashCacheError::UnsafeStorage)
        ));
    }

    #[test]
    fn phase_13g3_incremental_cache_reuses_unchanged_and_invalidates_changes_and_subtrees() {
        let fixture = tempdir().expect("fixture");
        let folder = fixture.path().join("folder");
        fs::create_dir(&folder).expect("folder");
        let first = folder.join("first");
        let second = folder.join("second");
        fs::write(&first, b"one").expect("first");
        fs::write(&second, b"two").expect("second");
        let mut cache = DuplicateHashCache::default();
        cache.insert_current(first.clone(), [1; 32]).expect("first");
        cache
            .insert_current(second.clone(), [2; 32])
            .expect("second");
        assert_eq!(cache.lookup(&first), Some([1; 32]));

        thread::sleep(Duration::from_millis(2));
        fs::write(&first, b"changed").expect("change");
        assert_eq!(cache.lookup(&first), None);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.invalidate_paths(&[folder]), 1);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn phase_13g3_hash_cache_never_attaches_an_old_digest_to_a_replaced_file() {
        let fixture = tempdir().expect("fixture");
        let source = fixture.path().join("source");
        let replacement = fixture.path().join("replacement");
        fs::write(&source, b"old bytes").expect("source");
        fs::write(&replacement, b"new bytes").expect("replacement");
        let stamp_before = DuplicateHashCache::source_stamp(&source).expect("initial stamp");

        fs::rename(&replacement, &source).expect("replace source");
        let mut cache = DuplicateHashCache::default();
        assert!(
            !cache
                .insert_if_unchanged(source.clone(), [9; 32], stamp_before)
                .expect("reject changed source")
        );
        assert_eq!(cache.lookup(&source), None);
        assert_eq!(cache.len(), 0);
    }
}
