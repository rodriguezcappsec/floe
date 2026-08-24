use std::{
    fs::File,
    io::{self, Cursor, Read},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::SystemTime,
};

use floe_core::{DirectoryEntry, EntryKind};
use image::{ImageFormat, ImageReader, Limits};
use rustix::fs::{Mode, OFlags};
use thiserror::Error;

pub const LIST_THUMBNAIL_EDGE: u16 = 32;
pub const MIN_THUMBNAIL_EDGE: u16 = LIST_THUMBNAIL_EDGE;
pub const MAX_THUMBNAIL_EDGE: u16 = 192;
pub const MAX_SOURCE_BYTES: u64 = 32 * 1024 * 1024;
pub const WORK_QUEUE_CAPACITY: usize = 64;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ThumbnailFormat {
    Png,
    Jpeg,
}

impl ThumbnailFormat {
    fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        if extension.eq_ignore_ascii_case("png") {
            Some(Self::Png)
        } else if extension.eq_ignore_ascii_case("jpg") || extension.eq_ignore_ascii_case("jpeg") {
            Some(Self::Jpeg)
        } else {
            None
        }
    }

    const fn image_format(self) -> ImageFormat {
        match self {
            Self::Png => ImageFormat::Png,
            Self::Jpeg => ImageFormat::Jpeg,
        }
    }
}

/// Exact, metadata-sensitive identity for one in-memory thumbnail.
///
/// The original path is never reconstructed from display text. Only enumerated
/// regular files are accepted, so links are not followed for thumbnails. The
/// requested edge is part of the key so list and grid cache entries cannot be
/// confused.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ThumbnailKey {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
    format: ThumbnailFormat,
    edge: u16,
}

impl ThumbnailKey {
    pub fn from_entry(entry: &DirectoryEntry) -> Option<Self> {
        Self::from_entry_at_size(entry, LIST_THUMBNAIL_EDGE)
    }

    pub fn from_entry_at_size(entry: &DirectoryEntry, edge: u16) -> Option<Self> {
        if entry.kind() != EntryKind::RegularFile
            || !(MIN_THUMBNAIL_EDGE..=MAX_THUMBNAIL_EDGE).contains(&edge)
        {
            return None;
        }
        Some(Self {
            path: entry.path().to_path_buf(),
            size: entry.size()?,
            modified: entry.modified()?,
            format: ThumbnailFormat::from_path(entry.path())?,
            edge,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    #[cfg(test)]
    pub(crate) fn for_test(path: PathBuf, size: u64) -> Self {
        Self {
            path,
            size,
            modified: SystemTime::UNIX_EPOCH,
            format: ThumbnailFormat::Png,
            edge: LIST_THUMBNAIL_EDGE,
        }
    }
}

#[derive(Debug)]
pub struct ThumbnailPixels {
    width: i32,
    height: i32,
    rowstride: usize,
    has_alpha: bool,
    pixels: Vec<u8>,
}

impl ThumbnailPixels {
    pub fn into_parts(self) -> (i32, i32, usize, bool, Vec<u8>) {
        (
            self.width,
            self.height,
            self.rowstride,
            self.has_alpha,
            self.pixels,
        )
    }
}

#[derive(Debug, Error)]
pub enum ThumbnailError {
    #[error("thumbnail source is no longer a regular file")]
    NotRegularFile,
    #[error("thumbnail source exceeds the {MAX_SOURCE_BYTES}-byte safety limit")]
    SourceTooLarge,
    #[error("thumbnail source changed after directory enumeration")]
    SourceChanged,
    #[error("thumbnail source could not be read: {0}")]
    Read(#[from] io::Error),
    #[error("thumbnail could not be decoded: {0}")]
    Decode(String),
    #[error("thumbnail decoder returned an unsupported pixel layout")]
    UnsupportedPixelLayout,
}

pub struct ThumbnailResponse {
    pub generation: u64,
    pub key: ThumbnailKey,
    pub result: Result<ThumbnailPixels, ThumbnailError>,
}

struct ThumbnailRequest {
    generation: u64,
    key: ThumbnailKey,
}

#[derive(Debug)]
pub enum ThumbnailSubmitError {
    Full(ThumbnailKey),
    Disconnected,
}

/// Fixed-capacity, single-worker thumbnail decoder.
///
/// Image parsing and scaling remain local to the worker. Only owned pixel
/// bytes cross back to GTK, where a `MemoryTexture` is created on the main loop.
pub struct ThumbnailWorker {
    requests: Option<SyncSender<ThumbnailRequest>>,
    responses: Receiver<ThumbnailResponse>,
    latest_generation: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    next_generation: u64,
}

impl ThumbnailWorker {
    pub fn spawn() -> io::Result<Self> {
        Self::spawn_internal(WORK_QUEUE_CAPACITY, None)
    }

    fn spawn_internal(capacity: usize, start_gate: Option<Receiver<()>>) -> io::Result<Self> {
        let (request_sender, request_receiver) = mpsc::sync_channel::<ThumbnailRequest>(capacity);
        let (response_sender, response_receiver) = mpsc::channel();
        let latest_generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&latest_generation);
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);
        let worker = thread::Builder::new()
            .name("floe-thumbnail-worker".to_owned())
            .spawn(move || {
                if let Some(start_gate) = start_gate
                    && start_gate.recv().is_err()
                {
                    return;
                }
                while let Ok(request) = request_receiver.recv() {
                    if worker_shutdown.load(Ordering::Acquire) {
                        break;
                    }
                    if worker_generation.load(Ordering::Acquire) != request.generation {
                        continue;
                    }
                    let result = decode_thumbnail(&request.key);
                    if worker_shutdown.load(Ordering::Acquire)
                        || worker_generation.load(Ordering::Acquire) != request.generation
                    {
                        continue;
                    }
                    if response_sender
                        .send(ThumbnailResponse {
                            generation: request.generation,
                            key: request.key,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;

        Ok(Self {
            requests: Some(request_sender),
            responses: response_receiver,
            latest_generation,
            shutdown,
            worker: Some(worker),
            next_generation: 0,
        })
    }

    pub fn begin_generation(&mut self) -> u64 {
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        self.latest_generation
            .store(self.next_generation, Ordering::Release);
        self.next_generation
    }

    pub fn try_request(
        &self,
        generation: u64,
        key: ThumbnailKey,
    ) -> Result<(), ThumbnailSubmitError> {
        let Some(sender) = self.requests.as_ref() else {
            return Err(ThumbnailSubmitError::Disconnected);
        };
        match sender.try_send(ThumbnailRequest { generation, key }) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(request)) => Err(ThumbnailSubmitError::Full(request.key)),
            Err(TrySendError::Disconnected(_)) => Err(ThumbnailSubmitError::Disconnected),
        }
    }

    pub fn try_response(&self) -> Option<ThumbnailResponse> {
        self.responses.try_recv().ok()
    }
}

impl Drop for ThumbnailWorker {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        self.requests.take();
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("thumbnail worker panicked during shutdown");
        }
    }
}

fn decode_thumbnail(key: &ThumbnailKey) -> Result<ThumbnailPixels, ThumbnailError> {
    let descriptor = rustix::fs::open(
        key.path(),
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(io::Error::from)?;
    let mut source = File::from(descriptor);
    let metadata = source.metadata()?;
    if !metadata.is_file() {
        return Err(ThumbnailError::NotRegularFile);
    }
    if metadata.len() > MAX_SOURCE_BYTES {
        return Err(ThumbnailError::SourceTooLarge);
    }
    if key.size != metadata.len() || metadata.modified().ok() != Some(key.modified) {
        return Err(ThumbnailError::SourceChanged);
    }

    let mut encoded = Vec::with_capacity(metadata.len().min(MAX_SOURCE_BYTES) as usize);
    source
        .by_ref()
        .take(MAX_SOURCE_BYTES + 1)
        .read_to_end(&mut encoded)?;
    if encoded.len() as u64 > MAX_SOURCE_BYTES {
        return Err(ThumbnailError::SourceTooLarge);
    }

    let mut reader = ImageReader::with_format(Cursor::new(encoded), key.format.image_format());
    let mut limits = Limits::default();
    limits.max_image_width = Some(65_535);
    limits.max_image_height = Some(65_535);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|error| ThumbnailError::Decode(error.to_string()))?;
    let edge = u32::from(key.edge);
    let thumbnail = if decoded.width() <= edge && decoded.height() <= edge {
        decoded.into_rgba8()
    } else {
        decoded.thumbnail(edge, edge).into_rgba8()
    };
    let width =
        i32::try_from(thumbnail.width()).map_err(|_| ThumbnailError::UnsupportedPixelLayout)?;
    let height =
        i32::try_from(thumbnail.height()).map_err(|_| ThumbnailError::UnsupportedPixelLayout)?;
    let rowstride = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or(ThumbnailError::UnsupportedPixelLayout)?;
    Ok(ThumbnailPixels {
        width,
        height,
        rowstride,
        has_alpha: true,
        pixels: thumbnail.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        fs::{self, File},
        io::Write,
        thread,
        time::Duration,
    };

    #[cfg(unix)]
    use std::os::unix::{ffi::OsStringExt, fs::symlink};

    use floe_core::{DirectoryEntry, enumerate_directory};
    use tempfile::tempdir;

    use super::*;

    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xb5,
        0x1c, 0x0c, 0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x64,
        0xf8, 0x0f, 0x00, 0x01, 0x05, 0x01, 0x01, 0x27, 0x18, 0xe3, 0x66, 0x00, 0x00, 0x00, 0x00,
        0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    fn entry_for<'a>(entries: &'a [DirectoryEntry], path: &Path) -> &'a DirectoryEntry {
        entries
            .iter()
            .find(|entry| entry.path() == path)
            .expect("test entry should be enumerated")
    }

    fn wait_for_response(worker: &ThumbnailWorker) -> ThumbnailResponse {
        (0..200)
            .find_map(|_| {
                let response = worker.try_response();
                if response.is_none() {
                    thread::sleep(Duration::from_millis(5));
                }
                response
            })
            .expect("thumbnail response should arrive")
    }

    #[cfg(unix)]
    #[test]
    fn phase_6c_key_whitelists_formats_and_preserves_non_utf8_path_identity() {
        let directory = tempdir().expect("temporary directory should be created");
        let raw_name = OsString::from_vec(vec![b'i', 0x80, b'.', b'p', b'n', b'g']);
        let png_path = directory.path().join(&raw_name);
        let gif_path = directory.path().join("ignored.gif");
        let link_path = directory.path().join("link.png");
        fs::write(&png_path, PNG_1X1).expect("PNG should be written");
        fs::write(&gif_path, b"GIF89a").expect("GIF marker should be written");
        symlink(&png_path, &link_path).expect("symlink should be created");
        let listing = enumerate_directory(directory.path()).expect("directory should enumerate");

        let key = ThumbnailKey::from_entry(entry_for(listing.entries(), &png_path))
            .expect("PNG should be eligible");
        assert_eq!(key.path(), png_path);
        assert_eq!(key.format, ThumbnailFormat::Png);
        assert!(ThumbnailKey::from_entry(entry_for(listing.entries(), &gif_path)).is_none());
        assert!(ThumbnailKey::from_entry(entry_for(listing.entries(), &link_path)).is_none());
    }

    #[test]
    fn phase_6c_key_changes_when_enumerated_metadata_changes() {
        let directory = tempdir().expect("temporary directory should be created");
        let path = directory.path().join("image.png");
        fs::write(&path, PNG_1X1).expect("PNG should be written");
        let first = enumerate_directory(directory.path()).expect("directory should enumerate");
        let first_key = ThumbnailKey::from_entry(entry_for(first.entries(), &path))
            .expect("PNG should be eligible");
        let mut changed = PNG_1X1.to_vec();
        changed.push(0);
        fs::write(&path, changed).expect("PNG should change");
        let second = enumerate_directory(directory.path()).expect("directory should enumerate");
        let second_key = ThumbnailKey::from_entry(entry_for(second.entries(), &path))
            .expect("changed PNG should remain eligible");
        assert_ne!(first_key, second_key);
        assert!(matches!(
            decode_thumbnail(&first_key),
            Err(ThumbnailError::SourceChanged)
        ));
    }

    #[test]
    fn phase_6d_requested_edge_is_bounded_and_part_of_cache_identity() {
        let directory = tempdir().expect("temporary directory should be created");
        let path = directory.path().join("image.png");
        fs::write(&path, PNG_1X1).expect("PNG should be written");
        let listing = enumerate_directory(directory.path()).expect("directory should enumerate");
        let entry = entry_for(listing.entries(), &path);

        let list_key = ThumbnailKey::from_entry_at_size(entry, LIST_THUMBNAIL_EDGE)
            .expect("list-size PNG should be eligible");
        let grid_key = ThumbnailKey::from_entry_at_size(entry, MAX_THUMBNAIL_EDGE)
            .expect("grid-size PNG should be eligible");
        assert_ne!(list_key, grid_key);
        assert_eq!(list_key.edge, LIST_THUMBNAIL_EDGE);
        assert_eq!(grid_key.edge, MAX_THUMBNAIL_EDGE);
        assert!(ThumbnailKey::from_entry_at_size(entry, MIN_THUMBNAIL_EDGE - 1).is_none());
        assert!(ThumbnailKey::from_entry_at_size(entry, MAX_THUMBNAIL_EDGE + 1).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn phase_6c_decoder_does_not_follow_a_symlink_replacement() {
        let directory = tempdir().expect("temporary directory should be created");
        let source_path = directory.path().join("source.png");
        let target_path = directory.path().join("target.png");
        fs::write(&source_path, PNG_1X1).expect("source PNG should be written");
        fs::write(&target_path, PNG_1X1).expect("target PNG should be written");
        let listing = enumerate_directory(directory.path()).expect("directory should enumerate");
        let key = ThumbnailKey::from_entry(entry_for(listing.entries(), &source_path))
            .expect("source PNG should be eligible before replacement");

        fs::remove_file(&source_path).expect("source PNG should be removed");
        symlink(&target_path, &source_path).expect("source path should become a symlink");

        assert!(matches!(
            decode_thumbnail(&key),
            Err(ThumbnailError::Read(_))
        ));
    }

    #[test]
    fn phase_6c_decoder_handles_png_jpeg_and_bounded_scaling() {
        let directory = tempdir().expect("temporary directory should be created");
        let path = directory.path().join("image.png");
        let wide_path = directory.path().join("wide.png");
        let jpeg_path = directory.path().join("image.jpg");
        fs::write(&path, PNG_1X1).expect("PNG should be written");
        image::DynamicImage::new_rgba8(64, 16)
            .save_with_format(&wide_path, ImageFormat::Png)
            .expect("wide PNG should be written");
        image::DynamicImage::new_rgb8(2, 1)
            .save_with_format(&jpeg_path, ImageFormat::Jpeg)
            .expect("JPEG should be written");
        let listing = enumerate_directory(directory.path()).expect("directory should enumerate");
        let key = ThumbnailKey::from_entry(entry_for(listing.entries(), &path))
            .expect("PNG should be eligible");
        let pixels = decode_thumbnail(&key).expect("PNG should decode");
        assert_eq!((pixels.width, pixels.height), (1, 1));
        assert!(pixels.rowstride >= 4);
        assert!(!pixels.pixels.is_empty());

        let wide_key = ThumbnailKey::from_entry(entry_for(listing.entries(), &wide_path))
            .expect("wide PNG should be eligible");
        let wide = decode_thumbnail(&wide_key).expect("wide PNG should decode");
        assert_eq!((wide.width, wide.height), (32, 8));

        let jpeg_key = ThumbnailKey::from_entry(entry_for(listing.entries(), &jpeg_path))
            .expect("JPEG should be eligible");
        let jpeg = decode_thumbnail(&jpeg_key).expect("JPEG should decode");
        assert_eq!((jpeg.width, jpeg.height), (2, 1));
    }

    #[test]
    fn phase_6c_decoder_rejects_oversized_source_before_reading_it() {
        let directory = tempdir().expect("temporary directory should be created");
        let path = directory.path().join("oversized.png");
        let mut source = File::create(&path).expect("source should be created");
        source
            .write_all(PNG_1X1)
            .expect("PNG prefix should be written");
        source
            .set_len(MAX_SOURCE_BYTES + 1)
            .expect("sparse source should be extended");
        let listing = enumerate_directory(directory.path()).expect("directory should enumerate");
        let key = ThumbnailKey::from_entry(entry_for(listing.entries(), &path))
            .expect("PNG should be eligible by name");
        assert!(matches!(
            decode_thumbnail(&key),
            Err(ThumbnailError::SourceTooLarge)
        ));
    }

    #[test]
    fn phase_6c_worker_skips_stale_generations_and_returns_current_pixels() {
        let directory = tempdir().expect("temporary directory should be created");
        let path = directory.path().join("image.png");
        fs::write(&path, PNG_1X1).expect("PNG should be written");
        let listing = enumerate_directory(directory.path()).expect("directory should enumerate");
        let key = ThumbnailKey::from_entry(entry_for(listing.entries(), &path))
            .expect("PNG should be eligible");
        let mut worker = ThumbnailWorker::spawn().expect("worker should start");
        let stale = worker.begin_generation();
        let current = worker.begin_generation();
        worker
            .try_request(stale, key.clone())
            .expect("stale request should enter the queue");
        worker
            .try_request(current, key)
            .expect("current request should enter the queue");

        let response = wait_for_response(&worker);
        assert_eq!(response.generation, current);
        assert!(response.result.is_ok());
        assert!(worker.try_response().is_none());
    }

    #[test]
    fn phase_6c_bounded_queue_reports_full_without_blocking_submitter() {
        let directory = tempdir().expect("temporary directory should be created");
        let path = directory.path().join("image.png");
        fs::write(&path, PNG_1X1).expect("PNG should be written");
        let listing = enumerate_directory(directory.path()).expect("directory should enumerate");
        let key = ThumbnailKey::from_entry(entry_for(listing.entries(), &path))
            .expect("PNG should be eligible");
        let (gate_sender, gate_receiver) = mpsc::channel();
        let mut worker = ThumbnailWorker::spawn_internal(1, Some(gate_receiver))
            .expect("paused worker should start");
        let generation = worker.begin_generation();
        worker
            .try_request(generation, key.clone())
            .expect("first request should fill the queue");
        assert!(matches!(
            worker.try_request(generation, key),
            Err(ThumbnailSubmitError::Full(_))
        ));
        gate_sender.send(()).expect("worker should be released");
        assert!(wait_for_response(&worker).result.is_ok());
    }
}
