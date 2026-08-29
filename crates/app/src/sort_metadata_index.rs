//! Explicit, bounded advanced metadata indexing for whole-directory sorting.

use std::{
    collections::HashMap,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use floe_core::{DirectoryEntry, DirectorySort, IndexedSortMetadata, SortColumn};
use rustix::fs::{Mode, OFlags};
use thiserror::Error;

use crate::{
    advanced_metadata::{AdvancedMetadataState, load_advanced_metadata},
    inspector::{ImageDimensionFacts, load_image_dimensions},
};

const MAGIC: &[u8; 9] = b"FLOEMSI01";
const REQUEST_CAPACITY: usize = 8;
const RESPONSE_CAPACITY: usize = 32;
pub const INDEX_ENTRY_CAPACITY: usize = 4_096;
const CACHE_ENTRY_CAPACITY: usize = 32_768;
const CACHE_ENCODED_CAPACITY: u64 = 32 * 1024 * 1024;
const PATH_CAPACITY: usize = 1024 * 1024;
const TEXT_FILE_CAPACITY: u64 = 4 * 1024 * 1024;
const TEXT_TOTAL_CAPACITY: u64 = 128 * 1024 * 1024;
const ADVANCED_TOTAL_CAPACITY: u64 = 512 * 1024 * 1024;
const VIDEO_PROVIDER_CAPACITY: usize = 512;
const VIDEO_PROVIDER_TIMEOUT: Duration = Duration::from_secs(5);
const TEXT: u32 = 1;
const IMAGE: u32 = 1 << 1;
const AUDIO: u32 = 1 << 2;
const VIDEO: u32 = 1 << 3;
const OTHER: u32 = 1 << 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileStamp {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

impl FileStamp {
    fn current(path: &Path) -> Result<Self, MetadataIndexError> {
        let metadata = fs::symlink_metadata(path).map_err(MetadataIndexError::Io)?;
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct CacheRecord {
    stamp: FileStamp,
    loaded: u32,
    metadata: IndexedSortMetadata,
    last_used: u64,
}

#[derive(Default)]
struct MetadataCache {
    entries: HashMap<PathBuf, CacheRecord>,
    tick: u64,
    dirty: bool,
}

impl MetadataCache {
    fn load(path: &Path) -> Result<Self, MetadataIndexError> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(MetadataIndexError::Io(error)),
        };
        validate_private_file(path, &metadata)?;
        if metadata.len() > CACHE_ENCODED_CAPACITY {
            return Err(MetadataIndexError::Capacity);
        }
        let descriptor = rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::empty(),
        )
        .map_err(|error| MetadataIndexError::Io(io::Error::from(error)))?;
        let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
        File::from(descriptor)
            .take(CACHE_ENCODED_CAPACITY + 1)
            .read_to_end(&mut bytes)
            .map_err(MetadataIndexError::Io)?;
        Self::decode(&bytes)
    }

    fn value(
        &mut self,
        path: &Path,
        stamp: FileStamp,
        category: u32,
    ) -> Option<IndexedSortMetadata> {
        let record = self.entries.get_mut(path)?;
        if record.stamp != stamp || record.loaded & category == 0 {
            self.entries.remove(path);
            self.dirty = true;
            return None;
        }
        self.tick = self.tick.wrapping_add(1).max(1);
        record.last_used = self.tick;
        Some(record.metadata.clone())
    }

    fn insert(
        &mut self,
        path: PathBuf,
        stamp: FileStamp,
        category: u32,
        metadata: IndexedSortMetadata,
    ) {
        self.tick = self.tick.wrapping_add(1).max(1);
        let record = self.entries.entry(path).or_insert_with(|| CacheRecord {
            stamp,
            loaded: 0,
            metadata: IndexedSortMetadata::default(),
            last_used: self.tick,
        });
        if record.stamp != stamp {
            *record = CacheRecord {
                stamp,
                loaded: 0,
                metadata: IndexedSortMetadata::default(),
                last_used: self.tick,
            };
        }
        merge_metadata(&mut record.metadata, metadata, category);
        record.loaded |= category;
        record.last_used = self.tick;
        if self.entries.len() > CACHE_ENTRY_CAPACITY {
            let remove = self.entries.len() - CACHE_ENTRY_CAPACITY + CACHE_ENTRY_CAPACITY / 10;
            let mut oldest = self
                .entries
                .iter()
                .map(|(path, record)| (record.last_used, path.clone()))
                .collect::<Vec<_>>();
            oldest.sort_unstable();
            for (_, path) in oldest.into_iter().take(remove) {
                self.entries.remove(&path);
            }
        }
        self.dirty = true;
    }

    fn invalidate(&mut self, paths: &[PathBuf]) {
        let before = self.entries.len();
        self.entries
            .retain(|cached, _| !paths.iter().any(|changed| cached.starts_with(changed)));
        self.dirty |= before != self.entries.len();
    }

    fn clear(&mut self) {
        self.dirty |= !self.entries.is_empty();
        self.entries.clear();
    }

    fn persist(&mut self, path: &Path) -> Result<(), MetadataIndexError> {
        if !self.dirty {
            return Ok(());
        }
        let parent = path.parent().ok_or(MetadataIndexError::UnsafeStorage)?;
        ensure_private_directory(parent)?;
        let bytes = self.encode()?;
        let temporary = path.with_extension(format!("tmp-{}", self.tick.wrapping_add(1)));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true).mode(0o600);
        let result = (|| {
            let mut file = options.open(&temporary).map_err(MetadataIndexError::Io)?;
            file.write_all(&bytes).map_err(MetadataIndexError::Io)?;
            file.sync_all().map_err(MetadataIndexError::Io)?;
            fs::rename(&temporary, path).map_err(MetadataIndexError::Io)?;
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(MetadataIndexError::Io)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        } else {
            self.dirty = false;
        }
        result
    }

    fn encode(&self) -> Result<Vec<u8>, MetadataIndexError> {
        let mut records = self.entries.iter().collect::<Vec<_>>();
        records.sort_by_key(|(path, _)| *path);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        put_u32(&mut bytes, records.len())?;
        for (path, record) in records {
            put_bytes(&mut bytes, path.as_os_str().as_bytes())?;
            for value in [record.stamp.device, record.stamp.inode, record.stamp.size] {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            for value in [
                record.stamp.modified_seconds,
                record.stamp.modified_nanoseconds,
                record.stamp.changed_seconds,
                record.stamp.changed_nanoseconds,
            ] {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            bytes.extend_from_slice(&record.loaded.to_le_bytes());
            bytes.extend_from_slice(&record.last_used.to_le_bytes());
            encode_metadata(&mut bytes, &record.metadata)?;
            if bytes.len() as u64 > CACHE_ENCODED_CAPACITY {
                return Err(MetadataIndexError::Capacity);
            }
        }
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self, MetadataIndexError> {
        let mut cursor = Cursor { bytes, offset: 0 };
        if cursor.take(MAGIC.len())? != MAGIC {
            return Err(MetadataIndexError::Corrupt);
        }
        let count = cursor.u32()? as usize;
        if count > CACHE_ENTRY_CAPACITY {
            return Err(MetadataIndexError::Capacity);
        }
        let mut cache = Self::default();
        for _ in 0..count {
            let path = PathBuf::from(OsString::from_vec(cursor.bytes()?.to_vec()));
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
            let loaded = cursor.u32()?;
            let last_used = cursor.u64()?;
            let metadata = decode_metadata(&mut cursor)?;
            if cache
                .entries
                .insert(
                    path,
                    CacheRecord {
                        stamp,
                        loaded,
                        metadata,
                        last_used,
                    },
                )
                .is_some()
            {
                return Err(MetadataIndexError::Corrupt);
            }
            cache.tick = cache.tick.max(last_used);
        }
        if cursor.offset != bytes.len() {
            return Err(MetadataIndexError::Corrupt);
        }
        Ok(cache)
    }
}

#[derive(Debug)]
enum CommandKind {
    Sort {
        entries: Vec<Arc<DirectoryEntry>>,
        sort: DirectorySort,
        persist: bool,
    },
    Invalidate(Vec<PathBuf>),
    Clear,
}

#[derive(Debug)]
struct Request {
    generation: u64,
    kind: CommandKind,
}

#[derive(Debug)]
pub enum MetadataIndexEventKind {
    Progress {
        completed: usize,
        total: usize,
        cache_hits: usize,
    },
    Sorted {
        entries: Vec<Arc<DirectoryEntry>>,
        sort: DirectorySort,
    },
    Failed {
        error: MetadataIndexError,
        sort: DirectorySort,
    },
    Cleared,
}

#[derive(Debug)]
pub struct MetadataIndexEvent {
    pub generation: u64,
    pub kind: MetadataIndexEventKind,
}

#[derive(Debug, Error)]
pub enum MetadataIndexError {
    #[error("metadata index is limited to {INDEX_ENTRY_CAPACITY} entries")]
    TooManyEntries,
    #[error("metadata index was cancelled")]
    Cancelled,
    #[error("too many video files for one metadata scan")]
    TooManyVideoFiles,
    #[error("metadata scan reached its 512 MiB read budget")]
    ReadBudget,
    #[error("metadata index I/O failed: {0}")]
    Io(io::Error),
    #[error("metadata index cache is corrupt")]
    Corrupt,
    #[error("metadata index cache exceeds its capacity")]
    Capacity,
    #[error("metadata index cache storage is not private regular storage")]
    UnsafeStorage,
    #[error("metadata index source path is unsafe")]
    UnsafeSource,
}

#[derive(Debug, Error)]
pub enum MetadataIndexSubmitError {
    #[error("metadata index worker queue is full")]
    Full,
    #[error("metadata index worker is unavailable")]
    Disconnected,
}

pub struct MetadataIndexWorker {
    sender: Option<SyncSender<Request>>,
    receiver: Receiver<MetadataIndexEvent>,
    latest_generation: Arc<AtomicU64>,
    next_generation: u64,
    worker: Option<JoinHandle<()>>,
}

impl MetadataIndexWorker {
    pub fn spawn() -> io::Result<Self> {
        Self::spawn_at(
            gtk::glib::user_cache_dir()
                .join("floe")
                .join("sort-metadata-v1"),
        )
    }

    fn spawn_at(cache_path: PathBuf) -> io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<Request>(REQUEST_CAPACITY);
        let (responses, receiver) = mpsc::sync_channel(RESPONSE_CAPACITY);
        let latest_generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&latest_generation);
        let worker = thread::Builder::new()
            .name("floe-sort-metadata-index".to_owned())
            .spawn(move || {
                let mut cache = MetadataCache::load(&cache_path).unwrap_or_else(|error| {
                    tracing::warn!(%error, "ignoring unavailable metadata sort cache");
                    MetadataCache::default()
                });
                let ffprobe = discover_executable("ffprobe");
                while let Ok(request) = requests.recv() {
                    match request.kind {
                        CommandKind::Sort { entries, sort, persist } => {
                            let result = index_and_sort(
                                entries,
                                sort,
                                &mut cache,
                                ffprobe.as_deref(),
                                &worker_generation,
                                request.generation,
                                &responses,
                            );
                            let kind = match result {
                                Ok(entries) => {
                                    if persist {
                                        if let Err(error) = cache.persist(&cache_path) {
                                            tracing::warn!(%error, "metadata sort cache could not be saved");
                                        }
                                    } else {
                                        cache.clear();
                                        let _ = fs::remove_file(&cache_path);
                                    }
                                    MetadataIndexEventKind::Sorted { entries, sort }
                                }
                                Err(MetadataIndexError::Cancelled) => continue,
                                Err(error) => MetadataIndexEventKind::Failed { error, sort },
                            };
                            let _ = responses.send(MetadataIndexEvent {
                                generation: request.generation,
                                kind,
                            });
                        }
                        CommandKind::Invalidate(paths) => {
                            cache.invalidate(&paths);
                            let _ = cache.persist(&cache_path);
                        }
                        CommandKind::Clear => {
                            cache.clear();
                            let _ = fs::remove_file(&cache_path);
                            let _ = responses.send(MetadataIndexEvent {
                                generation: request.generation,
                                kind: MetadataIndexEventKind::Cleared,
                            });
                        }
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            receiver,
            latest_generation,
            next_generation: 0,
            worker: Some(worker),
        })
    }

    pub fn request_sort(
        &mut self,
        entries: Vec<Arc<DirectoryEntry>>,
        sort: DirectorySort,
        persist: bool,
    ) -> Result<u64, MetadataIndexSubmitError> {
        let generation = self.advance_generation();
        self.send(Request {
            generation,
            kind: CommandKind::Sort {
                entries,
                sort,
                persist,
            },
        })?;
        Ok(generation)
    }

    pub fn cancel(&mut self) {
        self.advance_generation();
    }

    pub fn invalidate(&mut self, paths: Vec<PathBuf>) -> Result<(), MetadataIndexSubmitError> {
        let generation = self.latest_generation.load(Ordering::Acquire);
        self.send(Request {
            generation,
            kind: CommandKind::Invalidate(paths),
        })
    }

    pub fn clear(&mut self) -> Result<u64, MetadataIndexSubmitError> {
        let generation = self.advance_generation();
        self.send(Request {
            generation,
            kind: CommandKind::Clear,
        })?;
        Ok(generation)
    }

    pub fn try_response(&self) -> Option<MetadataIndexEvent> {
        self.receiver.try_recv().ok()
    }

    fn advance_generation(&mut self) -> u64 {
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        self.latest_generation
            .store(self.next_generation, Ordering::Release);
        self.next_generation
    }

    fn send(&self, request: Request) -> Result<(), MetadataIndexSubmitError> {
        let Some(sender) = &self.sender else {
            return Err(MetadataIndexSubmitError::Disconnected);
        };
        match sender.try_send(request) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(MetadataIndexSubmitError::Full),
            Err(TrySendError::Disconnected(_)) => Err(MetadataIndexSubmitError::Disconnected),
        }
    }
}

impl Drop for MetadataIndexWorker {
    fn drop(&mut self) {
        self.sender.take();
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("metadata index worker panicked during shutdown");
        }
    }
}

fn index_and_sort(
    entries: Vec<Arc<DirectoryEntry>>,
    sort: DirectorySort,
    cache: &mut MetadataCache,
    ffprobe: Option<&Path>,
    generation: &AtomicU64,
    expected_generation: u64,
    responses: &SyncSender<MetadataIndexEvent>,
) -> Result<Vec<Arc<DirectoryEntry>>, MetadataIndexError> {
    if entries.len() > INDEX_ENTRY_CAPACITY {
        return Err(MetadataIndexError::TooManyEntries);
    }
    let category = category_for(sort.column);
    if category == VIDEO
        && entries
            .iter()
            .filter(|entry| is_video_path(entry.path()))
            .count()
            > VIDEO_PROVIDER_CAPACITY
    {
        return Err(MetadataIndexError::TooManyVideoFiles);
    }
    let mut text_budget = TEXT_TOTAL_CAPACITY;
    let mut advanced_budget = ADVANCED_TOTAL_CAPACITY;
    let total = entries.len();
    let mut cache_hits = 0;
    let mut indexed = Vec::with_capacity(total);
    for (index, entry) in entries.into_iter().enumerate() {
        if generation.load(Ordering::Acquire) != expected_generation {
            return Err(MetadataIndexError::Cancelled);
        }
        let stamp = FileStamp::current(entry.path())?;
        let metadata = if let Some(metadata) = cache.value(entry.path(), stamp, category) {
            cache_hits += 1;
            metadata
        } else {
            if matches!(category, IMAGE | AUDIO) {
                let cost = if category == IMAGE {
                    entry.size().unwrap_or(0)
                } else {
                    entry
                        .size()
                        .unwrap_or(0)
                        .min(crate::advanced_metadata::ADVANCED_METADATA_READ_CAPACITY)
                };
                if cost > advanced_budget {
                    return Err(MetadataIndexError::ReadBudget);
                }
                advanced_budget -= cost;
            }
            let metadata =
                extract_metadata(entry.as_ref(), category, ffprobe, &mut text_budget, || {
                    generation.load(Ordering::Acquire) != expected_generation
                })?;
            if FileStamp::current(entry.path())? != stamp {
                return Err(MetadataIndexError::Cancelled);
            }
            cache.insert(
                entry.path().to_path_buf(),
                stamp,
                category,
                metadata.clone(),
            );
            metadata
        };
        let mut owned = entry.as_ref().clone();
        owned.set_indexed_sort_metadata(metadata);
        indexed.push(Arc::new(owned));
        if index == 0 || (index + 1) % 32 == 0 || index + 1 == total {
            let _ = responses.try_send(MetadataIndexEvent {
                generation: expected_generation,
                kind: MetadataIndexEventKind::Progress {
                    completed: index + 1,
                    total,
                    cache_hits,
                },
            });
        }
    }
    indexed.sort_by(|left, right| sort.compare_entries(left, right));
    Ok(indexed)
}

fn category_for(column: SortColumn) -> u32 {
    match column {
        SortColumn::DocumentWordCount | SortColumn::DocumentLineCount => TEXT,
        SortColumn::ImageDimensions
        | SortColumn::ImageOrientation
        | SortColumn::ImageWidth
        | SortColumn::ImageHeight => IMAGE,
        SortColumn::AudioArtist
        | SortColumn::AudioAlbum
        | SortColumn::AudioDuration
        | SortColumn::AudioTrack
        | SortColumn::AudioGenre
        | SortColumn::AudioBitrate => AUDIO,
        SortColumn::VideoDuration
        | SortColumn::VideoDimensions
        | SortColumn::VideoWidth
        | SortColumn::VideoHeight
        | SortColumn::VideoFrameRate
        | SortColumn::VideoBitrate => VIDEO,
        _ => OTHER,
    }
}

fn extract_metadata(
    entry: &DirectoryEntry,
    category: u32,
    ffprobe: Option<&Path>,
    text_budget: &mut u64,
    cancelled: impl Fn() -> bool,
) -> Result<IndexedSortMetadata, MetadataIndexError> {
    if cancelled() {
        return Err(MetadataIndexError::Cancelled);
    }
    let mut result = IndexedSortMetadata::default();
    match category {
        TEXT => extract_text(entry.path(), &mut result, text_budget)?,
        IMAGE => extract_image(entry, &mut result),
        AUDIO => extract_audio(entry, &mut result),
        VIDEO => {
            if let Some(ffprobe) = ffprobe {
                extract_video(ffprobe, entry.path(), &mut result, cancelled)?;
            }
        }
        OTHER => extract_other(entry.path(), &mut result)?,
        _ => {}
    }
    Ok(result)
}

fn extract_text(
    path: &Path,
    result: &mut IndexedSortMetadata,
    total_budget: &mut u64,
) -> Result<(), MetadataIndexError> {
    if !is_text_path(path) {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path).map_err(MetadataIndexError::Io)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > TEXT_FILE_CAPACITY
    {
        return Ok(());
    }
    let allowed = metadata.len().min(*total_budget);
    if allowed < metadata.len() {
        return Ok(());
    }
    let descriptor = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|error| MetadataIndexError::Io(io::Error::from(error)))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::from(descriptor)
        .take(TEXT_FILE_CAPACITY + 1)
        .read_to_end(&mut bytes)
        .map_err(MetadataIndexError::Io)?;
    *total_budget = total_budget.saturating_sub(bytes.len() as u64);
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return Ok(());
    };
    result.word_count = Some(text.split_whitespace().count() as u64);
    result.line_count = Some(if text.is_empty() {
        0
    } else {
        text.lines().count() as u64
    });
    Ok(())
}

fn extract_image(entry: &DirectoryEntry, result: &mut IndexedSortMetadata) {
    if let Ok(source_metadata) = fs::symlink_metadata(entry.path()) {
        if let ImageDimensionFacts::Dimensions(size) =
            load_image_dimensions(entry.path(), &source_metadata)
        {
            result.image_width = Some(size.width);
            result.image_height = Some(size.height);
        }
    }
    if let Ok(AdvancedMetadataState::Present(metadata)) =
        load_advanced_metadata(entry.path(), entry.size(), entry.modified())
    {
        if let Some(exif) = metadata.exif {
            result.image_orientation = exif
                .fields
                .iter()
                .find(|field| field.label == "Orientation")
                .map(|field| field.value.as_bytes().to_vec().into_boxed_slice());
        }
    }
}

fn extract_audio(entry: &DirectoryEntry, result: &mut IndexedSortMetadata) {
    if let Ok(AdvancedMetadataState::Present(metadata)) =
        load_advanced_metadata(entry.path(), entry.size(), entry.modified())
    {
        if let Some(media) = metadata.media {
            result.audio_artist = media
                .artist
                .map(String::into_bytes)
                .map(Vec::into_boxed_slice);
            result.audio_album = media
                .album
                .map(String::into_bytes)
                .map(Vec::into_boxed_slice);
            result.audio_duration_millis = media.duration.map(|value| value.as_millis() as u64);
            result.audio_track = media.track;
            result.audio_genre = media
                .genre
                .map(String::into_bytes)
                .map(Vec::into_boxed_slice);
            result.audio_bitrate = media.audio_bitrate.map(u64::from);
        }
    }
}

fn extract_other(path: &Path, result: &mut IndexedSortMetadata) -> Result<(), MetadataIndexError> {
    let metadata = fs::symlink_metadata(path).map_err(MetadataIndexError::Io)?;
    result.permissions = Some(metadata.mode() & 0o7777);
    result.owner = Some(metadata.uid());
    result.group = Some(metadata.gid());
    if metadata.file_type().is_symlink() {
        result.link_destination = fs::read_link(path).ok().map(PathBuf::into_os_string);
    }
    Ok(())
}

fn extract_video(
    ffprobe: &Path,
    path: &Path,
    result: &mut IndexedSortMetadata,
    cancelled: impl Fn() -> bool,
) -> Result<(), MetadataIndexError> {
    if !is_video_path(path) {
        return Ok(());
    }
    let source = fs::symlink_metadata(path).map_err(MetadataIndexError::Io)?;
    if !source.is_file() || source.file_type().is_symlink() {
        return Ok(());
    }
    let mut child = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate,bit_rate:format=duration,bit_rate",
            "-of",
            "default=noprint_wrappers=1",
            "-i",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(MetadataIndexError::Io)?;
    let started = Instant::now();
    loop {
        if cancelled() || started.elapsed() >= VIDEO_PROVIDER_TIMEOUT {
            if let Some(group) = i32::try_from(child.id())
                .ok()
                .and_then(rustix::process::Pid::from_raw)
            {
                let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
            }
            let _ = child.wait();
            return if cancelled() {
                Err(MetadataIndexError::Cancelled)
            } else {
                Ok(())
            };
        }
        match child.try_wait().map_err(MetadataIndexError::Io)? {
            Some(status) => {
                if !status.success() {
                    return Ok(());
                }
                let mut output = String::new();
                if let Some(stdout) = child.stdout.take() {
                    stdout
                        .take(64 * 1024)
                        .read_to_string(&mut output)
                        .map_err(MetadataIndexError::Io)?;
                }
                parse_ffprobe(&output, result);
                return Ok(());
            }
            None => thread::sleep(Duration::from_millis(20)),
        }
    }
}

fn parse_ffprobe(output: &str, result: &mut IndexedSortMetadata) {
    for line in output.lines() {
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        match name {
            "width" => result.video_width = value.parse().ok(),
            "height" => result.video_height = value.parse().ok(),
            "r_frame_rate" => {
                let rate = value.split_once('/').and_then(|(left, right)| {
                    let numerator = left.parse::<u64>().ok()?;
                    let denominator = right.parse::<u64>().ok()?;
                    (denominator != 0).then_some(numerator.saturating_mul(1_000) / denominator)
                });
                result.video_frame_rate_milli = rate;
            }
            "duration" => {
                result.video_duration_millis = value
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite() && *value >= 0.0)
                    .map(|value| (value * 1_000.0).round() as u64);
            }
            "bit_rate" => {
                result.video_bitrate = result.video_bitrate.or_else(|| value.parse().ok());
            }
            _ => {}
        }
    }
}

fn discover_executable(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .as_os_str()
        .as_bytes()
        .split(|byte| *byte == b':')
        .filter(|component| !component.is_empty())
        .map(|component| PathBuf::from(OsString::from_vec(component.to_vec())).join(name))
        .find(|candidate| {
            fs::symlink_metadata(candidate).is_ok_and(|metadata| {
                metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && metadata.permissions().mode() & 0o111 != 0
            })
        })
}

fn is_text_path(path: &Path) -> bool {
    extension_matches(
        path,
        &[
            b"txt",
            b"md",
            b"markdown",
            b"rst",
            b"csv",
            b"tsv",
            b"json",
            b"xml",
            b"yaml",
            b"yml",
            b"toml",
            b"rs",
            b"c",
            b"h",
            b"cpp",
            b"hpp",
            b"py",
            b"js",
            b"ts",
            b"css",
            b"html",
            b"sh",
        ],
    )
}

fn is_video_path(path: &Path) -> bool {
    extension_matches(
        path,
        &[
            b"mp4", b"mkv", b"webm", b"mov", b"avi", b"m4v", b"mpeg", b"mpg", b"ogv",
        ],
    )
}

fn extension_matches(path: &Path, candidates: &[&[u8]]) -> bool {
    let Some(extension) = path.extension().map(OsStrExt::as_bytes) else {
        return false;
    };
    candidates
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

fn merge_metadata(target: &mut IndexedSortMetadata, source: IndexedSortMetadata, category: u32) {
    match category {
        TEXT => {
            target.word_count = source.word_count;
            target.line_count = source.line_count;
        }
        IMAGE => {
            target.image_width = source.image_width;
            target.image_height = source.image_height;
            target.image_orientation = source.image_orientation;
        }
        AUDIO => {
            target.audio_artist = source.audio_artist;
            target.audio_album = source.audio_album;
            target.audio_duration_millis = source.audio_duration_millis;
            target.audio_track = source.audio_track;
            target.audio_genre = source.audio_genre;
            target.audio_bitrate = source.audio_bitrate;
        }
        VIDEO => {
            target.video_duration_millis = source.video_duration_millis;
            target.video_width = source.video_width;
            target.video_height = source.video_height;
            target.video_frame_rate_milli = source.video_frame_rate_milli;
            target.video_bitrate = source.video_bitrate;
        }
        OTHER => {
            target.link_destination = source.link_destination;
            target.permissions = source.permissions;
            target.owner = source.owner;
            target.group = source.group;
        }
        _ => {}
    }
}

fn encode_metadata(
    bytes: &mut Vec<u8>,
    value: &IndexedSortMetadata,
) -> Result<(), MetadataIndexError> {
    for number in [
        value.word_count,
        value.line_count,
        value.audio_duration_millis,
        value.audio_bitrate,
        value.video_duration_millis,
        value.video_frame_rate_milli,
        value.video_bitrate,
    ] {
        bytes.extend_from_slice(&number.unwrap_or(u64::MAX).to_le_bytes());
    }
    for number in [
        value.image_width,
        value.image_height,
        value.audio_track,
        value.video_width,
        value.video_height,
        value.permissions,
        value.owner,
        value.group,
    ] {
        bytes.extend_from_slice(&number.unwrap_or(u32::MAX).to_le_bytes());
    }
    for text in [
        &value.image_orientation,
        &value.audio_artist,
        &value.audio_album,
        &value.audio_genre,
    ] {
        put_optional_bytes(bytes, text.as_deref())?;
    }
    put_optional_bytes(
        bytes,
        value.link_destination.as_deref().map(OsStrExt::as_bytes),
    )?;
    Ok(())
}

fn decode_metadata(cursor: &mut Cursor<'_>) -> Result<IndexedSortMetadata, MetadataIndexError> {
    let word_count = optional_u64(cursor.u64()?);
    let line_count = optional_u64(cursor.u64()?);
    let audio_duration_millis = optional_u64(cursor.u64()?);
    let audio_bitrate = optional_u64(cursor.u64()?);
    let video_duration_millis = optional_u64(cursor.u64()?);
    let video_frame_rate_milli = optional_u64(cursor.u64()?);
    let video_bitrate = optional_u64(cursor.u64()?);
    let image_width = optional_u32(cursor.u32()?);
    let image_height = optional_u32(cursor.u32()?);
    let audio_track = optional_u32(cursor.u32()?);
    let video_width = optional_u32(cursor.u32()?);
    let video_height = optional_u32(cursor.u32()?);
    let permissions = optional_u32(cursor.u32()?);
    let owner = optional_u32(cursor.u32()?);
    let group = optional_u32(cursor.u32()?);
    let image_orientation = cursor
        .optional_bytes()?
        .map(|value| value.to_vec().into_boxed_slice());
    let audio_artist = cursor
        .optional_bytes()?
        .map(|value| value.to_vec().into_boxed_slice());
    let audio_album = cursor
        .optional_bytes()?
        .map(|value| value.to_vec().into_boxed_slice());
    let audio_genre = cursor
        .optional_bytes()?
        .map(|value| value.to_vec().into_boxed_slice());
    let link_destination = cursor
        .optional_bytes()?
        .map(|value| OsString::from_vec(value.to_vec()));
    Ok(IndexedSortMetadata {
        word_count,
        line_count,
        image_width,
        image_height,
        image_orientation,
        audio_artist,
        audio_album,
        audio_duration_millis,
        audio_track,
        audio_genre,
        audio_bitrate,
        video_duration_millis,
        video_width,
        video_height,
        video_frame_rate_milli,
        video_bitrate,
        link_destination,
        permissions,
        owner,
        group,
    })
}

fn put_u32(bytes: &mut Vec<u8>, value: usize) -> Result<(), MetadataIndexError> {
    bytes.extend_from_slice(
        &u32::try_from(value)
            .map_err(|_| MetadataIndexError::Capacity)?
            .to_le_bytes(),
    );
    Ok(())
}

fn put_bytes(bytes: &mut Vec<u8>, value: &[u8]) -> Result<(), MetadataIndexError> {
    if value.is_empty() || value.len() > PATH_CAPACITY {
        return Err(MetadataIndexError::Capacity);
    }
    put_u32(bytes, value.len())?;
    bytes.extend_from_slice(value);
    Ok(())
}

fn put_optional_bytes(bytes: &mut Vec<u8>, value: Option<&[u8]>) -> Result<(), MetadataIndexError> {
    match value {
        Some(value) => {
            put_u32(bytes, value.len())?;
            bytes.extend_from_slice(value);
        }
        None => bytes.extend_from_slice(&u32::MAX.to_le_bytes()),
    }
    Ok(())
}

fn optional_u64(value: u64) -> Option<u64> {
    (value != u64::MAX).then_some(value)
}
fn optional_u32(value: u32) -> Option<u32> {
    (value != u32::MAX).then_some(value)
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], MetadataIndexError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(MetadataIndexError::Corrupt)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(MetadataIndexError::Corrupt)?;
        self.offset = end;
        Ok(value)
    }
    fn u32(&mut self) -> Result<u32, MetadataIndexError> {
        Ok(u32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| MetadataIndexError::Corrupt)?,
        ))
    }
    fn u64(&mut self) -> Result<u64, MetadataIndexError> {
        Ok(u64::from_le_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| MetadataIndexError::Corrupt)?,
        ))
    }
    fn i64(&mut self) -> Result<i64, MetadataIndexError> {
        Ok(i64::from_le_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| MetadataIndexError::Corrupt)?,
        ))
    }
    fn bytes(&mut self) -> Result<&'a [u8], MetadataIndexError> {
        let length = self.u32()? as usize;
        if length == 0 || length > PATH_CAPACITY {
            return Err(MetadataIndexError::Capacity);
        }
        self.take(length)
    }
    fn optional_bytes(&mut self) -> Result<Option<&'a [u8]>, MetadataIndexError> {
        let length = self.u32()?;
        if length == u32::MAX {
            return Ok(None);
        }
        let length = length as usize;
        if length > PATH_CAPACITY {
            return Err(MetadataIndexError::Capacity);
        }
        self.take(length).map(Some)
    }
}

fn validate_source_path(path: &Path) -> Result<(), MetadataIndexError> {
    if !path.is_absolute()
        || path.file_name().is_none()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(MetadataIndexError::UnsafeSource);
    }
    Ok(())
}

fn validate_private_file(path: &Path, metadata: &fs::Metadata) -> Result<(), MetadataIndexError> {
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != rustix::process::getuid().as_raw()
    {
        return Err(MetadataIndexError::UnsafeStorage);
    }
    let parent = path.parent().ok_or(MetadataIndexError::UnsafeStorage)?;
    validate_private_directory(parent)
}

fn ensure_private_directory(path: &Path) -> Result<(), MetadataIndexError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder.create(path).map_err(MetadataIndexError::Io)?;
    validate_private_directory(path)
}

fn validate_private_directory(path: &Path) -> Result<(), MetadataIndexError> {
    let metadata = fs::symlink_metadata(path).map_err(MetadataIndexError::Io)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != rustix::process::getuid().as_raw()
    {
        return Err(MetadataIndexError::UnsafeStorage);
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use floe_core::{DirectorySort, SortDirection, enumerate_directory};
    use std::{ffi::OsString, os::unix::fs::symlink};
    use tempfile::tempdir;

    fn entry(path: PathBuf) -> Arc<DirectoryEntry> {
        let parent = path.parent().unwrap();
        Arc::new(
            enumerate_directory(parent)
                .unwrap()
                .into_entries()
                .into_iter()
                .find(|entry| entry.path() == path)
                .unwrap(),
        )
    }

    #[test]
    fn phase_20b1a_extract_counts_text_and_reads_no_follow_filesystem_facts() {
        let root = tempdir().unwrap();
        let text = root.path().join("notes.txt");
        fs::write(&text, b"one two\nthree\n").unwrap();
        let mut facts = IndexedSortMetadata::default();
        let mut budget = TEXT_TOTAL_CAPACITY;
        extract_text(&text, &mut facts, &mut budget).unwrap();
        assert_eq!((facts.word_count, facts.line_count), (Some(3), Some(2)));
        let link = root.path().join("link");
        symlink(OsString::from("notes.txt"), &link).unwrap();
        let mut link_facts = IndexedSortMetadata::default();
        extract_other(&link, &mut link_facts).unwrap();
        assert_eq!(
            link_facts.link_destination,
            Some(OsString::from("notes.txt"))
        );
        let mut linked_text = IndexedSortMetadata::default();
        extract_text(&link, &mut linked_text, &mut budget).unwrap();
        assert_eq!(linked_text.word_count, None);
    }

    #[test]
    fn phase_20b1a_cache_round_trips_private_raw_paths_and_invalidates() {
        let root = tempdir().unwrap();
        let private = root.path().join("private");
        fs::create_dir(&private).unwrap();
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700)).unwrap();
        let source = root.path().join(OsString::from_vec(vec![b'f', 0xff]));
        fs::write(&source, b"one two").unwrap();
        let stamp = FileStamp::current(&source).unwrap();
        let mut cache = MetadataCache::default();
        cache.insert(
            source.clone(),
            stamp,
            TEXT,
            IndexedSortMetadata {
                word_count: Some(2),
                ..Default::default()
            },
        );
        let cache_path = private.join("cache");
        cache.persist(&cache_path).unwrap();
        assert_eq!(
            fs::metadata(&cache_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let mut loaded = MetadataCache::load(&cache_path).unwrap();
        assert_eq!(
            loaded.value(&source, stamp, TEXT).unwrap().word_count,
            Some(2)
        );
        loaded.invalidate(std::slice::from_ref(&source));
        assert!(loaded.value(&source, stamp, TEXT).is_none());

        fs::write(&cache_path, b"broken").unwrap();
        fs::set_permissions(&cache_path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(
            MetadataCache::load(&cache_path),
            Err(MetadataIndexError::Corrupt)
        ));
        fs::write(&cache_path, MAGIC).unwrap();
        fs::set_permissions(&cache_path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            MetadataCache::load(&cache_path),
            Err(MetadataIndexError::UnsafeStorage)
        ));
        fs::remove_file(&cache_path).unwrap();
        let target = private.join("target");
        fs::write(&target, MAGIC).unwrap();
        symlink(&target, &cache_path).unwrap();
        assert!(matches!(
            MetadataCache::load(&cache_path),
            Err(MetadataIndexError::UnsafeStorage)
        ));
    }

    #[test]
    fn phase_20b1a_extract_worker_sorts_and_reuses_cache() {
        let root = tempdir().unwrap();
        let first = root.path().join("first.txt");
        let second = root.path().join("second.txt");
        fs::write(&first, b"one").unwrap();
        fs::write(&second, b"one two three").unwrap();
        let mut cache = MetadataCache::default();
        let generation = AtomicU64::new(1);
        let (sender, _receiver) = mpsc::sync_channel(32);
        let sort = DirectorySort::new(SortColumn::DocumentWordCount, SortDirection::Descending);
        let sorted = index_and_sort(
            vec![entry(first), entry(second)],
            sort,
            &mut cache,
            None,
            &generation,
            1,
            &sender,
        )
        .unwrap();
        assert_eq!(sorted[0].display_name_lossy(), "second.txt");
        assert_eq!(cache.entries.len(), 2);
        generation.store(2, Ordering::Release);
        let cancelled_entry = entry(cache.entries.keys().next().unwrap().clone());
        assert!(matches!(
            index_and_sort(
                vec![cancelled_entry],
                sort,
                &mut cache,
                None,
                &generation,
                1,
                &sender
            ),
            Err(MetadataIndexError::Cancelled)
        ));
    }
}
