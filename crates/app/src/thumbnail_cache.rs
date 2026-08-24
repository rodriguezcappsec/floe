use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{self, BufReader, Cursor, Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use glib::{Checksum, ChecksumType};
use image::RgbaImage;
use rustix::fs::{Mode, OFlags};
use thiserror::Error;

use crate::thumbnail::ThumbnailKey;

const SOFTWARE: &str = "Floe";
const CLEANUP_INTERVAL_WRITES: usize = 64;
pub(crate) const MAX_CACHE_FILE_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const MAX_OWNED_ENTRIES: usize = 2_048;
pub(crate) const MAX_TOTAL_OWNED_BYTES: u64 = 256 * 1024 * 1024;
pub(crate) const MAX_OWNED_AGE: Duration = Duration::from_secs(90 * 24 * 60 * 60);
static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CacheTier {
    Normal,
    Large,
}

impl CacheTier {
    pub(crate) const fn from_edge(edge: u16) -> Self {
        if edge <= 128 {
            Self::Normal
        } else {
            Self::Large
        }
    }

    const fn directory_name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Large => "large",
        }
    }

    pub(crate) const fn edge(self) -> u32 {
        match self {
            Self::Normal => 128,
            Self::Large => 256,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct CacheLimits {
    max_entries: usize,
    max_bytes: u64,
    max_age: Duration,
}

impl Default for CacheLimits {
    fn default() -> Self {
        Self {
            max_entries: MAX_OWNED_ENTRIES,
            max_bytes: MAX_TOTAL_OWNED_BYTES,
            max_age: MAX_OWNED_AGE,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ThumbnailCacheConfig {
    cache_home: PathBuf,
    limits: CacheLimits,
}

impl ThumbnailCacheConfig {
    pub(crate) fn from_environment() -> Option<Self> {
        let xdg = env::var_os("XDG_CACHE_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .filter(|path| path.is_absolute());
        let cache_home = xdg.or_else(|| {
            env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|home| home.join(".cache"))
        })?;
        Some(Self {
            cache_home,
            limits: CacheLimits::default(),
        })
    }

    #[cfg(test)]
    pub(crate) fn for_test(cache_home: PathBuf) -> Self {
        Self {
            cache_home,
            limits: CacheLimits::default(),
        }
    }

    #[cfg(test)]
    fn with_limits(mut self, max_entries: usize, max_bytes: u64, max_age: Duration) -> Self {
        self.limits = CacheLimits {
            max_entries,
            max_bytes,
            max_age,
        };
        self
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ThumbnailCacheKey {
    canonical_uri: String,
    md5: String,
    tier: CacheTier,
    source_size: u64,
    modified_seconds: u64,
    modified_nanoseconds: u32,
}

impl ThumbnailCacheKey {
    fn from_thumbnail(key: &ThumbnailKey) -> Result<Self, ThumbnailCacheError> {
        let absolute_path = if key.path().is_absolute() {
            key.path().to_path_buf()
        } else {
            env::current_dir()?.join(key.path())
        };
        let canonical_uri = glib::filename_to_uri(&absolute_path, None)
            .map_err(ThumbnailCacheError::CanonicalUri)?
            .to_string();
        let mut checksum =
            Checksum::new(ChecksumType::Md5).ok_or(ThumbnailCacheError::DigestUnavailable)?;
        checksum.update(canonical_uri.as_bytes());
        let md5 = checksum
            .string()
            .ok_or(ThumbnailCacheError::DigestUnavailable)?;
        let modified = key
            .modified()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ThumbnailCacheError::InvalidSourceTime)?;
        Ok(Self {
            canonical_uri,
            md5,
            tier: CacheTier::from_edge(key.edge()),
            source_size: key.size(),
            modified_seconds: modified.as_secs(),
            modified_nanoseconds: modified.subsec_nanos(),
        })
    }
}

#[derive(Debug, Error)]
pub(crate) enum ThumbnailCacheError {
    #[error("thumbnail cache I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("could not form a canonical file URI: {0}")]
    CanonicalUri(glib::Error),
    #[error("MD5 checksum support is unavailable")]
    DigestUnavailable,
    #[error("source modification time predates the Unix epoch")]
    InvalidSourceTime,
}

pub(crate) struct ThumbnailCache {
    config: ThumbnailCacheConfig,
    writes_since_cleanup: usize,
    owned_entries: usize,
    owned_bytes: u64,
}

impl ThumbnailCache {
    pub(crate) fn new(config: ThumbnailCacheConfig) -> Self {
        Self {
            config,
            writes_since_cleanup: 0,
            owned_entries: 0,
            owned_bytes: 0,
        }
    }

    pub(crate) fn initialize(&mut self) -> Result<(), ThumbnailCacheError> {
        let (owned_entries, owned_bytes) = self.cleanup_at(SystemTime::now())?;
        self.owned_entries = owned_entries;
        self.owned_bytes = owned_bytes;
        Ok(())
    }

    pub(crate) fn load(
        &mut self,
        key: &ThumbnailKey,
    ) -> Result<Option<RgbaImage>, ThumbnailCacheError> {
        let cache_key = ThumbnailCacheKey::from_thumbnail(key)?;
        let path = self.thumbnail_path(&cache_key);
        let Some(cached) = read_valid_thumbnail(&path, &cache_key)? else {
            return Ok(None);
        };
        if cached.floe_owned {
            let marker_path = self.marker_path(&cache_key);
            let marker_existed = marker_path
                .symlink_metadata()
                .is_ok_and(|metadata| metadata.file_type().is_file());
            if self.write_ownership_marker(&cache_key).is_ok() && !marker_existed {
                self.owned_entries = self.owned_entries.saturating_add(1);
                self.owned_bytes = self
                    .owned_bytes
                    .saturating_add(path.metadata().map_or(0, |metadata| metadata.len()));
                if self.owned_entries > self.config.limits.max_entries
                    || self.owned_bytes > self.config.limits.max_bytes
                {
                    let (owned_entries, owned_bytes) = self.cleanup_at(SystemTime::now())?;
                    self.owned_entries = owned_entries;
                    self.owned_bytes = owned_bytes;
                }
            }
        }
        Ok(Some(cached.image))
    }

    pub(crate) fn store(
        &mut self,
        key: &ThumbnailKey,
        image: &RgbaImage,
    ) -> Result<(), ThumbnailCacheError> {
        let cache_key = ThumbnailCacheKey::from_thumbnail(key)?;
        let thumbnail_directory = self.thumbnail_directory(cache_key.tier);
        ensure_private_directory(&self.config.cache_home.join("thumbnails"))?;
        ensure_private_directory(&thumbnail_directory)?;
        let path = self.thumbnail_path(&cache_key);
        let marker_path = self.marker_path(&cache_key);
        let was_owned = marker_path
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_file())
            && read_software(&path).as_deref() == Some(SOFTWARE);
        let previous_bytes = if was_owned {
            path.metadata().ok().map_or(0, |metadata| metadata.len())
        } else {
            0
        };
        write_thumbnail_atomically(&path, image, &cache_key)?;
        self.write_ownership_marker(&cache_key)?;

        if !was_owned {
            self.owned_entries = self.owned_entries.saturating_add(1);
        }
        let current_bytes = path.metadata().map_or(0, |metadata| metadata.len());
        self.owned_bytes = self
            .owned_bytes
            .saturating_sub(previous_bytes)
            .saturating_add(current_bytes);
        self.writes_since_cleanup += 1;
        if self.writes_since_cleanup >= CLEANUP_INTERVAL_WRITES
            || self.owned_entries > self.config.limits.max_entries
            || self.owned_bytes > self.config.limits.max_bytes
        {
            let (owned_entries, owned_bytes) = self.cleanup_at(SystemTime::now())?;
            self.owned_entries = owned_entries;
            self.owned_bytes = owned_bytes;
            self.writes_since_cleanup = 0;
        }
        Ok(())
    }

    fn thumbnail_directory(&self, tier: CacheTier) -> PathBuf {
        self.config
            .cache_home
            .join("thumbnails")
            .join(tier.directory_name())
    }

    fn ownership_directory(&self, tier: CacheTier) -> PathBuf {
        self.config
            .cache_home
            .join("floe")
            .join("thumbnail-ownership")
            .join(tier.directory_name())
    }

    fn thumbnail_path(&self, key: &ThumbnailCacheKey) -> PathBuf {
        self.thumbnail_directory(key.tier)
            .join(format!("{}.png", key.md5))
    }

    fn marker_path(&self, key: &ThumbnailCacheKey) -> PathBuf {
        self.ownership_directory(key.tier).join(&key.md5)
    }

    fn write_ownership_marker(&self, key: &ThumbnailCacheKey) -> Result<(), ThumbnailCacheError> {
        let floe_directory = self.config.cache_home.join("floe");
        let ownership_root = floe_directory.join("thumbnail-ownership");
        let ownership_directory = self.ownership_directory(key.tier);
        ensure_private_directory(&floe_directory)?;
        ensure_private_directory(&ownership_root)?;
        ensure_private_directory(&ownership_directory)?;
        write_bytes_atomically(&self.marker_path(key), b"Floe thumbnail ownership\n")?;
        Ok(())
    }

    fn cleanup_at(&self, now: SystemTime) -> Result<(usize, u64), ThumbnailCacheError> {
        let mut owned = Vec::new();
        for tier in [CacheTier::Normal, CacheTier::Large] {
            self.collect_owned_tier_at(tier, now, &mut owned)?;
        }
        owned.sort_by_key(|entry| entry.modified);
        let mut total_bytes = owned.iter().map(|entry| entry.bytes).sum::<u64>();
        let mut total_entries = owned.len();
        for entry in owned {
            if total_entries <= self.config.limits.max_entries
                && total_bytes <= self.config.limits.max_bytes
            {
                break;
            }
            remove_owned_pair(&entry.thumbnail_path, &entry.marker_path);
            total_entries = total_entries.saturating_sub(1);
            total_bytes = total_bytes.saturating_sub(entry.bytes);
        }
        Ok((total_entries, total_bytes))
    }

    fn collect_owned_tier_at(
        &self,
        tier: CacheTier,
        now: SystemTime,
        owned: &mut Vec<OwnedThumbnail>,
    ) -> Result<(), ThumbnailCacheError> {
        let marker_directory = self.ownership_directory(tier);
        let marker_entries = match fs::read_dir(&marker_directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        for entry in marker_entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    tracing::debug!(%error, "could not inspect thumbnail ownership marker");
                    continue;
                }
            };
            let marker_path = entry.path();
            let marker_metadata = match fs::symlink_metadata(&marker_path) {
                Ok(metadata) if metadata.file_type().is_file() => metadata,
                Ok(_) => continue,
                Err(_) => continue,
            };
            let Some(digest) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !is_md5_name(&digest) {
                continue;
            }
            let thumbnail_path = self.thumbnail_directory(tier).join(format!("{digest}.png"));
            let thumbnail_metadata = match fs::symlink_metadata(&thumbnail_path) {
                Ok(metadata) if metadata.file_type().is_file() => metadata,
                _ => {
                    let _ = fs::remove_file(&marker_path);
                    continue;
                }
            };
            if read_software(&thumbnail_path).as_deref() != Some(SOFTWARE) {
                let _ = fs::remove_file(&marker_path);
                continue;
            }
            let modified = marker_metadata
                .modified()
                .or_else(|_| thumbnail_metadata.modified())
                .unwrap_or(UNIX_EPOCH);
            let age = now.duration_since(modified).unwrap_or(Duration::ZERO);
            if age > self.config.limits.max_age {
                remove_owned_pair(&thumbnail_path, &marker_path);
                continue;
            }
            owned.push(OwnedThumbnail {
                thumbnail_path,
                marker_path,
                bytes: thumbnail_metadata.len(),
                modified,
            });
        }

        Ok(())
    }

    #[cfg(test)]
    fn paths_for_test(&self, key: &ThumbnailKey) -> (PathBuf, PathBuf) {
        let cache_key = ThumbnailCacheKey::from_thumbnail(key).expect("test key should be valid");
        (
            self.thumbnail_path(&cache_key),
            self.marker_path(&cache_key),
        )
    }
}

struct CachedThumbnail {
    image: RgbaImage,
    floe_owned: bool,
}

struct OwnedThumbnail {
    thumbnail_path: PathBuf,
    marker_path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

fn ensure_private_directory(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "thumbnail cache directory is not a directory",
        ));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

fn write_thumbnail_atomically(
    path: &Path,
    image: &RgbaImage,
    key: &ThumbnailCacheKey,
) -> Result<(), ThumbnailCacheError> {
    write_atomically(path, |file| {
        let mut encoder = png::Encoder::new(file, image.width(), image.height());
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .add_text_chunk("Thumb::URI".to_owned(), key.canonical_uri.clone())
            .map_err(|error| io::Error::other(error.to_string()))?;
        encoder
            .add_text_chunk("Thumb::MTime".to_owned(), key.modified_seconds.to_string())
            .map_err(|error| io::Error::other(error.to_string()))?;
        encoder
            .add_text_chunk("Thumb::Size".to_owned(), key.source_size.to_string())
            .map_err(|error| io::Error::other(error.to_string()))?;
        encoder
            .add_text_chunk(
                "Floe::MTimeNsec".to_owned(),
                key.modified_nanoseconds.to_string(),
            )
            .map_err(|error| io::Error::other(error.to_string()))?;
        encoder
            .add_text_chunk("Software".to_owned(), SOFTWARE.to_owned())
            .map_err(|error| io::Error::other(error.to_string()))?;
        let mut writer = encoder
            .write_header()
            .map_err(|error| io::Error::other(error.to_string()))?;
        writer
            .write_image_data(image.as_raw())
            .map_err(|error| io::Error::other(error.to_string()))
    })
    .map_err(ThumbnailCacheError::Io)
}

fn write_bytes_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomically(path, |file| file.write_all(bytes))
}

fn write_atomically(
    path: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "cache path has no parent"))?;
    let temporary_id = TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".floe-{}-{temporary_id}.temporary",
        std::process::id()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let mut file = options.open(&temporary)?;
    let result = (|| {
        write(&mut file)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn read_valid_thumbnail(
    path: &Path,
    key: &ThumbnailCacheKey,
) -> Result<Option<CachedThumbnail>, ThumbnailCacheError> {
    let bytes = match read_regular_file_bounded(path, MAX_CACHE_FILE_BYTES) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) if error.raw_os_error() == Some(rustix::io::Errno::LOOP.raw_os_error()) => {
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    let mut decoder = png::Decoder::new(Cursor::new(&bytes));
    decoder.set_limits(png::Limits {
        bytes: 4 * 1024 * 1024,
    });
    let mut reader = match decoder.read_info() {
        Ok(reader) => reader,
        Err(_) => return Ok(None),
    };
    let info = reader.info();
    let expected_mtime = key.modified_seconds.to_string();
    let expected_size = key.source_size.to_string();
    let expected_nanoseconds = key.modified_nanoseconds.to_string();
    let software = text_value(info, "Software");
    let floe_timestamp_matches = software.as_deref() != Some(SOFTWARE)
        || text_value(info, "Floe::MTimeNsec").as_deref() == Some(expected_nanoseconds.as_str());
    if info.bit_depth != png::BitDepth::Eight
        || info.interlaced
        || !matches!(
            info.color_type,
            png::ColorType::Rgba | png::ColorType::GrayscaleAlpha
        )
        || info.width == 0
        || info.height == 0
        || info.width > key.tier.edge()
        || info.height > key.tier.edge()
        || text_value(info, "Thumb::URI").as_deref() != Some(&key.canonical_uri)
        || text_value(info, "Thumb::MTime").as_deref() != Some(expected_mtime.as_str())
        || text_value(info, "Thumb::Size").as_deref() != Some(expected_size.as_str())
        || !floe_timestamp_matches
    {
        return Ok(None);
    }
    let floe_owned = software.as_deref() == Some(SOFTWARE);
    let Some(buffer_size) = reader.output_buffer_size() else {
        return Ok(None);
    };
    let mut decoded = vec![0; buffer_size];
    let output = match reader.next_frame(&mut decoded) {
        Ok(output) => output,
        Err(_) => return Ok(None),
    };
    decoded.truncate(output.buffer_size());
    let rgba = match output.color_type {
        png::ColorType::Rgba => decoded,
        png::ColorType::GrayscaleAlpha => decoded
            .chunks_exact(2)
            .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
            .collect(),
        _ => return Ok(None),
    };
    let Some(image) = RgbaImage::from_raw(output.width, output.height, rgba) else {
        return Ok(None);
    };
    Ok(Some(CachedThumbnail { image, floe_owned }))
}

fn read_regular_file_bounded(path: &Path, maximum: u64) -> io::Result<Vec<u8>> {
    let descriptor = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(io::Error::from)?;
    let mut file = File::from(descriptor);
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "cache entry is not a regular file",
        ));
    }
    if metadata.len() > maximum {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "cache entry exceeds its safety limit",
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    Read::by_ref(&mut file)
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "cache entry exceeds its safety limit",
        ));
    }
    Ok(bytes)
}

fn text_value(info: &png::Info<'_>, keyword: &str) -> Option<String> {
    info.uncompressed_latin1_text
        .iter()
        .find(|chunk| chunk.keyword == keyword)
        .map(|chunk| chunk.text.clone())
        .or_else(|| {
            info.compressed_latin1_text
                .iter()
                .find(|chunk| chunk.keyword == keyword)
                .and_then(|chunk| chunk.get_text().ok())
        })
        .or_else(|| {
            info.utf8_text
                .iter()
                .find(|chunk| chunk.keyword == keyword)
                .and_then(|chunk| chunk.get_text().ok())
        })
}

fn read_software(path: &Path) -> Option<String> {
    let bytes = read_regular_file_bounded(path, MAX_CACHE_FILE_BYTES).ok()?;
    let mut decoder = png::Decoder::new(BufReader::new(Cursor::new(bytes)));
    decoder.set_limits(png::Limits { bytes: 1024 * 1024 });
    let reader = decoder.read_info().ok()?;
    text_value(reader.info(), "Software")
}

fn is_md5_name(name: &str) -> bool {
    name.len() == 32 && name.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn remove_owned_pair(thumbnail_path: &Path, marker_path: &Path) {
    if read_software(thumbnail_path).as_deref() == Some(SOFTWARE) {
        let _ = fs::remove_file(thumbnail_path);
    }
    let _ = fs::remove_file(marker_path);
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::symlink},
    };

    use image::Rgba;
    use tempfile::tempdir;

    use super::*;
    use crate::thumbnail::{LIST_THUMBNAIL_EDGE, MAX_THUMBNAIL_EDGE};

    fn test_image() -> RgbaImage {
        RgbaImage::from_pixel(4, 2, Rgba([12, 34, 56, 255]))
    }

    fn write_custom_thumbnail(path: &Path, chunks: &[(&str, String)]) {
        ensure_private_directory(path.parent().expect("thumbnail should have a parent"))
            .expect("thumbnail directory should be created");
        let image = test_image();
        write_atomically(path, |file| {
            let mut encoder = png::Encoder::new(file, image.width(), image.height());
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            for (keyword, value) in chunks {
                encoder
                    .add_text_chunk((*keyword).to_owned(), value.clone())
                    .map_err(|error| io::Error::other(error.to_string()))?;
            }
            let mut writer = encoder
                .write_header()
                .map_err(|error| io::Error::other(error.to_string()))?;
            writer
                .write_image_data(image.as_raw())
                .map_err(|error| io::Error::other(error.to_string()))
        })
        .expect("custom thumbnail should be written");
    }

    #[cfg(unix)]
    #[test]
    fn phase_6e_non_utf8_uri_and_digest_identity_are_stable() {
        let directory = tempdir().expect("temporary directory should be created");
        let name = OsString::from_vec(vec![b'i', 0x80, b'.', b'p', b'n', b'g']);
        let key = ThumbnailKey::for_test(directory.path().join(name), 17);
        let first = ThumbnailCacheKey::from_thumbnail(&key).expect("key should form");
        let second = ThumbnailCacheKey::from_thumbnail(&key).expect("key should be stable");
        assert!(first.canonical_uri.starts_with("file:///"));
        assert!(first.canonical_uri.contains("%80"));
        assert_eq!(first.md5, second.md5);
        assert!(is_md5_name(&first.md5));
    }

    #[test]
    fn phase_6e_tier_mapping_covers_all_requested_edges() {
        assert_eq!(CacheTier::from_edge(LIST_THUMBNAIL_EDGE), CacheTier::Normal);
        assert_eq!(CacheTier::from_edge(128), CacheTier::Normal);
        assert_eq!(CacheTier::from_edge(129), CacheTier::Large);
        assert_eq!(CacheTier::from_edge(MAX_THUMBNAIL_EDGE), CacheTier::Large);
    }

    #[test]
    fn phase_6e_valid_cache_round_trip_and_metadata_invalidation() {
        let directory = tempdir().expect("temporary directory should be created");
        let mut cache = ThumbnailCache::new(ThumbnailCacheConfig::for_test(
            directory.path().to_path_buf(),
        ));
        let key = ThumbnailKey::for_test(directory.path().join("image.png"), 17);
        cache
            .store(&key, &test_image())
            .expect("cache write should succeed");
        let loaded = cache
            .load(&key)
            .expect("cache read should succeed")
            .expect("cache entry should match");
        assert_eq!(loaded.dimensions(), (4, 2));

        let changed_size = ThumbnailKey::for_test(key.path().to_path_buf(), 18);
        assert!(
            cache
                .load(&changed_size)
                .expect("invalidated read should not fail")
                .is_none()
        );
    }

    #[test]
    fn phase_6e_floe_cache_invalidates_same_second_subsecond_changes() {
        let directory = tempdir().expect("temporary directory should be created");
        let mut cache = ThumbnailCache::new(ThumbnailCacheConfig::for_test(
            directory.path().to_path_buf(),
        ));
        let path = directory.path().join("image.png");
        let first = ThumbnailKey::for_test_with_modified(
            path.clone(),
            17,
            UNIX_EPOCH + Duration::from_nanos(1),
            LIST_THUMBNAIL_EDGE,
        );
        cache
            .store(&first, &test_image())
            .expect("cache write should succeed");
        let changed = ThumbnailKey::for_test_with_modified(
            path,
            17,
            UNIX_EPOCH + Duration::from_nanos(2),
            LIST_THUMBNAIL_EDGE,
        );
        assert!(
            cache
                .load(&changed)
                .expect("subsecond invalidation should not fail")
                .is_none()
        );
    }

    #[test]
    fn phase_6e_uri_mtime_and_missing_metadata_are_invalidated() {
        let directory = tempdir().expect("temporary directory should be created");
        let mut cache = ThumbnailCache::new(ThumbnailCacheConfig::for_test(
            directory.path().to_path_buf(),
        ));
        let key = ThumbnailKey::for_test(directory.path().join("image.png"), 17);
        let cache_key = ThumbnailCacheKey::from_thumbnail(&key).expect("cache key should form");
        let thumbnail_path = cache.thumbnail_path(&cache_key);

        write_custom_thumbnail(
            &thumbnail_path,
            &[
                ("Thumb::URI", "file:///wrong".to_owned()),
                ("Thumb::MTime", cache_key.modified_seconds.to_string()),
                ("Thumb::Size", cache_key.source_size.to_string()),
            ],
        );
        assert!(cache.load(&key).expect("URI mismatch is a miss").is_none());

        write_custom_thumbnail(
            &thumbnail_path,
            &[
                ("Thumb::URI", cache_key.canonical_uri.clone()),
                (
                    "Thumb::MTime",
                    cache_key.modified_seconds.saturating_add(1).to_string(),
                ),
                ("Thumb::Size", cache_key.source_size.to_string()),
            ],
        );
        assert!(
            cache
                .load(&key)
                .expect("mtime mismatch is a miss")
                .is_none()
        );

        write_custom_thumbnail(
            &thumbnail_path,
            &[("Thumb::URI", cache_key.canonical_uri.clone())],
        );
        assert!(
            cache
                .load(&key)
                .expect("missing required metadata is a miss")
                .is_none()
        );
    }

    #[test]
    fn phase_6e_corrupt_oversized_and_symlink_cache_entries_are_rejected() {
        let directory = tempdir().expect("temporary directory should be created");
        let mut cache = ThumbnailCache::new(ThumbnailCacheConfig::for_test(
            directory.path().to_path_buf(),
        ));
        let key = ThumbnailKey::for_test(directory.path().join("image.png"), 17);
        cache
            .store(&key, &test_image())
            .expect("cache write should succeed");
        let (thumbnail_path, _) = cache.paths_for_test(&key);

        fs::write(&thumbnail_path, b"not a PNG").expect("corrupt cache should be written");
        assert!(cache.load(&key).expect("corruption is a miss").is_none());

        let oversized = File::create(&thumbnail_path).expect("cache file should be recreated");
        oversized
            .set_len(MAX_CACHE_FILE_BYTES + 1)
            .expect("sparse cache should be extended");
        assert!(cache.load(&key).is_err());

        fs::remove_file(&thumbnail_path).expect("oversized cache should be removed");
        let target = directory.path().join("target.png");
        fs::write(&target, b"not relevant").expect("target should exist");
        symlink(&target, &thumbnail_path).expect("cache symlink should be created");
        assert!(cache.load(&key).expect("symlink is a miss").is_none());
    }

    #[test]
    fn phase_6e_writes_are_private_atomic_and_owned() {
        let directory = tempdir().expect("temporary directory should be created");
        let mut cache = ThumbnailCache::new(ThumbnailCacheConfig::for_test(
            directory.path().to_path_buf(),
        ));
        let key = ThumbnailKey::for_test(directory.path().join("image.png"), 17);
        cache
            .store(&key, &test_image())
            .expect("cache write should succeed");
        let (thumbnail_path, marker_path) = cache.paths_for_test(&key);
        assert_eq!(
            fs::metadata(thumbnail_path.parent().expect("cache parent"))
                .expect("cache directory metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&thumbnail_path)
                .expect("thumbnail metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&marker_path)
                .expect("marker metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let temporary_files = fs::read_dir(thumbnail_path.parent().expect("cache parent"))
            .expect("cache directory should be readable")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".temporary"))
            .count();
        assert_eq!(temporary_files, 0);
        assert_eq!(read_software(&thumbnail_path).as_deref(), Some(SOFTWARE));
    }

    #[test]
    fn phase_6e_cleanup_prunes_only_owned_entries_by_count_and_age() {
        let directory = tempdir().expect("temporary directory should be created");
        let config = ThumbnailCacheConfig::for_test(directory.path().to_path_buf()).with_limits(
            1,
            u64::MAX,
            Duration::from_secs(10),
        );
        let mut cache = ThumbnailCache::new(config);
        let first = ThumbnailKey::for_test(directory.path().join("first.png"), 1);
        let second = ThumbnailKey::for_test_at_size(
            directory.path().join("second.png"),
            2,
            crate::thumbnail::MAX_THUMBNAIL_EDGE,
        );
        cache
            .store(&first, &test_image())
            .expect("first cache write");
        cache
            .store(&second, &test_image())
            .expect("second cache write");
        let (first_thumbnail, first_marker) = cache.paths_for_test(&first);
        let (second_thumbnail, second_marker) = cache.paths_for_test(&second);
        cache
            .cleanup_at(SystemTime::now())
            .expect("count cleanup should succeed");
        assert_eq!(
            usize::from(first_thumbnail.exists()) + usize::from(second_thumbnail.exists()),
            1
        );
        assert_eq!(
            usize::from(first_marker.exists()) + usize::from(second_marker.exists()),
            1
        );

        let survivor = if first_thumbnail.exists() {
            first
        } else {
            second
        };
        let (survivor_thumbnail, survivor_marker) = cache.paths_for_test(&survivor);
        cache
            .cleanup_at(SystemTime::now() + Duration::from_secs(20))
            .expect("age cleanup should succeed");
        assert!(!survivor_thumbnail.exists());
        assert!(!survivor_marker.exists());
    }

    #[test]
    fn phase_6e_cleanup_respects_byte_limit_and_never_prunes_foreign_cache() {
        let directory = tempdir().expect("temporary directory should be created");
        let config = ThumbnailCacheConfig::for_test(directory.path().to_path_buf()).with_limits(
            usize::MAX,
            1,
            Duration::MAX,
        );
        let mut cache = ThumbnailCache::new(config);
        let owned = ThumbnailKey::for_test(directory.path().join("owned.png"), 1);
        cache
            .store(&owned, &test_image())
            .expect("owned cache write should complete");
        let (owned_thumbnail, owned_marker) = cache.paths_for_test(&owned);
        assert!(!owned_thumbnail.exists());
        assert!(!owned_marker.exists());

        let foreign = ThumbnailKey::for_test(directory.path().join("foreign.png"), 2);
        let foreign_key =
            ThumbnailCacheKey::from_thumbnail(&foreign).expect("foreign cache key should form");
        let foreign_thumbnail = cache.thumbnail_path(&foreign_key);
        let foreign_marker = cache.marker_path(&foreign_key);
        write_custom_thumbnail(
            &foreign_thumbnail,
            &[
                ("Thumb::URI", foreign_key.canonical_uri.clone()),
                ("Thumb::MTime", foreign_key.modified_seconds.to_string()),
                ("Thumb::Size", foreign_key.source_size.to_string()),
                ("Software", "Another file manager".to_owned()),
            ],
        );
        ensure_private_directory(
            foreign_marker
                .parent()
                .expect("foreign marker should have a parent"),
        )
        .expect("marker directory should be created");
        write_bytes_atomically(&foreign_marker, b"stale Floe marker")
            .expect("stale marker should be written");
        cache
            .cleanup_at(SystemTime::now())
            .expect("foreign cleanup should succeed");
        assert!(foreign_thumbnail.exists());
        assert!(!foreign_marker.exists());
    }
}
