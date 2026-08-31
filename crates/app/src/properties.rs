//! Bounded read-only Properties aggregation over Inspector and GIO facts.

use std::{
    collections::VecDeque,
    ffi::OsString,
    mem::MaybeUninit,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

use floe_core::{
    PermissionChange, PermissionIdentity, PermissionRequest, PermissionRequestError,
    PermissionScope,
};
use gtk::{gio, gio::prelude::*};
use rustix::fs::{AtFlags, FileType, Mode, OFlags, RawDir};
use thiserror::Error;

use crate::advanced_metadata::AdvancedMetadataState;
use crate::inspector::{
    ImageDimensionFacts, InspectorFacts, InspectorRequest, InspectorRequestError,
    SymlinkTargetStatus, collect_inspector_facts,
};

pub const PROPERTIES_QUEUE_CAPACITY: usize = 8;
pub const PROPERTIES_RESULT_CAPACITY: usize = 8;
pub const RECURSIVE_ENTRY_CAPACITY: usize = 250_000;
const RECURSIVE_DEPTH_CAPACITY: usize = 1_024;
const FILESYSTEM_ATTRIBUTES: &str =
    "filesystem::type,filesystem::size,filesystem::free,filesystem::readonly";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertiesRequest {
    pub generation: u64,
    pub inspector: InspectorRequest,
}

impl PropertiesRequest {
    pub fn new(inspector: InspectorRequest) -> Result<Self, PropertiesError> {
        if inspector.generation == 0 {
            return Err(PropertiesError::InvalidRequest);
        }
        Ok(Self {
            generation: inspector.generation,
            inspector,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemProperties {
    pub queried_path: PathBuf,
    pub filesystem_type: Option<String>,
    pub total: Option<u64>,
    pub free: Option<u64>,
    pub read_only: Option<bool>,
    pub mount_root: Option<PathBuf>,
    pub mount_name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertiesSnapshot {
    pub inspector: InspectorFacts,
    pub filesystem: Result<FilesystemProperties, String>,
    pub recursive_folders: Arc<[RecursiveFolderProperties]>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecursiveFolderProperties {
    pub path: PathBuf,
    pub entries: usize,
    pub regular_files: usize,
    pub directories: usize,
    pub symbolic_links: usize,
    pub known_bytes: u64,
    pub unreadable_entries: usize,
    pub bytes_overflowed: bool,
    pub truncated: bool,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PropertiesError {
    #[error("Properties request is invalid")]
    InvalidRequest,
    #[error("Properties request was superseded")]
    Superseded,
    #[error("Inspector metadata failed: {0}")]
    Inspector(InspectorRequestError),
}

#[derive(Debug, Error)]
pub enum PropertiesSubmitError {
    #[error("Properties queue is full")]
    Full(PropertiesRequest),
    #[error("Properties worker disconnected")]
    Disconnected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertiesResponse {
    pub generation: u64,
    pub result: Result<PropertiesSnapshot, PropertiesError>,
}

pub struct PropertiesWorker {
    sender: Option<SyncSender<PropertiesRequest>>,
    responses: Arc<Mutex<VecDeque<PropertiesResponse>>>,
    latest_generation: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl PropertiesWorker {
    pub fn spawn() -> std::io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<PropertiesRequest>(PROPERTIES_QUEUE_CAPACITY);
        let responses = Arc::new(Mutex::new(VecDeque::with_capacity(
            PROPERTIES_RESULT_CAPACITY,
        )));
        let latest_generation = Arc::new(AtomicU64::new(0));
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_responses = Arc::clone(&responses);
        let worker_generation = Arc::clone(&latest_generation);
        let worker_shutdown = Arc::clone(&shutdown);
        let worker = thread::Builder::new()
            .name("floe-properties".to_owned())
            .spawn(move || {
                while let Ok(request) = requests.recv() {
                    if worker_shutdown.load(Ordering::Acquire) {
                        return;
                    }
                    let generation = request.generation;
                    let result = load_properties(request, &worker_generation, &worker_shutdown);
                    let Ok(mut queue) = worker_responses.lock() else {
                        return;
                    };
                    if queue.len() == PROPERTIES_RESULT_CAPACITY {
                        queue.pop_front();
                    }
                    queue.push_back(PropertiesResponse { generation, result });
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

    pub fn submit(&self, request: PropertiesRequest) -> Result<(), PropertiesSubmitError> {
        let Some(sender) = self.sender.as_ref() else {
            return Err(PropertiesSubmitError::Disconnected);
        };
        self.latest_generation
            .store(request.generation, Ordering::Release);
        match sender.try_send(request) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(request)) => Err(PropertiesSubmitError::Full(request)),
            Err(TrySendError::Disconnected(_)) => Err(PropertiesSubmitError::Disconnected),
        }
    }

    pub fn try_response(&self) -> Option<PropertiesResponse> {
        self.responses.lock().ok()?.pop_front()
    }

    pub fn supersede(&self, generation: u64) {
        self.latest_generation.store(generation, Ordering::Release);
    }
}

impl Drop for PropertiesWorker {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        self.latest_generation.fetch_add(1, Ordering::AcqRel);
        self.sender.take();
        // Recursive and mount metadata calls can stall indefinitely. Detach the
        // cancelled read-only worker instead of blocking GTK's main thread.
        self.worker.take();
    }
}

fn load_properties(
    request: PropertiesRequest,
    latest_generation: &AtomicU64,
    shutdown: &AtomicBool,
) -> Result<PropertiesSnapshot, PropertiesError> {
    if latest_generation.load(Ordering::Acquire) != request.generation
        || shutdown.load(Ordering::Acquire)
    {
        return Err(PropertiesError::Superseded);
    }
    let filesystem_path = request.inspector.directory.clone();
    let inspector = collect_inspector_facts(request.inspector, latest_generation, shutdown)
        .map_err(PropertiesError::Inspector)?;
    let filesystem =
        query_filesystem_properties(filesystem_path).map_err(|error| error.to_string());
    let mut budget = RECURSIVE_ENTRY_CAPACITY;
    let mut recursive_folders = Vec::new();
    for entry in inspector.metadata.iter() {
        if entry
            .result
            .as_ref()
            .is_ok_and(|facts| facts.folder.is_some())
        {
            recursive_folders.push(walk_recursive_folder(
                entry.path.clone(),
                request.generation,
                latest_generation,
                shutdown,
                &mut budget,
            ));
        }
    }
    if latest_generation.load(Ordering::Acquire) != request.generation
        || shutdown.load(Ordering::Acquire)
    {
        return Err(PropertiesError::Superseded);
    }
    Ok(PropertiesSnapshot {
        inspector,
        filesystem,
        recursive_folders: recursive_folders.into(),
    })
}

fn walk_recursive_folder(
    path: PathBuf,
    generation: u64,
    latest: &AtomicU64,
    shutdown: &AtomicBool,
    budget: &mut usize,
) -> RecursiveFolderProperties {
    let mut facts = RecursiveFolderProperties {
        path: path.clone(),
        ..Default::default()
    };
    let directory = rustix::fs::open(
        &path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    );
    match directory {
        Ok(directory) => walk_directory_fd(
            &directory, 0, generation, latest, shutdown, budget, &mut facts,
        ),
        Err(_) => facts.unreadable_entries = 1,
    }
    facts
}

fn walk_directory_fd(
    directory: &rustix::fd::OwnedFd,
    depth: usize,
    generation: u64,
    latest: &AtomicU64,
    shutdown: &AtomicBool,
    budget: &mut usize,
    facts: &mut RecursiveFolderProperties,
) {
    if depth >= RECURSIVE_DEPTH_CAPACITY {
        facts.truncated = true;
        return;
    }
    let mut buffer = [MaybeUninit::<u8>::uninit(); 8_192];
    let mut entries = RawDir::new(directory, &mut buffer);
    while let Some(entry) = entries.next() {
        if shutdown.load(Ordering::Acquire) || latest.load(Ordering::Acquire) != generation {
            facts.truncated = true;
            return;
        }
        let Ok(entry) = entry else {
            facts.unreadable_entries += 1;
            continue;
        };
        if entry.file_name().to_bytes() == b"." || entry.file_name().to_bytes() == b".." {
            continue;
        }
        if *budget == 0 {
            facts.truncated = true;
            return;
        }
        *budget -= 1;
        facts.entries += 1;
        let name = entry.file_name().to_owned();
        let stat = match rustix::fs::statat(directory, name.as_c_str(), AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) => stat,
            Err(_) => {
                facts.unreadable_entries += 1;
                continue;
            }
        };
        let file_type = FileType::from_raw_mode(stat.st_mode);
        if file_type.is_symlink() {
            facts.symbolic_links += 1;
        } else if file_type.is_file() {
            facts.regular_files += 1;
            let size = u64::try_from(stat.st_size).unwrap_or(u64::MAX);
            match facts.known_bytes.checked_add(size) {
                Some(total) => facts.known_bytes = total,
                None => {
                    facts.known_bytes = u64::MAX;
                    facts.bytes_overflowed = true;
                }
            }
        } else if file_type.is_dir() {
            facts.directories += 1;
            match rustix::fs::openat(
                directory,
                name.as_c_str(),
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
                Mode::empty(),
            ) {
                Ok(child) => walk_directory_fd(
                    &child,
                    depth + 1,
                    generation,
                    latest,
                    shutdown,
                    budget,
                    facts,
                ),
                Err(_) => facts.unreadable_entries += 1,
            }
        }
    }
}

fn query_filesystem_properties(path: PathBuf) -> Result<FilesystemProperties, glib::Error> {
    let file = gio::File::for_path(&path);
    let info = file.query_filesystem_info(FILESYSTEM_ATTRIBUTES, None::<&gio::Cancellable>)?;
    let filesystem_type = info
        .attribute_string(gio::FILE_ATTRIBUTE_FILESYSTEM_TYPE)
        .map(|value| value.to_string());
    let total = info
        .has_attribute(gio::FILE_ATTRIBUTE_FILESYSTEM_SIZE)
        .then(|| info.attribute_uint64(gio::FILE_ATTRIBUTE_FILESYSTEM_SIZE));
    let free = info
        .has_attribute(gio::FILE_ATTRIBUTE_FILESYSTEM_FREE)
        .then(|| info.attribute_uint64(gio::FILE_ATTRIBUTE_FILESYSTEM_FREE));
    let read_only = info
        .has_attribute(gio::FILE_ATTRIBUTE_FILESYSTEM_READONLY)
        .then(|| info.boolean(gio::FILE_ATTRIBUTE_FILESYSTEM_READONLY));
    let mount = file.find_enclosing_mount(None::<&gio::Cancellable>).ok();
    Ok(FilesystemProperties {
        queried_path: path,
        filesystem_type,
        total,
        free,
        read_only,
        mount_root: mount.as_ref().and_then(|mount| mount.root().path()),
        mount_name: mount.map(|mount| mount.name().to_string()),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyRow {
    pub label: &'static str,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertiesPresentation {
    pub title: String,
    pub general: Vec<PropertyRow>,
    pub filesystem: Vec<PropertyRow>,
    pub selection_count: usize,
    pub open_with_available: bool,
    pub checksum_available: bool,
    pub permissions: PermissionDefaults,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionDefaults {
    pub targets: Arc<[PathBuf]>,
    pub common_file_mode: Option<u32>,
    pub common_directory_mode: Option<u32>,
    pub common_uid: Option<u32>,
    pub common_gid: Option<u32>,
    pub has_files: bool,
    pub has_directories: bool,
    pub editable: bool,
}

pub fn checksum_targets_for_presentation(
    presentation: &PropertiesPresentation,
) -> Option<Arc<[PathBuf]>> {
    (presentation.checksum_available && presentation.permissions.targets.len() == 1)
        .then(|| Arc::clone(&presentation.permissions.targets))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutableEdit {
    Unchanged,
    Enable,
    Disable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionEditorInput {
    pub file_mode: String,
    pub directory_mode: String,
    pub executable: ExecutableEdit,
    pub owner: String,
    pub group: String,
    pub recursive: bool,
    pub acknowledged: bool,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PermissionEditorError {
    #[error("file mode must be an octal value from 0000 to 7777")]
    InvalidFileMode,
    #[error("directory mode must be an octal value from 0000 to 7777")]
    InvalidDirectoryMode,
    #[error("owner must be a numeric UID or valid local account name")]
    InvalidOwner,
    #[error("group must be a numeric GID or valid local group name")]
    InvalidGroup,
    #[error("explicit modes cannot be combined with an executable toggle")]
    AmbiguousMode,
    #[error("select at least one permission change")]
    NoChange,
    #[error("acknowledge recursive or ownership changes before applying")]
    ConfirmationRequired,
    #[error(transparent)]
    Request(#[from] PermissionRequestError),
}

pub fn build_permission_request(
    defaults: &PermissionDefaults,
    input: &PermissionEditorInput,
) -> Result<PermissionRequest, PermissionEditorError> {
    let file_mode = parse_mode(&input.file_mode).ok_or(PermissionEditorError::InvalidFileMode)?;
    let directory_mode =
        parse_mode(&input.directory_mode).ok_or(PermissionEditorError::InvalidDirectoryMode)?;
    let executable = match input.executable {
        ExecutableEdit::Unchanged => None,
        ExecutableEdit::Enable => Some(true),
        ExecutableEdit::Disable => Some(false),
    };
    if executable.is_some() && (file_mode.is_some() || directory_mode.is_some()) {
        return Err(PermissionEditorError::AmbiguousMode);
    }
    let owner = parse_identity(&input.owner).map_err(|_| PermissionEditorError::InvalidOwner)?;
    let group = parse_identity(&input.group).map_err(|_| PermissionEditorError::InvalidGroup)?;
    if file_mode.is_none()
        && directory_mode.is_none()
        && executable.is_none()
        && owner.is_none()
        && group.is_none()
    {
        return Err(PermissionEditorError::NoChange);
    }
    if (input.recursive || owner.is_some() || group.is_some()) && !input.acknowledged {
        return Err(PermissionEditorError::ConfirmationRequired);
    }
    let change = PermissionChange::new(file_mode, directory_mode, executable, owner, group)?;
    PermissionRequest::new(
        defaults.targets.to_vec(),
        if input.recursive {
            PermissionScope::Recursive
        } else {
            PermissionScope::Direct
        },
        change,
    )
    .map_err(Into::into)
}

fn parse_mode(value: &str) -> Option<Option<u32>> {
    let value = value.trim();
    if value.is_empty() {
        return Some(None);
    }
    (value.len() <= 4)
        .then(|| {
            u32::from_str_radix(value, 8)
                .ok()
                .filter(|mode| *mode <= 0o7777)
        })
        .flatten()
        .map(Some)
}

fn parse_identity(value: &str) -> Result<Option<PermissionIdentity>, PermissionRequestError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.as_bytes().iter().all(u8::is_ascii_digit) {
        return value
            .parse::<u32>()
            .map(PermissionIdentity::Id)
            .map(Some)
            .map_err(|_| PermissionRequestError::InvalidIdentityName(OsString::from(value)));
    }
    PermissionIdentity::local_name(OsString::from(value)).map(Some)
}

pub fn present(snapshot: &PropertiesSnapshot) -> PropertiesPresentation {
    let facts = &snapshot.inspector;
    let count = facts.selection_count();
    let title = if count == 1 {
        facts.selection_paths[0]
            .file_name()
            .unwrap_or_else(|| facts.selection_paths[0].as_os_str())
            .to_string_lossy()
            .into_owned()
    } else {
        format!("{count} Items")
    };
    let mut general = vec![
        row(
            "Selection",
            format!("{count} item{}", if count == 1 { "" } else { "s" }),
        ),
        row("Location", facts.common_parent.to_string_lossy()),
        row(
            "Kind",
            format!(
                "{} files, {} folders, {} links, {} other",
                facts.regular_files, facts.directories, facts.symbolic_links, facts.other_entries
            ),
        ),
        row(
            "Known size",
            format!(
                "{} bytes{}",
                facts.known_bytes,
                if facts.unknown_sizes > 0 {
                    " plus unknown sizes"
                } else {
                    ""
                }
            ),
        ),
    ];
    if count == 1 {
        general.push(row("Path", facts.selection_paths[0].to_string_lossy()));
        if let Some(result) = facts.metadata.first() {
            match &result.result {
                Ok(entry) => {
                    general.push(row(
                        "MIME type",
                        entry
                            .mime_type
                            .clone()
                            .unwrap_or_else(|| "Unknown".to_owned()),
                    ));
                    general.push(row(
                        "Unix identity",
                        match (entry.unix_uid, entry.unix_gid, entry.unix_mode) {
                            (Some(uid), Some(gid), Some(mode)) => {
                                format!("UID {uid} · GID {gid} · mode {:04o}", mode & 0o7777)
                            }
                            _ => "Unknown".to_owned(),
                        },
                    ));
                    general.push(row("Created", format_time(entry.created)));
                    general.push(row("Modified", format_time(entry.modified)));
                    general.push(row("Accessed", format_time(entry.accessed)));
                    if let ImageDimensionFacts::Dimensions(size) = entry.image_dimensions {
                        general.push(row(
                            "Dimensions",
                            format!("{} × {} pixels", size.width, size.height),
                        ));
                    }
                    match &entry.advanced_metadata {
                        AdvancedMetadataState::Present(metadata) => {
                            if let Some(exif) = &metadata.exif {
                                for field in exif.fields.iter() {
                                    general.push(row(field.label, field.value.clone()));
                                }
                                if exif.values_truncated {
                                    general.push(row("EXIF text", "Truncated by safety limits"));
                                }
                            }
                            if let Some(media) = &metadata.media {
                                if let Some(duration) = media.duration {
                                    general.push(row("Duration", format_media_duration(duration)));
                                }
                                for (label, value) in [
                                    ("Title", media.title.as_deref()),
                                    ("Artist", media.artist.as_deref()),
                                    ("Album", media.album.as_deref()),
                                    ("Genre", media.genre.as_deref()),
                                ] {
                                    if let Some(value) = value {
                                        general.push(row(label, value));
                                    }
                                }
                                if let Some(track) = media.track {
                                    general.push(row(
                                        "Track",
                                        media.track_total.map_or_else(
                                            || track.to_string(),
                                            |total| format!("{track} of {total}"),
                                        ),
                                    ));
                                }
                                if media.values_truncated {
                                    general
                                        .push(row("Media tag text", "Truncated by safety limits"));
                                }
                            }
                        }
                        AdvancedMetadataState::LimitExceeded => {
                            general.push(row("Advanced metadata", "Withheld by safety limits"))
                        }
                        AdvancedMetadataState::Malformed(error) => {
                            general.push(row("Advanced metadata", format!("Malformed: {error}")));
                        }
                        AdvancedMetadataState::Unsupported | AdvancedMetadataState::NoMetadata => {}
                    }
                    if let Some(link) = &entry.symlink {
                        let status = match link.status {
                            SymlinkTargetStatus::EntryPresent => "entry present",
                            SymlinkTargetStatus::Missing => "missing",
                            SymlinkTargetStatus::Inaccessible => "inaccessible",
                        };
                        general.push(row(
                            "Link target",
                            format!("{} ({status})", link.stored_target.to_string_lossy()),
                        ));
                    }
                    if let Some(folder) = &entry.folder {
                        general.push(row(
                            "Folder contents",
                            format!(
                                "{} immediate items · {} known bytes (non-recursive{})",
                                folder.inspected_children,
                                folder.known_immediate_bytes,
                                if folder.truncated { ", limited" } else { "" }
                            ),
                        ));
                    }
                }
                Err(error) => general.push(row("Metadata", format!("Unavailable: {error}"))),
            }
        }
    } else {
        let loaded = facts
            .metadata
            .iter()
            .filter(|entry| entry.result.is_ok())
            .count();
        general.push(row(
            "Metadata",
            format!(
                "{loaded} loaded · {} unavailable; differing values are not merged",
                facts.metadata.len().saturating_sub(loaded)
            ),
        ));
        if let Some(common) = common_value(facts.metadata.iter().map(|entry| {
            entry
                .result
                .as_ref()
                .ok()
                .and_then(|facts| facts.mime_type.clone())
        })) {
            general.push(row("Common MIME type", common));
        }
    }
    if !snapshot.recursive_folders.is_empty() {
        let folders = snapshot.recursive_folders.len();
        let entries = snapshot
            .recursive_folders
            .iter()
            .fold(0usize, |sum, item| sum.saturating_add(item.entries));
        let bytes = snapshot
            .recursive_folders
            .iter()
            .fold(0u64, |sum, item| sum.saturating_add(item.known_bytes));
        let limited = snapshot.recursive_folders.iter().any(|item| item.truncated);
        general.push(row(
            "Recursive folder totals",
            format!(
                "{folders} folder{} · {entries} entries · {bytes} known bytes{}",
                if folders == 1 { "" } else { "s" },
                if limited { " · limited" } else { "" }
            ),
        ));
    }
    let filesystem = match &snapshot.filesystem {
        Ok(fs) => vec![
            row("Containing path", fs.queried_path.to_string_lossy()),
            row(
                "Filesystem type",
                fs.filesystem_type
                    .clone()
                    .unwrap_or_else(|| "Unknown".to_owned()),
            ),
            row(
                "Capacity",
                match (fs.free, fs.total) {
                    (Some(free), Some(total)) => format!("{free} bytes free of {total}"),
                    (Some(free), None) => format!("{free} bytes free"),
                    (None, Some(total)) => format!("{total} bytes total"),
                    _ => "Unknown".to_owned(),
                },
            ),
            row(
                "Read-only",
                match fs.read_only {
                    Some(true) => "Yes",
                    Some(false) => "No",
                    None => "Unknown",
                },
            ),
            row(
                "Mount",
                match (&fs.mount_name, &fs.mount_root) {
                    (Some(name), Some(root)) => format!("{name} · {}", root.to_string_lossy()),
                    (Some(name), None) => name.clone(),
                    (None, Some(root)) => root.to_string_lossy().into_owned(),
                    _ => "Unknown".to_owned(),
                },
            ),
        ],
        Err(error) => vec![row("Filesystem", format!("Unavailable: {error}"))],
    };
    let mut file_modes = Vec::new();
    let mut directory_modes = Vec::new();
    let mut owners = Vec::new();
    let mut groups = Vec::new();
    let mut editable = facts.symbolic_links == 0 && facts.metadata.len() == count;
    for result in facts.metadata.iter() {
        match &result.result {
            Ok(entry) if entry.symlink.is_none() => {
                if entry.folder.is_some() {
                    if let Some(mode) = entry.unix_mode {
                        directory_modes.push(mode & 0o7777);
                    }
                } else if let Some(mode) = entry.unix_mode {
                    file_modes.push(mode & 0o7777);
                }
                if let Some(uid) = entry.unix_uid {
                    owners.push(uid);
                }
                if let Some(gid) = entry.unix_gid {
                    groups.push(gid);
                }
                editable &= entry.unix_mode.is_some();
            }
            _ => editable = false,
        }
    }
    let permissions = PermissionDefaults {
        targets: Arc::clone(&facts.selection_paths),
        common_file_mode: common_numeric(&file_modes),
        common_directory_mode: common_numeric(&directory_modes),
        common_uid: common_numeric(&owners),
        common_gid: common_numeric(&groups),
        has_files: !file_modes.is_empty(),
        has_directories: !directory_modes.is_empty(),
        editable,
    };
    PropertiesPresentation {
        title,
        general,
        filesystem,
        selection_count: count,
        open_with_available: count == 1 && facts.regular_files == 1,
        checksum_available: checksum_surface_available(count, facts.regular_files),
        permissions,
    }
}

fn checksum_surface_available(selection_count: usize, regular_files: usize) -> bool {
    selection_count == 1 && regular_files == 1
}

fn common_numeric(values: &[u32]) -> Option<u32> {
    let first = *values.first()?;
    values.iter().all(|value| *value == first).then_some(first)
}

fn row(label: &'static str, value: impl ToString) -> PropertyRow {
    PropertyRow {
        label,
        value: value.to_string(),
    }
}

fn format_time(value: Option<std::time::SystemTime>) -> String {
    use std::time::UNIX_EPOCH;

    let Some(value) = value else {
        return "Unknown".to_owned();
    };
    let seconds = match value.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).ok(),
        Err(error) => i64::try_from(error.duration().as_secs())
            .ok()
            .and_then(i64::checked_neg),
    };
    seconds
        .and_then(|seconds| glib::DateTime::from_unix_local(seconds).ok())
        .and_then(|local| local.format("%x · %T").ok())
        .map_or_else(|| "Unknown".to_owned(), |formatted| formatted.to_string())
}

fn format_media_duration(duration: std::time::Duration) -> String {
    let total = duration.as_secs();
    let hours = total / 3_600;
    let minutes = (total % 3_600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn common_value<T: Eq + Clone>(values: impl Iterator<Item = Option<T>>) -> Option<T> {
    let mut common = None;
    for value in values {
        let value = value?;
        if common.as_ref().is_some_and(|current| current != &value) {
            return None;
        }
        common.get_or_insert(value);
    }
    common
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_23f_checksum_surface_is_explicit_and_single_regular_file_only() {
        assert!(checksum_surface_available(1, 1));
        assert!(!checksum_surface_available(0, 0));
        assert!(!checksum_surface_available(1, 0));
        assert!(!checksum_surface_available(2, 2));
    }

    #[test]
    fn phase_23_reliability_properties_checksum_keeps_presented_path_identity() {
        let presented = PathBuf::from("/tmp/presented-a.txt");
        let later_selection = PathBuf::from("/tmp/later-selected-b.txt");
        let presentation = PropertiesPresentation {
            title: "Properties".to_owned(),
            general: Vec::new(),
            filesystem: Vec::new(),
            selection_count: 1,
            open_with_available: true,
            checksum_available: true,
            permissions: PermissionDefaults {
                targets: Arc::from([presented.clone()]),
                common_file_mode: None,
                common_directory_mode: None,
                common_uid: None,
                common_gid: None,
                has_files: true,
                has_directories: false,
                editable: true,
            },
        };

        let checksum_targets =
            checksum_targets_for_presentation(&presentation).expect("checksum target");
        assert_eq!(checksum_targets.as_ref(), [presented]);
        assert_ne!(checksum_targets[0], later_selection);
    }
    use floe_core::enumerate_directory;
    use std::{fs, sync::Arc, thread};
    use tempfile::tempdir;

    fn request(root: &std::path::Path, generation: u64) -> PropertiesRequest {
        let entries = enumerate_directory(root)
            .expect("listing")
            .into_entries()
            .into_iter()
            .map(Arc::new)
            .collect::<Vec<_>>();
        PropertiesRequest::new(
            InspectorRequest::from_entries(generation, root.to_path_buf(), &entries)
                .expect("inspector request"),
        )
        .expect("properties request")
    }

    #[test]
    fn phase_10c_properties_model_preserves_exact_single_and_truthful_multi_values() {
        let root = tempdir().expect("root");
        fs::write(root.path().join("a.txt"), b"a").expect("a");
        fs::write(root.path().join("b.txt"), b"bb").expect("b");
        let latest = AtomicU64::new(1);
        let shutdown = AtomicBool::new(false);
        let facts = collect_inspector_facts(request(root.path(), 1).inspector, &latest, &shutdown)
            .expect("facts");
        let presentation = present(&PropertiesSnapshot {
            inspector: facts,
            filesystem: Err("fixture".to_owned()),
            recursive_folders: Arc::from([]),
        });
        assert_eq!(presentation.title, "2 Items");
        assert!(
            presentation
                .general
                .iter()
                .any(|row| row.label == "Common MIME type" && row.value == "text/plain")
        );
        assert!(!presentation.open_with_available);
        assert!(
            presentation
                .general
                .iter()
                .all(|row| row.label != "Unix identity")
        );
    }

    #[test]
    fn phase_10c_filesystem_properties_worker_is_bounded_exact_and_generation_safe() {
        let root = tempdir().expect("root");
        fs::write(root.path().join("item"), b"data").expect("item");
        fs::create_dir(root.path().join("folder")).expect("folder");
        fs::create_dir(root.path().join("folder/nested")).expect("nested");
        fs::write(root.path().join("folder/nested/data"), b"12345").expect("nested data");
        let worker = PropertiesWorker::spawn().expect("worker");
        worker.submit(request(root.path(), 7)).expect("submit");
        let response = loop {
            if let Some(response) = worker.try_response() {
                break response;
            }
            thread::yield_now();
        };
        let snapshot = response.result.expect("snapshot");
        assert_eq!(
            snapshot.filesystem.expect("filesystem").queried_path,
            root.path()
        );
        assert_eq!(snapshot.recursive_folders.len(), 1);
        assert_eq!(snapshot.recursive_folders[0].regular_files, 1);
        assert_eq!(snapshot.recursive_folders[0].directories, 1);
        assert_eq!(snapshot.recursive_folders[0].known_bytes, 5);
    }

    #[test]
    fn phase_10f_advanced_metadata_ui_is_truthful_in_properties() {
        use crate::{
            advanced_metadata::{
                AdvancedMetadata, AdvancedMetadataState, ExifMetadata, MediaMetadata, MetadataField,
            },
            inspector::{
                ImageDimensionFacts, InspectorEntryFacts, InspectorEntryResult, InspectorFacts,
            },
        };

        let path = PathBuf::from("/tmp/track.flac");
        let snapshot_for = |advanced_metadata| PropertiesSnapshot {
            inspector: InspectorFacts {
                selection_paths: Arc::from([path.clone()]),
                regular_files: 1,
                directories: 0,
                symbolic_links: 0,
                other_entries: 0,
                known_bytes: 42,
                unknown_sizes: 0,
                bytes_overflowed: false,
                common_parent: PathBuf::from("/tmp"),
                metadata: Arc::from([InspectorEntryResult {
                    path: path.clone(),
                    result: Ok(InspectorEntryFacts {
                        path: path.clone(),
                        mime_type: Some("audio/flac".to_owned()),
                        created: None,
                        modified: None,
                        accessed: None,
                        unix_uid: None,
                        unix_gid: None,
                        unix_mode: None,
                        symlink: None,
                        image_dimensions: ImageDimensionFacts::NotImage,
                        advanced_metadata,
                        folder: None,
                    }),
                }]),
            },
            filesystem: Err("fixture".to_owned()),
            recursive_folders: Arc::from([]),
        };

        let presentation = present(&snapshot_for(AdvancedMetadataState::Present(
            AdvancedMetadata {
                exif: Some(ExifMetadata {
                    fields: Arc::from([MetadataField {
                        label: "Camera maker",
                        value: "FloeCam".to_owned(),
                    }]),
                    values_truncated: false,
                }),
                media: Some(MediaMetadata {
                    duration: Some(std::time::Duration::from_secs(65)),
                    artist: Some("Floe Artist".to_owned()),
                    ..MediaMetadata::default()
                }),
            },
        )));
        assert!(
            presentation
                .general
                .iter()
                .any(|row| row.label == "Camera maker" && row.value == "FloeCam")
        );
        assert!(
            presentation
                .general
                .iter()
                .any(|row| row.label == "Duration" && row.value == "1:05")
        );
        assert!(
            presentation
                .general
                .iter()
                .any(|row| row.label == "Artist" && row.value == "Floe Artist")
        );

        for (state, expected) in [
            (
                AdvancedMetadataState::LimitExceeded,
                "Withheld by safety limits",
            ),
            (
                AdvancedMetadataState::Malformed("invalid frame".to_owned()),
                "Malformed: invalid frame",
            ),
        ] {
            let presentation = present(&snapshot_for(state));
            let row = presentation
                .general
                .iter()
                .find(|row| row.label == "Advanced metadata")
                .expect("explicit advanced metadata state");
            assert_eq!(row.value, expected);
            assert!(!row.value.contains("verified"));
            assert!(!row.value.contains("malicious"));
        }
    }

    #[test]
    fn phase_10d_permissions_ui_validates_exact_jobs_and_risky_confirmation() {
        let defaults = PermissionDefaults {
            targets: Arc::from([PathBuf::from("/tmp/exact-a"), PathBuf::from("/tmp/exact-b")]),
            common_file_mode: Some(0o644),
            common_directory_mode: Some(0o755),
            common_uid: Some(1000),
            common_gid: Some(1000),
            has_files: true,
            has_directories: true,
            editable: true,
        };
        let mut input = PermissionEditorInput {
            file_mode: "0640".to_owned(),
            directory_mode: "0750".to_owned(),
            executable: ExecutableEdit::Unchanged,
            owner: "local-user".to_owned(),
            group: "100".to_owned(),
            recursive: true,
            acknowledged: false,
        };
        assert_eq!(
            build_permission_request(&defaults, &input),
            Err(PermissionEditorError::ConfirmationRequired)
        );
        input.acknowledged = true;
        let request = build_permission_request(&defaults, &input).expect("permission request");
        assert_eq!(request.targets(), defaults.targets.as_ref());
        assert_eq!(request.scope(), PermissionScope::Recursive);
        assert!(matches!(
            request.change().owner,
            Some(PermissionIdentity::LocalName(ref name)) if name == "local-user"
        ));
        assert_eq!(request.change().group, Some(PermissionIdentity::Id(100)));

        input.file_mode = "8888".to_owned();
        assert_eq!(
            build_permission_request(&defaults, &input),
            Err(PermissionEditorError::InvalidFileMode)
        );
        input.file_mode.clear();
        input.directory_mode.clear();
        input.owner.clear();
        input.group.clear();
        input.recursive = false;
        assert_eq!(
            build_permission_request(&defaults, &input),
            Err(PermissionEditorError::NoChange)
        );
    }
}
