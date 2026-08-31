//! Bounded, lazy, GTK-independent Inspector metadata providers.

use std::{
    collections::VecDeque,
    fs::{self, File, Metadata},
    io::BufReader,
    mem::MaybeUninit,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::SystemTime,
};

use floe_core::{DirectoryEntry, EntryKind};
use gtk::gio;
use image::{ImageDecoder, ImageReader, Limits};
use rustix::fs::{AtFlags, FileType, Mode, OFlags, RawDir};
use thiserror::Error;

use crate::advanced_metadata::{
    AdvancedMetadataError, AdvancedMetadataState, load_advanced_metadata,
};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

pub const INSPECTOR_SELECTION_CAPACITY: usize = 4_096;
pub const INSPECTOR_QUEUE_CAPACITY: usize = 16;
pub const INSPECTOR_RESULT_CAPACITY: usize = 16;
pub const INSPECTOR_FOLDER_ENTRY_CAPACITY: usize = 16_384;
const INSPECTOR_IMAGE_SOURCE_CAPACITY: u64 = 64 * 1024 * 1024;
const INSPECTOR_IMAGE_DECODED_CAPACITY: u64 = 256 * 1024 * 1024;
const INSPECTOR_IMAGE_DIMENSION_CAPACITY: u32 = 65_535;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectorEntryKey {
    path: PathBuf,
    kind: EntryKind,
    size: Option<u64>,
    modified: Option<SystemTime>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectorRequest {
    pub generation: u64,
    pub directory: PathBuf,
    pub entries: Arc<[InspectorEntryKey]>,
}

impl InspectorRequest {
    pub fn from_entries(
        generation: u64,
        directory: PathBuf,
        entries: &[Arc<DirectoryEntry>],
    ) -> Result<Self, InspectorRequestError> {
        if generation == 0 || entries.is_empty() || entries.len() > INSPECTOR_SELECTION_CAPACITY {
            return Err(InspectorRequestError::InvalidSelection);
        }
        let mut keys = Vec::with_capacity(entries.len());
        for entry in entries {
            if entry.path().parent() != Some(directory.as_path()) {
                return Err(InspectorRequestError::OutsideDirectory);
            }
            keys.push(InspectorEntryKey {
                path: entry.path().to_path_buf(),
                kind: entry.kind(),
                size: entry.size(),
                modified: entry.modified(),
            });
        }
        Ok(Self {
            generation,
            directory,
            entries: keys.into(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImageDimensionFacts {
    NotImage,
    Dimensions(ImageDimensions),
    Unavailable,
    LimitExceeded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymlinkTargetStatus {
    EntryPresent,
    Missing,
    Inaccessible,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymlinkFacts {
    pub stored_target: PathBuf,
    pub status: SymlinkTargetStatus,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FolderAggregate {
    pub inspected_children: usize,
    pub regular_files: usize,
    pub directories: usize,
    pub symbolic_links: usize,
    pub other_entries: usize,
    pub known_immediate_bytes: u64,
    pub unknown_sizes: usize,
    pub bytes_overflowed: bool,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectorEntryFacts {
    pub path: PathBuf,
    pub mime_type: Option<String>,
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub unix_uid: Option<u32>,
    pub unix_gid: Option<u32>,
    pub unix_mode: Option<u32>,
    pub symlink: Option<SymlinkFacts>,
    pub image_dimensions: ImageDimensionFacts,
    pub advanced_metadata: AdvancedMetadataState,
    pub folder: Option<FolderAggregate>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum InspectorEntryError {
    #[error("entry disappeared while metadata was loading")]
    Missing,
    #[error("entry changed while metadata was loading")]
    Changed,
    #[error("entry metadata is inaccessible: {0}")]
    Inaccessible(String),
    #[error("metadata request was superseded")]
    Superseded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectorEntryResult {
    pub path: PathBuf,
    pub result: Result<InspectorEntryFacts, InspectorEntryError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectorFacts {
    pub selection_paths: Arc<[PathBuf]>,
    pub regular_files: usize,
    pub directories: usize,
    pub symbolic_links: usize,
    pub other_entries: usize,
    pub known_bytes: u64,
    pub unknown_sizes: usize,
    pub bytes_overflowed: bool,
    pub common_parent: PathBuf,
    pub metadata: Arc<[InspectorEntryResult]>,
}

impl InspectorFacts {
    pub fn selection_count(&self) -> usize {
        self.selection_paths.len()
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum InspectorRequestError {
    #[error("Inspector selection is empty, oversized, or has an invalid generation")]
    InvalidSelection,
    #[error("Inspector selection contains an item outside its exact directory")]
    OutsideDirectory,
    #[error("Inspector request was superseded")]
    Superseded,
}

#[derive(Debug, Error)]
pub enum InspectorSubmitError {
    #[error("Inspector queue is full")]
    Full(InspectorRequest),
    #[error("Inspector worker disconnected")]
    Disconnected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectorResponse {
    pub generation: u64,
    pub result: Result<InspectorFacts, InspectorRequestError>,
}

pub struct InspectorWorker {
    sender: Option<SyncSender<InspectorRequest>>,
    responses: Arc<Mutex<VecDeque<InspectorResponse>>>,
    latest_generation: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl InspectorWorker {
    pub fn spawn() -> std::io::Result<Self> {
        Self::spawn_internal(None)
    }

    fn spawn_internal(start_gate: Option<Receiver<()>>) -> std::io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<InspectorRequest>(INSPECTOR_QUEUE_CAPACITY);
        let responses = Arc::new(Mutex::new(VecDeque::with_capacity(
            INSPECTOR_RESULT_CAPACITY,
        )));
        let latest_generation = Arc::new(AtomicU64::new(0));
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_responses = Arc::clone(&responses);
        let worker_generation = Arc::clone(&latest_generation);
        let worker_shutdown = Arc::clone(&shutdown);
        let worker = thread::Builder::new()
            .name("floe-inspector".to_owned())
            .spawn(move || {
                if start_gate.is_some_and(|gate| gate.recv().is_err()) {
                    return;
                }
                while let Ok(request) = requests.recv() {
                    if worker_shutdown.load(Ordering::Acquire) {
                        return;
                    }
                    let generation = request.generation;
                    let result =
                        collect_inspector_facts(request, &worker_generation, &worker_shutdown);
                    let response = InspectorResponse { generation, result };
                    let Ok(mut queue) = worker_responses.lock() else {
                        return;
                    };
                    if queue.len() == INSPECTOR_RESULT_CAPACITY {
                        queue.pop_front();
                    }
                    queue.push_back(response);
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            responses,
            latest_generation,
            shutdown,
            worker: Some(worker),
        })
    }

    pub fn submit(&self, request: InspectorRequest) -> Result<(), InspectorSubmitError> {
        let Some(sender) = self.sender.as_ref() else {
            return Err(InspectorSubmitError::Disconnected);
        };
        self.latest_generation
            .store(request.generation, Ordering::Release);
        match sender.try_send(request) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(request)) => Err(InspectorSubmitError::Full(request)),
            Err(TrySendError::Disconnected(_)) => Err(InspectorSubmitError::Disconnected),
        }
    }

    pub fn try_response(&self) -> Option<InspectorResponse> {
        self.responses.lock().ok()?.pop_front()
    }
}

impl Drop for InspectorWorker {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        self.latest_generation.fetch_add(1, Ordering::AcqRel);
        self.sender.take();
        // Cancellation is cooperative, but an in-flight filesystem call is not.
        // Dropping JoinHandle detaches safely and keeps GTK responsive.
        self.worker.take();
    }
}

pub(crate) fn collect_inspector_facts(
    request: InspectorRequest,
    latest_generation: &AtomicU64,
    shutdown: &AtomicBool,
) -> Result<InspectorFacts, InspectorRequestError> {
    if request.entries.is_empty() || request.entries.len() > INSPECTOR_SELECTION_CAPACITY {
        return Err(InspectorRequestError::InvalidSelection);
    }
    if cancelled(request.generation, latest_generation, shutdown) {
        return Err(InspectorRequestError::Superseded);
    }

    let mut facts = InspectorFacts {
        selection_paths: request
            .entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>()
            .into(),
        regular_files: 0,
        directories: 0,
        symbolic_links: 0,
        other_entries: 0,
        known_bytes: 0,
        unknown_sizes: 0,
        bytes_overflowed: false,
        common_parent: request.directory.clone(),
        metadata: Arc::from([]),
    };

    for entry in request.entries.iter() {
        if entry.path.parent() != Some(request.directory.as_path()) {
            return Err(InspectorRequestError::OutsideDirectory);
        }
        match entry.kind {
            EntryKind::RegularFile => facts.regular_files += 1,
            EntryKind::Directory => facts.directories += 1,
            EntryKind::SymbolicLink { .. } => facts.symbolic_links += 1,
            _ => facts.other_entries += 1,
        }
        if let Some(size) = entry.size {
            accumulate_bytes(&mut facts.known_bytes, &mut facts.bytes_overflowed, size);
        } else {
            facts.unknown_sizes += 1;
        }
    }

    let mut folder_budget = INSPECTOR_FOLDER_ENTRY_CAPACITY;
    let mut metadata = Vec::with_capacity(request.entries.len());
    for entry in request.entries.iter() {
        if cancelled(request.generation, latest_generation, shutdown) {
            return Err(InspectorRequestError::Superseded);
        }
        metadata.push(InspectorEntryResult {
            path: entry.path.clone(),
            result: collect_entry_facts(
                entry,
                request.generation,
                latest_generation,
                shutdown,
                &mut folder_budget,
            ),
        });
    }
    facts.metadata = metadata.into();
    Ok(facts)
}

fn collect_entry_facts(
    key: &InspectorEntryKey,
    generation: u64,
    latest_generation: &AtomicU64,
    shutdown: &AtomicBool,
    folder_budget: &mut usize,
) -> Result<InspectorEntryFacts, InspectorEntryError> {
    let before = fs::symlink_metadata(&key.path).map_err(map_entry_io_error)?;
    validate_listing_identity(key, &before)?;
    let identity = SourceIdentity::from_metadata(&before);
    let (content_type, _) = gio::content_type_guess(Some(&key.path), None::<&[u8]>);
    let mime_type = (!content_type.is_empty()).then(|| content_type.to_string());

    let symlink = if before.file_type().is_symlink() {
        let stored_target = fs::read_link(&key.path).map_err(map_entry_io_error)?;
        let target_path = if stored_target.is_absolute() {
            stored_target.clone()
        } else {
            key.path
                .parent()
                .unwrap_or_else(|| Path::new("/"))
                .join(&stored_target)
        };
        let status = match fs::symlink_metadata(target_path) {
            Ok(_) => SymlinkTargetStatus::EntryPresent,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                SymlinkTargetStatus::Missing
            }
            Err(_) => SymlinkTargetStatus::Inaccessible,
        };
        Some(SymlinkFacts {
            stored_target,
            status,
        })
    } else {
        None
    };

    let image_dimensions = if before.is_file()
        && mime_type
            .as_deref()
            .is_some_and(|value| value.starts_with("image/"))
    {
        load_image_dimensions(&key.path, &before)
    } else {
        ImageDimensionFacts::NotImage
    };
    let advanced_metadata = if before.is_file() {
        load_advanced_metadata(&key.path, key.size, key.modified).map_err(|error| match error {
            AdvancedMetadataError::Missing => InspectorEntryError::Missing,
            AdvancedMetadataError::Changed | AdvancedMetadataError::NotRegular => {
                InspectorEntryError::Changed
            }
            AdvancedMetadataError::Inaccessible(message) => {
                InspectorEntryError::Inaccessible(message)
            }
        })?
    } else {
        AdvancedMetadataState::Unsupported
    };

    let folder = if before.is_dir() {
        Some(aggregate_folder(
            &key.path,
            generation,
            latest_generation,
            shutdown,
            folder_budget,
        )?)
    } else {
        None
    };

    if cancelled(generation, latest_generation, shutdown) {
        return Err(InspectorEntryError::Superseded);
    }
    let after = fs::symlink_metadata(&key.path).map_err(map_entry_io_error)?;
    if identity != SourceIdentity::from_metadata(&after) {
        return Err(InspectorEntryError::Changed);
    }

    #[cfg(unix)]
    let (unix_uid, unix_gid, unix_mode) =
        (Some(before.uid()), Some(before.gid()), Some(before.mode()));
    #[cfg(not(unix))]
    let (unix_uid, unix_gid, unix_mode) = (None, None, None);

    Ok(InspectorEntryFacts {
        path: key.path.clone(),
        mime_type,
        created: before.created().ok(),
        modified: before.modified().ok(),
        accessed: before.accessed().ok(),
        unix_uid,
        unix_gid,
        unix_mode,
        symlink,
        image_dimensions,
        advanced_metadata,
        folder,
    })
}

fn aggregate_folder(
    path: &Path,
    generation: u64,
    latest_generation: &AtomicU64,
    shutdown: &AtomicBool,
    remaining_budget: &mut usize,
) -> Result<FolderAggregate, InspectorEntryError> {
    let directory = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|error| InspectorEntryError::Inaccessible(error.to_string()))?;
    let mut buffer = [MaybeUninit::<u8>::uninit(); 8_192];
    let mut entries = RawDir::new(&directory, &mut buffer);
    let mut aggregate = FolderAggregate::default();
    while let Some(entry) = entries.next() {
        if cancelled(generation, latest_generation, shutdown) {
            return Err(InspectorEntryError::Superseded);
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                aggregate.other_entries += 1;
                aggregate.unknown_sizes += 1;
                continue;
            }
        };
        if entry.file_name().to_bytes() == b"." || entry.file_name().to_bytes() == b".." {
            continue;
        }
        if *remaining_budget == 0 {
            aggregate.truncated = true;
            break;
        }
        *remaining_budget -= 1;
        aggregate.inspected_children += 1;
        let child =
            match rustix::fs::statat(&directory, entry.file_name(), AtFlags::SYMLINK_NOFOLLOW) {
                Ok(metadata) => metadata,
                Err(_) => {
                    aggregate.other_entries += 1;
                    aggregate.unknown_sizes += 1;
                    continue;
                }
            };
        let file_type = FileType::from_raw_mode(child.st_mode);
        if file_type.is_symlink() {
            aggregate.symbolic_links += 1;
        } else if file_type.is_dir() {
            aggregate.directories += 1;
        } else if file_type.is_file() {
            aggregate.regular_files += 1;
            let child_size = u64::try_from(child.st_size).unwrap_or(u64::MAX);
            accumulate_bytes(
                &mut aggregate.known_immediate_bytes,
                &mut aggregate.bytes_overflowed,
                child_size,
            );
        } else {
            aggregate.other_entries += 1;
            aggregate.unknown_sizes += 1;
        }
    }
    Ok(aggregate)
}

fn accumulate_bytes(total: &mut u64, overflowed: &mut bool, next: u64) {
    if let Some(sum) = total.checked_add(next) {
        *total = sum;
    } else {
        *total = u64::MAX;
        *overflowed = true;
    }
}

pub(crate) fn load_image_dimensions(path: &Path, metadata: &Metadata) -> ImageDimensionFacts {
    if metadata.len() > INSPECTOR_IMAGE_SOURCE_CAPACITY {
        return ImageDimensionFacts::LimitExceeded;
    }
    let descriptor = match rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    ) {
        Ok(descriptor) => descriptor,
        Err(_) => return ImageDimensionFacts::Unavailable,
    };
    let source = File::from(descriptor);
    let source_metadata = match source.metadata() {
        Ok(current)
            if SourceIdentity::from_metadata(&current)
                == SourceIdentity::from_metadata(metadata) =>
        {
            current
        }
        _ => return ImageDimensionFacts::Unavailable,
    };
    if !source_metadata.is_file() {
        return ImageDimensionFacts::Unavailable;
    }
    let mut reader = ImageReader::new(BufReader::new(source));
    reader = match reader.with_guessed_format() {
        Ok(reader) => reader,
        Err(_) => return ImageDimensionFacts::Unavailable,
    };
    if reader.format().is_none() {
        return ImageDimensionFacts::Unavailable;
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(INSPECTOR_IMAGE_DIMENSION_CAPACITY);
    limits.max_image_height = Some(INSPECTOR_IMAGE_DIMENSION_CAPACITY);
    limits.max_alloc = Some(INSPECTOR_IMAGE_DECODED_CAPACITY);
    reader.limits(limits);
    let decoder = match reader.into_decoder() {
        Ok(decoder) => decoder,
        Err(_) => return ImageDimensionFacts::Unavailable,
    };
    let (width, height) = decoder.dimensions();
    if width == 0
        || height == 0
        || width > INSPECTOR_IMAGE_DIMENSION_CAPACITY
        || height > INSPECTOR_IMAGE_DIMENSION_CAPACITY
        || decoder.total_bytes() > INSPECTOR_IMAGE_DECODED_CAPACITY
    {
        return ImageDimensionFacts::LimitExceeded;
    }
    ImageDimensionFacts::Dimensions(ImageDimensions { width, height })
}

fn validate_listing_identity(
    key: &InspectorEntryKey,
    metadata: &Metadata,
) -> Result<(), InspectorEntryError> {
    let current_kind = if metadata.file_type().is_symlink() {
        EntryKind::SymbolicLink {
            target_is_directory: false,
        }
    } else if metadata.is_dir() {
        EntryKind::Directory
    } else if metadata.is_file() {
        EntryKind::RegularFile
    } else {
        EntryKind::Other
    };
    let current_size = metadata.is_file().then_some(metadata.len());
    let current_modified = metadata.modified().ok();
    let same_kind = matches!(
        (key.kind, current_kind),
        (EntryKind::Directory, EntryKind::Directory)
            | (EntryKind::RegularFile, EntryKind::RegularFile)
            | (
                EntryKind::SymbolicLink { .. },
                EntryKind::SymbolicLink { .. }
            )
            | (EntryKind::Other, EntryKind::Other)
    );
    if !same_kind || current_size != key.size || current_modified != key.modified {
        return Err(InspectorEntryError::Changed);
    }
    Ok(())
}

fn map_entry_io_error(error: std::io::Error) -> InspectorEntryError {
    match error.kind() {
        std::io::ErrorKind::NotFound => InspectorEntryError::Missing,
        _ => InspectorEntryError::Inaccessible(error.to_string()),
    }
}

fn cancelled(generation: u64, latest_generation: &AtomicU64, shutdown: &AtomicBool) -> bool {
    shutdown.load(Ordering::Acquire) || latest_generation.load(Ordering::Acquire) != generation
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SourceIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    mode: u32,
    #[cfg(unix)]
    changed_seconds: i64,
    #[cfg(unix)]
    changed_nanoseconds: i64,
    len: u64,
    modified: Option<SystemTime>,
}

impl SourceIdentity {
    fn from_metadata(metadata: &Metadata) -> Self {
        Self {
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
            #[cfg(unix)]
            mode: metadata.mode(),
            #[cfg(unix)]
            changed_seconds: metadata.ctime(),
            #[cfg(unix)]
            changed_nanoseconds: metadata.ctime_nsec(),
            len: metadata.len(),
            modified: metadata.modified().ok(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::{ffi::OsStringExt, fs::symlink},
        sync::mpsc,
    };

    use floe_core::enumerate_directory;
    use image::{ImageBuffer, Rgba};
    use tempfile::tempdir;

    use super::*;

    fn list_entries(path: &Path) -> Vec<Arc<DirectoryEntry>> {
        enumerate_directory(path)
            .expect("listing")
            .into_entries()
            .into_iter()
            .map(Arc::new)
            .collect()
    }

    fn response(worker: &InspectorWorker) -> InspectorResponse {
        loop {
            if let Some(response) = worker.try_response() {
                return response;
            }
            thread::yield_now();
        }
    }

    #[test]
    fn phase_10a_inspector_worker_aggregates_raw_selection_on_bounded_thread() {
        let root = tempdir().expect("Inspector root");
        let raw = root
            .path()
            .join(std::ffi::OsString::from_vec(b"raw-\xff".to_vec()));
        fs::write(&raw, b"12345").expect("raw file");
        fs::create_dir(root.path().join("folder")).expect("folder");
        let entries = list_entries(root.path());
        let request = InspectorRequest::from_entries(7, root.path().to_path_buf(), &entries)
            .expect("request");
        let worker = InspectorWorker::spawn().expect("worker");
        worker.submit(request).expect("submit");
        let facts = response(&worker).result.expect("facts");
        assert_eq!(facts.selection_count(), 2);
        assert_eq!(facts.regular_files, 1);
        assert_eq!(facts.directories, 1);
        assert_eq!(facts.known_bytes, 5);
        assert!(
            facts
                .selection_paths
                .iter()
                .any(|path| path.as_os_str().as_encoded_bytes().contains(&0xff))
        );
        assert!(matches!(
            InspectorRequest::from_entries(0, root.path().to_path_buf(), &entries),
            Err(InspectorRequestError::InvalidSelection)
        ));
        let oversized =
            std::iter::repeat_n(Arc::clone(&entries[0]), INSPECTOR_SELECTION_CAPACITY + 1)
                .collect::<Vec<_>>();
        assert!(matches!(
            InspectorRequest::from_entries(8, root.path().to_path_buf(), &oversized),
            Err(InspectorRequestError::InvalidSelection)
        ));
    }

    #[test]
    fn phase_10b_metadata_worker_is_bounded_raw_path_and_superseding() {
        let root = tempdir().expect("root");
        fs::write(root.path().join("item"), b"data").expect("fixture");
        let entries = list_entries(root.path());
        let (gate_sender, gate_receiver) = mpsc::channel();
        let worker = InspectorWorker::spawn_internal(Some(gate_receiver)).expect("worker");
        for generation in 1..=INSPECTOR_QUEUE_CAPACITY {
            worker
                .submit(
                    InspectorRequest::from_entries(
                        generation as u64,
                        root.path().to_path_buf(),
                        &entries,
                    )
                    .expect("request"),
                )
                .expect("queued");
        }
        assert!(matches!(
            worker.submit(
                InspectorRequest::from_entries(99, root.path().to_path_buf(), &entries,)
                    .expect("request")
            ),
            Err(InspectorSubmitError::Full(_))
        ));
        gate_sender.send(()).expect("release");
        let first = response(&worker);
        assert_eq!(first.result, Err(InspectorRequestError::Superseded));
    }

    #[test]
    fn phase_10b_metadata_facts_cover_image_link_unix_and_disappearing_sources() {
        let root = tempdir().expect("root");
        let image_path = root.path().join("sample.png");
        ImageBuffer::<Rgba<u8>, _>::from_pixel(17, 9, Rgba([1, 2, 3, 255]))
            .save(&image_path)
            .expect("image");
        let raw_target = std::ffi::OsString::from_vec(b"target-\xff".to_vec());
        fs::write(root.path().join(&raw_target), b"target").expect("target");
        symlink(&raw_target, root.path().join("link")).expect("link");
        let entries = list_entries(root.path());
        let worker = InspectorWorker::spawn().expect("worker");
        worker
            .submit(
                InspectorRequest::from_entries(1, root.path().to_path_buf(), &entries)
                    .expect("request"),
            )
            .expect("submit");
        let facts = response(&worker).result.expect("facts");
        let image = facts
            .metadata
            .iter()
            .find(|result| result.path == image_path)
            .expect("image result")
            .result
            .as_ref()
            .expect("image facts");
        assert!(
            image
                .mime_type
                .as_deref()
                .is_some_and(|mime| mime.starts_with("image/"))
        );
        assert_eq!(
            image.image_dimensions,
            ImageDimensionFacts::Dimensions(ImageDimensions {
                width: 17,
                height: 9
            })
        );
        assert!(image.modified.is_some());
        assert!(image.unix_uid.is_some());
        assert!(image.unix_gid.is_some());
        assert!(image.unix_mode.is_some());
        let link = facts
            .metadata
            .iter()
            .find(|result| result.path.ends_with("link"))
            .expect("link result")
            .result
            .as_ref()
            .expect("link facts")
            .symlink
            .as_ref()
            .expect("symlink facts");
        assert_eq!(link.stored_target.as_os_str(), raw_target.as_os_str());
        assert_eq!(link.status, SymlinkTargetStatus::EntryPresent);

        let stale_entries = list_entries(root.path());
        fs::remove_file(&image_path).expect("remove");
        worker
            .submit(
                InspectorRequest::from_entries(2, root.path().to_path_buf(), &stale_entries)
                    .expect("request"),
            )
            .expect("submit");
        let facts = response(&worker)
            .result
            .expect("aggregate remains available");
        assert!(facts.metadata.iter().any(|entry| {
            entry.path == image_path && entry.result == Err(InspectorEntryError::Missing)
        }));
    }

    #[test]
    fn phase_10b_folder_aggregate_is_non_recursive_and_truthfully_bounded() {
        let root = tempdir().expect("root");
        let folder_path = root.path().join("folder");
        fs::create_dir(&folder_path).expect("folder");
        fs::write(folder_path.join("direct"), b"12345").expect("direct file");
        fs::create_dir(folder_path.join("nested")).expect("nested");
        fs::write(folder_path.join("nested").join("not-counted"), b"123456789")
            .expect("nested file");
        symlink("missing", folder_path.join("broken")).expect("broken link");
        let worker = InspectorWorker::spawn().expect("worker");
        worker
            .submit(
                InspectorRequest::from_entries(
                    1,
                    root.path().to_path_buf(),
                    &list_entries(root.path()),
                )
                .expect("request"),
            )
            .expect("submit");
        let facts = response(&worker).result.expect("facts");
        let folder = facts.metadata[0]
            .result
            .as_ref()
            .expect("folder facts")
            .folder
            .as_ref()
            .expect("aggregate");
        assert_eq!(folder.inspected_children, 3);
        assert_eq!(folder.regular_files, 1);
        assert_eq!(folder.directories, 1);
        assert_eq!(folder.symbolic_links, 1);
        assert_eq!(folder.known_immediate_bytes, 5);
        assert!(!folder.truncated);

        let latest = AtomicU64::new(1);
        let shutdown = AtomicBool::new(false);
        let mut budget = 1;
        let limited = aggregate_folder(&folder_path, 1, &latest, &shutdown, &mut budget)
            .expect("limited aggregate");
        assert_eq!(limited.inspected_children, 1);
        assert!(limited.truncated);
        let mut total = u64::MAX;
        let mut overflowed = false;
        accumulate_bytes(&mut total, &mut overflowed, 1);
        assert_eq!(total, u64::MAX);
        assert!(overflowed);
    }
}
