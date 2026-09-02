use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

use adw::prelude::*;
use floe_core::{
    ContentSearchMatch, DirectoryEntry, DirectoryGrouping, DirectorySort, EntryKind, SortColumn,
    SortDirection, SplitRatio, SplitSide,
};
use gtk::{gio, glib};

use crate::{
    appearance::{Appearance, AppearanceManager, AppearancePreset},
    context_menu::{ContextMenuGroup, ContextMenuPreferences},
    custom_actions::CustomActionDefinition,
    devices::{
        DeviceAction, DeviceActionStatus, DeviceActions, DeviceMountState, DeviceRootKind,
        DeviceSnapshot,
    },
    drag_drop::{DropDestination, DropDispatcher, install_drop_target},
    iconography::{EntryIcon, EntryIconStyle, LIST_ICON_EDGE, grid_icon_edge, icon_for_entry},
    launcher::OpenWithOptions,
    locations::Location,
    metadata::{LinkTargetStatus, MetadataCache, MetadataDetails, MetadataError, MetadataKey},
    miller_view::MillerView,
    preferences::{
        ClickPolicy, ColorSchemePreference, SIDEBAR_WIDTH_MIN, SidebarDensity, ViewPreferences,
        WindowSize, clamp_sidebar_width,
    },
    selection_slice::SelectionSlice,
    thumbnail::{LIST_THUMBNAIL_EDGE, ThumbnailError, ThumbnailKey, ThumbnailPixels},
    view::{FileViewDensity, GRID_SIZES, GridSize, ListColumn, ListColumnLayout, ViewMode},
};

pub const SIDEBAR_COMPACT_MIN_WIDTH: i32 = 128;
pub const SIDEBAR_COLLAPSED_WIDTH: i32 = 56;
pub const OPERATION_ISLAND_WIDTH: i32 = 340;
pub const OPERATION_ISLAND_INSET: i32 = 12;
const OPERATION_ISLAND_CANCEL_MIN_WIDTH: i32 = 40;
const OPERATION_ISLAND_ACTION_MIN_WIDTH: i32 = 72;
const SIDEBAR_DENSITY_MENU_ITEMS: [(&str, &str); 3] = [
    ("Compact", "win.sidebar-density::compact"),
    ("Balanced", "win.sidebar-density::balanced"),
    ("Comfortable", "win.sidebar-density::comfortable"),
];
const RESET_SIDEBAR_WIDTH_MENU_ITEM: (&str, &str) =
    ("Reset Sidebar Width", "win.reset-sidebar-width");
const OPERATION_HISTORY_MENU_ITEM: (&str, &str) = ("Operation History", "win.operation-history");
const SETTINGS_MENU_ITEM: (&str, &str) = ("Settings…", "win.settings");
const KEYBOARD_SHORTCUTS_MENU_ITEM: (&str, &str) =
    ("Keyboard Shortcuts…", "win.keyboard-shortcuts");
const DESKTOP_INTEGRATION_MENU_ITEM: (&str, &str) =
    ("Desktop Integration…", "win.desktop-integration-status");
pub const VIM_MODE_ON_LABEL: &str = "Vim On";
const FOLDER_FILTER_MODES: [&str; 3] = ["Text", "Glob", "Regex"];
const ADVANCED_TYPE_FILTERS: [&str; 5] =
    ["Any type", "Files", "Folders", "Symbolic links", "Other"];
const ADVANCED_SIZE_FILTERS: [&str; 5] =
    ["Any size", "Empty", "Under 1 MB", "1–100 MB", "Over 100 MB"];
const ADVANCED_DATE_FILTERS: [&str; 5] = [
    "Any date",
    "Last 24 hours",
    "Last 7 days",
    "Last 30 days",
    "Last year",
];
const ADVANCED_OWNER_FILTERS: [&str; 2] = ["Anyone", "Me"];
const ADVANCED_HIDDEN_FILTERS: [&str; 3] =
    ["Current hidden setting", "Include hidden", "Hidden only"];
pub(crate) const SEARCH_SURFACE_MODES: [&str; 3] =
    ["Quick Filter", "Search Files", "Search Contents"];
pub(crate) const SEARCH_SURFACE_MODE_HELP: [&str; 3] = [
    "Quick Filter narrows the items already shown in this folder. It does not search subfolders or read the filesystem again.",
    "Search Files finds filenames on disk in this folder or its subfolders. It never reads file contents.",
    "Search Contents explicitly reads bounded local text files in this folder or its subfolders. It skips binary, unsupported, remote, linked, and over-limit files.",
];
pub(crate) const SEARCH_RESULT_ORDER_LABELS: [&str; 3] =
    ["Name", "Modified (newest)", "Size (largest)"];
pub(crate) const SAVED_SEARCH_CONTROL_LABELS: [&str; 6] = [
    "Saved searches",
    "Recent searches (this session)",
    "Save search",
    "Delete saved",
    "Clear recent",
    "Search result order",
];
pub(crate) const SEARCH_INDEX_CAPABILITY_HELP: &str = "Optional private filenames and metadata only. Hidden entries and file contents are never indexed; stale, unavailable, or ineligible indexes fall back to complete live search.";
const FOLDER_FILTER_MODE_HELP: [&str; 3] = [
    "Contains these characters. Letter case is ignored unless Match case is enabled. Example: vacation finds My Vacation.jpg",
    "Uses wildcard patterns: * matches any characters and ? matches one character. Examples: *.pdf or photo-??.jpg",
    "Uses advanced regular-expression patterns. Example: ^invoice-[0-9]+\\.pdf$",
];
const FOLDER_FILTER_MODE_SUMMARIES: [&str; 3] = [
    "Contains text (case-insensitive)",
    "Wildcards such as *.pdf or photo-??.jpg",
    "Advanced regular-expression pattern",
];
pub const VIM_MODE_OFF_LABEL: &str = "Vim Off";
pub const VIM_MODE_TOOLTIP: &str = "Vim navigation mode: h/j/k/l, g/G, o";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperationIslandRow {
    TitleAndCancel,
    Detail,
    Progress,
    RecoveryActions,
}

const OPERATION_ISLAND_STRUCTURE: [OperationIslandRow; 4] = [
    OperationIslandRow::TitleAndCancel,
    OperationIslandRow::Detail,
    OperationIslandRow::Progress,
    OperationIslandRow::RecoveryActions,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OperationIslandLayout {
    outer_width: i32,
    inset: i32,
    cancel_min_width: i32,
    action_min_width: i32,
}

impl OperationIslandLayout {
    const CURRENT: Self = Self {
        outer_width: OPERATION_ISLAND_WIDTH,
        inset: OPERATION_ISLAND_INSET,
        cancel_min_width: OPERATION_ISLAND_CANCEL_MIN_WIDTH,
        action_min_width: OPERATION_ISLAND_ACTION_MIN_WIDTH,
    };

    const fn content_width(self) -> i32 {
        self.outer_width - (self.inset * 2)
    }

    const fn child_minimums_fit(self) -> bool {
        self.content_width() >= self.cancel_min_width
            && self.content_width() >= self.action_min_width
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SidebarDensityMetrics {
    section_gap: i32,
    row_gap: i32,
    outer_margin: i32,
}

const fn sidebar_density_metrics(density: SidebarDensity) -> SidebarDensityMetrics {
    match density {
        SidebarDensity::Compact => SidebarDensityMetrics {
            section_gap: 4,
            row_gap: 2,
            outer_margin: 6,
        },
        SidebarDensity::Balanced => SidebarDensityMetrics {
            section_gap: 8,
            row_gap: 2,
            outer_margin: 8,
        },
        SidebarDensity::Comfortable => SidebarDensityMetrics {
            section_gap: 12,
            row_gap: 2,
            outer_margin: 12,
        },
    }
}

fn sidebar_density_class(density: SidebarDensity) -> &'static str {
    match density {
        SidebarDensity::Compact => "sidebar-compact",
        SidebarDensity::Balanced => "sidebar-balanced",
        SidebarDensity::Comfortable => "sidebar-comfortable",
    }
}

fn initial_sidebar_width(preferences: ViewPreferences, appearance_default: i32) -> i32 {
    preferences
        .sidebar_width
        .map(clamp_sidebar_width)
        .map(i32::from)
        .unwrap_or(appearance_default)
}

fn initial_window_size(preferences: &ViewPreferences) -> WindowSize {
    preferences.window_size.unwrap_or_default()
}

fn sidebar_pane_resize_policy() -> (bool, bool) {
    (false, true)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceActivation {
    Navigate(PathBuf),
    Mount,
    Unavailable(&'static str),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceRowPolicy {
    pub status: String,
    pub activation: DeviceActivation,
    pub can_unmount: bool,
    pub can_eject: bool,
}

pub fn device_row_policy(snapshot: &DeviceSnapshot) -> DeviceRowPolicy {
    device_row_policy_for(
        snapshot.mount_state,
        snapshot.root_kind,
        snapshot.actions,
        snapshot.local_root(),
    )
}

fn device_row_policy_for(
    mount_state: DeviceMountState,
    root_kind: DeviceRootKind,
    actions: DeviceActions,
    local_root: Option<&std::path::Path>,
) -> DeviceRowPolicy {
    let busy_action = [
        DeviceAction::Mount,
        DeviceAction::Unmount,
        DeviceAction::Eject,
    ]
    .into_iter()
    .find(|action| actions.status(*action) == DeviceActionStatus::Busy);

    let activation = match (mount_state, root_kind) {
        (DeviceMountState::Mounted, DeviceRootKind::Local) => local_root
            .map(|path| DeviceActivation::Navigate(path.to_path_buf()))
            .unwrap_or(DeviceActivation::Unavailable(
                "The local mount path is unavailable.",
            )),
        (DeviceMountState::Mounted, DeviceRootKind::NonLocal) => {
            DeviceActivation::Unavailable("Remote and network locations are not supported yet.")
        }
        (DeviceMountState::Mounted, DeviceRootKind::Multiple) => {
            DeviceActivation::Unavailable("This drive has multiple mounted locations.")
        }
        (DeviceMountState::Mounted, DeviceRootKind::None) => {
            DeviceActivation::Unavailable("The mounted location is unavailable.")
        }
        (DeviceMountState::Unmounted, _) => match actions.mount {
            DeviceActionStatus::Available => DeviceActivation::Mount,
            DeviceActionStatus::Busy => {
                DeviceActivation::Unavailable("A storage action is already running.")
            }
            DeviceActionStatus::Unavailable(reason) => {
                DeviceActivation::Unavailable(reason.message(DeviceAction::Mount))
            }
        },
    };

    let status = if let Some(action) = busy_action {
        action.present_participle().to_owned()
    } else {
        match (mount_state, root_kind) {
            (DeviceMountState::Unmounted, _) if actions.mount == DeviceActionStatus::Available => {
                "Unmounted"
            }
            (DeviceMountState::Unmounted, _) => "Unavailable",
            (DeviceMountState::Mounted, DeviceRootKind::Local) => "Mounted",
            (DeviceMountState::Mounted, DeviceRootKind::NonLocal) => "Remote",
            (DeviceMountState::Mounted, DeviceRootKind::Multiple) => "Multiple locations",
            (DeviceMountState::Mounted, DeviceRootKind::None) => "Unavailable",
        }
        .to_owned()
    };

    DeviceRowPolicy {
        status,
        activation,
        can_unmount: actions.unmount == DeviceActionStatus::Available,
        can_eject: actions.eject == DeviceActionStatus::Available,
    }
}

#[cfg(test)]
pub fn bookmark_paths_after_remove(paths: &[PathBuf], index: usize) -> Option<Vec<PathBuf>> {
    if index >= paths.len() {
        return None;
    }
    let mut revised = paths.to_vec();
    revised.remove(index);
    Some(revised)
}

pub fn bookmark_actions_enabled(loaded: bool, save_in_flight: bool) -> bool {
    loaded && !save_in_flight
}

#[cfg(test)]
pub(crate) const FILE_CONTEXT_ACTIONS: [(&str, &str); 32] = [
    ("Open", "win.open"),
    ("Open With…", "win.open-with"),
    ("Copy", "win.copy"),
    ("Copy and Verify…", "win.copy-and-verify"),
    (
        "Verified Removable Transfer…",
        "win.verified-removable-transfer",
    ),
    ("Cut", "win.cut"),
    ("Duplicate", "win.duplicate"),
    ("Rename…", "win.rename"),
    ("Create Symbolic Link…", "win.create-symbolic-link"),
    ("Create Hard Link…", "win.create-hard-link"),
    ("Reveal Link Target", "win.reveal-link-target"),
    ("Copy Name", "win.copy-name"),
    ("Copy Path", "win.copy-path"),
    ("Copy Relative Path", "win.copy-relative-path"),
    ("Copy URI", "win.copy-uri"),
    ("Calculate Checksums…", "win.checksum"),
    ("Check for Duplicates…", "win.check-duplicates"),
    ("Move to Trash", "win.trash"),
    ("Delete Permanently…", "win.permanent-delete"),
    ("Properties", "win.properties"),
    ("Open Terminal Here", "win.open-terminal"),
    ("Extract Here", "win.extract-here"),
    ("Extract To…", "win.extract-to"),
    ("Compress…", "win.compress"),
    ("Batch Rename…", "win.batch-rename"),
    ("Undo Last Batch Rename", "win.undo-batch-rename"),
    ("Customize Context Menus…", "win.context-menu-settings"),
    ("Reveal in Folder", "win.reveal-in-folder"),
    ("Protect Folder", "win.protect-folder"),
    ("Unprotect Folder", "win.unprotect-folder"),
    ("Protected Folders…", "win.protected-folders"),
    ("Audit Permissions…", "win.audit-permissions"),
];
pub(crate) const TRASH_CONTEXT_ACTIONS: [(&str, &str); 4] = [
    ("Restore", "win.restore"),
    ("Calculate Checksums…", "win.checksum"),
    ("Delete Permanently…", "win.permanent-delete"),
    ("Properties", "win.properties"),
];
#[cfg(test)]
pub(crate) const BACKGROUND_CONTEXT_ACTIONS: [(&str, &str); 14] = [
    ("New Folder…", "win.new-folder"),
    ("New Empty File…", "win.new-empty-file"),
    ("New From Template…", "win.new-from-template"),
    ("Paste", "win.paste"),
    ("Select All", "win.select-all"),
    ("Invert Selection", "win.invert-selection"),
    ("Refresh", "win.refresh"),
    ("Edit Location", "win.location"),
    ("Open Terminal Here", "win.open-terminal"),
    ("Check for Duplicates…", "win.check-duplicates"),
    ("Customize Context Menus…", "win.context-menu-settings"),
    ("Protect Folder", "win.protect-folder"),
    ("Unprotect Folder", "win.unprotect-folder"),
    ("Protected Folders…", "win.protected-folders"),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContextSelection {
    Preserve,
    SelectOnly,
}

fn context_selection_for_secondary(already_selected: bool) -> ContextSelection {
    if already_selected {
        ContextSelection::Preserve
    } else {
        ContextSelection::SelectOnly
    }
}

const CONFLICT_DECISION_LABELS: [&str; 2] = ["Keep Existing", "Retry with New Name"];
const LIST_COLUMN_LABELS: [&str; 5] = ["Name", "Type", "Size", "Modified", "Extension"];
const LIST_SORT_COLUMNS: [SortColumn; 5] = [
    SortColumn::Name,
    SortColumn::Type,
    SortColumn::Size,
    SortColumn::Modified,
    SortColumn::Extension,
];
const TYPE_COLUMN_WIDTH: i32 = 11;
const SIZE_COLUMN_WIDTH: i32 = 10;
const MODIFIED_COLUMN_WIDTH: i32 = 18;
const THUMBNAIL_CACHE_CAPACITY: usize = 256;
pub const SORT_ACTIONS: [(&str, SortColumn); 5] = [
    ("sort-name", SortColumn::Name),
    ("sort-type", SortColumn::Type),
    ("sort-size", SortColumn::Size),
    ("sort-modified", SortColumn::Modified),
    ("sort-extension", SortColumn::Extension),
];

#[derive(Clone)]
enum CachedThumbnail {
    Ready(gtk::gdk::Texture),
    Fallback,
}

struct ThumbnailPresentationState {
    disabled: bool,
    completed: HashMap<ThumbnailKey, CachedThumbnail>,
    cache_order: VecDeque<ThumbnailKey>,
    pending: HashSet<ThumbnailKey>,
    requests: VecDeque<ThumbnailKey>,
    bindings: Vec<(glib::WeakRef<gtk::Image>, ThumbnailKey)>,
}

impl ThumbnailPresentationState {
    fn enqueue(&mut self, key: ThumbnailKey) {
        if self.pending.insert(key.clone()) {
            self.requests.push_back(key);
        }
    }

    fn insert_completed(&mut self, key: ThumbnailKey, cached: CachedThumbnail) {
        self.cache_order.retain(|cached_key| cached_key != &key);
        self.cache_order.push_back(key.clone());
        self.completed.insert(key, cached);
        while self.completed.len() > THUMBNAIL_CACHE_CAPACITY {
            let Some(expired) = self.cache_order.pop_front() else {
                break;
            };
            self.completed.remove(&expired);
        }
    }
}

#[derive(Clone)]
pub struct ThumbnailPresentation {
    state: Rc<RefCell<ThumbnailPresentationState>>,
    icon_style: Rc<Cell<EntryIconStyle>>,
}

impl ThumbnailPresentation {
    fn new(icon_style: Rc<Cell<EntryIconStyle>>) -> Self {
        Self {
            state: Rc::new(RefCell::new(ThumbnailPresentationState {
                disabled: false,
                completed: HashMap::new(),
                cache_order: VecDeque::new(),
                pending: HashSet::new(),
                requests: VecDeque::new(),
                bindings: Vec::new(),
            })),
            icon_style,
        }
    }

    fn request_thumbnail(&self, image: &gtk::Image, entry: &DirectoryEntry) {
        self.request_thumbnail_with_icon_size(image, entry, LIST_THUMBNAIL_EDGE, LIST_ICON_EDGE);
    }

    fn request_thumbnail_at_size(&self, image: &gtk::Image, entry: &DirectoryEntry, edge: u16) {
        self.request_thumbnail_with_icon_size(image, entry, edge, grid_icon_edge(edge));
    }

    fn request_thumbnail_with_icon_size(
        &self,
        image: &gtk::Image,
        entry: &DirectoryEntry,
        edge: u16,
        icon_edge: i32,
    ) {
        let scale = crate::completeness::PresentationScale::new(image.scale_factor(), 100);
        let edge = scale.logical_thumbnail_edge(edge);
        let _device_pixel_hint = scale.device_pixel_hint(edge);
        image.remove_css_class("floe-thumbnail");
        apply_entry_icon(image, entry, icon_edge, self.icon_style.get());

        let mut state = self.state.borrow_mut();
        state
            .bindings
            .retain(|(weak, _)| weak.upgrade().is_some_and(|bound| bound != *image));
        if state.disabled {
            return;
        }
        let key = if edge == LIST_THUMBNAIL_EDGE {
            ThumbnailKey::from_entry(entry)
        } else {
            ThumbnailKey::from_entry_at_size(entry, edge)
        };
        let Some(key) = key else {
            return;
        };
        match state.completed.get(&key).cloned() {
            Some(CachedThumbnail::Ready(texture)) => {
                apply_thumbnail(image, &texture, edge);
            }
            Some(CachedThumbnail::Fallback) => {}
            None => {
                state.bindings.push((image.downgrade(), key.clone()));
                state.enqueue(key);
            }
        }
    }

    fn unbind(&self, image: &gtk::Image) {
        self.state
            .borrow_mut()
            .bindings
            .retain(|(weak, _)| weak.upgrade().is_some_and(|bound| bound != *image));
    }

    pub fn take_request(&self) -> Option<ThumbnailKey> {
        self.state.borrow_mut().requests.pop_front()
    }

    pub fn retry_request(&self, key: ThumbnailKey) {
        let mut state = self.state.borrow_mut();
        if !state.disabled && state.pending.contains(&key) {
            state.requests.push_front(key);
        }
    }

    pub fn begin_generation(&self) {
        let mut state = self.state.borrow_mut();
        state.disabled = false;
        state.pending.clear();
        state.requests.clear();
        state.bindings.clear();
    }

    pub fn disable(&self) {
        let mut state = self.state.borrow_mut();
        state.disabled = true;
        state.pending.clear();
        state.requests.clear();
        state.bindings.clear();
    }

    pub fn complete(&self, key: ThumbnailKey, result: Result<ThumbnailPixels, ThumbnailError>) {
        let cached = match result {
            Ok(pixels) => CachedThumbnail::Ready(texture_from_pixels(pixels)),
            Err(error) => {
                tracing::debug!(%error, "thumbnail unavailable; retaining generic file icon");
                CachedThumbnail::Fallback
            }
        };
        let mut state = self.state.borrow_mut();
        state.pending.remove(&key);
        state.insert_completed(key.clone(), cached.clone());
        state.bindings.retain(|(weak, bound_key)| {
            let Some(image) = weak.upgrade() else {
                return false;
            };
            if bound_key == &key {
                if let CachedThumbnail::Ready(texture) = &cached {
                    apply_thumbnail(&image, texture, key.edge());
                }
                return false;
            }
            true
        });
    }
}

fn texture_from_pixels(pixels: ThumbnailPixels) -> gtk::gdk::Texture {
    let (width, height, rowstride, has_alpha, pixels) = pixels.into_parts();
    let format = if has_alpha {
        gtk::gdk::MemoryFormat::R8g8b8a8
    } else {
        gtk::gdk::MemoryFormat::R8g8b8
    };
    let bytes = glib::Bytes::from_owned(pixels);
    gtk::gdk::MemoryTexture::new(width, height, format, &bytes, rowstride).upcast()
}

#[derive(Clone)]
struct MetadataLabels {
    mime: glib::WeakRef<gtk::Label>,
    created: glib::WeakRef<gtk::Label>,
    accessed: glib::WeakRef<gtk::Label>,
    permissions: glib::WeakRef<gtk::Label>,
    dimensions: glib::WeakRef<gtk::Label>,
    duration: glib::WeakRef<gtk::Label>,
    artist: glib::WeakRef<gtk::Label>,
    album: glib::WeakRef<gtk::Label>,
    track: glib::WeakRef<gtk::Label>,
    owner: glib::WeakRef<gtk::Label>,
    group: glib::WeakRef<gtk::Label>,
    link_target: glib::WeakRef<gtk::Label>,
}

impl MetadataLabels {
    fn new(labels: [&gtk::Label; 12]) -> Self {
        let [
            mime,
            created,
            accessed,
            permissions,
            dimensions,
            duration,
            artist,
            album,
            track,
            owner,
            group,
            link_target,
        ] = labels;
        Self {
            mime: mime.downgrade(),
            created: created.downgrade(),
            accessed: accessed.downgrade(),
            permissions: permissions.downgrade(),
            dimensions: dimensions.downgrade(),
            duration: duration.downgrade(),
            artist: artist.downgrade(),
            album: album.downgrade(),
            track: track.downgrade(),
            owner: owner.downgrade(),
            group: group.downgrade(),
            link_target: link_target.downgrade(),
        }
    }

    fn is_alive(&self) -> bool {
        self.mime.upgrade().is_some()
            && self.created.upgrade().is_some()
            && self.accessed.upgrade().is_some()
            && self.permissions.upgrade().is_some()
            && self.dimensions.upgrade().is_some()
            && self.duration.upgrade().is_some()
            && self.artist.upgrade().is_some()
            && self.album.upgrade().is_some()
            && self.track.upgrade().is_some()
            && self.owner.upgrade().is_some()
            && self.group.upgrade().is_some()
            && self.link_target.upgrade().is_some()
    }

    fn same_row(&self, other: &Self) -> bool {
        self.mime
            .upgrade()
            .zip(other.mime.upgrade())
            .is_some_and(|(left, right)| left == right)
    }

    fn clear(&self) {
        for label in [
            self.mime.upgrade(),
            self.created.upgrade(),
            self.accessed.upgrade(),
            self.permissions.upgrade(),
            self.dimensions.upgrade(),
            self.duration.upgrade(),
            self.artist.upgrade(),
            self.album.upgrade(),
            self.track.upgrade(),
            self.owner.upgrade(),
            self.group.upgrade(),
            self.link_target.upgrade(),
        ]
        .into_iter()
        .flatten()
        {
            label.set_label("");
            label.set_tooltip_text(None);
        }
    }
}

struct MetadataBinding {
    key: MetadataKey,
    labels: MetadataLabels,
}

#[derive(Default)]
struct MetadataPresentationState {
    cache: MetadataCache,
    bindings: Vec<MetadataBinding>,
}

#[derive(Clone, Default)]
pub struct MetadataPresentation {
    state: Rc<RefCell<MetadataPresentationState>>,
}

impl MetadataPresentation {
    fn request(&self, entry: &DirectoryEntry, labels: MetadataLabels, include_advanced: bool) {
        labels.clear();
        let key = MetadataKey::from_entry(entry, include_advanced);
        let cached = {
            let mut state = self.state.borrow_mut();
            state
                .bindings
                .retain(|binding| !binding.labels.same_row(&labels));
            state.cache.request(key.clone()).cloned()
        };
        if let Some(Ok(details)) = cached {
            apply_metadata_details(&labels, &details);
            return;
        }
        if cached.is_none() {
            self.state
                .borrow_mut()
                .bindings
                .push(MetadataBinding { key, labels });
        }
    }

    pub fn take_request(&self) -> Option<MetadataKey> {
        self.state.borrow_mut().cache.take_request()
    }

    pub fn retry(&self, key: MetadataKey) {
        self.state.borrow_mut().cache.retry(key);
    }

    pub fn complete(&self, key: MetadataKey, result: Result<MetadataDetails, MetadataError>) {
        let details = result.as_ref().ok().cloned();
        let mut state = self.state.borrow_mut();
        state.cache.complete(key.clone(), result);
        state.bindings.retain(|binding| {
            if !binding.labels.is_alive() {
                return false;
            }
            if binding.key == key {
                if let Some(details) = details.as_ref() {
                    apply_metadata_details(&binding.labels, details);
                }
                return false;
            }
            true
        });
    }

    pub fn begin_generation(&self) {
        let mut state = self.state.borrow_mut();
        state.cache.clear_pending();
        state.bindings.clear();
    }
}

fn apply_metadata_details(labels: &MetadataLabels, details: &MetadataDetails) {
    if let Some(label) = labels.mime.upgrade() {
        let text = details.mime_type.as_deref().unwrap_or_default();
        label.set_label(text);
        label.set_tooltip_text((!text.is_empty()).then_some(text));
    }
    if let Some(label) = labels.created.upgrade() {
        let text = details
            .created
            .and_then(format_modified)
            .unwrap_or_default();
        label.set_label(&text);
        label.set_tooltip_text((!text.is_empty()).then_some(text.as_str()));
    }
    if let Some(label) = labels.accessed.upgrade() {
        let text = details
            .accessed
            .and_then(format_modified)
            .unwrap_or_default();
        label.set_label(&text);
        label.set_tooltip_text((!text.is_empty()).then_some(text.as_str()));
    }
    if let Some(label) = labels.permissions.upgrade() {
        let text = details
            .unix_mode
            .map(format_permissions)
            .unwrap_or_default();
        label.set_label(&text);
        label.set_tooltip_text((!text.is_empty()).then_some(text.as_str()));
    }
    if let Some(label) = labels.owner.upgrade() {
        let text = details
            .owner
            .map(|value| value.to_string())
            .unwrap_or_default();
        label.set_label(&text);
        label.set_tooltip_text((!text.is_empty()).then_some(text.as_str()));
    }
    if let Some(label) = labels.group.upgrade() {
        let text = details
            .group
            .map(|value| value.to_string())
            .unwrap_or_default();
        label.set_label(&text);
        label.set_tooltip_text((!text.is_empty()).then_some(text.as_str()));
    }
    if let Some(label) = labels.link_target.upgrade() {
        let text = details
            .link_target
            .as_ref()
            .map_or_else(String::new, |link| {
                let suffix = match link.status {
                    LinkTargetStatus::Present => "",
                    LinkTargetStatus::Missing => " (broken)",
                    LinkTargetStatus::Inaccessible => " (inaccessible)",
                };
                format!("{}{suffix}", link.target.to_string_lossy())
            });
        label.set_label(&text);
        label.set_tooltip_text((!text.is_empty()).then_some(text.as_str()));
    }
    let [dimensions, duration, artist, album, track] = advanced_column_texts(details);
    for (label, text) in [
        (labels.dimensions.upgrade(), dimensions),
        (labels.duration.upgrade(), duration),
        (labels.artist.upgrade(), artist),
        (labels.album.upgrade(), album),
        (labels.track.upgrade(), track),
    ] {
        if let Some(label) = label {
            label.set_label(&text);
            label.set_tooltip_text((!text.is_empty()).then_some(text.as_str()));
        }
    }
}

fn advanced_column_texts(details: &MetadataDetails) -> [String; 5] {
    use crate::{advanced_metadata::AdvancedMetadataState, inspector::ImageDimensionFacts};

    let dimensions = match details.image_dimensions {
        ImageDimensionFacts::Dimensions(size) => format!("{} × {}", size.width, size.height),
        ImageDimensionFacts::LimitExceeded => "Limited".to_owned(),
        ImageDimensionFacts::Unavailable => "Unavailable".to_owned(),
        ImageDimensionFacts::NotImage => String::new(),
    };
    let AdvancedMetadataState::Present(advanced) = &details.advanced else {
        let state = match &details.advanced {
            AdvancedMetadataState::LimitExceeded => "Limited",
            AdvancedMetadataState::Malformed(_) => "Malformed",
            AdvancedMetadataState::Unsupported | AdvancedMetadataState::NoMetadata => "",
            AdvancedMetadataState::Present(_) => unreachable!(),
        };
        return [
            dimensions,
            state.to_owned(),
            String::new(),
            String::new(),
            String::new(),
        ];
    };
    let Some(media) = &advanced.media else {
        return [
            dimensions,
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ];
    };
    let duration = media
        .duration
        .map(format_media_duration)
        .unwrap_or_default();
    let track = media.track.map_or_else(String::new, |track| {
        media
            .track_total
            .map_or_else(|| track.to_string(), |total| format!("{track}/{total}"))
    });
    [
        dimensions,
        duration,
        media.artist.clone().unwrap_or_default(),
        media.album.clone().unwrap_or_default(),
        track,
    ]
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

fn format_permissions(mode: u32) -> String {
    let mut text = String::with_capacity(10);
    text.push(match mode & 0o170000 {
        0o040000 => 'd',
        0o120000 => 'l',
        _ => '-',
    });
    for (read, write, execute) in [
        (0o400, 0o200, 0o100),
        (0o040, 0o020, 0o010),
        (0o004, 0o002, 0o001),
    ] {
        text.push(if mode & read != 0 { 'r' } else { '-' });
        text.push(if mode & write != 0 { 'w' } else { '-' });
        text.push(if mode & execute != 0 { 'x' } else { '-' });
    }
    text
}

#[derive(Clone)]
pub struct SortHeaderWidgets {
    pub column: SortColumn,
    pub button: gtk::Button,
    label: gtk::Label,
}

#[derive(Clone)]
pub struct ListColumnHeaderWidgets {
    pub column: ListColumn,
    pub widget: gtk::Widget,
}

pub struct OpenWithDialogWidgets {
    pub dialog: adw::Dialog,
    pub default_label: gtk::Label,
    pub list: gtk::ListBox,
    pub cancel_button: gtk::Button,
    pub set_default_button: gtk::Button,
    pub reset_default_button: gtk::Button,
    pub open_button: gtk::Button,
}

pub struct PropertiesDialogWidgets {
    pub dialog: adw::Dialog,
    pub open_with_button: gtk::Button,
    pub checksum_button: gtk::Button,
    pub privacy_safety_button: gtk::Button,
    pub threat_scan_button: gtk::Button,
    pub sanitize_button: gtk::Button,
    pub permission_audit_button: gtk::Button,
    pub edit_permissions_button: gtk::Button,
    pub close_button: gtk::Button,
}

pub struct PermissionAuditDialogWidgets {
    pub dialog: adw::Dialog,
    pub fix_button: gtk::Button,
    pub close_button: gtk::Button,
}

pub struct PermissionDialogWidgets {
    pub dialog: adw::Dialog,
    pub file_mode_entry: gtk::Entry,
    pub directory_mode_entry: gtk::Entry,
    pub executable_dropdown: gtk::DropDown,
    pub owner_entry: gtk::Entry,
    pub group_entry: gtk::Entry,
    pub recursive_check: gtk::CheckButton,
    pub acknowledge_check: gtk::CheckButton,
    pub error_label: gtk::Label,
    pub cancel_button: gtk::Button,
    pub apply_button: gtk::Button,
}

pub struct ChecksumDialogWidgets {
    pub dialog: adw::Dialog,
    pub algorithm_dropdown: gtk::DropDown,
    pub expected_entry: gtk::Entry,
    pub error_label: gtk::Label,
    pub cancel_button: gtk::Button,
    pub calculate_button: gtk::Button,
}

pub struct ChecksumResultsDialogWidgets {
    pub dialog: adw::Dialog,
    pub copy_button: gtk::Button,
    pub close_button: gtk::Button,
}

pub struct VerifiedCopyResultDialogWidgets {
    pub dialog: adw::Dialog,
    pub retry_button: gtk::Button,
    pub close_button: gtk::Button,
}

pub struct ConflictDialogWidgets {
    pub dialog: adw::Dialog,
    pub name_entry: gtk::Entry,
    pub name_error: gtk::Label,
    pub cancel_button: gtk::Button,
    pub keep_existing_button: gtk::Button,
    pub keep_both_button: gtk::Button,
    pub skip_all_button: gtk::Button,
    pub replace_button: gtk::Button,
    pub replace_all_button: gtk::Button,
    pub retry_button: gtk::Button,
}

pub const REPLACE_CONFLICT_EXPLANATION: &str =
    "Floe identity-checks both items and privately retains the old destination for Undo.";
pub const REPLACE_ALL_SCOPE_EXPLANATION: &str = "Replace All applies only to later compatible conflicts in this batch and captures fresh identities for every item.";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationHistoryItem {
    pub title: String,
    pub detail: String,
    pub can_undo: bool,
    pub can_redo: bool,
}

pub struct OperationHistoryDialogWidgets {
    pub dialog: adw::Dialog,
    pub clear_completed_button: gtk::Button,
    pub undo_buttons: Vec<gtk::Button>,
    pub redo_buttons: Vec<gtk::Button>,
}

pub const OPERATION_HISTORY_DURABILITY_EXPLANATION: &str = "Recent reversible work is stored privately for 30 days. Floe rechecks the exact item before Undo or Redo; interrupted or uncertain actions require review.";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryDialogItem {
    pub id: u64,
    pub title: String,
    pub detail: String,
    pub can_retry: bool,
    pub can_resolve: bool,
    pub source: Option<PathBuf>,
    pub destination: PathBuf,
}

pub struct RecoveryDialogWidgets {
    pub dialog: adw::Dialog,
    pub retry_buttons: Vec<gtk::Button>,
    pub reveal_source_buttons: Vec<gtk::Button>,
    pub reveal_destination_buttons: Vec<gtk::Button>,
    pub resolve_buttons: Vec<gtk::Button>,
}

#[derive(Clone)]
pub struct OperationWidgets {
    pub revealer: gtk::Revealer,
    pub operation_label: gtk::Label,
    pub operation_detail: gtk::Label,
    pub operation_progress: gtk::ProgressBar,
    pub operation_retry: gtk::Button,
    pub operation_pause: gtk::Button,
    pub operation_history: gtk::Button,
    pub operation_cancel: gtk::Button,
}

#[derive(Clone)]
pub struct RenameDialogWidgets {
    pub dialog: adw::Dialog,
    pub rename_entry: gtk::Entry,
    pub rename_error: gtk::Label,
    pub cancel_button: gtk::Button,
    pub rename_button: gtk::Button,
}

#[derive(Clone)]
pub struct PermanentDeleteDialogWidgets {
    pub dialog: adw::Dialog,
    pub cancel_button: gtk::Button,
    pub delete_button: gtk::Button,
}

pub struct BrowserWidgets {
    pub window: adw::ApplicationWindow,
    appearance_manager: AppearanceManager,
    entry_icon_style: Rc<Cell<EntryIconStyle>>,
    pub toast_overlay: adw::ToastOverlay,
    pub background_feedback_revealer: gtk::Revealer,
    pub background_feedback_list: gtk::Box,
    pub back_button: gtk::Button,
    pub forward_button: gtk::Button,
    pub parent_button: gtk::Button,
    pub hidden_button: gtk::ToggleButton,
    pub path_label: gtk::Label,
    pub breadcrumb_box: gtk::Box,
    pub recent_locations_button: gtk::Button,
    pub path_stack: gtk::Stack,
    pub location_entry: gtk::Entry,
    pub location_error: gtk::Label,
    pub location_suggestions: gtk::Popover,
    pub location_suggestions_box: gtk::ListBox,
    pub tab_bar: gtk::Box,
    pub selection: gtk::MultiSelection,
    pub list_view: gtk::ListView,
    pub grid_view: gtk::GridView,
    pub grouped_grid_view: gtk::ScrolledWindow,
    pub miller_view: MillerView,
    pub view_stack: gtk::Stack,
    pub list_header: gtk::Box,
    pub list_context_menu: gtk::PopoverMenu,
    pub grid_context_menu: gtk::PopoverMenu,
    pub list_background_menu: gtk::PopoverMenu,
    pub grid_background_menu: gtk::PopoverMenu,
    file_context_model: gio::Menu,
    background_context_model: gio::Menu,
    pub list_view_button: gtk::ToggleButton,
    pub grid_view_button: gtk::ToggleButton,
    pub miller_view_button: gtk::ToggleButton,
    pub sort_menu_button: gtk::MenuButton,
    pub vim_mode_button: gtk::ToggleButton,
    pub grid_size_controls: gtk::Box,
    pub grid_size_scale: gtk::Scale,
    pub empty_state: gtk::Box,
    pub empty_label: gtk::Label,
    pub search_bar: gtk::Box,
    pub search_mode: gtk::DropDown,
    pub filter_entry: gtk::SearchEntry,
    pub filter_mode: gtk::DropDown,
    pub filter_feedback: gtk::Label,
    pub advanced_filter_toggle: gtk::ToggleButton,
    pub advanced_filter_box: gtk::Box,
    pub advanced_type: gtk::DropDown,
    pub advanced_extension: gtk::Entry,
    pub advanced_mime: gtk::Entry,
    pub advanced_size: gtk::DropDown,
    pub advanced_date: gtk::DropDown,
    pub advanced_owner: gtk::DropDown,
    pub advanced_hidden: gtk::DropDown,
    pub advanced_match_case: gtk::CheckButton,
    pub advanced_apply: gtk::Button,
    pub advanced_clear: gtk::Button,
    pub search_scope: gtk::DropDown,
    pub search_button: gtk::Button,
    pub search_stop_button: gtk::Button,
    pub saved_searches: gtk::DropDown,
    pub recent_searches: gtk::DropDown,
    pub search_result_order: gtk::DropDown,
    pub save_search_button: gtk::Button,
    pub delete_saved_search_button: gtk::Button,
    pub clear_recent_searches_button: gtk::Button,
    pub search_index_toggle: gtk::CheckButton,
    pub search_index_menu_button: gtk::MenuButton,
    pub search_feedback: gtk::Label,
    pub search_results_view: gtk::ListView,
    pub search_context_menu: gtk::PopoverMenu,
    pub search_background_menu: gtk::PopoverMenu,
    pub spinner: gtk::Spinner,
    pub status_label: gtk::Label,
    pub sort_headers: Vec<SortHeaderWidgets>,
    pub column_headers: Vec<ListColumnHeaderWidgets>,
    pub group_header_spacer: gtk::Widget,
    pub thumbnails: ThumbnailPresentation,
    pub metadata: MetadataPresentation,
    pub location_buttons: Vec<gtk::Button>,
    pub bookmarks_box: gtk::Box,
    pub add_bookmark_button: gtk::Button,
    pub devices_box: gtk::Box,
    pub trash_button: gtk::Button,
    pub drop_dispatcher: DropDispatcher,
    pub workspace: gtk::Paned,
    pub split_pane: gtk::Paned,
    pub active_pane_shell: gtk::Box,
    pub active_pane_label: gtk::Label,
    pub inactive_pane: gtk::Box,
    pub inactive_pane_label: gtk::Label,
    pub inactive_pane_status: gtk::Label,
    pub inactive_pane_items: gtk::StringList,
    pub sidebar: gtk::Box,
    pub sidebar_content: gtk::ScrolledWindow,
    pub sidebar_default_width: i32,
    pub operations: OperationWidgets,
    list_layout: Rc<Cell<ListColumnLayout>>,
    list_grouping: Rc<Cell<DirectoryGrouping>>,
    collapsed_groups: Rc<RefCell<HashSet<String>>>,
    list_factory: gtk::SignalListItemFactory,
    grid_factory: RefCell<gtk::SignalListItemFactory>,
    grid_presentation_stack: gtk::Stack,
    grouped_grid: GroupedGridPresentation,
}

struct SidebarWidgets {
    content: gtk::ScrolledWindow,
    sidebar: gtk::Box,
    location_buttons: Vec<gtk::Button>,
    bookmarks_box: gtk::Box,
    add_bookmark_button: gtk::Button,
    devices_box: gtk::Box,
    trash_button: gtk::Button,
}

struct DirectoryPanelWidgets {
    content: gtk::Box,
    selection: gtk::MultiSelection,
    list_view: gtk::ListView,
    grid_view: gtk::GridView,
    grouped_grid_view: gtk::ScrolledWindow,
    miller_view: MillerView,
    view_stack: gtk::Stack,
    list_header: gtk::Box,
    list_context_menu: gtk::PopoverMenu,
    grid_context_menu: gtk::PopoverMenu,
    list_background_menu: gtk::PopoverMenu,
    grid_background_menu: gtk::PopoverMenu,
    file_context_model: gio::Menu,
    background_context_model: gio::Menu,
    empty_state: gtk::Box,
    empty_label: gtk::Label,
    search_bar: gtk::Box,
    search_mode: gtk::DropDown,
    filter_entry: gtk::SearchEntry,
    filter_mode: gtk::DropDown,
    filter_feedback: gtk::Label,
    advanced_filter_toggle: gtk::ToggleButton,
    advanced_filter_box: gtk::Box,
    advanced_type: gtk::DropDown,
    advanced_extension: gtk::Entry,
    advanced_mime: gtk::Entry,
    advanced_size: gtk::DropDown,
    advanced_date: gtk::DropDown,
    advanced_owner: gtk::DropDown,
    advanced_hidden: gtk::DropDown,
    advanced_match_case: gtk::CheckButton,
    advanced_apply: gtk::Button,
    advanced_clear: gtk::Button,
    search_scope: gtk::DropDown,
    search_button: gtk::Button,
    search_stop_button: gtk::Button,
    saved_searches: gtk::DropDown,
    recent_searches: gtk::DropDown,
    search_result_order: gtk::DropDown,
    save_search_button: gtk::Button,
    delete_saved_search_button: gtk::Button,
    clear_recent_searches_button: gtk::Button,
    search_index_toggle: gtk::CheckButton,
    search_index_menu_button: gtk::MenuButton,
    search_feedback: gtk::Label,
    search_results_view: gtk::ListView,
    search_context_menu: gtk::PopoverMenu,
    search_background_menu: gtk::PopoverMenu,
    spinner: gtk::Spinner,
    status_label: gtk::Label,
    sort_headers: Vec<SortHeaderWidgets>,
    column_headers: Vec<ListColumnHeaderWidgets>,
    group_header_spacer: gtk::Widget,
    thumbnails: ThumbnailPresentation,
    metadata: MetadataPresentation,
    list_layout: Rc<Cell<ListColumnLayout>>,
    list_grouping: Rc<Cell<DirectoryGrouping>>,
    collapsed_groups: Rc<RefCell<HashSet<String>>>,
    list_factory: gtk::SignalListItemFactory,
    grid_factory: RefCell<gtk::SignalListItemFactory>,
    grid_presentation_stack: gtk::Stack,
    grouped_grid: GroupedGridPresentation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GridGroupSection {
    label: String,
    start: u32,
    len: u32,
}

#[derive(Clone)]
struct GroupedGridPresentation {
    state: Rc<GroupedGridState>,
}

struct GroupedGridState {
    root: gtk::ScrolledWindow,
    sections: gtk::Box,
    section_grids: RefCell<Vec<gtk::GridView>>,
    selection: gtk::MultiSelection,
    primary_grid: gtk::GridView,
    context_menu: gtk::PopoverMenu,
    thumbnails: ThumbnailPresentation,
    grouping: Rc<Cell<DirectoryGrouping>>,
    collapsed_groups: Rc<RefCell<HashSet<String>>>,
    grid_size: Cell<GridSize>,
    density: Cell<FileViewDensity>,
    single_click: Cell<bool>,
    rebuild_pending: Cell<bool>,
    drop_dispatcher: DropDispatcher,
}

struct GroupedGridDependencies<'a> {
    selection: &'a gtk::MultiSelection,
    primary_grid: &'a gtk::GridView,
    context_menu: &'a gtk::PopoverMenu,
    thumbnails: &'a ThumbnailPresentation,
    grouping: &'a Rc<Cell<DirectoryGrouping>>,
    collapsed_groups: &'a Rc<RefCell<HashSet<String>>>,
    drop_dispatcher: &'a DropDispatcher,
}

struct GridFactoryDependencies<'a> {
    selection: &'a gtk::MultiSelection,
    context_menu: &'a gtk::PopoverMenu,
    thumbnails: &'a ThumbnailPresentation,
    grouping: &'a Rc<Cell<DirectoryGrouping>>,
    collapsed_groups: &'a Rc<RefCell<HashSet<String>>>,
    drop_dispatcher: &'a DropDispatcher,
}

impl GroupedGridPresentation {
    fn new(
        dependencies: GroupedGridDependencies<'_>,
        grid_size: GridSize,
        density: FileViewDensity,
    ) -> Self {
        let sections = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .hexpand(true)
            .vexpand(false)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(8)
            .margin_end(8)
            .build();
        sections.add_css_class("floe-grid-sections");
        let root = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .child(&sections)
            .vexpand(true)
            .build();
        root.add_css_class("floe-grouped-grid");

        let state = Rc::new(GroupedGridState {
            root,
            sections,
            section_grids: RefCell::new(Vec::new()),
            selection: dependencies.selection.clone(),
            primary_grid: dependencies.primary_grid.clone(),
            context_menu: dependencies.context_menu.clone(),
            thumbnails: dependencies.thumbnails.clone(),
            grouping: Rc::clone(dependencies.grouping),
            collapsed_groups: Rc::clone(dependencies.collapsed_groups),
            grid_size: Cell::new(grid_size),
            density: Cell::new(density),
            single_click: Cell::new(false),
            rebuild_pending: Cell::new(false),
            drop_dispatcher: dependencies.drop_dispatcher.clone(),
        });
        let presentation = Self { state };

        let weak_state = Rc::downgrade(&presentation.state);
        dependencies
            .selection
            .connect_items_changed(move |_, _, _, _| {
                let Some(state) = weak_state.upgrade() else {
                    return;
                };
                GroupedGridPresentation::queue_rebuild(&state);
            });
        presentation.rebuild();
        presentation
    }

    fn widget(&self) -> &gtk::ScrolledWindow {
        &self.state.root
    }

    fn set_grouping(&self, grouping: DirectoryGrouping) {
        self.state.grouping.set(grouping);
        self.rebuild();
    }

    fn set_grid_size(&self, size: GridSize) {
        if self.state.grid_size.replace(size) != size {
            self.rebuild();
        }
    }

    fn set_density(&self, density: FileViewDensity) {
        self.state.density.set(density);
        let class_name = file_view_density_class(density);
        for grid in self.state.section_grids.borrow().iter() {
            for class in ["view-compact", "view-comfortable", "view-spacious"] {
                grid.remove_css_class(class);
            }
            grid.add_css_class(class_name);
        }
    }

    fn set_single_click_activate(&self, active: bool) {
        self.state.single_click.set(active);
        for grid in self.state.section_grids.borrow().iter() {
            grid.set_single_click_activate(active);
        }
    }

    fn refresh_collapsed_groups(&self) {
        self.rebuild();
    }

    fn focus_first_section(&self) {
        if let Some(grid) = self
            .state
            .section_grids
            .borrow()
            .iter()
            .find(|grid| grid.is_visible())
        {
            grid.grab_focus();
        } else if let Some(header) = self
            .state
            .sections
            .first_child()
            .and_then(|section| section.first_child())
        {
            header.grab_focus();
        }
    }

    fn scroll_to(&self, global_index: u32) -> bool {
        let sections = grid_group_sections(&self.state.selection, self.state.grouping.get());
        let grids = self.state.section_grids.borrow();
        let Some((section, grid)) = sections.iter().zip(grids.iter()).find(|(section, grid)| {
            grid.is_visible()
                && global_index >= section.start
                && global_index < section.start.saturating_add(section.len)
        }) else {
            return false;
        };
        let info = gtk::ScrollInfo::new();
        info.set_enable_vertical(true);
        grid.scroll_to(
            global_index.saturating_sub(section.start),
            gtk::ListScrollFlags::NONE,
            Some(info),
        );
        true
    }

    fn queue_rebuild(state: &Rc<GroupedGridState>) {
        if state.rebuild_pending.replace(true) {
            return;
        }
        let weak_state = Rc::downgrade(state);
        glib::idle_add_local_once(move || {
            let Some(state) = weak_state.upgrade() else {
                return;
            };
            state.rebuild_pending.set(false);
            GroupedGridPresentation { state }.rebuild();
        });
    }

    fn rebuild(&self) {
        while let Some(child) = self.state.sections.first_child() {
            self.state.sections.remove(&child);
        }
        self.state.section_grids.borrow_mut().clear();

        let grouping = self.state.grouping.get();
        if grouping == DirectoryGrouping::None {
            return;
        }
        let sections = grid_group_sections(&self.state.selection, grouping);
        let collapsed = self.state.collapsed_groups.borrow();
        let no_grouping = Rc::new(Cell::new(DirectoryGrouping::None));
        let no_collapsed_groups = Rc::new(RefCell::new(HashSet::new()));

        for section in sections {
            let is_collapsed = collapsed.contains(&section.label);
            let section_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(4)
                .hexpand(true)
                .build();
            section_box.add_css_class("floe-grid-section");

            let header = gtk::Button::builder()
                .halign(gtk::Align::Fill)
                .hexpand(true)
                .has_frame(false)
                .build();
            initialize_group_header(&header, true);
            header.add_css_class("floe-grid-section-header");
            let header_label = gtk::Label::builder()
                .label(format!(
                    "{} {}",
                    if is_collapsed { "▸" } else { "▾" },
                    section.label
                ))
                .halign(gtk::Align::Fill)
                .hexpand(true)
                .xalign(0.0)
                .build();
            header.set_child(Some(&header_label));
            header.set_tooltip_text(Some(&section.label));
            set_accessible_label(&header, &header_label.label());
            header.update_state(&[gtk::accessible::State::Expanded(Some(!is_collapsed))]);
            section_box.append(&header);

            let model = SelectionSlice::new(&self.state.selection, section.start, section.len);
            let factory = build_grid_factory(
                GridFactoryDependencies {
                    selection: &self.state.selection,
                    context_menu: &self.state.context_menu,
                    thumbnails: &self.state.thumbnails,
                    grouping: &no_grouping,
                    collapsed_groups: &no_collapsed_groups,
                    drop_dispatcher: &self.state.drop_dispatcher,
                },
                self.state.grid_size.get(),
                section.start,
            );
            let grid = gtk::GridView::new(Some(model), Some(factory));
            grid.add_css_class("floe-directory-grid");
            grid.add_css_class("floe-grid-section-body");
            grid.add_css_class(file_view_density_class(self.state.density.get()));
            grid.set_single_click_activate(self.state.single_click.get());
            grid.set_enable_rubberband(true);
            grid.set_min_columns(1);
            grid.set_max_columns(24);
            grid.set_hexpand(true);
            grid.set_vexpand(false);
            grid.set_visible(!is_collapsed);

            let primary_grid = self.state.primary_grid.clone();
            let start = section.start;
            grid.connect_activate(move |_, position| {
                let global_position = start.saturating_add(position);
                primary_grid.emit_by_name::<()>("activate", &[&global_position]);
            });
            section_box.append(&grid);
            self.state.sections.append(&section_box);
            self.state.section_grids.borrow_mut().push(grid);
        }
    }
}

fn grid_group_sections(
    selection: &gtk::MultiSelection,
    grouping: DirectoryGrouping,
) -> Vec<GridGroupSection> {
    if grouping == DirectoryGrouping::None {
        return Vec::new();
    }
    let mut sections = Vec::<GridGroupSection>::new();
    for position in 0..selection.n_items() {
        let Some(object) = selection
            .item(position)
            .and_downcast::<glib::BoxedAnyObject>()
        else {
            continue;
        };
        let Ok(entry) = object.try_borrow::<std::sync::Arc<DirectoryEntry>>() else {
            continue;
        };
        let Some(label) = grouping.label(&entry) else {
            continue;
        };
        if let Some(section) = sections.last_mut().filter(|section| section.label == label) {
            section.len = section.len.saturating_add(1);
        } else {
            sections.push(GridGroupSection {
                label,
                start: position,
                len: 1,
            });
        }
    }
    sections
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SplitPaneLayout {
    ActiveOnly,
    ActiveThenInactive,
    InactiveThenActive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SplitPaneUpdate {
    NoChange,
    AddInactiveEnd,
    RemoveInactiveEnd,
    Rebuild,
}

fn split_pane_layout(is_split: bool, active_side: SplitSide) -> SplitPaneLayout {
    if !is_split {
        return SplitPaneLayout::ActiveOnly;
    }

    match active_side {
        SplitSide::Primary => SplitPaneLayout::ActiveThenInactive,
        SplitSide::Secondary => SplitPaneLayout::InactiveThenActive,
    }
}

fn split_pane_update(
    current: Option<SplitPaneLayout>,
    desired: SplitPaneLayout,
) -> SplitPaneUpdate {
    match (current, desired) {
        (Some(current), desired) if current == desired => SplitPaneUpdate::NoChange,
        (Some(SplitPaneLayout::ActiveOnly), SplitPaneLayout::ActiveThenInactive) => {
            SplitPaneUpdate::AddInactiveEnd
        }
        (Some(SplitPaneLayout::ActiveThenInactive), SplitPaneLayout::ActiveOnly) => {
            SplitPaneUpdate::RemoveInactiveEnd
        }
        _ => SplitPaneUpdate::Rebuild,
    }
}

impl BrowserWidgets {
    /// Detach transient widgets which were parented manually with
    /// `WidgetExt::set_parent` before GTK begins finalizing their owners.
    ///
    /// Unlike layout children, GTK does not automatically remove these
    /// popovers from their parents during Rust field destruction. In
    /// particular, retaining the location-completion popover until after its
    /// `GtkEntry` starts finalizing triggers a GTK lifecycle warning and can
    /// leave the remaining application windows unresponsive.
    pub fn prepare_for_window_close(&self) {
        self.location_suggestions.popdown();
        self.list_context_menu.popdown();
        self.grid_context_menu.popdown();
        self.search_context_menu.popdown();
        self.list_background_menu.popdown();
        self.grid_background_menu.popdown();

        for popover in [
            self.location_suggestions.upcast_ref::<gtk::Widget>(),
            self.list_context_menu.upcast_ref(),
            self.grid_context_menu.upcast_ref(),
            self.search_context_menu.upcast_ref(),
            self.list_background_menu.upcast_ref(),
            self.grid_background_menu.upcast_ref(),
        ] {
            if popover.parent().is_some() {
                popover.unparent();
            }
        }
    }

    pub fn appearance_preset(&self) -> AppearancePreset {
        self.appearance_manager.preset()
    }

    pub fn apply_appearance(&self, preset: AppearancePreset) {
        self.appearance_manager
            .apply(self.window.upcast_ref(), preset);
    }

    pub fn apply_appearance_preferences(&self, preferences: &ViewPreferences) {
        let scheme = match preferences.color_scheme {
            ColorSchemePreference::System => adw::ColorScheme::Default,
            ColorSchemePreference::Light => adw::ColorScheme::ForceLight,
            ColorSchemePreference::Dark => adw::ColorScheme::ForceDark,
        };
        adw::StyleManager::default().set_color_scheme(scheme);
        self.appearance_manager.apply_accessibility(
            self.window.upcast_ref(),
            preferences.font_family.as_deref(),
            preferences.font_scale_percent,
            preferences.reduced_motion,
        );
        self.miller_view
            .set_reduced_motion(preferences.reduced_motion);
    }

    pub fn entry_icon_style(&self) -> EntryIconStyle {
        self.entry_icon_style.get()
    }

    pub fn apply_entry_icon_style(&self, style: EntryIconStyle) {
        let previous = self.entry_icon_style.replace(style);
        self.window.remove_css_class(previous.css_class());
        self.window.add_css_class(style.css_class());
    }

    pub fn set_split_presentation(
        &self,
        is_split: bool,
        active_side: SplitSide,
        ratio: SplitRatio,
        opposite_path: Option<&Path>,
        opposite_items: &[String],
        opposite_total: usize,
    ) -> bool {
        let desired_layout = split_pane_layout(is_split, active_side);
        let current_layout = self.current_split_pane_layout();
        let update = split_pane_update(current_layout, desired_layout);
        let mut restore_view_focus = false;

        if update != SplitPaneUpdate::NoChange {
            restore_view_focus = gtk::prelude::GtkWindowExt::focus(&self.window)
                .is_some_and(|focus| focus.is_ancestor(&self.split_pane));
            if restore_view_focus {
                self.back_button.grab_focus();
            }
        }

        match update {
            SplitPaneUpdate::NoChange => {}
            SplitPaneUpdate::AddInactiveEnd => {
                self.split_pane.set_end_child(Some(&self.inactive_pane));
            }
            SplitPaneUpdate::RemoveInactiveEnd => {
                self.split_pane.set_end_child(None::<&gtk::Widget>);
            }
            SplitPaneUpdate::Rebuild => {
                self.split_pane.set_start_child(None::<&gtk::Widget>);
                self.split_pane.set_end_child(None::<&gtk::Widget>);
                match desired_layout {
                    SplitPaneLayout::ActiveOnly => self
                        .split_pane
                        .set_start_child(Some(&self.active_pane_shell)),
                    SplitPaneLayout::ActiveThenInactive => {
                        self.split_pane
                            .set_start_child(Some(&self.active_pane_shell));
                        self.split_pane.set_end_child(Some(&self.inactive_pane));
                    }
                    SplitPaneLayout::InactiveThenActive => {
                        self.split_pane.set_start_child(Some(&self.inactive_pane));
                        self.split_pane.set_end_child(Some(&self.active_pane_shell));
                    }
                }
            }
        }

        if !is_split {
            self.active_pane_label.set_visible(false);
            return restore_view_focus;
        }

        self.active_pane_label.set_visible(true);
        self.active_pane_label.set_label(match active_side {
            SplitSide::Primary => "Active pane — left",
            SplitSide::Secondary => "Active pane — right",
        });
        self.active_pane_shell
            .update_property(&[gtk::accessible::Property::Description("Active file pane")]);
        self.inactive_pane
            .update_property(&[gtk::accessible::Property::Description(
                "Inactive file pane; activate it to browse and refresh",
            )]);

        let item_refs = opposite_items
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        self.inactive_pane_items
            .splice(0, self.inactive_pane_items.n_items(), &item_refs);
        let path = opposite_path
            .map(|path| path.to_string_lossy())
            .unwrap_or_else(|| "Unavailable".into());
        self.inactive_pane_label
            .set_label(&format!("Other pane — {path}"));
        self.inactive_pane_status
            .set_label(&split_snapshot_status(opposite_items.len(), opposite_total));

        let width = self.split_pane.width().max(1);
        self.split_pane
            .set_position(split_primary_position(width, ratio));
        restore_view_focus
    }

    fn current_split_pane_layout(&self) -> Option<SplitPaneLayout> {
        let start = self.split_pane.start_child();
        let end = self.split_pane.end_child();
        let active = self.active_pane_shell.upcast_ref::<gtk::Widget>();
        let inactive = self.inactive_pane.upcast_ref::<gtk::Widget>();

        match (start.as_ref(), end.as_ref()) {
            (Some(start), None) if start == active => Some(SplitPaneLayout::ActiveOnly),
            (Some(start), Some(end)) if start == active && end == inactive => {
                Some(SplitPaneLayout::ActiveThenInactive)
            }
            (Some(start), Some(end)) if start == inactive && end == active => {
                Some(SplitPaneLayout::InactiveThenActive)
            }
            _ => None,
        }
    }

    pub fn apply_sidebar_density(&self, density: SidebarDensity) {
        for class_name in ["sidebar-compact", "sidebar-balanced", "sidebar-comfortable"] {
            self.sidebar.remove_css_class(class_name);
        }
        self.sidebar.add_css_class(sidebar_density_class(density));

        let metrics = sidebar_density_metrics(density);
        self.sidebar.set_spacing(metrics.section_gap);
        self.sidebar.set_margin_top(metrics.outer_margin);
        self.sidebar.set_margin_bottom(metrics.outer_margin);
        self.sidebar.set_margin_start(metrics.outer_margin);
        self.sidebar.set_margin_end(metrics.outer_margin);
    }

    pub fn apply_sidebar_collapsed(&self, collapsed: bool) {
        if collapsed {
            self.sidebar.add_css_class("sidebar-collapsed");
            self.sidebar_content
                .set_min_content_width(SIDEBAR_COLLAPSED_WIDTH);
            self.sidebar_content
                .set_width_request(SIDEBAR_COLLAPSED_WIDTH);
            self.workspace.set_position(SIDEBAR_COLLAPSED_WIDTH);
        } else {
            self.sidebar.remove_css_class("sidebar-collapsed");
            self.sidebar_content
                .set_min_content_width(i32::from(SIDEBAR_WIDTH_MIN));
            self.sidebar_content.set_width_request(-1);
        }
    }

    pub fn apply_file_view_policy(
        &self,
        density: FileViewDensity,
        layout: ListColumnLayout,
        grouping: DirectoryGrouping,
    ) {
        let grouping_changed = self.list_grouping.get() != grouping;
        let list_requires_rebind = self.list_layout.get() != layout || grouping_changed;
        self.list_layout.set(layout);
        self.list_grouping.set(grouping);
        self.grouped_grid.set_density(density);
        self.grouped_grid.set_grouping(grouping);
        self.grid_presentation_stack.set_visible_child_name(
            if grouping == DirectoryGrouping::None {
                "ungrouped"
            } else {
                "grouped"
            },
        );
        self.group_header_spacer
            .set_visible(grouping != DirectoryGrouping::None);
        for class_name in ["view-compact", "view-comfortable", "view-spacious"] {
            self.list_view.remove_css_class(class_name);
            self.grid_view.remove_css_class(class_name);
        }
        let class_name = file_view_density_class(density);
        self.list_view.add_css_class(class_name);
        self.grid_view.add_css_class(class_name);
        for header in &self.column_headers {
            header.widget.set_visible(layout.is_visible(header.column));
            header
                .widget
                .set_width_request(i32::from(layout.width(header.column)));
        }
        if let Some(icon_spacer) = self.group_header_spacer.next_sibling() {
            let mut previous = icon_spacer;
            for column in layout.order() {
                if let Some(header) = self
                    .column_headers
                    .iter()
                    .find(|header| header.column == column)
                {
                    self.list_header
                        .reorder_child_after(&header.widget, Some(&previous));
                    previous = header.widget.clone();
                }
            }
        }
        if list_requires_rebind {
            self.list_view
                .set_factory(None::<&gtk::SignalListItemFactory>);
            self.list_view.set_factory(Some(&self.list_factory));
        }
        if grouping_changed {
            self.grid_view
                .set_factory(None::<&gtk::SignalListItemFactory>);
            let grid_factory = self.grid_factory.borrow();
            self.grid_view.set_factory(Some(&*grid_factory));
        }
    }

    pub fn toggle_group_collapse(&self, label: &str) -> Vec<String> {
        if label.is_empty() {
            return self.collapsed_group_labels();
        }
        {
            let mut collapsed = self.collapsed_groups.borrow_mut();
            if !collapsed.remove(label) {
                collapsed.insert(label.to_owned());
            }
        }
        self.list_view
            .set_factory(None::<&gtk::SignalListItemFactory>);
        self.list_view.set_factory(Some(&self.list_factory));
        self.grid_view
            .set_factory(None::<&gtk::SignalListItemFactory>);
        let grid_factory = self.grid_factory.borrow();
        self.grid_view.set_factory(Some(&*grid_factory));
        self.grouped_grid.refresh_collapsed_groups();
        self.collapsed_group_labels()
    }

    pub fn collapsed_group_labels(&self) -> Vec<String> {
        let mut labels = self
            .collapsed_groups
            .borrow()
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        labels.sort();
        labels.truncate(crate::preferences::COLLAPSED_GROUP_CAPACITY);
        labels
    }

    pub fn set_view_mode(&self, mode: ViewMode) {
        self.view_stack.set_visible_child_name(mode.stack_name());
        self.list_header.set_visible(mode == ViewMode::List);
        self.grid_size_controls.set_visible(mode == ViewMode::Grid);
        self.list_view_button.set_active(mode == ViewMode::List);
        self.grid_view_button.set_active(mode == ViewMode::Grid);
        self.miller_view_button.set_active(mode == ViewMode::Miller);
    }

    pub fn apply_click_policy(&self, policy: ClickPolicy) {
        let single = policy.activates_on_single_click();
        self.list_view.set_single_click_activate(single);
        self.grid_view.set_single_click_activate(single);
        self.grouped_grid.set_single_click_activate(single);
        self.search_results_view.set_single_click_activate(single);
        self.miller_view.set_single_click_activate(single);
        let description = if single {
            "Single-click opens items; Enter always opens the selected item"
        } else {
            "Double-click opens items; Enter always opens the selected item"
        };
        self.list_view
            .update_property(&[gtk::accessible::Property::Description(description)]);
        self.grid_view
            .update_property(&[gtk::accessible::Property::Description(description)]);
    }

    pub fn set_grid_size(&self, size: GridSize) {
        self.grid_size_scale.set_value(size.index() as f64);
        let label = format!("Grid icon size: {} pixels", size.edge());
        self.grid_size_scale.set_tooltip_text(Some(&label));
        set_accessible_label(&self.grid_size_scale, &label);
        let factory = build_grid_factory(
            GridFactoryDependencies {
                selection: &self.selection,
                context_menu: &self.grid_context_menu,
                thumbnails: &self.thumbnails,
                grouping: &self.list_grouping,
                collapsed_groups: &self.collapsed_groups,
                drop_dispatcher: &self.drop_dispatcher,
            },
            size,
            0,
        );
        self.grid_view.set_factory(Some(&factory));
        self.grid_factory.replace(factory);
        self.grouped_grid.set_grid_size(size);
    }

    pub fn focus_view(&self, mode: ViewMode) {
        match mode {
            ViewMode::List => {
                self.list_view.grab_focus();
            }
            ViewMode::Grid => {
                if self.list_grouping.get() == DirectoryGrouping::None {
                    self.grid_view.grab_focus();
                } else {
                    self.grouped_grid.focus_first_section();
                }
            }
            ViewMode::Miller => {
                self.miller_view.focus_active();
            }
        }
    }

    pub fn scroll_to_operation_result(&self, mode: ViewMode, index: u32) -> bool {
        let info = gtk::ScrollInfo::new();
        info.set_enable_vertical(true);
        match mode {
            ViewMode::List => {
                self.list_view
                    .scroll_to(index, gtk::ListScrollFlags::NONE, Some(info));
                true
            }
            ViewMode::Grid if self.list_grouping.get() == DirectoryGrouping::None => {
                self.grid_view
                    .scroll_to(index, gtk::ListScrollFlags::NONE, Some(info));
                true
            }
            ViewMode::Grid => self.grouped_grid.scroll_to(index),
            ViewMode::Miller => self.miller_view.scroll_active_to(index),
        }
    }

    pub fn operation_result_emphasis_targets(&self) -> Vec<gtk::Widget> {
        vec![
            self.list_view.clone().upcast(),
            self.grid_view.clone().upcast(),
            self.grouped_grid_view.clone().upcast(),
            self.miller_view.widget().clone().upcast(),
            self.search_results_view.clone().upcast(),
        ]
    }

    pub fn set_views_sensitive(&self, sensitive: bool) {
        self.list_view.set_sensitive(sensitive);
        self.grid_view.set_sensitive(sensitive);
        self.grouped_grid_view.set_sensitive(sensitive);
        self.search_results_view.set_sensitive(sensitive);
        self.miller_view.widget().set_sensitive(sensitive);
    }

    pub fn set_trash_mode(&self, active: bool) {
        if active {
            let file_model = build_trash_context_menu_model();
            let background_model = build_trash_background_context_menu_model();
            self.list_context_menu.set_menu_model(Some(&file_model));
            self.grid_context_menu.set_menu_model(Some(&file_model));
            self.list_background_menu
                .set_menu_model(Some(&background_model));
            self.grid_background_menu
                .set_menu_model(Some(&background_model));
        } else {
            self.list_context_menu
                .set_menu_model(Some(&self.file_context_model));
            self.grid_context_menu
                .set_menu_model(Some(&self.file_context_model));
            self.list_background_menu
                .set_menu_model(Some(&self.background_context_model));
            self.grid_background_menu
                .set_menu_model(Some(&self.background_context_model));
        }
        self.hidden_button.set_sensitive(!active);
        self.add_bookmark_button.set_sensitive(!active);
        if let Some(icon) = self.empty_state.first_child().and_downcast::<gtk::Image>() {
            icon.set_icon_name(Some(if active {
                "floe-phosphor-trash-symbolic"
            } else {
                "floe-phosphor-folder-symbolic"
            }));
            if let Some(label) = icon.next_sibling().and_downcast::<gtk::Label>() {
                label.set_label(if active {
                    "Trash is empty"
                } else {
                    "This folder is empty"
                });
            }
        }
        for header in &self.sort_headers {
            match (active, header.column) {
                (true, SortColumn::Type) => header.label.set_label("Original"),
                (true, SortColumn::Modified) => header.label.set_label("Deleted"),
                (false, SortColumn::Type | SortColumn::Modified) => {
                    header.label.set_label(header.column.label());
                }
                _ => {}
            }
        }
    }

    pub fn popdown_context_menus(&self) {
        self.list_context_menu.popdown();
        self.grid_context_menu.popdown();
        self.search_context_menu.popdown();
        self.search_background_menu.popdown();
        self.list_background_menu.popdown();
        self.grid_background_menu.popdown();
    }

    pub fn context_menu_visible(&self) -> bool {
        [
            &self.list_context_menu,
            &self.grid_context_menu,
            &self.search_context_menu,
            &self.search_background_menu,
            &self.list_background_menu,
            &self.grid_background_menu,
        ]
        .into_iter()
        .any(gtk::prelude::WidgetExt::is_visible)
    }

    pub fn apply_context_menu_preferences(
        &self,
        preferences: ContextMenuPreferences,
        custom_actions: &[CustomActionDefinition],
    ) {
        self.popdown_context_menus();
        populate_file_context_menu_model(&self.file_context_model, preferences, custom_actions);
        populate_background_context_menu_model(&self.background_context_model, preferences);
    }

    pub fn context_menu(&self, mode: ViewMode) -> &gtk::PopoverMenu {
        match mode {
            ViewMode::List => &self.list_context_menu,
            ViewMode::Grid => &self.grid_context_menu,
            ViewMode::Miller => &self.list_context_menu,
        }
    }

    pub fn background_menu(&self, mode: ViewMode) -> &gtk::PopoverMenu {
        match mode {
            ViewMode::List => &self.list_background_menu,
            ViewMode::Grid => &self.grid_background_menu,
            ViewMode::Miller => &self.list_background_menu,
        }
    }
}

pub fn split_primary_position(width: i32, ratio: SplitRatio) -> i32 {
    let width = width.max(1);
    ((i64::from(width) * i64::from(ratio.basis_points())) / 10_000) as i32
}

pub fn split_snapshot_status(cached_count: usize, total_count: usize) -> String {
    if total_count > cached_count {
        return format!("First {cached_count} of {total_count} items cached — activate to refresh");
    }
    match cached_count {
        0 => "Activate pane to load or refresh".to_owned(),
        1 => "1 cached item — activate to refresh".to_owned(),
        count => format!("{count} cached items — activate to refresh"),
    }
}

const fn file_view_density_class(density: FileViewDensity) -> &'static str {
    match density {
        FileViewDensity::Compact => "view-compact",
        FileViewDensity::Comfortable => "view-comfortable",
        FileViewDensity::Spacious => "view-spacious",
    }
}

fn organize_header_options_menu(
    create: &gio::Menu,
    open_inspect: &gio::Menu,
    file_operations: &gio::Menu,
    view_layout: &gio::Menu,
    tools_safety: &gio::Menu,
    utility: &gio::Menu,
) -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append_submenu(Some("Create"), create);
    menu.append_submenu(Some("Open & Inspect"), open_inspect);
    menu.append_submenu(Some("File Operations"), file_operations);
    menu.append_submenu(Some("View & Layout"), view_layout);
    menu.append_submenu(Some("Tools & Safety"), tools_safety);
    menu.append_section(None, utility);
    menu
}

pub(crate) fn build_sort_by_menu_model() -> gio::Menu {
    let menu = gio::Menu::new();
    let criteria = gio::Menu::new();
    for column in [
        SortColumn::Name,
        SortColumn::NaturalName,
        SortColumn::Size,
        SortColumn::Modified,
        SortColumn::Created,
        SortColumn::Accessed,
        SortColumn::Type,
        SortColumn::Rating,
        SortColumn::Tags,
        SortColumn::Comment,
    ] {
        criteria.append(
            Some(column.label()),
            Some(&format!("win.sort-column::{}", column.persisted())),
        );
    }
    menu.append_section(None, &criteria);

    let advanced = gio::Menu::new();
    for (category, fields) in [
        (
            "Document",
            &[SortColumn::DocumentWordCount, SortColumn::DocumentLineCount] as &[SortColumn],
        ),
        (
            "Image",
            &[
                SortColumn::ImageDimensions,
                SortColumn::ImageOrientation,
                SortColumn::ImageWidth,
                SortColumn::ImageHeight,
            ],
        ),
        (
            "Audio",
            &[
                SortColumn::AudioArtist,
                SortColumn::AudioAlbum,
                SortColumn::AudioDuration,
                SortColumn::AudioTrack,
                SortColumn::AudioGenre,
                SortColumn::AudioBitrate,
            ],
        ),
        (
            "Video",
            &[
                SortColumn::VideoDuration,
                SortColumn::VideoDimensions,
                SortColumn::VideoWidth,
                SortColumn::VideoHeight,
                SortColumn::VideoFrameRate,
                SortColumn::VideoBitrate,
            ],
        ),
    ] {
        let submenu = gio::Menu::new();
        for column in fields {
            submenu.append(
                Some(column.label()),
                Some(&format!("win.sort-column::{}", column.persisted())),
            );
        }
        advanced.append_submenu(Some(category), &submenu);
    }
    let other = gio::Menu::new();
    other.append(
        Some(SortColumn::Extension.label()),
        Some("win.sort-column::extension"),
    );
    for column in [
        SortColumn::Path,
        SortColumn::LinkDestination,
        SortColumn::Permissions,
        SortColumn::Owner,
        SortColumn::Group,
    ] {
        other.append(
            Some(column.label()),
            Some(&format!("win.sort-column::{}", column.persisted())),
        );
    }
    advanced.append_submenu(Some("Other"), &other);
    menu.append_section(None, &advanced);

    let direction = gio::Menu::new();
    direction.append(
        Some("Ascending / Oldest First"),
        Some("win.sort-direction::ascending"),
    );
    direction.append(
        Some("Descending / Newest First"),
        Some("win.sort-direction::descending"),
    );
    menu.append_section(None, &direction);

    let placement = gio::Menu::new();
    placement.append(Some("Folders First"), Some("win.folders-first"));
    placement.append(Some("Hidden Files Last"), Some("win.hidden-last"));
    menu.append_section(None, &placement);

    let index_controls = gio::Menu::new();
    index_controls.append(
        Some("Cancel Metadata Scan"),
        Some("win.cancel-metadata-sort"),
    );
    index_controls.append(
        Some("Clear Metadata Cache"),
        Some("win.clear-metadata-sort-cache"),
    );
    menu.append_section(None, &index_controls);
    menu
}

pub fn build(
    application: &adw::Application,
    locations: &[Location],
    appearance: Appearance,
    preferences: ViewPreferences,
) -> BrowserWidgets {
    let window_size = initial_window_size(&preferences);
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title("Floe")
        .icon_name(crate::iconography::APPLICATION_ICON_NAME)
        .default_width(window_size.width())
        .default_height(window_size.height())
        .width_request(720)
        .height_request(480)
        .build();
    window.add_css_class("floe-window");
    window.add_css_class(preferences.icon_style.css_class());
    let appearance_manager = AppearanceManager::new(window.upcast_ref(), appearance.preset);
    let entry_icon_style = Rc::new(Cell::new(preferences.icon_style));

    let back_button = icon_button(
        "floe-phosphor-arrow-left-symbolic",
        "Back (Alt+Left)",
        "win.back",
    );
    let forward_button = icon_button(
        "floe-phosphor-arrow-right-symbolic",
        "Forward (Alt+Right)",
        "win.forward",
    );
    let parent_button = icon_button(
        "floe-phosphor-arrow-up-symbolic",
        "Parent folder (Alt+Up)",
        "win.parent",
    );
    let hidden_button = gtk::ToggleButton::builder()
        .icon_name("floe-phosphor-eye-symbolic")
        .tooltip_text("Show hidden files (Ctrl+H)")
        .action_name("win.hidden")
        .build();
    set_accessible_label(&hidden_button, "Show hidden files");
    let header_search_button = icon_button(
        "floe-phosphor-magnifying-glass-symbolic",
        "Search and filter (Ctrl+F)",
        "win.folder-filter",
    );
    set_accessible_label(&header_search_button, "Search and filter files");
    let open_button = icon_button(
        "floe-phosphor-folder-open-symbolic",
        "Open selected item (Enter)",
        "win.open",
    );
    open_button.set_sensitive(false);

    let path_label = gtk::Label::builder()
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .max_width_chars(58)
        .single_line_mode(true)
        .build();
    path_label.add_css_class("floe-path");
    path_label.set_visible(false);
    let breadcrumb_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(2)
        .hexpand(true)
        .build();
    breadcrumb_box.set_accessible_role(gtk::AccessibleRole::Group);
    breadcrumb_box.update_property(&[
        gtk::accessible::Property::Label("Current folder breadcrumbs"),
        gtk::accessible::Property::Description(
            "Activate a folder segment to navigate directly to that ancestor",
        ),
    ]);
    let recent_locations_button = icon_button(
        "floe-phosphor-clock-counter-clockwise-symbolic",
        "Recent locations",
        "win.recent-locations",
    );
    set_accessible_label(&recent_locations_button, "Recent locations");
    let edit_location_button = gtk::Button::builder()
        .label("Edit")
        .action_name("win.location")
        .tooltip_text("Edit location (Ctrl+L)")
        .has_frame(false)
        .build();
    set_accessible_label(&edit_location_button, "Edit folder location");
    let breadcrumb_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_width(false)
        .min_content_width(160)
        .max_content_width(560)
        .hexpand(true)
        .child(&breadcrumb_box)
        .build();
    let path_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .hexpand(true)
        .build();
    path_box.append(&breadcrumb_scroll);
    path_box.append(&path_label);
    path_box.append(&recent_locations_button);
    path_box.append(&edit_location_button);

    let location_entry = gtk::Entry::builder()
        .placeholder_text("Enter a local path")
        .hexpand(true)
        .width_chars(42)
        .build();
    location_entry.set_tooltip_text(Some(
        "Enter an absolute folder path, then press Enter. Press Escape to cancel.",
    ));
    set_accessible_label(&location_entry, "Folder location");

    let location_error = gtk::Label::builder()
        .halign(gtk::Align::Start)
        .wrap(true)
        .visible(false)
        .build();
    location_error.set_accessible_role(gtk::AccessibleRole::Alert);
    location_error.add_css_class("error");
    location_error.add_css_class("caption");
    let location_suggestions_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    location_suggestions_box.update_property(&[
        gtk::accessible::Property::Label("Folder location suggestions"),
        gtk::accessible::Property::Description(
            "Matching local folders; activate one to navigate to its exact path",
        ),
    ]);
    let location_suggestions_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .max_content_height(280)
        .propagate_natural_height(true)
        .child(&location_suggestions_box)
        .build();
    let location_suggestions = gtk::Popover::builder()
        .autohide(true)
        .has_arrow(false)
        .child(&location_suggestions_scroll)
        .build();
    location_suggestions.set_parent(&location_entry);

    let location_editor = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .hexpand(true)
        .build();
    location_editor.append(&location_entry);
    location_editor.append(&location_error);

    let path_stack = gtk::Stack::new();
    path_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    path_stack.add_named(&path_box, Some("path"));
    path_stack.add_named(&location_editor, Some("entry"));
    path_stack.set_visible_child_name("path");

    let header = adw::HeaderBar::new();
    header.pack_start(&back_button);
    header.pack_start(&forward_button);
    header.pack_start(&parent_button);
    header.set_title_widget(Some(&path_stack));
    let create_model = gio::Menu::new();
    let open_inspect_model = gio::Menu::new();
    let file_operations_model = gio::Menu::new();
    let transfer_model = gio::Menu::new();
    let rename_duplicate_model = gio::Menu::new();
    let links_model = gio::Menu::new();
    let copy_details_model = gio::Menu::new();
    let trash_model = gio::Menu::new();
    let view_layout_model = gio::Menu::new();
    let tools_safety_model = gio::Menu::new();
    let utility_model = gio::Menu::new();

    create_model.append(Some("New Folder…"), Some("win.new-folder"));
    create_model.append(Some("New Empty File…"), Some("win.new-empty-file"));
    create_model.append(Some("New From Template…"), Some("win.new-from-template"));
    open_inspect_model.append(Some("Open With…"), Some("win.open-with"));
    open_inspect_model.append(Some("Open Terminal Here"), Some("win.open-terminal"));
    open_inspect_model.append(
        Some("Preferred Terminal…"),
        Some("win.terminal-preferences"),
    );
    open_inspect_model.append(Some("Properties"), Some("win.properties"));
    tools_safety_model.append(Some("Calculate Checksums…"), Some("win.checksum"));
    let privacy_safety_model = gio::Menu::new();
    privacy_safety_model.append(
        Some("Inspect Privacy & Safety…"),
        Some("win.inspect-privacy-safety"),
    );
    privacy_safety_model.append(Some("Audit Permissions…"), Some("win.audit-permissions"));
    privacy_safety_model.append(Some("Scan with Local ClamAV…"), Some("win.scan-threats"));
    privacy_safety_model.append(
        Some("Create Sanitized Copy…"),
        Some("win.create-sanitized-copy"),
    );
    privacy_safety_model.append(
        Some("Cancel Local ClamAV Scan"),
        Some("win.cancel-threat-scan"),
    );
    privacy_safety_model.append(
        Some("Cancel Metadata Sanitization"),
        Some("win.cancel-sanitization"),
    );
    tools_safety_model.append_submenu(Some("Privacy & Safety"), &privacy_safety_model);
    tools_safety_model.append(Some("Operation Recovery…"), Some("win.recovery-center"));

    let protected_folders_model = gio::Menu::new();
    protected_folders_model.append(Some("Protect Folder"), Some("win.protect-folder"));
    protected_folders_model.append(Some("Unprotect Folder"), Some("win.unprotect-folder"));
    protected_folders_model.append(Some("Protected Folders…"), Some("win.protected-folders"));
    tools_safety_model.append_submenu(Some("Protected Folders"), &protected_folders_model);
    let integrity_model = gio::Menu::new();
    integrity_model.append(
        Some("Save SHA-256 Fingerprint"),
        Some("win.save-sha256-fingerprint"),
    );
    integrity_model.append(
        Some("Verify Saved Fingerprint"),
        Some("win.verify-saved-fingerprint"),
    );
    integrity_model.append(Some("Generate SHA256SUMS"), Some("win.generate-sha256sums"));
    integrity_model.append(
        Some("Verify Selected Manifest"),
        Some("win.verify-sha256sums"),
    );
    integrity_model.append(
        Some("Create Integrity Baseline"),
        Some("win.create-integrity-baseline"),
    );
    integrity_model.append(
        Some("Update Integrity Baseline"),
        Some("win.update-integrity-baseline"),
    );
    integrity_model.append(
        Some("Verify Integrity Baseline"),
        Some("win.verify-integrity-baseline"),
    );
    integrity_model.append(
        Some("Start Integrity Monitoring"),
        Some("win.start-integrity-monitoring"),
    );
    integrity_model.append(
        Some("Stop Integrity Monitoring"),
        Some("win.stop-integrity-monitoring"),
    );
    integrity_model.append(
        Some("Delete Integrity Baseline"),
        Some("win.delete-integrity-baseline"),
    );
    tools_safety_model.append_submenu(Some("Integrity"), &integrity_model);
    let archive_model = gio::Menu::new();
    archive_model.append(Some("Extract Here"), Some("win.extract-here"));
    archive_model.append(Some("Extract To…"), Some("win.extract-to"));
    archive_model.append(Some("Compress…"), Some("win.compress"));
    tools_safety_model.append_submenu(Some("Archives"), &archive_model);
    utility_model.append(
        Some("Customize Context Menus…"),
        Some("win.context-menu-settings"),
    );
    transfer_model.append(Some("Copy"), Some("win.copy"));
    transfer_model.append(Some("Copy and Verify…"), Some("win.copy-and-verify"));
    transfer_model.append(
        Some("Verified Removable Transfer…"),
        Some("win.verified-removable-transfer"),
    );
    transfer_model.append(Some("Move"), Some("win.cut"));
    rename_duplicate_model.append(Some("Duplicate"), Some("win.duplicate"));
    rename_duplicate_model.append(Some("Rename…"), Some("win.rename"));
    rename_duplicate_model.append(Some("Batch Rename…"), Some("win.batch-rename"));
    rename_duplicate_model.append(
        Some("Undo Last Batch Rename"),
        Some("win.undo-batch-rename"),
    );
    links_model.append(
        Some("Create Symbolic Link…"),
        Some("win.create-symbolic-link"),
    );
    links_model.append(Some("Create Hard Link…"), Some("win.create-hard-link"));
    links_model.append(Some("Reveal Link Target"), Some("win.reveal-link-target"));
    copy_details_model.append(Some("Copy Name"), Some("win.copy-name"));
    copy_details_model.append(Some("Copy Path"), Some("win.copy-path"));
    copy_details_model.append(Some("Copy Relative Path"), Some("win.copy-relative-path"));
    copy_details_model.append(Some("Copy URI"), Some("win.copy-uri"));
    trash_model.append(Some("Move to Trash"), Some("win.trash"));
    trash_model.append(Some("Delete Permanently…"), Some("win.permanent-delete"));
    trash_model.append(Some("Restore"), Some("win.restore"));
    trash_model.append(Some("Empty Trash…"), Some("win.empty-trash"));

    file_operations_model.append_submenu(Some("Transfer"), &transfer_model);
    file_operations_model.append_submenu(Some("Rename & Duplicate"), &rename_duplicate_model);
    file_operations_model.append_submenu(Some("Links"), &links_model);
    file_operations_model.append_submenu(Some("Copy Details"), &copy_details_model);
    file_operations_model.append_submenu(Some("Trash"), &trash_model);

    let sidebar_density_model = gio::Menu::new();
    for (label, action) in SIDEBAR_DENSITY_MENU_ITEMS {
        sidebar_density_model.append(Some(label), Some(action));
    }
    let sidebar_model = gio::Menu::new();
    sidebar_model.append(
        Some("Collapse or Expand Sidebar"),
        Some("win.toggle-sidebar"),
    );
    sidebar_model.append_submenu(Some("Sidebar Density"), &sidebar_density_model);
    sidebar_model.append(
        Some(RESET_SIDEBAR_WIDTH_MENU_ITEM.0),
        Some(RESET_SIDEBAR_WIDTH_MENU_ITEM.1),
    );
    view_layout_model.append_submenu(Some("Sidebar"), &sidebar_model);
    view_layout_model.append(
        Some("Operation Completion Notifications"),
        Some("win.completion-notifications"),
    );
    let appearance_model = gio::Menu::new();
    for preset in AppearancePreset::ALL {
        appearance_model.append(
            Some(preset.label()),
            Some(&format!("win.appearance::{}", preset.persisted())),
        );
    }
    view_layout_model.append_submenu(Some("Appearance"), &appearance_model);

    let icon_style_model = gio::Menu::new();
    for style in EntryIconStyle::ALL {
        icon_style_model.append(
            Some(style.label()),
            Some(&format!("win.icon-style::{}", style.persisted())),
        );
    }
    view_layout_model.append_submenu(Some("File & Folder Icons"), &icon_style_model);
    let file_density_model = gio::Menu::new();
    for (label, value) in [
        ("Compact", "compact"),
        ("Comfortable", "comfortable"),
        ("Spacious", "spacious"),
    ] {
        file_density_model.append(Some(label), Some(&format!("win.file-density::{value}")));
    }
    let grouping_model = gio::Menu::new();
    for (label, value) in [
        ("None", "none"),
        ("Type", "type"),
        ("Extension", "extension"),
        ("Date", "date"),
        ("Size", "size"),
    ] {
        grouping_model.append(Some(label), Some(&format!("win.grouping::{value}")));
    }
    let directory_model = gio::Menu::new();
    directory_model.append(
        Some("Folders First"),
        Some("win.directory-placement::first"),
    );
    directory_model.append(Some("Folders Last"), Some("win.directory-placement::last"));
    let columns_model = gio::Menu::new();
    let name_column_menu = gio::Menu::new();
    name_column_menu.append(Some("Narrower"), Some("win.narrow-name"));
    name_column_menu.append(Some("Wider"), Some("win.widen-name"));
    name_column_menu.append(Some("Auto Size"), Some("win.autosize-name"));
    name_column_menu.append(Some("Move Left"), Some("win.move-column-left-name"));
    name_column_menu.append(Some("Move Right"), Some("win.move-column-right-name"));
    columns_model.append_submenu(Some("Name"), &name_column_menu);
    for column in ListColumn::OPTIONAL {
        let column_menu = gio::Menu::new();
        column_menu.append(
            Some("Show Column"),
            Some(&format!("win.column-{}", column.persisted())),
        );
        column_menu.append(
            Some("Narrower"),
            Some(&format!("win.narrow-{}", column.persisted())),
        );
        column_menu.append(
            Some("Wider"),
            Some(&format!("win.widen-{}", column.persisted())),
        );
        column_menu.append(
            Some("Auto Size"),
            Some(&format!("win.autosize-{}", column.persisted())),
        );
        column_menu.append(
            Some("Move Left"),
            Some(&format!("win.move-column-left-{}", column.persisted())),
        );
        column_menu.append(
            Some("Move Right"),
            Some(&format!("win.move-column-right-{}", column.persisted())),
        );
        columns_model.append_submenu(Some(column.label()), &column_menu);
    }
    let browser_view_model = gio::Menu::new();
    let sort_by_model = build_sort_by_menu_model();
    browser_view_model.append_submenu(Some("Sort By"), &sort_by_model);
    browser_view_model.append_submenu(Some("File Density"), &file_density_model);
    browser_view_model.append_submenu(Some("Group By"), &grouping_model);
    browser_view_model.append_submenu(Some("Folder Placement"), &directory_model);
    browser_view_model.append_submenu(Some("Columns"), &columns_model);
    let miller_width_model = gio::Menu::new();
    miller_width_model.append(Some("Narrower"), Some("win.narrow-miller-columns"));
    miller_width_model.append(Some("Wider"), Some("win.widen-miller-columns"));
    browser_view_model.append_submenu(Some("Miller Column Width"), &miller_width_model);
    browser_view_model.append(
        Some("Remember View per Folder"),
        Some("win.remember-folder-view"),
    );
    browser_view_model.append(Some("Vim Navigation Mode"), Some("win.vim-mode"));
    view_layout_model.append_submenu(Some("Browser View"), &browser_view_model);
    let split_view_model = gio::Menu::new();
    split_view_model.append(Some("Toggle Split View"), Some("win.toggle-split"));
    split_view_model.append(Some("Switch Active Pane"), Some("win.switch-split-side"));
    split_view_model.append(Some("Swap Panes"), Some("win.swap-split-sides"));
    split_view_model.append(Some("Close Split"), Some("win.close-split"));
    split_view_model.append(Some("Narrow Primary Pane"), Some("win.narrow-primary-pane"));
    split_view_model.append(Some("Widen Primary Pane"), Some("win.widen-primary-pane"));
    split_view_model.append(
        Some("Open Folder in Other Pane"),
        Some("win.open-opposite-pane"),
    );
    split_view_model.append(
        Some("Copy to Other Pane"),
        Some("win.copy-to-opposite-pane"),
    );
    split_view_model.append(
        Some("Move to Other Pane"),
        Some("win.move-to-opposite-pane"),
    );
    split_view_model.append(
        Some("Create Links in Other Pane"),
        Some("win.link-to-opposite-pane"),
    );
    view_layout_model.append_submenu(Some("Split View"), &split_view_model);

    utility_model.append(Some(SETTINGS_MENU_ITEM.0), Some(SETTINGS_MENU_ITEM.1));
    utility_model.append(Some("New Window"), Some("app.new-window"));
    utility_model.append(
        Some(OPERATION_HISTORY_MENU_ITEM.0),
        Some(OPERATION_HISTORY_MENU_ITEM.1),
    );
    utility_model.append(
        Some(KEYBOARD_SHORTCUTS_MENU_ITEM.0),
        Some(KEYBOARD_SHORTCUTS_MENU_ITEM.1),
    );
    utility_model.append(
        Some(DESKTOP_INTEGRATION_MENU_ITEM.0),
        Some(DESKTOP_INTEGRATION_MENU_ITEM.1),
    );

    let file_actions_model = organize_header_options_menu(
        &create_model,
        &open_inspect_model,
        &file_operations_model,
        &view_layout_model,
        &tools_safety_model,
        &utility_model,
    );
    let file_actions = gtk::MenuButton::builder()
        .icon_name("floe-phosphor-dots-three-symbolic")
        .tooltip_text("Main menu, organized by task")
        .menu_model(&file_actions_model)
        .build();
    set_accessible_label(&file_actions, "Main menu");
    file_actions.update_property(&[gtk::accessible::Property::Description(
        "Create, open, manage, view, and inspect files; access tools and settings",
    )]);

    let sort_menu_button = gtk::MenuButton::builder()
        .icon_name("floe-phosphor-arrows-down-up-symbolic")
        .tooltip_text("Sort files and folders")
        .menu_model(&sort_by_model)
        .build();
    set_accessible_label(&sort_menu_button, "Sort files and folders");
    sort_menu_button.update_property(&[gtk::accessible::Property::Description(
        "Choose the sort property, direction, folder placement, and hidden-file placement",
    )]);

    let list_view_button = gtk::ToggleButton::builder()
        .icon_name("floe-phosphor-list-bullets-symbolic")
        .tooltip_text("List view (Ctrl+1)")
        .action_name("win.view-list")
        .build();
    set_accessible_label(&list_view_button, "List view");
    let grid_view_button = gtk::ToggleButton::builder()
        .icon_name("floe-phosphor-squares-four-symbolic")
        .tooltip_text("Grid view (Ctrl+2)")
        .action_name("win.view-grid")
        .group(&list_view_button)
        .build();
    set_accessible_label(&grid_view_button, "Grid view");
    let miller_view_button = gtk::ToggleButton::builder()
        .label("Columns")
        .tooltip_text("Miller column view")
        .action_name("win.view-miller")
        .group(&list_view_button)
        .build();
    set_accessible_label(&miller_view_button, "Miller column view");
    list_view_button.set_active(preferences.mode == ViewMode::List);
    grid_view_button.set_active(preferences.mode == ViewMode::Grid);
    miller_view_button.set_active(preferences.mode == ViewMode::Miller);
    let view_controls = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    view_controls.add_css_class("linked");
    view_controls.append(&list_view_button);
    view_controls.append(&grid_view_button);
    view_controls.append(&miller_view_button);

    let zoom_out_button = icon_button(
        "floe-phosphor-minus-symbolic",
        "Decrease grid icon size (Ctrl+-)",
        "win.zoom-out",
    );
    let zoom_in_button = icon_button(
        "floe-phosphor-plus-symbolic",
        "Increase grid icon size (Ctrl++)",
        "win.zoom-in",
    );
    let grid_size_scale = gtk::Scale::with_range(
        gtk::Orientation::Horizontal,
        0.0,
        (GRID_SIZES.len() - 1) as f64,
        1.0,
    );
    grid_size_scale.set_value(preferences.grid_size.index() as f64);
    grid_size_scale.set_draw_value(false);
    grid_size_scale.set_digits(0);
    grid_size_scale.set_width_request(112);
    grid_size_scale.set_tooltip_text(Some("Grid icon size"));
    set_accessible_label(&grid_size_scale, "Grid icon size");
    let grid_size_controls = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(2)
        .build();
    grid_size_controls.add_css_class("linked");
    grid_size_controls.append(&zoom_out_button);
    grid_size_controls.append(&grid_size_scale);
    grid_size_controls.append(&zoom_in_button);
    grid_size_controls.set_visible(preferences.mode == ViewMode::Grid);

    let vim_mode_button = gtk::ToggleButton::builder()
        .label(if preferences.vim_mode {
            VIM_MODE_ON_LABEL
        } else {
            VIM_MODE_OFF_LABEL
        })
        .tooltip_text(VIM_MODE_TOOLTIP)
        .action_name("win.vim-mode")
        .build();
    vim_mode_button.add_css_class("flat");
    set_accessible_label(
        &vim_mode_button,
        if preferences.vim_mode {
            "Vim navigation mode enabled"
        } else {
            "Vim navigation mode disabled"
        },
    );

    header.pack_end(&hidden_button);
    header.pack_end(&header_search_button);
    header.pack_end(&open_button);
    header.pack_end(&file_actions);
    header.pack_end(&vim_mode_button);
    header.pack_end(&grid_size_controls);
    header.pack_end(&view_controls);
    header.pack_end(&sort_menu_button);

    let sidebar = build_sidebar(
        locations,
        i32::from(SIDEBAR_WIDTH_MIN),
        preferences.sidebar_density,
    );
    let drop_dispatcher = DropDispatcher::default();
    let DirectoryPanelWidgets {
        content,
        selection,
        list_view,
        grid_view,
        grouped_grid_view,
        miller_view,
        view_stack,
        list_header,
        list_context_menu,
        grid_context_menu,
        list_background_menu,
        grid_background_menu,
        file_context_model,
        background_context_model,
        empty_state,
        empty_label,
        search_bar,
        search_mode,
        filter_entry,
        filter_mode,
        filter_feedback,
        advanced_filter_toggle,
        advanced_filter_box,
        advanced_type,
        advanced_extension,
        advanced_mime,
        advanced_size,
        advanced_date,
        advanced_owner,
        advanced_hidden,
        advanced_match_case,
        advanced_apply,
        advanced_clear,
        search_scope,
        search_button,
        search_stop_button,
        saved_searches,
        recent_searches,
        search_result_order,
        save_search_button,
        delete_saved_search_button,
        clear_recent_searches_button,
        search_index_toggle,
        search_index_menu_button,
        search_feedback,
        search_results_view,
        search_context_menu,
        search_background_menu,
        spinner,
        status_label,
        sort_headers,
        column_headers,
        group_header_spacer,
        thumbnails,
        metadata,
        list_layout,
        list_grouping,
        collapsed_groups,
        list_factory,
        grid_factory,
        grid_presentation_stack,
        grouped_grid,
    } = build_directory_panel(preferences.clone(), &drop_dispatcher, &entry_icon_style);

    content.set_width_request(420);
    let active_pane_label = gtk::Label::builder()
        .label("Active pane")
        .xalign(0.0)
        .margin_start(10)
        .margin_end(10)
        .margin_top(6)
        .margin_bottom(2)
        .build();
    active_pane_label.add_css_class("heading");
    active_pane_label.set_visible(false);
    let active_pane_shell = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();
    active_pane_shell.add_css_class("floe-active-pane");
    active_pane_shell.append(&active_pane_label);
    active_pane_shell.append(&content);

    let inactive_pane_label = gtk::Label::builder()
        .label("Other pane")
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .hexpand(true)
        .build();
    inactive_pane_label.add_css_class("heading");
    let activate_inactive = gtk::Button::builder()
        .label("Activate Pane")
        .action_name("win.switch-split-side")
        .tooltip_text("Activate other pane (F6)")
        .build();
    set_accessible_label(&activate_inactive, "Activate other file pane");
    let inactive_header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_start(10)
        .margin_end(10)
        .margin_top(8)
        .margin_bottom(4)
        .build();
    inactive_header.append(&inactive_pane_label);
    inactive_header.append(&activate_inactive);
    let inactive_pane_items = gtk::StringList::new(&[]);
    let inactive_selection = gtk::NoSelection::new(Some(inactive_pane_items.clone()));
    let inactive_factory = gtk::SignalListItemFactory::new();
    inactive_factory.connect_setup(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let label = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .margin_start(12)
            .margin_end(12)
            .margin_top(3)
            .margin_bottom(3)
            .build();
        item.set_child(Some(&label));
    });
    inactive_factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(label) = item.child().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(value) = item.item().and_downcast::<gtk::StringObject>() else {
            return;
        };
        label.set_label(value.string().as_str());
    });
    let inactive_list = gtk::ListView::new(Some(inactive_selection), Some(inactive_factory));
    inactive_list.set_can_focus(false);
    inactive_list.add_css_class("floe-split-snapshot");
    let inactive_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .child(&inactive_list)
        .build();
    let inactive_pane_status = gtk::Label::builder()
        .label("Activate pane to load or refresh")
        .xalign(0.0)
        .margin_start(10)
        .margin_end(10)
        .margin_top(4)
        .margin_bottom(8)
        .build();
    inactive_pane_status.add_css_class("caption");
    inactive_pane_status.add_css_class("dim-label");
    let inactive_pane = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .width_request(240)
        .hexpand(true)
        .vexpand(true)
        .build();
    inactive_pane.add_css_class("floe-panel");
    inactive_pane.append(&inactive_header);
    inactive_pane.append(&inactive_scroller);
    inactive_pane.append(&inactive_pane_status);

    let split_pane = gtk::Paned::builder()
        .orientation(gtk::Orientation::Horizontal)
        .wide_handle(true)
        .resize_start_child(true)
        .resize_end_child(true)
        .shrink_start_child(false)
        .shrink_end_child(false)
        .hexpand(true)
        .vexpand(true)
        .build();
    split_pane.add_css_class("floe-split-pane");
    split_pane.set_start_child(Some(&active_pane_shell));

    let restored_sidebar_width = initial_sidebar_width(preferences, appearance.sidebar_width());
    let (resize_sidebar, resize_content) = sidebar_pane_resize_policy();
    let workspace = gtk::Paned::builder()
        .orientation(gtk::Orientation::Horizontal)
        .position(restored_sidebar_width)
        .wide_handle(true)
        .resize_start_child(resize_sidebar)
        .resize_end_child(resize_content)
        .shrink_start_child(true)
        .shrink_end_child(false)
        .hexpand(true)
        .vexpand(true)
        .build();
    workspace.add_css_class("floe-workspace");
    workspace.set_start_child(Some(&sidebar.content));
    workspace.set_end_child(Some(&split_pane));

    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    let tab_bar = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(2)
        .hexpand(true)
        .build();
    tab_bar.add_css_class("floe-tab-bar");
    let tab_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .hexpand(true)
        .child(&tab_bar)
        .build();
    tab_scroller.add_css_class("floe-tab-scroller");
    let new_tab_button = icon_button(
        "floe-phosphor-plus-symbolic",
        "New Tab (Ctrl+T)",
        "win.new-tab",
    );
    set_accessible_label(&new_tab_button, "New tab");
    new_tab_button.add_css_class("floe-tab-new");
    let tab_strip = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .build();
    tab_strip.add_css_class("floe-tab-strip");
    tab_strip.append(&tab_scroller);
    tab_strip.append(&new_tab_button);
    let background_feedback_list = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .build();
    let background_feedback_surface = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(12)
        .margin_end(12)
        .build();
    background_feedback_surface.add_css_class("card");
    background_feedback_surface.add_css_class("floe-background-feedback");
    background_feedback_surface.append(&background_feedback_list);
    let background_feedback_revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .reveal_child(false)
        .child(&background_feedback_surface)
        .build();
    background_feedback_revealer.set_accessible_role(gtk::AccessibleRole::Group);
    background_feedback_revealer.update_property(&[
        gtk::accessible::Property::Label("Background activity"),
        gtk::accessible::Property::Description(
            "Persistent running and completed background task feedback",
        ),
    ]);

    root.append(&header);
    root.append(&tab_strip);
    root.append(&background_feedback_revealer);
    root.append(&workspace);

    let operations = build_operations_island();
    let content_overlay = gtk::Overlay::new();
    content_overlay.set_child(Some(&root));
    content_overlay.add_overlay(&operations.revealer);

    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&content_overlay));
    window.set_content(Some(&toast_overlay));
    crate::contextual_help::install_on_tree(&window);

    BrowserWidgets {
        window,
        appearance_manager,
        entry_icon_style,
        toast_overlay,
        background_feedback_revealer,
        background_feedback_list,
        back_button,
        forward_button,
        parent_button,
        hidden_button,
        path_label,
        breadcrumb_box,
        recent_locations_button,
        path_stack,
        location_entry,
        location_error,
        location_suggestions,
        location_suggestions_box,
        tab_bar,
        selection,
        list_view,
        grid_view,
        grouped_grid_view,
        miller_view,
        view_stack,
        list_header,
        list_context_menu,
        grid_context_menu,
        list_background_menu,
        grid_background_menu,
        file_context_model,
        background_context_model,
        list_view_button,
        grid_view_button,
        miller_view_button,
        sort_menu_button,
        vim_mode_button,
        grid_size_controls,
        grid_size_scale,
        empty_state,
        empty_label,
        search_bar,
        search_mode,
        filter_entry,
        filter_mode,
        filter_feedback,
        advanced_filter_toggle,
        advanced_filter_box,
        advanced_type,
        advanced_extension,
        advanced_mime,
        advanced_size,
        advanced_date,
        advanced_owner,
        advanced_hidden,
        advanced_match_case,
        advanced_apply,
        advanced_clear,
        search_scope,
        search_button,
        search_stop_button,
        saved_searches,
        recent_searches,
        search_result_order,
        save_search_button,
        delete_saved_search_button,
        clear_recent_searches_button,
        search_index_toggle,
        search_index_menu_button,
        search_feedback,
        search_results_view,
        search_context_menu,
        search_background_menu,
        spinner,
        status_label,
        sort_headers,
        column_headers,
        group_header_spacer,
        thumbnails,
        metadata,
        location_buttons: sidebar.location_buttons,
        bookmarks_box: sidebar.bookmarks_box,
        add_bookmark_button: sidebar.add_bookmark_button,
        devices_box: sidebar.devices_box,
        trash_button: sidebar.trash_button,
        drop_dispatcher,
        workspace,
        split_pane,
        active_pane_shell,
        active_pane_label,
        inactive_pane,
        inactive_pane_label,
        inactive_pane_status,
        inactive_pane_items,
        sidebar_content: sidebar.content.clone(),
        sidebar: sidebar.sidebar,
        sidebar_default_width: appearance.sidebar_width(),
        operations,
        list_layout,
        list_grouping,
        collapsed_groups,
        list_factory,
        grid_factory,
        grid_presentation_stack,
        grouped_grid,
    }
}

pub fn build_rename_dialog(current_name: &str) -> RenameDialogWidgets {
    build_name_dialog(
        "Rename item",
        "New filename",
        current_name,
        "Rename",
        "Rename error",
    )
}

pub fn build_name_dialog(
    title: &str,
    entry_accessible_label: &str,
    current_name: &str,
    action_label: &str,
    error_accessible_label: &str,
) -> RenameDialogWidgets {
    let rename_entry = gtk::Entry::builder()
        .text(current_name)
        .activates_default(true)
        .hexpand(true)
        .build();
    set_accessible_label(&rename_entry, entry_accessible_label);

    let rename_error = gtk::Label::builder()
        .label("Invalid name")
        .halign(gtk::Align::Start)
        .wrap(true)
        .visible(false)
        .build();
    rename_error.add_css_class("error");
    set_accessible_label(&rename_error, error_accessible_label);

    let cancel_button = gtk::Button::with_label("Cancel");
    let rename_button = gtk::Button::with_label(action_label);
    rename_button.add_css_class("suggested-action");

    let actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .halign(gtk::Align::End)
        .spacing(8)
        .build();
    actions.append(&cancel_button);
    actions.append(&rename_button);

    let heading = gtk::Label::builder()
        .label(title)
        .halign(gtk::Align::Start)
        .build();
    heading.add_css_class("title-2");

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    content.append(&heading);
    content.append(&rename_entry);
    content.append(&rename_error);
    content.append(&actions);

    let dialog = adw::Dialog::builder()
        .title(title)
        .content_width(420)
        .child(&content)
        .default_widget(&rename_button)
        .focus_widget(&rename_entry)
        .build();

    RenameDialogWidgets {
        dialog,
        rename_entry,
        rename_error,
        cancel_button,
        rename_button,
    }
}

pub fn build_permanent_delete_dialog(target_labels: &[String]) -> PermanentDeleteDialogWidgets {
    let count = target_labels.len();
    let heading = gtk::Label::builder()
        .label(if count == 1 {
            "Delete this item permanently?".to_owned()
        } else {
            format!("Delete {count} items permanently?")
        })
        .halign(gtk::Align::Start)
        .wrap(true)
        .build();
    heading.add_css_class("title-2");

    let warning = gtk::Label::builder()
        .label("This action is irreversible. The selected items will not be moved to Trash and Floe cannot restore them.")
        .halign(gtk::Align::Start)
        .wrap(true)
        .xalign(0.0)
        .build();
    warning.add_css_class("floe-status");

    let targets_heading = gtk::Label::builder()
        .label("Exact targets")
        .halign(gtk::Align::Start)
        .build();
    targets_heading.add_css_class("heading");

    let target_buffer = gtk::TextBuffer::builder()
        .text(target_labels.join("\n"))
        .build();
    let targets = gtk::TextView::builder()
        .buffer(&target_buffer)
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::None)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(8)
        .right_margin(8)
        .build();
    set_accessible_label(&targets, "Exact permanent deletion targets");

    let target_scroller = gtk::ScrolledWindow::builder()
        .child(&targets)
        .has_frame(true)
        .min_content_height(76)
        .max_content_height(220)
        .propagate_natural_height(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .build();

    let cancel_button = gtk::Button::with_label("Cancel");
    let delete_button = gtk::Button::with_label("Delete Permanently");
    delete_button.add_css_class("destructive-action");

    let actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .halign(gtk::Align::End)
        .spacing(8)
        .build();
    actions.append(&cancel_button);
    actions.append(&delete_button);

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    content.append(&heading);
    content.append(&warning);
    content.append(&targets_heading);
    content.append(&target_scroller);
    content.append(&actions);

    let dialog = adw::Dialog::builder()
        .title("Delete Permanently")
        .content_width(560)
        .child(&content)
        .default_widget(&cancel_button)
        .focus_widget(&cancel_button)
        .build();

    PermanentDeleteDialogWidgets {
        dialog,
        cancel_button,
        delete_button,
    }
}

pub fn build_empty_trash_dialog(target_labels: &[String]) -> PermanentDeleteDialogWidgets {
    let widgets = build_permanent_delete_dialog(target_labels);
    widgets.dialog.set_title("Empty Trash");
    widgets
}

pub fn build_conflict_dialog(
    source_name: &str,
    destination: &str,
    source_description: &str,
    destination_description: &str,
    replace_supported: bool,
    replace_all_supported: bool,
) -> ConflictDialogWidgets {
    let heading = gtk::Label::builder()
        .label("An item already exists")
        .halign(gtk::Align::Start)
        .build();
    heading.add_css_class("title-2");

    let explanation = gtk::Label::builder()
        .label(if replace_supported {
            REPLACE_CONFLICT_EXPLANATION
        } else {
            "Keep or skip the existing item, keep both with a safe name, or retry with a different filename."
        })
        .halign(gtk::Align::Start)
        .wrap(true)
        .build();
    explanation.add_css_class("floe-status");

    let source_row = adw::ActionRow::builder()
        .title("Incoming item")
        .subtitle(format!("{source_name}\n{source_description}"))
        .build();
    source_row.add_prefix(&gtk::Image::from_icon_name("floe-phosphor-file-symbolic"));
    let destination_row = adw::ActionRow::builder()
        .title("Existing destination")
        .subtitle(format!("{destination}\n{destination_description}"))
        .build();
    destination_row.add_prefix(&gtk::Image::from_icon_name("floe-phosphor-folder-symbolic"));
    let context = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .build();
    context.add_css_class("boxed-list");
    context.append(&source_row);
    context.append(&destination_row);

    let name_label = gtk::Label::builder()
        .label("Retry with a different filename")
        .halign(gtk::Align::Start)
        .build();
    let name_entry = gtk::Entry::builder()
        .placeholder_text("Enter a different filename")
        .activates_default(true)
        .hexpand(true)
        .build();
    set_accessible_label(&name_entry, "Different filename");
    let name_error = gtk::Label::builder()
        .label("Enter one filename without slashes")
        .halign(gtk::Align::Start)
        .wrap(true)
        .visible(false)
        .build();
    name_error.add_css_class("error");
    set_accessible_label(&name_error, "Filename error");
    name_entry.update_relation(&[
        gtk::accessible::Relation::LabelledBy(&[name_label.upcast_ref()]),
        gtk::accessible::Relation::DescribedBy(&[name_error.upcast_ref()]),
    ]);

    let cancel_button = gtk::Button::with_label("Cancel");
    let keep_existing_button = gtk::Button::with_label(CONFLICT_DECISION_LABELS[0]);
    let keep_both_button = gtk::Button::with_label("Keep Both");
    let skip_all_button = gtk::Button::with_label("Skip All");
    let replace_button = gtk::Button::with_label("Replace");
    replace_button.add_css_class("destructive-action");
    replace_button.set_visible(replace_supported);
    replace_button.set_tooltip_text(Some(
        "Replace this exact destination after a second confirmation; Floe privately retains the old version for Undo",
    ));
    set_accessible_label(&replace_button, "Replace this existing item");

    let replace_all_button = gtk::Button::with_label("Replace All");
    replace_all_button.add_css_class("destructive-action");
    replace_all_button.set_visible(replace_all_supported);
    replace_all_button.set_tooltip_text(Some(REPLACE_ALL_SCOPE_EXPLANATION));
    set_accessible_label(
        &replace_all_button,
        "Replace compatible conflicts in this batch",
    );
    let retry_button = gtk::Button::with_label(CONFLICT_DECISION_LABELS[1]);
    retry_button.add_css_class("suggested-action");
    retry_button.set_sensitive(false);

    let replace_hint = gtk::Label::builder()
        .label("Destructive choice — old version retained privately for Undo")
        .halign(gtk::Align::End)
        .wrap(true)
        .visible(replace_supported)
        .build();
    replace_hint.add_css_class("floe-status");

    let replace_actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .halign(gtk::Align::End)
        .spacing(8)
        .visible(replace_supported)
        .build();
    replace_actions.append(&replace_button);
    replace_actions.append(&replace_all_button);

    let safe_actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .halign(gtk::Align::End)
        .spacing(8)
        .build();
    safe_actions.append(&cancel_button);
    safe_actions.append(&keep_existing_button);
    safe_actions.append(&keep_both_button);
    safe_actions.append(&skip_all_button);
    safe_actions.append(&retry_button);

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    content.append(&heading);
    content.append(&explanation);
    content.append(&context);
    content.append(&name_label);
    content.append(&name_entry);
    content.append(&name_error);
    content.append(&replace_hint);
    content.append(&replace_actions);
    content.append(&safe_actions);

    let dialog = adw::Dialog::builder()
        .title("Resolve destination conflict")
        .content_width(520)
        .child(&content)
        .default_widget(&retry_button)
        .focus_widget(&name_entry)
        .build();

    ConflictDialogWidgets {
        dialog,
        name_entry,
        name_error,
        cancel_button,
        keep_existing_button,
        keep_both_button,
        skip_all_button,
        replace_button,
        replace_all_button,
        retry_button,
    }
}

pub fn build_operation_history_dialog(
    items: &[OperationHistoryItem],
    can_clear_completed: bool,
) -> OperationHistoryDialogWidgets {
    let heading = gtk::Label::builder()
        .label("Operation history")
        .halign(gtk::Align::Start)
        .build();
    heading.add_css_class("title-2");
    let explanation = gtk::Label::builder()
        .label(OPERATION_HISTORY_DURABILITY_EXPLANATION)
        .halign(gtk::Align::Start)
        .wrap(true)
        .build();
    explanation.add_css_class("floe-status");

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .build();
    list.add_css_class("boxed-list");
    let mut undo_buttons = Vec::with_capacity(items.len());
    let mut redo_buttons = Vec::with_capacity(items.len());
    for item in items {
        let row = adw::ActionRow::builder()
            .title(&item.title)
            .subtitle(&item.detail)
            .build();
        let undo = gtk::Button::builder()
            .label("Undo")
            .tooltip_text("Undo this move or rename")
            .valign(gtk::Align::Center)
            .visible(item.can_undo)
            .build();
        set_accessible_label(&undo, "Undo operation");
        row.add_suffix(&undo);
        let redo = gtk::Button::builder()
            .label("Redo")
            .tooltip_text("Repeat this operation without overwriting existing data")
            .valign(gtk::Align::Center)
            .visible(item.can_redo)
            .build();
        set_accessible_label(&redo, "Redo operation");
        row.add_suffix(&redo);
        list.append(&row);
        undo_buttons.push(undo);
        redo_buttons.push(redo);
    }
    if items.is_empty() {
        let empty = adw::ActionRow::builder()
            .title("No operations yet")
            .subtitle("Completed and recoverable work will appear here for this session.")
            .build();
        list.append(&empty);
    }

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .min_content_height(280)
        .child(&list)
        .build();
    let clear_completed_button = gtk::Button::with_label("Clear Completed");
    clear_completed_button.set_sensitive(can_clear_completed);
    set_accessible_label(
        &clear_completed_button,
        "Clear completed operations from session history",
    );
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    content.append(&heading);
    content.append(&explanation);
    content.append(&scroller);
    content.append(&clear_completed_button);
    let dialog = adw::Dialog::builder()
        .title("Operation History")
        .content_width(520)
        .content_height(440)
        .child(&content)
        .build();

    OperationHistoryDialogWidgets {
        dialog,
        clear_completed_button,
        undo_buttons,
        redo_buttons,
    }
}

pub fn build_recovery_dialog(items: &[RecoveryDialogItem]) -> RecoveryDialogWidgets {
    let heading = gtk::Label::builder()
        .label("Operation Recovery")
        .halign(gtk::Align::Start)
        .build();
    heading.add_css_class("title-2");
    let explanation = gtk::Label::builder()
        .label("Floe found interrupted file operations. Review current source and destination files before retrying or marking a record resolved. Floe never removes uncertain output automatically.")
        .halign(gtk::Align::Start)
        .wrap(true)
        .xalign(0.0)
        .build();
    explanation.add_css_class("floe-status");

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .build();
    list.add_css_class("boxed-list");
    let mut retry_buttons = Vec::with_capacity(items.len());
    let mut reveal_source_buttons = Vec::with_capacity(items.len());
    let mut reveal_destination_buttons = Vec::with_capacity(items.len());
    let mut resolve_buttons = Vec::with_capacity(items.len());
    for item in items {
        let row = adw::ActionRow::builder()
            .title(&item.title)
            .subtitle(&item.detail)
            .build();
        let retry = gtk::Button::builder()
            .label("Retry")
            .valign(gtk::Align::Center)
            .sensitive(item.can_retry)
            .tooltip_text(if item.can_retry {
                "Retry only because the source exists and destination is absent"
            } else {
                "Retry is unavailable while current filesystem state is uncertain"
            })
            .build();
        retry.update_property(&[gtk::accessible::Property::Label(
            "Retry interrupted operation",
        )]);
        let reveal_source = gtk::Button::builder()
            .label("Source")
            .valign(gtk::Align::Center)
            .sensitive(item.source.is_some())
            .tooltip_text("Reveal the recorded source")
            .build();
        reveal_source
            .update_property(&[gtk::accessible::Property::Label("Reveal recovery source")]);
        let reveal_destination = gtk::Button::builder()
            .label("Destination")
            .valign(gtk::Align::Center)
            .tooltip_text("Reveal the recorded destination or its containing folder")
            .build();
        reveal_destination.update_property(&[gtk::accessible::Property::Label(
            "Reveal recovery destination",
        )]);
        let resolve = gtk::Button::builder()
            .label("Mark Resolved")
            .valign(gtk::Align::Center)
            .sensitive(item.can_resolve)
            .tooltip_text("Remove only this recovery record; files are not changed")
            .build();
        resolve.update_property(&[gtk::accessible::Property::Label(
            "Mark recovery record resolved",
        )]);
        row.add_suffix(&retry);
        row.add_suffix(&reveal_source);
        row.add_suffix(&reveal_destination);
        row.add_suffix(&resolve);
        list.append(&row);
        retry_buttons.push(retry);
        reveal_source_buttons.push(reveal_source);
        reveal_destination_buttons.push(reveal_destination);
        resolve_buttons.push(resolve);
    }
    if items.is_empty() {
        list.append(
            &adw::ActionRow::builder()
                .title("No interrupted operations")
                .subtitle("The recovery journal has no records needing review.")
                .build(),
        );
    }

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .min_content_height(300)
        .child(&list)
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    content.append(&heading);
    content.append(&explanation);
    content.append(&scroller);
    let dialog = adw::Dialog::builder()
        .title("Operation Recovery")
        .content_width(760)
        .content_height(500)
        .child(&content)
        .build();
    dialog.update_property(&[gtk::accessible::Property::Label("Operation Recovery")]);
    RecoveryDialogWidgets {
        dialog,
        retry_buttons,
        reveal_source_buttons,
        reveal_destination_buttons,
        resolve_buttons,
    }
}

pub fn build_properties_dialog(
    presentation: &crate::properties::PropertiesPresentation,
) -> PropertiesDialogWidgets {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .margin_top(20)
        .margin_bottom(20)
        .margin_start(20)
        .margin_end(20)
        .build();
    let heading = gtk::Label::builder()
        .label(&presentation.title)
        .halign(gtk::Align::Start)
        .wrap(true)
        .xalign(0.0)
        .build();
    heading.add_css_class("title-2");
    content.append(&heading);
    for (title, rows) in [
        ("General", &presentation.general),
        ("Filesystem and mount", &presentation.filesystem),
        ("Permission audit", &presentation.permission_audit.summary),
    ] {
        let section = gtk::Label::builder()
            .label(title)
            .halign(gtk::Align::Start)
            .xalign(0.0)
            .build();
        section.add_css_class("heading");
        content.append(&section);
        for row in rows {
            let line = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(12)
                .build();
            let label = gtk::Label::builder()
                .label(row.label)
                .halign(gtk::Align::Start)
                .xalign(0.0)
                .width_chars(17)
                .build();
            label.add_css_class("dim-label");
            let value = gtk::Label::builder()
                .label(&row.value)
                .halign(gtk::Align::Start)
                .hexpand(true)
                .selectable(true)
                .wrap(true)
                .xalign(0.0)
                .build();
            line.append(&label);
            line.append(&value);
            content.append(&line);
        }
    }
    let association = gtk::Label::builder()
        .label("Open With")
        .halign(gtk::Align::Start)
        .xalign(0.0)
        .build();
    association.add_css_class("heading");
    content.append(&association);
    let open_with_button = gtk::Button::with_label(if presentation.open_with_available {
        "Choose Application…"
    } else {
        "Available for one regular file"
    });
    open_with_button.set_sensitive(presentation.open_with_available);
    open_with_button.set_halign(gtk::Align::Start);
    content.append(&open_with_button);

    let checksum_button = gtk::Button::with_label("Calculate SHA-256…");
    checksum_button.set_halign(gtk::Align::Start);
    checksum_button.set_sensitive(presentation.checksum_available);
    checksum_button.set_tooltip_text(Some(
        "Calculate on demand. A checksum is not proof of authenticity or safety.",
    ));
    content.append(&checksum_button);

    let privacy_heading = gtk::Label::builder()
        .label("Privacy & Safety")
        .halign(gtk::Align::Start)
        .xalign(0.0)
        .build();
    privacy_heading.add_css_class("heading");
    content.append(&privacy_heading);
    let privacy_safety_button = gtk::Button::with_label("Inspect Privacy & Safety…");
    privacy_safety_button.set_halign(gtk::Align::Start);
    content.append(&privacy_safety_button);
    let threat_scan_button = gtk::Button::with_label("Scan with Local ClamAV…");
    threat_scan_button.set_halign(gtk::Align::Start);
    threat_scan_button.set_tooltip_text(Some(
        "Requires a separately installed and running clamd service; no-signature is not proof of safety",
    ));
    content.append(&threat_scan_button);
    let sanitize_button = gtk::Button::with_label("Create Sanitized Copy…");
    sanitize_button.set_halign(gtk::Align::Start);
    sanitize_button.set_sensitive(presentation.open_with_available);
    sanitize_button.set_tooltip_text(Some(
        "Available for one supported regular JPEG, PNG, or WebP; the source remains unchanged",
    ));
    content.append(&sanitize_button);
    let permissions_heading = gtk::Label::builder()
        .label("Permissions")
        .halign(gtk::Align::Start)
        .xalign(0.0)
        .build();
    permissions_heading.add_css_class("heading");
    content.append(&permissions_heading);
    let permission_audit_button = gtk::Button::with_label("Review Permission Audit…");
    permission_audit_button.set_halign(gtk::Align::Start);
    permission_audit_button.set_tooltip_text(Some(
        "Review bounded mode, ownership, ACL, xattr, capability, immutable, and mount evidence",
    ));
    content.append(&permission_audit_button);
    let edit_permissions_button = gtk::Button::with_label(if presentation.permissions.editable {
        "Edit Permissions…"
    } else {
        "Unavailable for this selection"
    });
    edit_permissions_button.set_sensitive(presentation.permissions.editable);
    edit_permissions_button.set_halign(gtk::Align::Start);
    edit_permissions_button.set_tooltip_text(Some(
        "Change Unix modes or ownership through a bounded background job",
    ));
    content.append(&edit_permissions_button);
    let note = gtk::Label::builder()
        .label(
            "Viewing Properties is read-only. Permission changes require Edit Permissions and explicit Apply.",
        )
        .halign(gtk::Align::Start)
        .wrap(true)
        .xalign(0.0)
        .build();
    note.add_css_class("floe-status");
    content.append(&note);
    let close_button = gtk::Button::with_label("Close");
    close_button.add_css_class("suggested-action");
    close_button.set_halign(gtk::Align::End);
    content.append(&close_button);
    let scroller = gtk::ScrolledWindow::builder()
        .child(&content)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_height(420)
        .build();
    let dialog = adw::Dialog::builder()
        .title("Properties")
        .content_width(620)
        .content_height(560)
        .child(&scroller)
        .build();
    dialog.update_property(&[gtk::accessible::Property::Label("Read-only Properties")]);
    PropertiesDialogWidgets {
        dialog,
        open_with_button,
        checksum_button,
        privacy_safety_button,
        threat_scan_button,
        sanitize_button,
        permission_audit_button,
        edit_permissions_button,
        close_button,
    }
}

pub fn build_permission_audit_dialog(
    presentation: &crate::properties::PermissionAuditPresentation,
) -> PermissionAuditDialogWidgets {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .margin_top(20)
        .margin_bottom(20)
        .margin_start(20)
        .margin_end(20)
        .build();
    let heading = gtk::Label::builder()
        .label("Permission Audit")
        .halign(gtk::Align::Start)
        .xalign(0.0)
        .css_classes(["title-2"])
        .build();
    content.append(&heading);
    let explanation = gtk::Label::builder()
        .label("Floe reports local evidence, not a complete security verdict. Advanced metadata remains inspection-only.")
        .halign(gtk::Align::Start)
        .xalign(0.0)
        .wrap(true)
        .css_classes(["dim-label"])
        .build();
    content.append(&explanation);
    for row in &presentation.summary {
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let label = gtk::Label::builder()
            .label(row.label)
            .xalign(0.0)
            .width_chars(17)
            .css_classes(["dim-label"])
            .build();
        let value = gtk::Label::builder()
            .label(&row.value)
            .xalign(0.0)
            .wrap(true)
            .hexpand(true)
            .build();
        line.append(&label);
        line.append(&value);
        content.append(&line);
    }
    let details = gtk::Label::builder()
        .label(&presentation.details)
        .halign(gtk::Align::Fill)
        .xalign(0.0)
        .wrap(true)
        .selectable(true)
        .css_classes(["monospace"])
        .build();
    details.update_property(&[
        gtk::accessible::Property::Label("Permission audit evidence"),
        gtk::accessible::Property::Description(
            "Exact reviewed evidence and limitations for selected items",
        ),
    ]);
    content.append(&details);

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    controls.set_halign(gtk::Align::End);
    let fix_button = gtk::Button::with_label("Review Conservative Fix…");
    fix_button.set_sensitive(presentation.fix.is_some());
    fix_button.set_tooltip_text(Some(
        "Preview a mode-bit-only change for one item; advanced metadata is never modified",
    ));
    let close_button = gtk::Button::with_label("Close");
    close_button.add_css_class("suggested-action");
    controls.append(&fix_button);
    controls.append(&close_button);
    content.append(&controls);

    let scroller = gtk::ScrolledWindow::builder()
        .child(&content)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_height(440)
        .build();
    let dialog = adw::Dialog::builder()
        .title("Permission Audit")
        .content_width(700)
        .content_height(620)
        .child(&scroller)
        .build();
    dialog.update_property(&[
        gtk::accessible::Property::Label("Permission Audit"),
        gtk::accessible::Property::Description(
            "Review Unix permissions and advanced filesystem evidence without changing it",
        ),
    ]);
    PermissionAuditDialogWidgets {
        dialog,
        fix_button,
        close_button,
    }
}

pub fn build_permission_dialog(
    defaults: &crate::properties::PermissionDefaults,
) -> PermissionDialogWidgets {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(20)
        .margin_bottom(20)
        .margin_start(20)
        .margin_end(20)
        .build();
    let heading = gtk::Label::builder()
        .label(format!(
            "Edit permissions for {} item{}",
            defaults.targets.len(),
            if defaults.targets.len() == 1 { "" } else { "s" }
        ))
        .halign(gtk::Align::Start)
        .wrap(true)
        .xalign(0.0)
        .build();
    heading.add_css_class("title-2");
    content.append(&heading);
    let explanation = gtk::Label::builder()
        .label("Leave a field blank to keep it unchanged. Modes are octal. Local owner/group names are resolved by the background worker. Symbolic links and mount crossings are never followed.")
        .halign(gtk::Align::Start)
        .wrap(true)
        .xalign(0.0)
        .build();
    explanation.add_css_class("floe-status");
    content.append(&explanation);

    let add_entry = |label: &str, placeholder: String, sensitive: bool| {
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .build();
        let row_label = gtk::Label::builder()
            .label(label)
            .halign(gtk::Align::Start)
            .xalign(0.0)
            .width_chars(18)
            .build();
        let entry = gtk::Entry::builder()
            .placeholder_text(placeholder)
            .hexpand(true)
            .sensitive(sensitive)
            .build();
        entry.update_property(&[gtk::accessible::Property::Label(label)]);
        row.append(&row_label);
        row.append(&entry);
        content.append(&row);
        entry
    };
    let file_mode_entry = add_entry(
        "File mode",
        defaults.common_file_mode.map_or_else(
            || "No change".to_owned(),
            |mode| format!("Current {:04o}", mode),
        ),
        defaults.has_files,
    );
    let directory_mode_entry = add_entry(
        "Directory mode",
        defaults.common_directory_mode.map_or_else(
            || "No change".to_owned(),
            |mode| format!("Current {:04o}", mode),
        ),
        defaults.has_directories,
    );
    let executable_dropdown = gtk::DropDown::from_strings(&[
        "Keep executable bits",
        "Make files executable",
        "Remove executable bits",
    ]);
    executable_dropdown
        .update_property(&[gtk::accessible::Property::Label("Executable file bits")]);
    content.append(&executable_dropdown);
    let owner_entry = add_entry(
        "Owner",
        defaults.common_uid.map_or_else(
            || "UID or local name".to_owned(),
            |uid| format!("Current UID {uid}"),
        ),
        true,
    );
    let group_entry = add_entry(
        "Group",
        defaults.common_gid.map_or_else(
            || "GID or local name".to_owned(),
            |gid| format!("Current GID {gid}"),
        ),
        true,
    );
    let recursive_check = gtk::CheckButton::with_label(
        "Apply recursively to selected folders (bounded, no mount crossings)",
    );
    recursive_check.set_sensitive(defaults.has_directories);
    content.append(&recursive_check);
    let acknowledge_check = gtk::CheckButton::with_label(
        "I understand recursive or ownership changes can partially commit before an error",
    );
    content.append(&acknowledge_check);
    let error_label = gtk::Label::builder()
        .halign(gtk::Align::Start)
        .wrap(true)
        .xalign(0.0)
        .build();
    error_label.add_css_class("error");
    error_label.update_property(&[gtk::accessible::Property::Label(
        "Permission validation message",
    )]);
    content.append(&error_label);
    let actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::End)
        .build();
    let cancel_button = gtk::Button::with_label("Cancel");
    let apply_button = gtk::Button::with_label("Apply");
    apply_button.add_css_class("destructive-action");
    actions.append(&cancel_button);
    actions.append(&apply_button);
    content.append(&actions);
    let dialog = adw::Dialog::builder()
        .title("Edit Permissions")
        .content_width(600)
        .content_height(620)
        .child(&content)
        .build();
    dialog.update_property(&[gtk::accessible::Property::Label("Edit Permissions")]);
    PermissionDialogWidgets {
        dialog,
        file_mode_entry,
        directory_mode_entry,
        executable_dropdown,
        owner_entry,
        group_entry,
        recursive_check,
        acknowledge_check,
        error_label,
        cancel_button,
        apply_button,
    }
}

pub fn build_checksum_dialog(target_count: usize) -> ChecksumDialogWidgets {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(20)
        .margin_bottom(20)
        .margin_start(20)
        .margin_end(20)
        .build();
    let heading = gtk::Label::builder()
        .label(format!(
            "Calculate checksum{} for {target_count} file{}",
            if target_count == 1 { "" } else { "s" },
            if target_count == 1 { "" } else { "s" }
        ))
        .halign(gtk::Align::Start)
        .wrap(true)
        .xalign(0.0)
        .build();
    heading.add_css_class("title-2");
    content.append(&heading);
    let algorithm_label = gtk::Label::builder()
        .label("Algorithm")
        .halign(gtk::Align::Start)
        .xalign(0.0)
        .build();
    content.append(&algorithm_label);
    let algorithm_dropdown =
        gtk::DropDown::from_strings(&["SHA-256", "SHA-512", "MD5 (legacy compatibility only)"]);
    algorithm_dropdown.update_property(&[gtk::accessible::Property::Label("Checksum algorithm")]);
    content.append(&algorithm_dropdown);
    let expected_entry = gtk::Entry::builder()
        .placeholder_text(if target_count == 1 {
            "Optional expected hexadecimal digest"
        } else {
            "Expected digest is available for one file"
        })
        .sensitive(target_count == 1)
        .build();
    expected_entry.update_property(&[gtk::accessible::Property::Label("Expected checksum")]);
    content.append(&expected_entry);
    let warning = gtk::Label::builder()
        .label("MD5 is offered only for legacy compatibility. A matching checksum compares bytes; it does not prove authenticity, authorship, freshness, or safety.")
        .halign(gtk::Align::Start)
        .wrap(true)
        .xalign(0.0)
        .build();
    warning.add_css_class("floe-status");
    content.append(&warning);
    let error_label = gtk::Label::builder()
        .halign(gtk::Align::Start)
        .wrap(true)
        .xalign(0.0)
        .build();
    error_label.add_css_class("error");
    error_label.update_property(&[gtk::accessible::Property::Label(
        "Checksum validation message",
    )]);
    content.append(&error_label);
    let actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::End)
        .build();
    let cancel_button = gtk::Button::with_label("Cancel");
    let calculate_button = gtk::Button::with_label("Calculate");
    calculate_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&calculate_button);
    content.append(&actions);
    let dialog = adw::Dialog::builder()
        .title("Calculate Checksums")
        .content_width(560)
        .content_height(430)
        .child(&content)
        .build();
    dialog.update_property(&[gtk::accessible::Property::Label("Calculate Checksums")]);
    ChecksumDialogWidgets {
        dialog,
        algorithm_dropdown,
        expected_entry,
        error_label,
        cancel_button,
        calculate_button,
    }
}

pub fn build_checksum_results_dialog(
    presentation: &crate::checksum_ui::ChecksumPresentation,
) -> ChecksumResultsDialogWidgets {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(20)
        .margin_bottom(20)
        .margin_start(20)
        .margin_end(20)
        .build();
    let heading = gtk::Label::builder()
        .label(&presentation.title)
        .halign(gtk::Align::Start)
        .xalign(0.0)
        .build();
    heading.add_css_class("title-2");
    content.append(&heading);
    let algorithm = gtk::Label::builder()
        .label(presentation.algorithm_label)
        .halign(gtk::Align::Start)
        .xalign(0.0)
        .build();
    algorithm.add_css_class("heading");
    content.append(&algorithm);
    for row in &presentation.rows {
        let card = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();
        let name = gtk::Label::builder()
            .label(&row.display_name)
            .halign(gtk::Align::Start)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .xalign(0.0)
            .build();
        let digest = gtk::Label::builder()
            .label(&row.digest)
            .halign(gtk::Align::Start)
            .selectable(true)
            .wrap(true)
            .xalign(0.0)
            .build();
        digest.add_css_class("monospace");
        let verification = gtk::Label::builder()
            .label(&row.verification)
            .halign(gtk::Align::Start)
            .wrap(true)
            .xalign(0.0)
            .build();
        verification.add_css_class("floe-status");
        card.append(&name);
        card.append(&digest);
        card.append(&verification);
        content.append(&card);
    }
    let notice = gtk::Label::builder()
        .label(presentation.notice)
        .halign(gtk::Align::Start)
        .wrap(true)
        .xalign(0.0)
        .build();
    notice.add_css_class("floe-status");
    content.append(&notice);
    let actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::End)
        .build();
    let copy_button = gtk::Button::with_label("Copy Digest Text");
    let close_button = gtk::Button::with_label("Close");
    close_button.add_css_class("suggested-action");
    actions.append(&copy_button);
    actions.append(&close_button);
    content.append(&actions);
    let scroller = gtk::ScrolledWindow::builder()
        .child(&content)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_height(360)
        .build();
    let dialog = adw::Dialog::builder()
        .title("Checksum Results")
        .content_width(680)
        .content_height(520)
        .child(&scroller)
        .build();
    dialog.update_property(&[gtk::accessible::Property::Label("Checksum Results")]);
    ChecksumResultsDialogWidgets {
        dialog,
        copy_button,
        close_button,
    }
}

pub fn build_verified_copy_result_dialog(
    presentation: &crate::verified_copy_executor::VerifiedCopyPresentation,
) -> VerifiedCopyResultDialogWidgets {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(20)
        .margin_bottom(20)
        .margin_start(20)
        .margin_end(20)
        .build();
    let heading = gtk::Label::builder()
        .label(&presentation.title)
        .halign(gtk::Align::Start)
        .xalign(0.0)
        .build();
    heading.add_css_class("title-2");
    content.append(&heading);
    let detail = gtk::Label::builder()
        .label(&presentation.detail)
        .wrap(true)
        .selectable(true)
        .halign(gtk::Align::Fill)
        .xalign(0.0)
        .build();
    content.append(&detail);
    let notice = gtk::Label::builder()
        .label(presentation.notice)
        .wrap(true)
        .halign(gtk::Align::Fill)
        .xalign(0.0)
        .build();
    notice.add_css_class("floe-status");
    content.append(&notice);
    let actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::End)
        .build();
    let retry_button = gtk::Button::with_label("Retry Copy and Verify");
    retry_button.set_visible(presentation.retry_enabled);
    retry_button.update_property(&[gtk::accessible::Property::Label("Retry Copy and Verify")]);
    let close_button = gtk::Button::with_label("Close");
    close_button.add_css_class("suggested-action");
    actions.append(&retry_button);
    actions.append(&close_button);
    content.append(&actions);
    let dialog = adw::Dialog::builder()
        .title("Copy and Verify Result")
        .content_width(620)
        .content_height(360)
        .child(&content)
        .build();
    dialog.update_property(&[gtk::accessible::Property::Label("Copy and Verify Result")]);
    VerifiedCopyResultDialogWidgets {
        dialog,
        retry_button,
        close_button,
    }
}

pub fn build_open_with_dialog(file_name: &str, options: &OpenWithOptions) -> OpenWithDialogWidgets {
    let heading = gtk::Label::builder()
        .label(format!("Open {file_name} with"))
        .halign(gtk::Align::Start)
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .build();
    heading.add_css_class("title-2");

    let current_default = options
        .applications
        .iter()
        .find(|application| application.is_default)
        .map_or_else(
            || "No current default application".to_owned(),
            |application| format!("Current default: {}", application.display_name),
        );
    let default_label = gtk::Label::builder()
        .label(current_default)
        .halign(gtk::Align::Start)
        .wrap(true)
        .build();
    default_label.add_css_class("floe-status");

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .activate_on_single_click(false)
        .build();
    list.add_css_class("boxed-list");
    let mut default_row = None;
    for (index, application) in options.applications.iter().enumerate() {
        let row = adw::ActionRow::builder()
            .title(&application.display_name)
            .subtitle(if application.is_default {
                "Current default"
            } else {
                ""
            })
            .activatable(true)
            .build();
        if let Some(icon) = application.app_info.icon() {
            row.add_prefix(&gtk::Image::from_gicon(&icon));
        }
        list.append(&row);
        let list_row = row.upcast::<gtk::ListBoxRow>();
        if application.is_default {
            default_row = Some(list_row.clone());
        }
        if index == 0 && default_row.is_none() {
            default_row = Some(list_row.clone());
        }
    }
    if let Some(row) = default_row.as_ref() {
        list.select_row(Some(row));
    }

    let scroller = gtk::ScrolledWindow::builder()
        .child(&list)
        .min_content_height(220)
        .max_content_height(420)
        .propagate_natural_height(true)
        .vexpand(true)
        .build();
    let cancel_button = gtk::Button::with_label("Cancel");
    let reset_default_button = gtk::Button::with_label("Reset Default");
    reset_default_button.set_sensitive(
        options
            .applications
            .iter()
            .any(|application| application.is_default),
    );
    reset_default_button.update_property(&[
        gtk::accessible::Property::Label("Reset default application"),
        gtk::accessible::Property::Description(
            "Clear the explicit desktop default for this file type",
        ),
    ]);
    let set_default_button = gtk::Button::with_label("Set as Default");
    let open_button = gtk::Button::with_label("Open");
    open_button.add_css_class("suggested-action");
    let actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::End)
        .build();
    actions.append(&cancel_button);
    actions.append(&reset_default_button);
    actions.append(&set_default_button);
    actions.append(&open_button);

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    content.append(&heading);
    content.append(&default_label);
    content.append(&scroller);
    content.append(&actions);

    let dialog = adw::Dialog::builder()
        .title("Open With")
        .content_width(480)
        .content_height(420)
        .child(&content)
        .default_widget(&open_button)
        .focus_widget(&list)
        .build();

    OpenWithDialogWidgets {
        dialog,
        default_label,
        list,
        cancel_button,
        set_default_button,
        reset_default_button,
        open_button,
    }
}

fn build_operations_island() -> OperationWidgets {
    debug_assert!(OperationIslandLayout::CURRENT.child_minimums_fit());
    debug_assert_eq!(
        OPERATION_ISLAND_STRUCTURE,
        [
            OperationIslandRow::TitleAndCancel,
            OperationIslandRow::Detail,
            OperationIslandRow::Progress,
            OperationIslandRow::RecoveryActions,
        ]
    );
    let operation_label = gtk::Label::builder()
        .label("Working on item")
        .halign(gtk::Align::Start)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .single_line_mode(true)
        .build();
    operation_label.add_css_class("heading");

    let operation_detail = gtk::Label::builder()
        .label("Preparing…")
        .halign(gtk::Align::Start)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .single_line_mode(true)
        .build();
    operation_detail.add_css_class("floe-status");

    let operation_progress = gtk::ProgressBar::builder().hexpand(true).build();
    set_accessible_label(&operation_progress, "File operation progress");

    let operation_cancel = gtk::Button::builder()
        .icon_name("floe-phosphor-stop-symbolic")
        .tooltip_text("Cancel file operation")
        .has_frame(false)
        .build();
    operation_cancel.add_css_class("operation-icon-action");
    set_accessible_label(&operation_cancel, "Cancel file operation");

    let operation_retry = gtk::Button::builder()
        .label("Retry")
        .tooltip_text("Retry file operation")
        .visible(false)
        .build();
    operation_retry.add_css_class("operation-text-action");
    let operation_pause = gtk::Button::builder()
        .label("Pause after current")
        .tooltip_text("Pause this batch after the current item finishes")
        .visible(false)
        .build();
    operation_pause.add_css_class("operation-text-action");
    set_accessible_label(&operation_pause, "Pause batch after current item");
    let operation_history = gtk::Button::builder()
        .icon_name("floe-phosphor-clock-counter-clockwise-symbolic")
        .tooltip_text("Operation history")
        .has_frame(false)
        .build();
    operation_history.add_css_class("operation-icon-action");
    set_accessible_label(&operation_history, "Open operation history");

    let title_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .build();
    title_row.append(&operation_label);
    title_row.append(&operation_history);
    title_row.append(&operation_cancel);

    let action_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .halign(gtk::Align::End)
        .spacing(8)
        .build();
    action_row.append(&operation_pause);
    action_row.append(&operation_retry);

    let island = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .margin_top(OPERATION_ISLAND_INSET)
        .margin_bottom(OPERATION_ISLAND_INSET)
        .margin_start(OPERATION_ISLAND_INSET)
        .margin_end(OPERATION_ISLAND_INSET)
        .width_request(OPERATION_ISLAND_WIDTH)
        .build();
    island.add_css_class("operations-island");
    island.append(&title_row);
    island.append(&operation_detail);
    island.append(&operation_progress);
    island.append(&action_row);

    let revealer = gtk::Revealer::builder()
        .halign(gtk::Align::End)
        .valign(gtk::Align::End)
        .transition_type(gtk::RevealerTransitionType::Crossfade)
        .transition_duration(160)
        .reveal_child(false)
        .child(&island)
        .build();

    OperationWidgets {
        revealer,
        operation_label,
        operation_detail,
        operation_progress,
        operation_retry,
        operation_pause,
        operation_history,
        operation_cancel,
    }
}

fn build_sidebar(
    locations: &[Location],
    minimum_width: i32,
    density: SidebarDensity,
) -> SidebarWidgets {
    let metrics = sidebar_density_metrics(density);
    let sidebar = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(metrics.section_gap)
        .margin_top(metrics.outer_margin)
        .margin_bottom(metrics.outer_margin)
        .margin_start(metrics.outer_margin)
        .margin_end(metrics.outer_margin)
        .vexpand(true)
        .build();
    sidebar.add_css_class("floe-sidebar");
    sidebar.add_css_class(sidebar_density_class(density));

    sidebar.append(&sidebar_heading("Places"));
    let places_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(metrics.row_gap)
        .build();

    let mut buttons = Vec::with_capacity(locations.len());
    for location in locations {
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .build();
        content.append(&gtk::Image::from_icon_name(location.icon_name));
        let label = gtk::Label::builder()
            .label(location.label)
            .halign(gtk::Align::Start)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        content.append(&label);

        let button = gtk::Button::builder()
            .child(&content)
            .has_frame(false)
            .build();
        set_accessible_label(&button, location.label);
        places_box.append(&button);
        buttons.push(button);
    }
    let trash_content = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .build();
    trash_content.append(&gtk::Image::from_icon_name("floe-phosphor-trash-symbolic"));
    trash_content.append(
        &gtk::Label::builder()
            .label("Trash")
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build(),
    );
    let trash_button = gtk::Button::builder()
        .child(&trash_content)
        .has_frame(false)
        .action_name("win.open-trash")
        .build();
    set_accessible_label(&trash_button, "Trash");
    places_box.append(&trash_button);
    sidebar.append(&places_box);

    let bookmark_heading = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .build();
    bookmark_heading.append(&sidebar_heading("Bookmarks"));
    let add_bookmark_button = gtk::Button::builder()
        .icon_name("floe-phosphor-plus-symbolic")
        .has_frame(false)
        .tooltip_text("Add current folder to Bookmarks")
        .halign(gtk::Align::End)
        .build();
    add_bookmark_button.add_css_class("sidebar-icon-button");
    set_accessible_label(&add_bookmark_button, "Add current folder to Bookmarks");
    bookmark_heading.append(&add_bookmark_button);
    sidebar.append(&bookmark_heading);

    let bookmarks_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .build();
    sidebar.append(&bookmarks_box);

    sidebar.append(&sidebar_heading("Devices"));
    let devices_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .build();
    sidebar.append(&devices_box);

    let mode = gtk::Label::builder()
        .label("Local files · Generic Wayland")
        .halign(gtk::Align::Start)
        .margin_top(8)
        .wrap(true)
        .build();
    mode.add_css_class("floe-status");
    sidebar.append(&mode);

    let content = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .propagate_natural_height(true)
        .propagate_natural_width(false)
        .width_request(minimum_width.max(SIDEBAR_COMPACT_MIN_WIDTH))
        .vexpand(true)
        .child(&sidebar)
        .build();
    content.add_css_class("floe-panel");

    SidebarWidgets {
        content,
        sidebar,
        location_buttons: buttons,
        bookmarks_box,
        add_bookmark_button,
        devices_box,
        trash_button,
    }
}

fn sidebar_heading(label: &str) -> gtk::Label {
    let heading = gtk::Label::builder()
        .label(label)
        .halign(gtk::Align::Start)
        .hexpand(true)
        .margin_start(2)
        .margin_top(4)
        .margin_bottom(2)
        .build();
    heading.add_css_class("heading");
    heading
}

fn initialize_group_header(group: &gtk::Button, grid: bool) {
    group.add_css_class("flat");
    group.add_css_class("heading");
    group.add_css_class("floe-group-label");
    if grid {
        group.add_css_class("floe-grid-group-label");
    }
    group.update_property(&[gtk::accessible::Property::Description(
        crate::completeness::GROUP_HEADER_DESCRIPTION,
    )]);
    let group_for_toggle = group.clone();
    group.connect_clicked(move |_| {
        let Some(label) = group_for_toggle.tooltip_text() else {
            return;
        };
        if !label.is_empty() {
            let _ = group_for_toggle.activate_action("win.toggle-group", Some(&label.to_variant()));
        }
    });
}

fn build_directory_panel(
    preferences: ViewPreferences,
    drop_dispatcher: &DropDispatcher,
    entry_icon_style: &Rc<Cell<EntryIconStyle>>,
) -> DirectoryPanelWidgets {
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    let selection = gtk::MultiSelection::new(Some(store));

    let file_context_model = build_configured_file_context_menu_model(preferences.context_menu);
    let background_context_model =
        build_configured_background_context_menu_model(preferences.context_menu);
    let list_context_menu = gtk::PopoverMenu::from_model(Some(&file_context_model));
    list_context_menu.set_has_arrow(false);
    let grid_context_menu = gtk::PopoverMenu::from_model(Some(&file_context_model));
    grid_context_menu.set_has_arrow(false);
    let search_context_menu = gtk::PopoverMenu::from_model(Some(&file_context_model));
    search_context_menu.set_has_arrow(false);
    let search_background_menu = gtk::PopoverMenu::from_model(Some(&background_context_model));
    search_background_menu.set_has_arrow(false);
    let list_background_menu = gtk::PopoverMenu::from_model(Some(&background_context_model));
    list_background_menu.set_has_arrow(false);
    let grid_background_menu = gtk::PopoverMenu::from_model(Some(&background_context_model));
    grid_background_menu.set_has_arrow(false);

    let thumbnails = ThumbnailPresentation::new(entry_icon_style.clone());
    let metadata = MetadataPresentation::default();
    let list_layout = Rc::new(Cell::new(preferences.columns));
    let list_grouping = Rc::new(Cell::new(preferences.sort.grouping));
    let collapsed_groups = Rc::new(RefCell::new(
        preferences
            .collapsed_groups
            .iter()
            .cloned()
            .collect::<HashSet<String>>(),
    ));
    let factory = gtk::SignalListItemFactory::new();
    let row_selection = selection.clone();
    let row_context_menu = list_context_menu.clone();
    let row_drop_dispatcher = drop_dispatcher.clone();
    factory.connect_setup(move |_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .build();
        row.add_css_class("floe-list-row");
        let group = gtk::Button::builder()
            .halign(gtk::Align::Start)
            .width_request(112)
            .has_frame(false)
            .visible(false)
            .build();
        initialize_group_header(&group, false);
        let icon = gtk::Image::builder().pixel_size(LIST_ICON_EDGE).build();
        icon.set_accessible_role(gtk::AccessibleRole::Presentation);
        let name = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .single_line_mode(true)
            .build();
        name.add_css_class("floe-entry-name");
        let entry_type = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .width_chars(TYPE_COLUMN_WIDTH)
            .max_width_chars(TYPE_COLUMN_WIDTH)
            .xalign(0.0)
            .single_line_mode(true)
            .build();
        entry_type.add_css_class("floe-entry-type");
        let size = gtk::Label::builder()
            .halign(gtk::Align::End)
            .width_chars(SIZE_COLUMN_WIDTH)
            .max_width_chars(SIZE_COLUMN_WIDTH)
            .xalign(1.0)
            .single_line_mode(true)
            .build();
        size.add_css_class("floe-entry-size");
        let modified = gtk::Label::builder()
            .halign(gtk::Align::End)
            .width_chars(MODIFIED_COLUMN_WIDTH)
            .max_width_chars(MODIFIED_COLUMN_WIDTH)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .xalign(1.0)
            .single_line_mode(true)
            .build();
        modified.add_css_class("floe-entry-modified");
        let extension = list_column_label(ListColumn::Extension);
        let mime = list_column_label(ListColumn::Mime);
        let created = list_column_label(ListColumn::Created);
        let accessed = list_column_label(ListColumn::Accessed);
        let permissions = list_column_label(ListColumn::Permissions);
        let dimensions = list_column_label(ListColumn::Dimensions);
        let duration = list_column_label(ListColumn::Duration);
        let artist = list_column_label(ListColumn::Artist);
        let album = list_column_label(ListColumn::Album);
        let track = list_column_label(ListColumn::Track);
        let owner = list_column_label(ListColumn::Owner);
        let group_id = list_column_label(ListColumn::Group);
        let path = list_column_label(ListColumn::Path);
        let link_target = list_column_label(ListColumn::LinkTarget);
        row.append(&group);
        row.append(&icon);
        row.append(&name);
        row.append(&entry_type);
        row.append(&size);
        row.append(&modified);
        row.append(&extension);
        row.append(&mime);
        row.append(&created);
        row.append(&accessed);
        row.append(&permissions);
        row.append(&dimensions);
        row.append(&duration);
        row.append(&artist);
        row.append(&album);
        row.append(&track);
        row.append(&owner);
        row.append(&group_id);
        row.append(&path);
        row.append(&link_target);

        let secondary_click = gtk::GestureClick::new();
        secondary_click.set_button(gtk::gdk::BUTTON_SECONDARY);
        let list_item_weak = list_item.downgrade();
        let selection = row_selection.clone();
        let context_menu = row_context_menu.clone();
        secondary_click.connect_pressed(move |gesture, _, x, y| {
            let Some(list_item) = list_item_weak.upgrade() else {
                return;
            };
            let position = list_item.position();
            if !is_bound_list_position(position) {
                return;
            }

            if context_selection_for_secondary(selection.is_selected(position))
                == ContextSelection::SelectOnly
            {
                selection.select_item(position, true);
            }
            let Some(row) = gesture.widget() else {
                return;
            };
            let parent = gtk::prelude::WidgetExt::parent(&context_menu);
            let Some(parent) = parent else {
                return;
            };
            let Some(point) =
                row.compute_point(&parent, &gtk::graphene::Point::new(x as f32, y as f32))
            else {
                return;
            };
            let pointing_to =
                gtk::gdk::Rectangle::new(point.x().round() as i32, point.y().round() as i32, 1, 1);
            context_menu.set_pointing_to(Some(&pointing_to));
            context_menu.popup();
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        row.add_controller(secondary_click);
        let middle_click = gtk::GestureClick::new();
        middle_click.set_button(2);
        let middle_item = list_item.downgrade();
        let middle_selection = row_selection.clone();
        middle_click.connect_released(move |gesture, _, _, _| {
            let Some(item) = middle_item.upgrade() else {
                return;
            };
            let position = item.position();
            if !is_bound_list_position(position) {
                return;
            }
            middle_selection.select_item(position, true);
            if let Some(widget) = gesture.widget() {
                let _ = widget.activate_action("win.open-background-tab", None);
            }
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        row.add_controller(middle_click);
        let destination_item = list_item.downgrade();
        let destination = Rc::new(move || {
            let item = destination_item.upgrade()?;
            let object = item.item()?.downcast::<glib::BoxedAnyObject>().ok()?;
            let entry = object.borrow::<std::sync::Arc<DirectoryEntry>>();
            entry
                .is_navigable_directory()
                .then(|| DropDestination::Directory(entry.path().to_path_buf()))
        });
        install_drop_target(&row, destination, row_drop_dispatcher.clone(), true, false);
        list_item.set_child(Some(&row));
    });
    let thumbnails_for_bind = thumbnails.clone();
    let metadata_for_bind = metadata.clone();
    let layout_for_bind = Rc::clone(&list_layout);
    let grouping_for_bind = Rc::clone(&list_grouping);
    let collapsed_for_bind = Rc::clone(&collapsed_groups);
    let selection_for_bind = selection.clone();
    factory.connect_bind(move |_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(row) = list_item.child().and_downcast::<gtk::Box>() else {
            return;
        };
        let Some(group) = row.first_child().and_downcast::<gtk::Button>() else {
            return;
        };
        let Some(icon) = group.next_sibling().and_downcast::<gtk::Image>() else {
            return;
        };
        let Some(name) = icon.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(entry_type) = name.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(size) = entry_type.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(modified) = size.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(extension) = modified.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(mime) = extension.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(created) = mime.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(accessed) = created.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(permissions) = accessed.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(dimensions) = permissions.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(duration) = dimensions.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(artist) = duration.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(album) = artist.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(track) = album.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(owner) = track.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(group_id) = owner.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(path) = group_id.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(link_target) = path.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(object) = list_item.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let entry = object.borrow::<std::sync::Arc<DirectoryEntry>>();
        let grouping = grouping_for_bind.get();
        let previous = list_item
            .position()
            .checked_sub(1)
            .and_then(|position| selection_for_bind.item(position))
            .and_downcast::<glib::BoxedAnyObject>()
            .map(|object| object.borrow::<std::sync::Arc<DirectoryEntry>>().clone());
        let group_label = visible_group_label(grouping, &entry, previous.as_deref());
        let entry_group = grouping.label(&entry);
        let collapsed = entry_group
            .as_ref()
            .is_some_and(|label| collapsed_for_bind.borrow().contains(label));
        let is_group_header = group_label.is_some();
        let presentation = crate::completeness::group_row_presentation(collapsed, is_group_header);
        row.set_visible(presentation.visible);
        list_item.set_selectable(presentation.selectable);
        group.set_visible(is_group_header);
        group.set_label(
            group_label
                .as_ref()
                .map(|label| format!("{} {label}", if collapsed { "▸" } else { "▾" }))
                .as_deref()
                .unwrap_or_default(),
        );
        group.set_tooltip_text(group_label.as_deref());
        group.update_state(&[gtk::accessible::State::Expanded(Some(
            presentation.expanded,
        ))]);
        icon.set_visible(!collapsed);
        let display_name = entry.display_name_lossy();
        name.set_label(&display_name);
        let tooltip = entry
            .trash_metadata()
            .and_then(|trash| trash.original_path())
            .map_or_else(
                || display_name.clone(),
                |original| format!("{display_name}\nOriginal: {}", original.to_string_lossy()),
            );
        name.set_tooltip_text(Some(&tooltip));
        thumbnails_for_bind.request_thumbnail(&icon, &entry);
        if let Some(trash) = entry.trash_metadata() {
            let original = trash
                .original_path()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Original location unavailable".to_owned());
            entry_type.set_label(&original);
            entry_type.set_tooltip_text(Some(&original));
            name.set_tooltip_text(Some(&format!("{display_name}\nOriginal: {original}")));
        } else {
            entry_type.set_label(entry_type_label(entry.kind()));
            entry_type.set_tooltip_text(None);
        }
        size.set_label(&entry.size().map(format_size).unwrap_or_default());
        let modified_text = entry.trash_metadata().map_or_else(
            || {
                entry
                    .modified()
                    .and_then(format_modified)
                    .unwrap_or_default()
            },
            |trash| {
                trash
                    .deletion_date()
                    .map(|date| date.replace('T', " · "))
                    .unwrap_or_else(|| "Unknown".to_owned())
            },
        );
        modified.set_label(&modified_text);
        modified.set_tooltip_text((!modified_text.is_empty()).then_some(modified_text.as_str()));
        let extension_text = entry.display_name();
        let extension_text = std::path::Path::new(extension_text)
            .extension()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        extension.set_label(&extension_text);
        extension.set_tooltip_text((!extension_text.is_empty()).then_some(extension_text.as_str()));
        let path_text = entry.path().as_os_str().to_string_lossy().into_owned();
        path.set_label(&path_text);
        path.set_tooltip_text(Some(&path_text));

        let layout = layout_for_bind.get();
        let column_widgets: [(ListColumn, &gtk::Widget); 18] = [
            (ListColumn::Name, name.upcast_ref()),
            (ListColumn::Type, entry_type.upcast_ref()),
            (ListColumn::Size, size.upcast_ref()),
            (ListColumn::Modified, modified.upcast_ref()),
            (ListColumn::Extension, extension.upcast_ref()),
            (ListColumn::Mime, mime.upcast_ref()),
            (ListColumn::Created, created.upcast_ref()),
            (ListColumn::Accessed, accessed.upcast_ref()),
            (ListColumn::Permissions, permissions.upcast_ref()),
            (ListColumn::Dimensions, dimensions.upcast_ref()),
            (ListColumn::Duration, duration.upcast_ref()),
            (ListColumn::Artist, artist.upcast_ref()),
            (ListColumn::Album, album.upcast_ref()),
            (ListColumn::Track, track.upcast_ref()),
            (ListColumn::Owner, owner.upcast_ref()),
            (ListColumn::Group, group_id.upcast_ref()),
            (ListColumn::Path, path.upcast_ref()),
            (ListColumn::LinkTarget, link_target.upcast_ref()),
        ];
        let mut previous = icon.clone().upcast::<gtk::Widget>();
        for column in layout.order() {
            if let Some((_, widget)) = column_widgets
                .iter()
                .find(|(candidate, _)| *candidate == column)
            {
                row.reorder_child_after(*widget, Some(&previous));
                previous = (*widget).clone();
            }
        }
        for (column, label) in [
            (ListColumn::Name, &name),
            (ListColumn::Type, &entry_type),
            (ListColumn::Size, &size),
            (ListColumn::Modified, &modified),
            (ListColumn::Extension, &extension),
            (ListColumn::Mime, &mime),
            (ListColumn::Created, &created),
            (ListColumn::Accessed, &accessed),
            (ListColumn::Permissions, &permissions),
            (ListColumn::Dimensions, &dimensions),
            (ListColumn::Duration, &duration),
            (ListColumn::Artist, &artist),
            (ListColumn::Album, &album),
            (ListColumn::Track, &track),
            (ListColumn::Owner, &owner),
            (ListColumn::Group, &group_id),
            (ListColumn::Path, &path),
            (ListColumn::LinkTarget, &link_target),
        ] {
            label.set_visible(!collapsed && layout.is_visible(column));
            label.set_width_request(i32::from(layout.width(column)));
        }
        if layout.needs_lazy_metadata() {
            metadata_for_bind.request(
                &entry,
                MetadataLabels::new([
                    &mime,
                    &created,
                    &accessed,
                    &permissions,
                    &dimensions,
                    &duration,
                    &artist,
                    &album,
                    &track,
                    &owner,
                    &group_id,
                    &link_target,
                ]),
                layout.needs_advanced_metadata(),
            );
        } else {
            MetadataLabels::new([
                &mime,
                &created,
                &accessed,
                &permissions,
                &dimensions,
                &duration,
                &artist,
                &album,
                &track,
                &owner,
                &group_id,
                &link_target,
            ])
            .clear();
        }
    });
    let thumbnails_for_unbind = thumbnails.clone();
    factory.connect_unbind(move |_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(row) = list_item.child().and_downcast::<gtk::Box>() else {
            return;
        };
        let Some(group) = row.first_child().and_downcast::<gtk::Button>() else {
            return;
        };
        let Some(icon) = group.next_sibling().and_downcast::<gtk::Image>() else {
            return;
        };
        thumbnails_for_unbind.unbind(&icon);
    });

    let list_view = gtk::ListView::new(Some(selection.clone()), Some(factory.clone()));
    list_view.add_css_class("floe-directory-list");
    list_view.set_single_click_activate(false);
    list_view.set_vexpand(true);
    list_context_menu.set_parent(&list_view);
    list_background_menu.set_parent(&list_view);
    install_background_context_menu(
        &list_view,
        &selection,
        &list_background_menu,
        "floe-list-row",
    );

    let list_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&list_view)
        .vexpand(true)
        .build();
    let grid_factory = build_grid_factory(
        GridFactoryDependencies {
            selection: &selection,
            context_menu: &grid_context_menu,
            thumbnails: &thumbnails,
            grouping: &list_grouping,
            collapsed_groups: &collapsed_groups,
            drop_dispatcher,
        },
        preferences.grid_size,
        0,
    );
    let grid_view = gtk::GridView::new(Some(selection.clone()), Some(grid_factory.clone()));
    grid_view.add_css_class("floe-directory-grid");
    grid_view.set_single_click_activate(false);
    grid_view.set_enable_rubberband(true);
    grid_view.set_min_columns(1);
    grid_view.set_max_columns(24);
    grid_view.set_vexpand(true);
    grid_context_menu.set_parent(&grid_view);
    grid_background_menu.set_parent(&grid_view);
    install_background_context_menu(
        &grid_view,
        &selection,
        &grid_background_menu,
        "floe-grid-cell",
    );
    let grid_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&grid_view)
        .vexpand(true)
        .build();
    let grouped_grid = GroupedGridPresentation::new(
        GroupedGridDependencies {
            selection: &selection,
            primary_grid: &grid_view,
            context_menu: &grid_context_menu,
            thumbnails: &thumbnails,
            grouping: &list_grouping,
            collapsed_groups: &collapsed_groups,
            drop_dispatcher,
        },
        preferences.grid_size,
        preferences.file_density,
    );
    let grouped_grid_view = grouped_grid.widget().clone();
    let grid_presentation_stack = gtk::Stack::new();
    grid_presentation_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    grid_presentation_stack.add_named(&grid_scroller, Some("ungrouped"));
    grid_presentation_stack.add_named(&grouped_grid_view, Some("grouped"));
    grid_presentation_stack.set_visible_child_name(
        if preferences.sort.grouping == DirectoryGrouping::None {
            "ungrouped"
        } else {
            "grouped"
        },
    );
    grid_presentation_stack.set_vexpand(true);

    grid_context_menu.unparent();
    grid_context_menu.set_parent(&grid_presentation_stack);
    grid_background_menu.unparent();
    grid_background_menu.set_parent(&grid_presentation_stack);
    install_background_context_menu(
        grouped_grid.widget(),
        &selection,
        &grid_background_menu,
        "floe-grid-cell",
    );

    let search_factory =
        build_filename_search_factory(&selection, &search_context_menu, entry_icon_style);
    let search_results_view =
        gtk::ListView::new(Some(selection.clone()), Some(search_factory.clone()));
    search_results_view.add_css_class("floe-directory-list");
    search_results_view.add_css_class("floe-search-results");
    search_results_view.set_single_click_activate(false);
    search_results_view.set_vexpand(true);
    search_context_menu.set_parent(&search_results_view);
    search_background_menu.set_parent(&search_results_view);
    install_background_context_menu(
        &search_results_view,
        &selection,
        &search_background_menu,
        "floe-search-result-row",
    );
    let search_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&search_results_view)
        .vexpand(true)
        .build();
    let miller_file_context: gio::MenuModel = file_context_model.clone().upcast();
    let miller_background_context: gio::MenuModel = background_context_model.clone().upcast();
    let miller_view = MillerView::new(
        &miller_file_context,
        &miller_background_context,
        drop_dispatcher,
    );
    miller_view.set_width(preferences.miller_column_width);
    miller_view.set_detail_width(preferences.inspector_width);
    let view_stack = gtk::Stack::new();
    view_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    view_stack.add_named(&list_scroller, Some("list"));
    view_stack.add_named(&grid_presentation_stack, Some("grid"));
    view_stack.add_named(miller_view.widget(), Some("miller"));
    view_stack.add_named(&search_scroller, Some("search-results"));
    view_stack.set_visible_child_name(preferences.mode.stack_name());
    view_stack.set_vexpand(true);

    let empty_icon = gtk::Image::builder()
        .icon_name("floe-phosphor-folder-symbolic")
        .pixel_size(48)
        .build();
    empty_icon.add_css_class("dim-label");
    let empty_label = gtk::Label::new(Some("This folder is empty"));
    empty_label.add_css_class("title-4");
    let empty_state = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    empty_state.append(&empty_icon);
    empty_state.append(&empty_label);
    empty_state.set_can_target(false);
    empty_state.set_visible(false);

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&view_stack));
    overlay.add_overlay(&empty_state);
    overlay.set_vexpand(true);

    let search_mode = gtk::DropDown::from_strings(&SEARCH_SURFACE_MODES);
    search_mode.set_tooltip_text(Some(SEARCH_SURFACE_MODE_HELP[0]));
    search_mode.update_property(&[gtk::accessible::Property::Description(
        SEARCH_SURFACE_MODE_HELP[0],
    )]);
    set_accessible_label(&search_mode, "Search mode");

    let filter_entry = gtk::SearchEntry::builder()
        .placeholder_text("Filter shown items")
        .hexpand(true)
        .build();
    filter_entry.set_search_delay(120);
    filter_entry.set_tooltip_text(Some(SEARCH_SURFACE_MODE_HELP[0]));
    set_accessible_label(&filter_entry, "Search query");
    let filter_mode = gtk::DropDown::from_strings(&FOLDER_FILTER_MODES);
    let filter_mode_factory = build_folder_filter_mode_factory();
    filter_mode.set_list_factory(Some(&filter_mode_factory));
    update_folder_filter_mode_help(&filter_mode);
    filter_mode.connect_selected_notify(update_folder_filter_mode_help);
    set_accessible_label(&filter_mode, "Filename matching mode");
    let filter_feedback = gtk::Label::builder()
        .label("All items")
        .halign(gtk::Align::Start)
        .xalign(0.0)
        .wrap(true)
        .build();
    filter_feedback.set_accessible_role(gtk::AccessibleRole::Alert);
    filter_feedback.add_css_class("caption");
    filter_feedback.add_css_class("dim-label");
    let advanced_filter_toggle = gtk::ToggleButton::with_label("Filters");
    advanced_filter_toggle.set_tooltip_text(Some(
        "Show type, extension, MIME, size, date, owner, hidden, and case filters",
    ));
    set_accessible_label(&advanced_filter_toggle, "Show advanced filters");

    let advanced_type = gtk::DropDown::from_strings(&ADVANCED_TYPE_FILTERS);
    advanced_type.set_tooltip_text(Some("Limit results by filesystem entry type"));
    set_accessible_label(&advanced_type, "File type filter");
    let advanced_extension = gtk::Entry::builder()
        .placeholder_text("Extension")
        .width_chars(10)
        .max_length(64)
        .build();
    advanced_extension.set_tooltip_text(Some(
        "Match one filename extension, with or without a leading dot (for example: pdf)",
    ));
    set_accessible_label(&advanced_extension, "Filename extension filter");
    let advanced_mime = gtk::Entry::builder()
        .placeholder_text("MIME, e.g. image/*")
        .width_chars(16)
        .max_length(128)
        .build();
    advanced_mime.set_tooltip_text(Some(
        "Match an exact MIME type or a family such as image/*. MIME is guessed from the name without reading file contents.",
    ));
    set_accessible_label(&advanced_mime, "MIME type filter");
    let advanced_size = gtk::DropDown::from_strings(&ADVANCED_SIZE_FILTERS);
    advanced_size.set_tooltip_text(Some("Limit regular files by byte size"));
    set_accessible_label(&advanced_size, "File size filter");
    let advanced_date = gtk::DropDown::from_strings(&ADVANCED_DATE_FILTERS);
    advanced_date.set_tooltip_text(Some("Limit results by modification time"));
    set_accessible_label(&advanced_date, "Modified date filter");
    let advanced_owner = gtk::DropDown::from_strings(&ADVANCED_OWNER_FILTERS);
    advanced_owner.set_tooltip_text(Some("Limit results to files owned by your Unix user ID"));
    set_accessible_label(&advanced_owner, "Owner filter");
    let advanced_hidden = gtk::DropDown::from_strings(&ADVANCED_HIDDEN_FILTERS);
    advanced_hidden.set_tooltip_text(Some(
        "Use Show Hidden, include both hidden and visible items, or show hidden items only",
    ));
    set_accessible_label(&advanced_hidden, "Hidden files filter");
    let advanced_match_case = gtk::CheckButton::with_label("Match case");
    advanced_match_case.set_tooltip_text(Some(
        "Make filename, extension, and MIME text matching case-sensitive",
    ));
    let advanced_apply = gtk::Button::with_label("Apply");
    advanced_apply.set_tooltip_text(Some("Apply all advanced filters together"));
    let advanced_clear = gtk::Button::with_label("Clear filters");
    advanced_clear.set_tooltip_text(Some("Reset advanced filters but keep the search text"));

    let advanced_filter_flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .column_spacing(8)
        .row_spacing(6)
        .max_children_per_line(6)
        .min_children_per_line(1)
        .homogeneous(false)
        .build();
    advanced_filter_flow.insert(&advanced_type, -1);
    advanced_filter_flow.insert(&advanced_extension, -1);
    advanced_filter_flow.insert(&advanced_mime, -1);
    advanced_filter_flow.insert(&advanced_size, -1);
    advanced_filter_flow.insert(&advanced_date, -1);
    advanced_filter_flow.insert(&advanced_owner, -1);
    advanced_filter_flow.insert(&advanced_hidden, -1);
    advanced_filter_flow.insert(&advanced_match_case, -1);
    advanced_filter_flow.insert(&advanced_apply, -1);
    advanced_filter_flow.insert(&advanced_clear, -1);
    let advanced_filter_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .visible(false)
        .build();
    advanced_filter_box.append(&advanced_filter_flow);
    let search_close_button = icon_button(
        "floe-phosphor-x-symbolic",
        "Clear and close search (Escape)",
        "win.close-search-surface",
    );
    set_accessible_label(&search_close_button, "Clear and close search");
    let search_label = gtk::Label::builder()
        .label("Search")
        .halign(gtk::Align::Start)
        .build();
    search_label.add_css_class("heading");
    let filter_bar = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .margin_bottom(8)
        .visible(false)
        .build();
    filter_bar.add_css_class("floe-filter-bar");
    filter_bar.add_css_class("floe-search-bar");
    let search_query_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .build();
    search_query_row.append(&search_label);
    search_query_row.append(&search_mode);
    search_query_row.append(&filter_entry);
    search_query_row.append(&search_close_button);

    let search_options_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .build();
    search_options_row.append(&filter_mode);
    search_options_row.append(&advanced_filter_toggle);
    search_options_row.append(&filter_feedback);

    let search_scope = gtk::DropDown::from_strings(&["This Folder", "Include Subfolders"]);
    search_scope.set_selected(1);
    search_scope.set_tooltip_text(Some(
        "Choose whether to search only this folder or descend into subfolders without following links or crossing mounted filesystems.",
    ));
    set_accessible_label(&search_scope, "File search scope");
    search_scope.set_visible(false);
    let search_button = gtk::Button::builder()
        .label("Search")
        .action_name("win.start-filename-search")
        .visible(false)
        .build();
    set_accessible_label(&search_button, "Start file search");
    let search_stop_button = gtk::Button::builder()
        .label("Stop")
        .action_name("win.stop-filename-search")
        .sensitive(false)
        .visible(false)
        .build();
    set_accessible_label(&search_stop_button, "Stop file search");
    let saved_searches = gtk::DropDown::from_strings(&[SAVED_SEARCH_CONTROL_LABELS[0]]);
    saved_searches.set_tooltip_text(Some(
        "Run an explicitly saved search against its original folder",
    ));
    set_accessible_label(&saved_searches, "Saved searches");
    let recent_searches = gtk::DropDown::from_strings(&[SAVED_SEARCH_CONTROL_LABELS[1]]);
    recent_searches.set_tooltip_text(Some("Run a recent search kept only until Floe closes"));
    set_accessible_label(&recent_searches, "Recent searches this session");
    let search_result_order = gtk::DropDown::from_strings(&SEARCH_RESULT_ORDER_LABELS);
    search_result_order.set_tooltip_text(Some("Order search results without changing files"));
    set_accessible_label(&search_result_order, "Search result order");
    let save_search_button = gtk::Button::builder()
        .label(SAVED_SEARCH_CONTROL_LABELS[2])
        .action_name("win.save-search")
        .build();
    save_search_button.set_tooltip_text(Some("Name and save this on-disk search"));
    let delete_saved_search_button = gtk::Button::builder()
        .label(SAVED_SEARCH_CONTROL_LABELS[3])
        .action_name("win.delete-saved-search")
        .build();
    delete_saved_search_button.set_tooltip_text(Some("Delete the selected saved search"));
    let clear_recent_searches_button = gtk::Button::builder()
        .label(SAVED_SEARCH_CONTROL_LABELS[4])
        .action_name("win.clear-recent-searches")
        .build();
    let search_index_toggle = gtk::CheckButton::with_label("Use index");
    search_index_toggle.set_active(preferences.search_index_enabled);
    search_index_toggle.set_tooltip_text(Some(
        "Use a private filename/metadata index when current; live search remains the fallback",
    ));
    set_accessible_label(&search_index_toggle, "Use optional search index");
    search_index_toggle.update_property(&[gtk::accessible::Property::Description(
        SEARCH_INDEX_CAPABILITY_HELP,
    )]);
    let search_index_build_button = gtk::Button::builder()
        .label("Build index")
        .action_name("win.build-search-index")
        .build();
    search_index_build_button.set_tooltip_text(Some(
        "Build a private index for this local folder and its visible subfolders",
    ));
    set_accessible_label(
        &search_index_build_button,
        "Build search index for this folder",
    );
    let search_index_clear_button = gtk::Button::builder()
        .label("Clear index")
        .action_name("win.clear-search-index")
        .build();
    search_index_clear_button.set_tooltip_text(Some("Remove Floe's current search index cache"));
    set_accessible_label(&search_index_clear_button, "Clear search index cache");
    let search_index_help = gtk::Label::builder()
        .label(SEARCH_INDEX_CAPABILITY_HELP)
        .xalign(0.0)
        .wrap(true)
        .max_width_chars(42)
        .build();
    search_index_help.add_css_class("caption");
    search_index_help.add_css_class("dim-label");
    let search_index_popover_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();
    search_index_popover_box.append(&search_index_help);
    search_index_popover_box.append(&search_index_toggle);
    search_index_popover_box.append(&search_index_build_button);
    search_index_popover_box.append(&search_index_clear_button);
    let search_index_popover = gtk::Popover::new();
    search_index_popover.set_child(Some(&search_index_popover_box));
    let search_index_menu_button = gtk::MenuButton::builder()
        .label("Index")
        .popover(&search_index_popover)
        .build();
    search_index_menu_button.set_tooltip_text(Some(SEARCH_INDEX_CAPABILITY_HELP));
    set_accessible_label(&search_index_menu_button, "Optional search index controls");
    clear_recent_searches_button
        .set_tooltip_text(Some("Clear searches remembered only for this Floe session"));
    let search_feedback = gtk::Label::builder()
        .label("Enter a filename to search")
        .halign(gtk::Align::Start)
        .xalign(0.0)
        .wrap(true)
        .visible(false)
        .build();
    search_feedback.set_accessible_role(gtk::AccessibleRole::Status);
    search_feedback.add_css_class("caption");
    search_feedback.add_css_class("dim-label");
    search_options_row.append(&search_scope);
    search_options_row.append(&search_button);
    search_options_row.append(&search_stop_button);
    search_options_row.append(&saved_searches);
    search_options_row.append(&recent_searches);
    search_options_row.append(&search_result_order);
    search_options_row.append(&save_search_button);
    search_options_row.append(&delete_saved_search_button);
    search_options_row.append(&clear_recent_searches_button);
    search_options_row.append(&search_index_menu_button);
    search_options_row.append(&search_feedback);
    filter_bar.append(&search_query_row);
    filter_bar.append(&search_options_row);
    filter_bar.append(&advanced_filter_box);

    let spinner = gtk::Spinner::new();
    let status_label = gtk::Label::builder()
        .label("Ready")
        .halign(gtk::Align::Start)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    status_label.add_css_class("floe-status");
    let status = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_start(12)
        .margin_end(12)
        .margin_top(7)
        .margin_bottom(7)
        .build();
    status.append(&spinner);
    status.append(&status_label);

    let panel = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();
    panel.add_css_class("floe-panel");
    let (list_header, sort_headers, column_headers, group_header_spacer) =
        build_list_header(preferences.columns, preferences.sort.grouping);
    list_header.set_visible(preferences.mode == ViewMode::List);
    panel.append(&filter_bar);
    panel.append(&list_header);
    panel.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    panel.append(&overlay);
    panel.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    panel.append(&status);

    DirectoryPanelWidgets {
        content: panel,
        selection,
        list_view,
        grid_view,
        grouped_grid_view,
        miller_view,
        view_stack,
        list_header,
        list_context_menu,
        grid_context_menu,
        list_background_menu,
        grid_background_menu,
        file_context_model,
        background_context_model,
        empty_state,
        empty_label,
        search_bar: filter_bar,
        search_mode,
        filter_entry,
        filter_mode,
        filter_feedback,
        advanced_filter_toggle,
        advanced_filter_box,
        advanced_type,
        advanced_extension,
        advanced_mime,
        advanced_size,
        advanced_date,
        advanced_owner,
        advanced_hidden,
        advanced_match_case,
        advanced_apply,
        advanced_clear,
        search_scope,
        search_button,
        search_stop_button,
        saved_searches,
        recent_searches,
        search_result_order,
        save_search_button,
        delete_saved_search_button,
        clear_recent_searches_button,
        search_index_toggle,
        search_index_menu_button,
        search_feedback,
        search_results_view,
        search_context_menu,
        search_background_menu,
        spinner,
        status_label,
        sort_headers,
        column_headers,
        group_header_spacer,
        thumbnails,
        metadata,
        list_layout,
        list_grouping,
        collapsed_groups,
        list_factory: factory,
        grid_factory: RefCell::new(grid_factory),
        grid_presentation_stack,
        grouped_grid,
    }
}

fn build_filename_search_factory(
    selection: &gtk::MultiSelection,
    context_menu: &gtk::PopoverMenu,
    entry_icon_style: &Rc<Cell<EntryIconStyle>>,
) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    let row_selection = selection.clone();
    let row_context_menu = context_menu.clone();
    factory.connect_setup(move |_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .margin_start(8)
            .margin_end(8)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        row.add_css_class("floe-list-row");
        row.add_css_class("floe-search-result-row");

        let icon = gtk::Image::builder().pixel_size(LIST_ICON_EDGE).build();
        icon.set_accessible_role(gtk::AccessibleRole::Presentation);
        let labels = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .hexpand(true)
            .build();
        let name = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .single_line_mode(true)
            .build();
        name.add_css_class("floe-entry-name");
        let detail = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .single_line_mode(true)
            .visible(false)
            .build();
        detail.add_css_class("caption");
        let folder = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .single_line_mode(true)
            .build();
        folder.add_css_class("caption");
        folder.add_css_class("dim-label");
        labels.append(&name);
        labels.append(&detail);
        labels.append(&folder);
        row.append(&icon);
        row.append(&labels);

        let secondary_click = gtk::GestureClick::new();
        secondary_click.set_button(gtk::gdk::BUTTON_SECONDARY);
        let list_item_weak = list_item.downgrade();
        let selection = row_selection.clone();
        let context_menu = row_context_menu.clone();
        secondary_click.connect_pressed(move |gesture, _, _, _| {
            let Some(list_item) = list_item_weak.upgrade() else {
                return;
            };
            let position = list_item.position();
            if !is_bound_list_position(position) {
                return;
            }
            if context_selection_for_secondary(selection.is_selected(position))
                == ContextSelection::SelectOnly
            {
                selection.select_item(position, true);
            }
            context_menu.set_pointing_to(None);
            context_menu.popup();
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        row.add_controller(secondary_click);
        list_item.set_child(Some(&row));
    });
    let icon_style = entry_icon_style.clone();
    factory.connect_bind(move |_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(row) = list_item.child().and_downcast::<gtk::Box>() else {
            return;
        };
        let Some(icon) = row.first_child().and_downcast::<gtk::Image>() else {
            return;
        };
        let Some(labels) = icon.next_sibling().and_downcast::<gtk::Box>() else {
            return;
        };
        let Some(name) = labels.first_child().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(detail) = name.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(folder) = detail.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(object) = list_item.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let (entry, content_detail) =
            if let Ok(content_match) = object.try_borrow::<std::sync::Arc<ContentSearchMatch>>() {
                (
                    content_match.entry().clone(),
                    Some(format!(
                        "Line {} · {}",
                        content_match.line_number(),
                        content_match.snippet()
                    )),
                )
            } else {
                let entry = object.borrow::<std::sync::Arc<DirectoryEntry>>();
                (entry.as_ref().clone(), None)
            };
        apply_entry_icon(&icon, &entry, LIST_ICON_EDGE, icon_style.get());
        let display_name = entry.display_name_lossy();
        name.set_label(&display_name);
        name.set_tooltip_text(Some(&display_name));
        detail.set_visible(content_detail.is_some());
        detail.set_label(content_detail.as_deref().unwrap_or_default());
        detail.set_tooltip_text(content_detail.as_deref());
        let containing_folder = entry
            .path()
            .parent()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/".to_owned());
        folder.set_label(&containing_folder);
        folder.set_tooltip_text(Some(&containing_folder));
        let accessible = content_detail.map_or_else(
            || format!("{display_name}, in {containing_folder}"),
            |detail| format!("{display_name}, {detail}, in {containing_folder}"),
        );
        row.update_property(&[gtk::accessible::Property::Label(&accessible)]);
    });
    factory
}

fn build_grid_factory(
    dependencies: GridFactoryDependencies<'_>,
    grid_size: GridSize,
    position_offset: u32,
) -> gtk::SignalListItemFactory {
    let GridFactoryDependencies {
        selection,
        context_menu,
        thumbnails,
        grouping,
        collapsed_groups,
        drop_dispatcher,
    } = dependencies;
    let factory = gtk::SignalListItemFactory::new();
    let row_selection = selection.clone();
    let row_context_menu = context_menu.clone();
    let edge = grid_size.edge();
    let tile_width = grid_size.tile_width();
    let cell_drop_dispatcher = drop_dispatcher.clone();
    factory.connect_setup(move |_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let cell = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Start)
            .width_request(tile_width)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(4)
            .margin_end(4)
            .build();
        cell.add_css_class("floe-grid-cell");
        let group_slot = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .height_request(20)
            .width_request(tile_width - 16)
            .build();
        let group = gtk::Button::builder()
            .halign(gtk::Align::Fill)
            .valign(gtk::Align::Center)
            .hexpand(true)
            .has_frame(false)
            .build();
        initialize_group_header(&group, true);
        group_slot.append(&group);
        let icon = gtk::Image::builder()
            .pixel_size(grid_icon_edge(edge))
            .width_request(i32::from(edge))
            .height_request(i32::from(edge))
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();
        icon.set_accessible_role(gtk::AccessibleRole::Presentation);
        let name = gtk::Label::builder()
            .halign(gtk::Align::Fill)
            .justify(gtk::Justification::Center)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .lines(2)
            .width_request(tile_width - 16)
            .xalign(0.5)
            .build();
        name.add_css_class("floe-grid-name");
        cell.append(&group_slot);
        cell.append(&icon);
        cell.append(&name);

        let secondary_click = gtk::GestureClick::new();
        secondary_click.set_button(gtk::gdk::BUTTON_SECONDARY);
        let list_item_weak = list_item.downgrade();
        let selection = row_selection.clone();
        let context_menu = row_context_menu.clone();
        secondary_click.connect_pressed(move |gesture, _, x, y| {
            let Some(list_item) = list_item_weak.upgrade() else {
                return;
            };
            let local_position = list_item.position();
            if !is_bound_list_position(local_position) {
                return;
            }
            let position = position_offset.saturating_add(local_position);
            if context_selection_for_secondary(selection.is_selected(position))
                == ContextSelection::SelectOnly
            {
                selection.select_item(position, true);
            }
            let Some(cell) = gesture.widget() else {
                return;
            };
            let Some(parent) = gtk::prelude::WidgetExt::parent(&context_menu) else {
                return;
            };
            let Some(point) =
                cell.compute_point(&parent, &gtk::graphene::Point::new(x as f32, y as f32))
            else {
                return;
            };
            context_menu.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
                point.x().round() as i32,
                point.y().round() as i32,
                1,
                1,
            )));
            context_menu.popup();
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        cell.add_controller(secondary_click);
        let middle_click = gtk::GestureClick::new();
        middle_click.set_button(2);
        let middle_item = list_item.downgrade();
        let middle_selection = row_selection.clone();
        middle_click.connect_released(move |gesture, _, _, _| {
            let Some(item) = middle_item.upgrade() else {
                return;
            };
            let local_position = item.position();
            if !is_bound_list_position(local_position) {
                return;
            }
            let position = position_offset.saturating_add(local_position);
            middle_selection.select_item(position, true);
            if let Some(widget) = gesture.widget() {
                let _ = widget.activate_action("win.open-background-tab", None);
            }
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        cell.add_controller(middle_click);
        let destination_item = list_item.downgrade();
        let destination = Rc::new(move || {
            let item = destination_item.upgrade()?;
            let object = item.item()?.downcast::<glib::BoxedAnyObject>().ok()?;
            let entry = object.borrow::<std::sync::Arc<DirectoryEntry>>();
            entry
                .is_navigable_directory()
                .then(|| DropDestination::Directory(entry.path().to_path_buf()))
        });
        install_drop_target(
            &cell,
            destination,
            cell_drop_dispatcher.clone(),
            true,
            false,
        );
        list_item.set_child(Some(&cell));
    });

    let thumbnails_for_bind = thumbnails.clone();
    let grouping_for_bind = Rc::clone(grouping);
    let collapsed_for_bind = Rc::clone(collapsed_groups);
    let selection_for_bind = selection.clone();
    factory.connect_bind(move |_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(cell) = list_item.child().and_downcast::<gtk::Box>() else {
            return;
        };
        let Some(group_slot) = cell.first_child().and_downcast::<gtk::Box>() else {
            return;
        };
        let Some(group) = group_slot.first_child().and_downcast::<gtk::Button>() else {
            return;
        };
        let Some(icon) = group_slot.next_sibling().and_downcast::<gtk::Image>() else {
            return;
        };
        let Some(name) = icon.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(object) = list_item.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let entry = object.borrow::<std::sync::Arc<DirectoryEntry>>();
        let grouping = grouping_for_bind.get();
        let previous = position_offset
            .saturating_add(list_item.position())
            .checked_sub(1)
            .and_then(|position| selection_for_bind.item(position))
            .and_downcast::<glib::BoxedAnyObject>()
            .map(|object| object.borrow::<std::sync::Arc<DirectoryEntry>>().clone());
        let group_label = visible_group_label(grouping, &entry, previous.as_deref());
        let entry_group = grouping.label(&entry);
        let collapsed = entry_group
            .as_ref()
            .is_some_and(|label| collapsed_for_bind.borrow().contains(label));
        let is_group_header = group_label.is_some();
        let presentation = crate::completeness::group_row_presentation(collapsed, is_group_header);
        cell.set_visible(presentation.visible);
        list_item.set_selectable(presentation.selectable);
        group_slot.set_visible(is_group_header);
        group.set_visible(is_group_header);
        group.set_label(
            group_label
                .as_ref()
                .map(|label| format!("{} {label}", if collapsed { "▸" } else { "▾" }))
                .as_deref()
                .unwrap_or_default(),
        );
        group.set_tooltip_text(group_label.as_deref());
        group.update_state(&[gtk::accessible::State::Expanded(Some(
            presentation.expanded,
        ))]);
        icon.set_visible(!collapsed);
        name.set_visible(!collapsed);
        let display_name = entry.display_name_lossy();
        name.set_label(&display_name);
        let tooltip = entry
            .trash_metadata()
            .and_then(|trash| trash.original_path())
            .map_or_else(
                || display_name.clone(),
                |original| format!("{display_name}\nOriginal: {}", original.to_string_lossy()),
            );
        name.set_tooltip_text(Some(&tooltip));
        thumbnails_for_bind.request_thumbnail_at_size(&icon, &entry, edge);
    });

    let thumbnails_for_unbind = thumbnails.clone();
    factory.connect_unbind(move |_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(cell) = list_item.child().and_downcast::<gtk::Box>() else {
            return;
        };
        let Some(group_slot) = cell.first_child().and_downcast::<gtk::Box>() else {
            return;
        };
        let Some(group) = group_slot.first_child().and_downcast::<gtk::Button>() else {
            return;
        };
        let Some(icon) = group_slot.next_sibling().and_downcast::<gtk::Image>() else {
            return;
        };
        let Some(name) = icon.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        cell.set_visible(true);
        list_item.set_selectable(true);
        icon.set_visible(true);
        name.set_visible(true);
        group.set_label("");
        group.set_tooltip_text(None);
        thumbnails_for_unbind.unbind(&icon);
    });
    factory
}

fn build_folder_filter_mode_factory() -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let title = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .xalign(0.0)
            .build();
        title.add_css_class("heading");
        let summary = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .xalign(0.0)
            .wrap(true)
            .max_width_chars(44)
            .build();
        summary.add_css_class("caption");
        summary.add_css_class("dim-label");
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .margin_top(4)
            .margin_bottom(4)
            .margin_start(4)
            .margin_end(4)
            .build();
        row.append(&title);
        row.append(&summary);
        list_item.set_child(Some(&row));
    });
    factory.connect_bind(|_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(mode) = list_item.item().and_downcast::<gtk::StringObject>() else {
            return;
        };
        let Some(index) = folder_filter_mode_index(mode.string().as_str()) else {
            return;
        };
        let Some(row) = list_item.child().and_downcast::<gtk::Box>() else {
            return;
        };
        let Some(title) = row.first_child().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(summary) = title.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        title.set_label(FOLDER_FILTER_MODES[index]);
        summary.set_label(FOLDER_FILTER_MODE_SUMMARIES[index]);
        row.set_tooltip_text(Some(FOLDER_FILTER_MODE_HELP[index]));
        row.update_property(&[gtk::accessible::Property::Description(
            FOLDER_FILTER_MODE_HELP[index],
        )]);
    });
    factory.connect_unbind(|_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(row) = list_item.child().and_downcast::<gtk::Box>() else {
            return;
        };
        row.set_tooltip_text(None);
    });
    factory
}

fn update_folder_filter_mode_help(filter_mode: &gtk::DropDown) {
    let index = usize::try_from(filter_mode.selected())
        .ok()
        .filter(|index| *index < FOLDER_FILTER_MODE_HELP.len())
        .unwrap_or(0);
    filter_mode.set_tooltip_text(Some(FOLDER_FILTER_MODE_HELP[index]));
    filter_mode.update_property(&[gtk::accessible::Property::Description(
        FOLDER_FILTER_MODE_HELP[index],
    )]);
}

fn folder_filter_mode_index(mode: &str) -> Option<usize> {
    FOLDER_FILTER_MODES
        .iter()
        .position(|candidate| candidate == &mode)
}

fn build_configured_file_context_menu_model(preferences: ContextMenuPreferences) -> gio::Menu {
    let menu = gio::Menu::new();
    populate_file_context_menu_model(&menu, preferences, &[]);
    menu
}

fn populate_file_context_menu_model(
    menu: &gio::Menu,
    preferences: ContextMenuPreferences,
    custom_actions: &[CustomActionDefinition],
) {
    menu.remove_all();

    let primary = gio::Menu::new();
    primary.append(Some("Open"), Some("win.open"));
    primary.append(Some("Open With…"), Some("win.open-with"));
    if preferences.is_visible(ContextMenuGroup::Terminal) {
        primary.append(Some("Open Terminal Here"), Some("win.open-terminal"));
    }
    primary.append(Some("Open in New Tab"), Some("win.open-new-tab"));
    primary.append(
        Some("Open Folder in New Window"),
        Some("win.open-new-window"),
    );
    primary.append(
        Some("Open in New Background Tab"),
        Some("win.open-background-tab"),
    );
    primary.append(
        Some("Open as Administrator…"),
        Some("win.open-as-administrator"),
    );
    primary.append(Some("Reveal in Folder"), Some("win.reveal-in-folder"));
    menu.append_section(None, &primary);
    if !custom_actions.is_empty() {
        let tools = gio::Menu::new();
        for action in custom_actions {
            let item = gio::MenuItem::new(Some(&action.name), None);
            item.set_action_and_target_value(
                Some("win.run-custom-action"),
                Some(&action.id.to_variant()),
            );
            tools.append_item(&item);
        }
        menu.append_section(Some("Custom Actions"), &tools);
    }

    if preferences.is_visible(ContextMenuGroup::SplitView) {
        let opposite = gio::Menu::new();
        opposite.append(
            Some("Open Folder in Other Pane"),
            Some("win.open-opposite-pane"),
        );
        opposite.append(
            Some("Copy to Other Pane"),
            Some("win.copy-to-opposite-pane"),
        );
        opposite.append(
            Some("Move to Other Pane"),
            Some("win.move-to-opposite-pane"),
        );
        opposite.append(
            Some("Create Links in Other Pane"),
            Some("win.link-to-opposite-pane"),
        );
        menu.append_section(Some("Other Pane"), &opposite);
    }

    let editing = gio::Menu::new();
    for (label, action) in [
        ("Copy", "win.copy"),
        ("Copy and Verify…", "win.copy-and-verify"),
        (
            "Verified Removable Transfer…",
            "win.verified-removable-transfer",
        ),
        ("Cut", "win.cut"),
        ("Duplicate", "win.duplicate"),
        ("Rename…", "win.rename"),
    ] {
        editing.append(Some(label), Some(action));
    }
    if preferences.is_visible(ContextMenuGroup::BatchRename) {
        editing.append(Some("Batch Rename…"), Some("win.batch-rename"));
        editing.append(
            Some("Undo Last Batch Rename"),
            Some("win.undo-batch-rename"),
        );
    }
    menu.append_section(None, &editing);

    if preferences.is_visible(ContextMenuGroup::Archives) {
        let archives = gio::Menu::new();
        archives.append(Some("Extract Here"), Some("win.extract-here"));
        archives.append(Some("Extract To…"), Some("win.extract-to"));
        archives.append(Some("Compress…"), Some("win.compress"));
        let section = gio::Menu::new();
        section.append_submenu(Some("Archives"), &archives);
        menu.append_section(None, &section);
    }

    if preferences.is_visible(ContextMenuGroup::Links) {
        let links = gio::Menu::new();
        links.append(
            Some("Create Symbolic Link…"),
            Some("win.create-symbolic-link"),
        );
        links.append(Some("Create Hard Link…"), Some("win.create-hard-link"));
        links.append(Some("Reveal Link Target"), Some("win.reveal-link-target"));
        let section = gio::Menu::new();
        section.append_submenu(Some("Links"), &links);
        menu.append_section(None, &section);
    }

    if preferences.is_visible(ContextMenuGroup::CopyDetails) {
        let copy_details = gio::Menu::new();
        for (label, action) in [
            ("Copy Name", "win.copy-name"),
            ("Copy Path", "win.copy-path"),
            ("Copy Relative Path", "win.copy-relative-path"),
            ("Copy URI", "win.copy-uri"),
        ] {
            copy_details.append(Some(label), Some(action));
        }
        let section = gio::Menu::new();
        section.append_submenu(Some("Copy Details"), &copy_details);
        menu.append_section(None, &section);
    }

    if preferences.is_visible(ContextMenuGroup::Checksums) {
        let checksums = gio::Menu::new();
        checksums.append(Some("Calculate Checksums…"), Some("win.checksum"));
        checksums.append(
            Some("Save SHA-256 Fingerprint"),
            Some("win.save-sha256-fingerprint"),
        );
        checksums.append(
            Some("Verify Saved Fingerprint"),
            Some("win.verify-saved-fingerprint"),
        );
        checksums.append(Some("Generate SHA256SUMS"), Some("win.generate-sha256sums"));
        checksums.append(
            Some("Verify Selected Manifest"),
            Some("win.verify-sha256sums"),
        );
        checksums.append(
            Some("Create Integrity Baseline"),
            Some("win.create-integrity-baseline"),
        );
        checksums.append(
            Some("Update Integrity Baseline"),
            Some("win.update-integrity-baseline"),
        );
        checksums.append(
            Some("Verify Integrity Baseline"),
            Some("win.verify-integrity-baseline"),
        );
        checksums.append(
            Some("Start Integrity Monitoring"),
            Some("win.start-integrity-monitoring"),
        );
        checksums.append(
            Some("Stop Integrity Monitoring"),
            Some("win.stop-integrity-monitoring"),
        );
        checksums.append(
            Some("Delete Integrity Baseline"),
            Some("win.delete-integrity-baseline"),
        );
        checksums.append(Some("Check for Duplicates…"), Some("win.check-duplicates"));
        menu.append_section(None, &checksums);
    }

    if preferences.is_visible(ContextMenuGroup::PrivacySafety) {
        let security = gio::Menu::new();
        security.append(
            Some("Inspect Privacy & Safety…"),
            Some("win.inspect-privacy-safety"),
        );
        security.append(Some("Audit Permissions…"), Some("win.audit-permissions"));
        security.append(Some("Scan with Local ClamAV…"), Some("win.scan-threats"));
        security.append(
            Some("Create Sanitized Copy…"),
            Some("win.create-sanitized-copy"),
        );
        let section = gio::Menu::new();
        section.append_submenu(Some("Privacy & Safety"), &security);
        menu.append_section(None, &section);
    }

    let destructive = gio::Menu::new();
    destructive.append(Some("Move to Trash"), Some("win.trash"));
    destructive.append(Some("Delete Permanently…"), Some("win.permanent-delete"));
    menu.append_section(None, &destructive);

    let protection = gio::Menu::new();
    protection.append(Some("Protect Folder"), Some("win.protect-folder"));
    protection.append(Some("Unprotect Folder"), Some("win.unprotect-folder"));
    protection.append(Some("Protected Folders…"), Some("win.protected-folders"));
    let protection_section = gio::Menu::new();
    protection_section.append_submenu(Some("Protected Folder"), &protection);
    menu.append_section(None, &protection_section);

    let details = gio::Menu::new();
    details.append(Some("Properties"), Some("win.properties"));
    details.append(
        Some("Customize Context Menus…"),
        Some("win.context-menu-settings"),
    );
    menu.append_section(None, &details);
}

fn build_configured_background_context_menu_model(
    preferences: ContextMenuPreferences,
) -> gio::Menu {
    let menu = gio::Menu::new();
    populate_background_context_menu_model(&menu, preferences);
    menu
}

fn populate_background_context_menu_model(menu: &gio::Menu, preferences: ContextMenuPreferences) {
    menu.remove_all();

    let create = gio::Menu::new();
    create.append(Some("New Folder…"), Some("win.new-folder"));
    create.append(Some("New Empty File…"), Some("win.new-empty-file"));
    create.append(Some("New From Template…"), Some("win.new-from-template"));
    create.append(Some("Paste"), Some("win.paste"));
    menu.append_section(None, &create);

    let view = gio::Menu::new();
    view.append(Some("Select All"), Some("win.select-all"));
    view.append(Some("Invert Selection"), Some("win.invert-selection"));
    view.append(Some("Refresh"), Some("win.refresh"));
    view.append(Some("Edit Location"), Some("win.location"));
    if preferences.is_visible(ContextMenuGroup::Terminal) {
        view.append(Some("Open Terminal Here"), Some("win.open-terminal"));
    }
    view.append(
        Some("Open as Administrator…"),
        Some("win.open-as-administrator"),
    );
    menu.append_section(None, &view);

    if preferences.is_visible(ContextMenuGroup::Checksums) {
        let tools = gio::Menu::new();
        tools.append(Some("Check for Duplicates…"), Some("win.check-duplicates"));
        menu.append_section(None, &tools);
    }

    if preferences.is_visible(ContextMenuGroup::SplitView) {
        let split = gio::Menu::new();
        split.append(Some("Toggle Split View"), Some("win.toggle-split"));
        split.append(Some("Switch Active Pane"), Some("win.switch-split-side"));
        split.append(Some("Swap Panes"), Some("win.swap-split-sides"));
        split.append(Some("Close Split"), Some("win.close-split"));
        split.append(Some("Narrow Primary Pane"), Some("win.narrow-primary-pane"));
        split.append(Some("Widen Primary Pane"), Some("win.widen-primary-pane"));
        menu.append_section(Some("Split View"), &split);
    }

    let protection = gio::Menu::new();
    protection.append(Some("Protect Folder"), Some("win.protect-folder"));
    protection.append(Some("Unprotect Folder"), Some("win.unprotect-folder"));
    protection.append(Some("Protected Folders…"), Some("win.protected-folders"));
    menu.append_section(Some("Protected Folder"), &protection);

    let settings = gio::Menu::new();
    settings.append(
        Some("Customize Context Menus…"),
        Some("win.context-menu-settings"),
    );
    menu.append_section(None, &settings);
}

#[cfg(test)]
fn build_file_context_menu_model() -> gio::Menu {
    let menu = gio::Menu::new();

    let primary = gio::Menu::new();
    primary.append(
        Some(FILE_CONTEXT_ACTIONS[0].0),
        Some(FILE_CONTEXT_ACTIONS[0].1),
    );
    primary.append(
        Some(FILE_CONTEXT_ACTIONS[1].0),
        Some(FILE_CONTEXT_ACTIONS[1].1),
    );
    primary.append(
        Some(FILE_CONTEXT_ACTIONS[18].0),
        Some(FILE_CONTEXT_ACTIONS[18].1),
    );
    primary.append(Some("Open in New Tab"), Some("win.open-new-tab"));
    primary.append(
        Some("Open Folder in New Window"),
        Some("win.open-new-window"),
    );
    primary.append(
        Some("Open in New Background Tab"),
        Some("win.open-background-tab"),
    );
    menu.append_section(None, &primary);
    let opposite = gio::Menu::new();
    opposite.append(
        Some("Open Folder in Other Pane"),
        Some("win.open-opposite-pane"),
    );
    opposite.append(
        Some("Copy to Other Pane"),
        Some("win.copy-to-opposite-pane"),
    );
    opposite.append(
        Some("Move to Other Pane"),
        Some("win.move-to-opposite-pane"),
    );
    opposite.append(
        Some("Create Links in Other Pane"),
        Some("win.link-to-opposite-pane"),
    );
    menu.append_section(Some("Other Pane"), &opposite);

    let editing = gio::Menu::new();
    for (label, action) in &FILE_CONTEXT_ACTIONS[2..8] {
        editing.append(Some(label), Some(action));
    }
    menu.append_section(None, &editing);

    let links = gio::Menu::new();
    for (label, action) in &FILE_CONTEXT_ACTIONS[8..11] {
        links.append(Some(label), Some(action));
    }
    menu.append_section(None, &links);

    let copy_identity = gio::Menu::new();
    for (label, action) in &FILE_CONTEXT_ACTIONS[11..15] {
        copy_identity.append(Some(label), Some(action));
    }
    menu.append_section(None, &copy_identity);

    let tools = gio::Menu::new();
    tools.append(
        Some(FILE_CONTEXT_ACTIONS[15].0),
        Some(FILE_CONTEXT_ACTIONS[15].1),
    );
    tools.append(
        Some(FILE_CONTEXT_ACTIONS[16].0),
        Some(FILE_CONTEXT_ACTIONS[16].1),
    );
    menu.append_section(None, &tools);

    let destructive = gio::Menu::new();
    destructive.append(
        Some(FILE_CONTEXT_ACTIONS[17].0),
        Some(FILE_CONTEXT_ACTIONS[17].1),
    );
    destructive.append(
        Some(FILE_CONTEXT_ACTIONS[18].0),
        Some(FILE_CONTEXT_ACTIONS[18].1),
    );
    menu.append_section(None, &destructive);

    let details = gio::Menu::new();
    details.append(
        Some(FILE_CONTEXT_ACTIONS[19].0),
        Some(FILE_CONTEXT_ACTIONS[19].1),
    );
    menu.append_section(None, &details);

    menu
}

fn build_trash_context_menu_model() -> gio::Menu {
    let menu = gio::Menu::new();
    let restore = gio::Menu::new();
    restore.append(
        Some(TRASH_CONTEXT_ACTIONS[0].0),
        Some(TRASH_CONTEXT_ACTIONS[0].1),
    );
    menu.append_section(None, &restore);
    let destructive = gio::Menu::new();
    destructive.append(
        Some(TRASH_CONTEXT_ACTIONS[1].0),
        Some(TRASH_CONTEXT_ACTIONS[1].1),
    );
    menu.append_section(None, &destructive);

    let details = gio::Menu::new();
    details.append(
        Some(TRASH_CONTEXT_ACTIONS[3].0),
        Some(TRASH_CONTEXT_ACTIONS[3].1),
    );
    menu.append_section(None, &details);

    menu
}

fn build_trash_background_context_menu_model() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some("Empty Trash…"), Some("win.empty-trash"));
    menu.append(Some("Refresh"), Some("win.refresh"));
    menu
}

fn install_background_context_menu<W>(
    view: &W,
    selection: &gtk::MultiSelection,
    context_menu: &gtk::PopoverMenu,
    entry_css_class: &'static str,
) where
    W: IsA<gtk::Widget> + Clone + 'static,
{
    let secondary_click = gtk::GestureClick::new();
    secondary_click.set_button(gtk::gdk::BUTTON_SECONDARY);
    let view_widget = view.clone().upcast::<gtk::Widget>();
    let selection = selection.clone();
    let context_menu = context_menu.clone();
    secondary_click.connect_pressed(move |gesture, _, x, y| {
        if view_widget
            .pick(x, y, gtk::PickFlags::DEFAULT)
            .is_some_and(|target| {
                widget_or_ancestor_has_css_class(&target, &view_widget, entry_css_class)
            })
        {
            return;
        }

        selection.unselect_all();
        let point = gtk::prelude::WidgetExt::parent(&context_menu)
            .and_then(|parent| {
                view_widget.compute_point(&parent, &gtk::graphene::Point::new(x as f32, y as f32))
            })
            .unwrap_or_else(|| gtk::graphene::Point::new(x as f32, y as f32));
        context_menu.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
            point.x().round() as i32,
            point.y().round() as i32,
            1,
            1,
        )));
        context_menu.popup();
        gesture.set_state(gtk::EventSequenceState::Claimed);
    });
    view.add_controller(secondary_click);
}

fn widget_or_ancestor_has_css_class(
    target: &gtk::Widget,
    root: &gtk::Widget,
    css_class: &str,
) -> bool {
    let mut current = Some(target.clone());
    while let Some(widget) = current {
        if widget.has_css_class(css_class) {
            return true;
        }
        if widget == *root {
            break;
        }
        current = widget.parent();
    }
    false
}

fn is_bound_list_position(position: u32) -> bool {
    position != gtk::INVALID_LIST_POSITION
}

fn visible_group_label(
    grouping: DirectoryGrouping,
    entry: &DirectoryEntry,
    previous: Option<&DirectoryEntry>,
) -> Option<String> {
    grouping
        .starts_group(entry, previous)
        .then(|| grouping.label(entry))
        .flatten()
}

fn icon_button(icon_name: &str, tooltip: &str, action_name: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon_name)
        .tooltip_text(tooltip)
        .action_name(action_name)
        .build();
    set_accessible_label(&button, tooltip);
    button
}

fn set_accessible_label(widget: &impl IsA<gtk::Accessible>, label: &str) {
    widget.update_property(&[gtk::accessible::Property::Label(label)]);
}

fn apply_entry_icon(
    image: &gtk::Image,
    entry: &DirectoryEntry,
    pixel_size: i32,
    style: EntryIconStyle,
) {
    // `GtkImage` can retain a paintable, GIcon, or icon-name representation.
    // Clear the previous representation before a live style switch so a
    // virtualized row never keeps a stale or missing paintable.
    image.clear();
    for icon in EntryIcon::ALL {
        image.remove_css_class(icon.css_class());
    }
    let icon = icon_for_entry(entry);
    image.add_css_class("floe-entry-icon");
    image.add_css_class(icon.css_class());
    image.set_pixel_size(pixel_size);
    if style == EntryIconStyle::System {
        image.set_from_gicon(&gio::ThemedIcon::from_names(icon.system_icon_names()));
    } else {
        image.set_icon_name(Some(icon.icon_name(style)));
    }
}

fn apply_thumbnail(image: &gtk::Image, texture: &gtk::gdk::Texture, edge: u16) {
    for icon in EntryIcon::ALL {
        image.remove_css_class(icon.css_class());
    }
    image.set_pixel_size(i32::from(edge));
    image.set_paintable(Some(texture));
    image.add_css_class("floe-thumbnail");
}

fn build_list_header(
    layout: ListColumnLayout,
    grouping: DirectoryGrouping,
) -> (
    gtk::Box,
    Vec<SortHeaderWidgets>,
    Vec<ListColumnHeaderWidgets>,
    gtk::Widget,
) {
    let header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .build();
    header.add_css_class("floe-list-header");

    let group_header_spacer = gtk::Box::builder().width_request(112).build();
    group_header_spacer.set_visible(grouping != DirectoryGrouping::None);
    header.append(&group_header_spacer);
    header.append(
        &gtk::Box::builder()
            .width_request(i32::from(LIST_THUMBNAIL_EDGE))
            .build(),
    );
    let mut widgets = Vec::with_capacity(LIST_SORT_COLUMNS.len());
    for (index, ((column, label), width)) in LIST_SORT_COLUMNS
        .into_iter()
        .zip(LIST_COLUMN_LABELS)
        .zip([
            None,
            Some(TYPE_COLUMN_WIDTH),
            Some(SIZE_COLUMN_WIDTH),
            Some(MODIFIED_COLUMN_WIDTH),
            Some(TYPE_COLUMN_WIDTH),
        ])
        .enumerate()
    {
        debug_assert_eq!(column.label(), label);
        let heading = gtk::Label::builder()
            .label(label)
            .halign(if index < 2 {
                gtk::Align::Start
            } else {
                gtk::Align::End
            })
            .hexpand(index == 0)
            .xalign(if index < 2 { 0.0 } else { 1.0 })
            .single_line_mode(true)
            .build();
        if let Some(width) = width {
            heading.set_width_chars(width);
            heading.set_max_width_chars(width);
        }
        let button = gtk::Button::builder()
            .child(&heading)
            .action_name(sort_action_name(column))
            .hexpand(index == 0)
            .build();
        let list_column = match column {
            SortColumn::Name => ListColumn::Name,
            SortColumn::Type => ListColumn::Type,
            SortColumn::Size => ListColumn::Size,
            SortColumn::Modified => ListColumn::Modified,
            SortColumn::Extension => ListColumn::Extension,
            _ => unreachable!("only visible list-header columns are iterated"),
        };
        button.set_width_request(i32::from(layout.width(list_column)));
        button.set_visible(layout.is_visible(list_column));
        button.add_css_class("flat");
        button.add_css_class("floe-sort-heading");
        install_column_resize_gesture(&button, list_column);
        header.append(&button);
        widgets.push(SortHeaderWidgets {
            column,
            button,
            label: heading,
        });
    }
    let mut column_headers = widgets
        .iter()
        .map(|header| ListColumnHeaderWidgets {
            column: match header.column {
                SortColumn::Name => ListColumn::Name,
                SortColumn::Type => ListColumn::Type,
                SortColumn::Size => ListColumn::Size,
                SortColumn::Modified => ListColumn::Modified,
                SortColumn::Extension => ListColumn::Extension,
                _ => unreachable!("only visible list-header columns are retained"),
            },
            widget: header.button.clone().upcast(),
        })
        .collect::<Vec<_>>();
    for column in [
        ListColumn::Mime,
        ListColumn::Created,
        ListColumn::Accessed,
        ListColumn::Permissions,
        ListColumn::Dimensions,
        ListColumn::Duration,
        ListColumn::Artist,
        ListColumn::Album,
        ListColumn::Track,
        ListColumn::Owner,
        ListColumn::Group,
        ListColumn::Path,
        ListColumn::LinkTarget,
    ] {
        let label = gtk::Label::builder()
            .label(column.label())
            .halign(gtk::Align::Start)
            .xalign(0.0)
            .single_line_mode(true)
            .width_request(i32::from(layout.width(column)))
            .visible(layout.is_visible(column))
            .build();
        label.add_css_class("floe-metadata-heading");
        install_column_resize_gesture(&label, column);
        header.append(&label);
        column_headers.push(ListColumnHeaderWidgets {
            column,
            widget: label.upcast(),
        });
    }

    (
        header,
        widgets,
        column_headers,
        group_header_spacer.upcast(),
    )
}

fn install_column_resize_gesture(widget: &impl IsA<gtk::Widget>, column: ListColumn) {
    let gesture = gtk::GestureDrag::new();
    let widget_weak = widget.as_ref().downgrade();
    gesture.connect_drag_end(move |_, offset_x, _| {
        let Some(widget) = widget_weak.upgrade() else {
            return;
        };
        let steps = (offset_x.abs() / 16.0).round().clamp(1.0, 8.0) as usize;
        let action = column_resize_action(column, offset_x >= 0.0);
        if offset_x.abs() < 4.0 {
            return;
        }
        for _ in 0..steps {
            let _ = widget.activate_action(&action, None);
        }
    });
    widget.add_controller(gesture);
}

fn column_resize_action(column: ListColumn, wider: bool) -> String {
    let direction = if wider { "widen" } else { "narrow" };
    format!("win.{direction}-{}", column.persisted())
}

fn sort_action_name(column: SortColumn) -> &'static str {
    match column {
        SortColumn::Name => "win.sort-name",
        SortColumn::Type => "win.sort-type",
        SortColumn::Size => "win.sort-size",
        SortColumn::Modified => "win.sort-modified",
        SortColumn::Extension => "win.sort-extension",
        SortColumn::Created => "win.sort-created",
        SortColumn::Accessed => "win.sort-accessed",
        SortColumn::Rating => "win.sort-rating",
        SortColumn::Tags => "win.sort-tags",
        SortColumn::Comment => "win.sort-comment",
        _ => "win.sort-name",
    }
}

pub fn update_sort_header(header: &SortHeaderWidgets, sort: DirectorySort) {
    let active = header.column == sort.column;
    let direction = active.then_some(sort.direction);
    header
        .label
        .set_label(&sort_heading_text(header.column, direction));

    let next_direction = direction
        .map(SortDirection::reversed)
        .unwrap_or(SortDirection::Ascending);
    let action = format!("Sort {} {}", header.column.label(), next_direction.label());
    let accessible = direction.map_or_else(
        || action.clone(),
        |direction| {
            format!(
                "Sorted by {}, {}. {action}",
                header.column.label(),
                direction.label()
            )
        },
    );
    header.button.set_tooltip_text(Some(&accessible));
    set_accessible_label(&header.button, &accessible);
    header
        .button
        .update_state(&[gtk::accessible::State::Pressed(if active {
            gtk::AccessibleTristate::True
        } else {
            gtk::AccessibleTristate::False
        })]);
    if active {
        header.button.add_css_class("active-sort");
    } else {
        header.button.remove_css_class("active-sort");
    }
}

fn sort_heading_text(column: SortColumn, direction: Option<SortDirection>) -> String {
    match direction {
        Some(SortDirection::Ascending) => format!("{} ↑", column.label()),
        Some(SortDirection::Descending) => format!("{} ↓", column.label()),
        None => column.label().to_owned(),
    }
}

fn entry_type_label(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Directory => "Folder",
        EntryKind::RegularFile => "File",
        EntryKind::SymbolicLink {
            target_is_directory: true,
        } => "Folder link",
        EntryKind::SymbolicLink {
            target_is_directory: false,
        } => "File link",
        EntryKind::Other => "Special",
    }
}

fn list_column_label(column: ListColumn) -> gtk::Label {
    let label = gtk::Label::builder()
        .halign(if matches!(column, ListColumn::Size) {
            gtk::Align::End
        } else {
            gtk::Align::Start
        })
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .single_line_mode(true)
        .xalign(if matches!(column, ListColumn::Size) {
            1.0
        } else {
            0.0
        })
        .build();
    label.add_css_class("floe-metadata-column");
    if matches!(
        column,
        ListColumn::Size | ListColumn::Modified | ListColumn::Created | ListColumn::Accessed
    ) {
        label.add_css_class("numeric");
    }
    label
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 7] = ["B", "KB", "MB", "GB", "TB", "PB", "EB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_modified(modified: SystemTime) -> Option<String> {
    let seconds = match modified.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).ok()?,
        Err(error) => i64::try_from(error.duration().as_secs())
            .ok()?
            .checked_neg()?,
    };
    let local = glib::DateTime::from_unix_local(seconds).ok()?;
    local
        .format("%x · %R")
        .ok()
        .map(|formatted| formatted.to_string())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, time::Duration};

    use super::*;

    #[test]
    fn phase_6u_ui_copy_explains_identity_undo_and_batch_scope() {
        assert!(REPLACE_CONFLICT_EXPLANATION.contains("identity-checks"));
        assert!(REPLACE_CONFLICT_EXPLANATION.contains("privately"));
        assert!(REPLACE_CONFLICT_EXPLANATION.contains("Undo"));
        assert!(REPLACE_ALL_SCOPE_EXPLANATION.contains("only"));
        assert!(REPLACE_ALL_SCOPE_EXPLANATION.contains("this batch"));
        assert!(REPLACE_ALL_SCOPE_EXPLANATION.contains("fresh identities"));
    }

    #[test]
    #[ignore = "requires graphical GTK session; run documented GTK component gate"]
    fn phase_6u_ui_gtk_conflict_actions_are_distinct_and_accessible() {
        gtk::init().expect("GTK component gate requires available display");
        adw::init().expect("libadwaita must initialize in GTK component gate");
        let widgets = build_conflict_dialog(
            "/tmp/incoming.txt",
            "/tmp/existing.txt",
            "File • 12 B • modified now",
            "File • 10 B • modified earlier",
            true,
            true,
        );
        assert!(widgets.replace_button.is_visible());
        assert!(widgets.replace_all_button.is_visible());
        assert!(widgets.replace_button.has_css_class("destructive-action"));
        assert!(
            widgets
                .replace_all_button
                .has_css_class("destructive-action")
        );
        assert_eq!(
            widgets.replace_button.accessible_role(),
            gtk::AccessibleRole::Button
        );
        assert_eq!(
            widgets.replace_all_button.accessible_role(),
            gtk::AccessibleRole::Button
        );
        assert!(widgets.keep_existing_button.is_visible());
        assert!(widgets.keep_both_button.is_visible());
        assert!(widgets.retry_button.is_visible());
    }

    #[cfg(any())]
    #[test]
    #[ignore = "requires graphical GTK session; run documented GTK component gate"]
    fn phase_6v_full_widget_selection_scroll_and_emphasis_contract() {
        gtk::init().expect("GTK component gate requires an available display");
        adw::init().expect("libadwaita must initialize in GTK component gate");
        let application = adw::Application::builder()
            .application_id("io.github.rodriguezcappsec.Floe.Phase6VComponentTest")
            .build();
        application
            .register(None::<&gio::Cancellable>)
            .expect("component-test application must register");
        let fixture = tempfile::tempdir().expect("temporary directory");
        fs::write(fixture.path().join("result.txt"), b"result").expect("result fixture");
        let entries = floe_core::enumerate_directory(fixture.path())
            .expect("enumerate fixture")
            .entries()
            .to_vec();
        let widgets = build(
            &application,
            &[],
            Appearance::for_preset(AppearancePreset::Native),
            ViewPreferences::default(),
        );
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        for entry in entries {
            store.append(&glib::BoxedAnyObject::new(std::sync::Arc::new(entry)));
        }
        widgets.selection.set_model(Some(&store));
        widgets.window.present();
        widgets.location_entry.grab_focus();
        for _ in 0..8 {
            if !glib::MainContext::default().pending() {
                break;
            }
            glib::MainContext::default().iteration(false);
        }
        let focus_before = gtk::prelude::GtkWindowExt::focus(&widgets.window);

        widgets.selection.select_item(0, true);
        assert!(widgets.selection.is_selected(0));
        assert!(widgets.scroll_to_operation_result(ViewMode::List, 0));
        assert_eq!(
            gtk::prelude::GtkWindowExt::focus(&widgets.window),
            focus_before
        );

        let targets = widgets.operation_result_emphasis_targets();
        assert_eq!(targets.len(), 5);
        for target in &targets {
            target.add_css_class("floe-operation-result");
            assert!(target.has_css_class("floe-operation-result"));
        }
        for target in targets {
            target.remove_css_class("floe-operation-result");
            assert!(!target.has_css_class("floe-operation-result"));
        }
        widgets.window.close();
        application.quit();
    }

    #[test]
    #[ignore = "requires graphical GTK session; run documented GTK component gate"]
    fn phase_6v_gtk_selection_scroll_and_emphasis_preserve_focus() {
        gtk::init().expect("GTK component gate requires available display");
        let strings = gtk::StringList::new(&["result.txt", "other.txt"]);
        let selection = gtk::MultiSelection::new(Some(strings));
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, object| {
            let item = object.downcast_ref::<gtk::ListItem>().expect("list item");
            item.set_child(Some(&gtk::Label::new(None)));
        });
        factory.connect_bind(|_, object| {
            let item = object.downcast_ref::<gtk::ListItem>().expect("list item");
            let label = item.child().and_downcast::<gtk::Label>().expect("label");
            let value = item
                .item()
                .and_downcast::<gtk::StringObject>()
                .expect("string item");
            label.set_label(&value.string());
        });
        let list = gtk::ListView::new(Some(selection.clone()), Some(factory));
        list.add_css_class("floe-directory-list");
        let location = gtk::Entry::new();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.append(&location);
        content.append(&list);
        let window = gtk::Window::builder().child(&content).build();
        window.present();
        location.grab_focus();
        let focus_before = gtk::prelude::GtkWindowExt::focus(&window);

        selection.select_item(0, true);
        assert!(selection.is_selected(0));
        let info = gtk::ScrollInfo::new();
        info.set_enable_vertical(true);
        list.scroll_to(0, gtk::ListScrollFlags::NONE, Some(info));
        assert_eq!(gtk::prelude::GtkWindowExt::focus(&window), focus_before);

        list.add_css_class("floe-operation-result");
        assert!(list.has_css_class("floe-operation-result"));
        list.remove_css_class("floe-operation-result");
        assert!(!list.has_css_class("floe-operation-result"));
        window.close();
    }

    #[test]
    fn phase_18y2_ui_model_exposes_durable_undo_redo_and_review_language() {
        let applied = OperationHistoryItem {
            title: "Copy report.pdf".to_owned(),
            detail: "Applied • Undo available".to_owned(),
            can_undo: true,
            can_redo: false,
        };
        let undone = OperationHistoryItem {
            title: "Move archive".to_owned(),
            detail: "Undone • Redo available".to_owned(),
            can_undo: false,
            can_redo: true,
        };
        assert!(applied.can_undo && !applied.can_redo);
        assert!(undone.can_redo && !undone.can_undo);
        assert!(OPERATION_HISTORY_DURABILITY_EXPLANATION.contains("privately"));
        assert!(OPERATION_HISTORY_DURABILITY_EXPLANATION.contains("30 days"));
        assert!(OPERATION_HISTORY_DURABILITY_EXPLANATION.contains("exact item"));
        assert!(OPERATION_HISTORY_DURABILITY_EXPLANATION.contains("require review"));

        let interrupted = RecoveryDialogItem {
            id: 7,
            title: "Interrupted Move Undo/Redo".to_owned(),
            detail: "Uncertain result • review exact paths".to_owned(),
            can_retry: false,
            can_resolve: true,
            source: Some(PathBuf::from("/tmp/source")),
            destination: PathBuf::from("/tmp/destination"),
        };
        assert!(!interrupted.can_retry);
        assert!(interrupted.can_resolve);
    }

    #[test]
    #[ignore = "requires graphical GTK session; run documented GTK component gate"]
    fn phase_testing_gtk_phase_18y2_history_controls_are_semantic_and_distinct() {
        gtk::init().expect("GTK component gate requires an available display");
        adw::init().expect("libadwaita must initialize in GTK component gate");
        let widgets = build_operation_history_dialog(
            &[
                OperationHistoryItem {
                    title: "Copy report.pdf".to_owned(),
                    detail: "Applied • Undo available".to_owned(),
                    can_undo: true,
                    can_redo: false,
                },
                OperationHistoryItem {
                    title: "Move archive".to_owned(),
                    detail: "Undone • Redo available".to_owned(),
                    can_undo: false,
                    can_redo: true,
                },
            ],
            true,
        );
        assert_eq!(widgets.undo_buttons.len(), 2);
        assert_eq!(widgets.redo_buttons.len(), 2);
        assert!(widgets.undo_buttons[0].is_visible());
        assert!(!widgets.redo_buttons[0].is_visible());
        assert!(!widgets.undo_buttons[1].is_visible());
        assert!(widgets.redo_buttons[1].is_visible());
        assert_eq!(
            widgets.undo_buttons[0].accessible_role(),
            gtk::AccessibleRole::Button
        );
        assert_eq!(
            widgets.redo_buttons[1].accessible_role(),
            gtk::AccessibleRole::Button
        );
    }

    #[test]
    fn phase_13d_content_search_ui_uses_plain_language_modes() {
        assert_eq!(
            SEARCH_SURFACE_MODES,
            ["Quick Filter", "Search Files", "Search Contents"]
        );
        assert_eq!(SEARCH_SURFACE_MODE_HELP.len(), SEARCH_SURFACE_MODES.len());
        assert!(SEARCH_SURFACE_MODE_HELP[0].contains("already shown"));
        assert!(SEARCH_SURFACE_MODE_HELP[0].contains("does not search subfolders"));
        assert!(SEARCH_SURFACE_MODE_HELP[1].contains("on disk"));
        assert!(SEARCH_SURFACE_MODE_HELP[1].contains("never reads file contents"));
        assert!(SEARCH_SURFACE_MODE_HELP[2].contains("explicitly reads"));
        assert!(SEARCH_SURFACE_MODE_HELP[2].contains("skips binary"));
        assert!(SEARCH_SURFACE_MODE_HELP[2].contains("remote"));
    }

    #[test]
    fn phase_13e_saved_search_ui_is_explicit_session_truthful_and_ordered() {
        assert_eq!(
            SAVED_SEARCH_CONTROL_LABELS,
            [
                "Saved searches",
                "Recent searches (this session)",
                "Save search",
                "Delete saved",
                "Clear recent",
                "Search result order",
            ]
        );
        assert!(SAVED_SEARCH_CONTROL_LABELS[1].contains("this session"));
        assert_eq!(
            SEARCH_RESULT_ORDER_LABELS,
            ["Name", "Modified (newest)", "Size (largest)"]
        );
    }

    #[test]
    fn phase_13e_search_result_order_labels_are_stable_and_plain_language() {
        assert_eq!(SEARCH_RESULT_ORDER_LABELS.len(), 3);
        assert!(
            SEARCH_RESULT_ORDER_LABELS
                .iter()
                .all(|label| !label.is_empty())
        );
        assert!(SEARCH_RESULT_ORDER_LABELS[1].contains("newest"));
        assert!(SEARCH_RESULT_ORDER_LABELS[2].contains("largest"));
    }

    #[test]
    fn phase_13f_search_index_ui_describes_capability_and_fallback_truthfully() {
        assert!(SEARCH_INDEX_CAPABILITY_HELP.contains("filenames and metadata only"));
        assert!(SEARCH_INDEX_CAPABILITY_HELP.contains("Hidden entries"));
        assert!(SEARCH_INDEX_CAPABILITY_HELP.contains("never indexed"));
        assert!(SEARCH_INDEX_CAPABILITY_HELP.contains("complete live search"));
    }

    /// Graphical GTK contract tests are deliberately a separate opt-in layer.
    /// They construct real Floe widgets and therefore require a working GTK
    /// display; ordinary `cargo test --workspace` must stay headless-safe.
    #[test]
    #[ignore = "requires a graphical GTK session; run the documented GTK component gate"]
    fn phase_testing_gtk_recovery_dialog_is_accessible_and_conservative() {
        gtk::init().expect("GTK component gate requires an available display");
        adw::init().expect("libadwaita must initialize in GTK component gate");
        let item = RecoveryDialogItem {
            id: 7,
            title: "Interrupted Copy".to_owned(),
            detail: "Source: Present • Destination: Missing".to_owned(),
            can_retry: true,
            can_resolve: true,
            source: Some(PathBuf::from("/tmp/source")),
            destination: PathBuf::from("/tmp/destination"),
        };
        let widgets = build_recovery_dialog(&[item]);
        assert_eq!(widgets.dialog.title().as_str(), "Operation Recovery");
        assert_eq!(widgets.retry_buttons.len(), 1);
        assert!(widgets.retry_buttons[0].is_sensitive());
        assert_eq!(
            widgets.resolve_buttons[0].accessible_role(),
            gtk::AccessibleRole::Button
        );
        assert_eq!(widgets.reveal_source_buttons.len(), 1);
        assert_eq!(widgets.reveal_destination_buttons.len(), 1);
    }

    #[test]
    #[ignore = "requires graphical GTK session; run documented GTK component gate"]
    fn phase_testing_gtk_phase_7g_header_filter_and_operations_accessibility_contract() {
        gtk::init().expect("GTK component gate requires an available display");
        adw::init().expect("libadwaita must initialize in the GTK component gate");
        let display = gtk::gdk::Display::default().expect("GTK display must be available");
        crate::iconography::register(&display);

        let application = adw::Application::builder()
            .application_id("io.github.rodriguezcappsec.Floe.ComponentTest")
            .build();
        application
            .register(None::<&gio::Cancellable>)
            .expect("component-test application must register before creating a window");
        let widgets = build(
            &application,
            &[],
            Appearance::for_preset(AppearancePreset::Native),
            ViewPreferences::default(),
        );

        let icon_fixture = tempfile::tempdir().expect("icon fixture root should be created");
        let pdf_path = icon_fixture.path().join("manual.pdf");
        let text_path = icon_fixture.path().join("notes.txt");
        fs::write(&pdf_path, b"%PDF-1.7\n").expect("PDF fixture should be written");
        fs::write(&text_path, b"plain text\n").expect("text fixture should be written");
        let listing = floe_core::enumerate_directory(icon_fixture.path())
            .expect("icon fixture root should enumerate");
        let pdf_entry = listing
            .entries()
            .iter()
            .find(|entry| entry.path() == pdf_path)
            .expect("PDF fixture should enumerate");
        let text_entry = listing
            .entries()
            .iter()
            .find(|entry| entry.path() == text_path)
            .expect("text fixture should enumerate");
        let pdf_image = gtk::Image::new();
        let text_image = gtk::Image::new();
        let theme = gtk::IconTheme::for_display(&display);

        for style in [EntryIconStyle::FloeColor, EntryIconStyle::Phosphor] {
            apply_entry_icon(&pdf_image, pdf_entry, LIST_ICON_EDGE, style);
            apply_entry_icon(&text_image, text_entry, LIST_ICON_EDGE, style);
            let pdf_name = pdf_image
                .icon_name()
                .expect("app-owned PDF icon name should be installed");
            let text_name = text_image
                .icon_name()
                .expect("app-owned text icon name should be installed");
            assert_ne!(pdf_name, text_name);
            let pdf_paintable = theme.lookup_icon(
                &pdf_name,
                &[],
                LIST_ICON_EDGE,
                1,
                gtk::TextDirection::Ltr,
                gtk::IconLookupFlags::empty(),
            );
            let text_paintable = theme.lookup_icon(
                &text_name,
                &[],
                LIST_ICON_EDGE,
                1,
                gtk::TextDirection::Ltr,
                gtk::IconLookupFlags::empty(),
            );
            assert_ne!(
                pdf_paintable.file().map(|file| file.uri()),
                text_paintable.file().map(|file| file.uri()),
                "PDF and text must resolve to distinct app-owned paintables in {style:?}"
            );
        }

        apply_entry_icon(
            &pdf_image,
            pdf_entry,
            LIST_ICON_EDGE,
            EntryIconStyle::System,
        );
        apply_entry_icon(
            &text_image,
            text_entry,
            LIST_ICON_EDGE,
            EntryIconStyle::System,
        );
        assert_eq!(pdf_image.storage_type(), gtk::ImageType::Gicon);
        assert_eq!(text_image.storage_type(), gtk::ImageType::Gicon);
        let pdf_gicon = pdf_image
            .gicon()
            .expect("system PDF fallback chain should be installed");
        let text_gicon = text_image
            .gicon()
            .expect("system text fallback chain should be installed");
        assert_ne!(pdf_gicon.to_string(), text_gicon.to_string());
        let pdf_paintable = theme.lookup_by_gicon(
            &pdf_gicon,
            LIST_ICON_EDGE,
            1,
            gtk::TextDirection::Ltr,
            gtk::IconLookupFlags::empty(),
        );
        let text_paintable = theme.lookup_by_gicon(
            &text_gicon,
            LIST_ICON_EDGE,
            1,
            gtk::TextDirection::Ltr,
            gtk::IconLookupFlags::empty(),
        );
        assert_ne!(
            pdf_paintable.file().map(|file| file.uri()),
            text_paintable.file().map(|file| file.uri()),
            "active System Theme must resolve PDF and text distinctly"
        );

        apply_entry_icon(
            &pdf_image,
            pdf_entry,
            LIST_ICON_EDGE,
            EntryIconStyle::FloeColor,
        );
        assert_eq!(pdf_image.storage_type(), gtk::ImageType::IconName);
        assert!(pdf_image.gicon().is_none(), "stale System GIcon must clear");
        assert_eq!(pdf_image.icon_name().as_deref(), Some("floe-file-pdf"));

        assert_eq!(widgets.entry_icon_style(), EntryIconStyle::FloeColor);
        assert!(widgets.window.has_css_class("icon-style-floe-color"));
        widgets.apply_entry_icon_style(EntryIconStyle::Phosphor);
        assert_eq!(widgets.entry_icon_style(), EntryIconStyle::Phosphor);
        assert!(widgets.window.has_css_class("icon-style-phosphor"));
        assert!(!widgets.window.has_css_class("icon-style-floe-color"));
        assert_eq!(
            widgets.back_button.icon_name().as_deref(),
            Some("floe-phosphor-arrow-left-symbolic")
        );

        for (button, action, tooltip) in [
            (&widgets.back_button, "win.back", "Back (Alt+Left)"),
            (
                &widgets.forward_button,
                "win.forward",
                "Forward (Alt+Right)",
            ),
            (
                &widgets.parent_button,
                "win.parent",
                "Parent folder (Alt+Up)",
            ),
        ] {
            assert_eq!(button.accessible_role(), gtk::AccessibleRole::Button);
            assert_eq!(button.action_name().as_deref(), Some(action));
            assert_eq!(button.tooltip_text().as_deref(), Some(tooltip));
        }

        assert_eq!(
            widgets.hidden_button.accessible_role(),
            gtk::AccessibleRole::ToggleButton
        );
        assert_eq!(
            widgets.sort_menu_button.accessible_role(),
            gtk::AccessibleRole::Button
        );
        assert_eq!(
            widgets.sort_menu_button.tooltip_text().as_deref(),
            Some("Sort files and folders")
        );
        assert_eq!(
            widgets.sort_menu_button.icon_name().as_deref(),
            Some("floe-phosphor-arrows-down-up-symbolic")
        );
        assert_eq!(
            widgets.hidden_button.action_name().as_deref(),
            Some("win.hidden")
        );
        assert_eq!(
            widgets.location_entry.accessible_role(),
            gtk::AccessibleRole::TextBox
        );
        assert_eq!(
            widgets.breadcrumb_box.accessible_role(),
            gtk::AccessibleRole::Group
        );
        assert_eq!(
            widgets.recent_locations_button.accessible_role(),
            gtk::AccessibleRole::Button
        );
        assert_eq!(
            widgets.recent_locations_button.action_name().as_deref(),
            Some("win.recent-locations")
        );
        assert_eq!(
            widgets.recent_locations_button.tooltip_text().as_deref(),
            Some("Recent locations")
        );
        assert_eq!(
            widgets.location_suggestions_box.accessible_role(),
            gtk::AccessibleRole::List
        );
        assert_eq!(
            widgets.filter_entry.accessible_role(),
            gtk::AccessibleRole::SearchBox
        );
        assert_eq!(
            widgets.filter_entry.placeholder_text().as_deref(),
            Some("Filter shown items")
        );
        assert_eq!(
            widgets.filter_entry.tooltip_text().as_deref(),
            Some(SEARCH_SURFACE_MODE_HELP[0])
        );
        assert_eq!(
            widgets.search_mode.accessible_role(),
            gtk::AccessibleRole::ComboBox
        );
        assert_eq!(
            widgets.search_scope.accessible_role(),
            gtk::AccessibleRole::ComboBox
        );
        assert_eq!(
            widgets.search_button.action_name().as_deref(),
            Some("win.start-filename-search")
        );
        assert_eq!(
            widgets.search_stop_button.action_name().as_deref(),
            Some("win.stop-filename-search")
        );
        assert_eq!(
            widgets.filter_feedback.accessible_role(),
            gtk::AccessibleRole::Alert
        );
        assert_eq!(
            widgets.advanced_filter_toggle.accessible_role(),
            gtk::AccessibleRole::ToggleButton
        );
        for dropdown in [
            &widgets.advanced_type,
            &widgets.advanced_size,
            &widgets.advanced_date,
            &widgets.advanced_owner,
            &widgets.advanced_hidden,
        ] {
            assert_eq!(dropdown.accessible_role(), gtk::AccessibleRole::ComboBox);
        }
        assert_eq!(
            widgets.advanced_extension.accessible_role(),
            gtk::AccessibleRole::TextBox
        );
        assert_eq!(
            widgets.advanced_mime.accessible_role(),
            gtk::AccessibleRole::TextBox
        );
        assert_eq!(
            widgets.advanced_match_case.accessible_role(),
            gtk::AccessibleRole::Checkbox
        );
        assert_eq!(
            widgets.advanced_match_case.label().as_deref(),
            Some("Match case")
        );
        assert_eq!(widgets.advanced_apply.label().as_deref(), Some("Apply"));
        assert_eq!(
            widgets.advanced_clear.label().as_deref(),
            Some("Clear filters")
        );

        assert_eq!(
            widgets.operations.operation_progress.accessible_role(),
            gtk::AccessibleRole::ProgressBar
        );
        assert_eq!(
            widgets.operations.operation_cancel.accessible_role(),
            gtk::AccessibleRole::Button
        );
        assert_eq!(
            widgets
                .operations
                .operation_cancel
                .tooltip_text()
                .as_deref(),
            Some("Cancel file operation")
        );
        assert_eq!(
            widgets.operations.operation_retry.label().as_deref(),
            Some("Retry")
        );
        assert_eq!(
            widgets.operations.operation_pause.label().as_deref(),
            Some("Pause after current")
        );

        widgets.window.close();
    }

    #[test]
    #[ignore = "requires graphical GTK session; run documented GTK component gate"]
    fn phase_testing_gtk_phase_20b2_accessibility_group_header_contract() {
        gtk::init().expect("GTK component gate requires an available display");
        let group = gtk::Button::builder().label("Date group").build();
        initialize_group_header(&group, true);

        assert_eq!(group.accessible_role(), gtk::AccessibleRole::Button);
        assert!(group.is_focusable());
        assert!(group.has_css_class("floe-group-label"));
        assert!(group.has_css_class("floe-grid-group-label"));
        assert!(group.has_css_class("heading"));
    }

    #[test]
    #[ignore = "requires graphical GTK session; run documented GTK component gate"]
    fn phase_testing_gtk_grouped_grid_uses_spanning_sections_and_shared_selection() {
        gtk::init().expect("GTK component gate requires an available display");

        let temporary = tempfile::tempdir().expect("temporary directory");
        fs::create_dir(temporary.path().join("folder-a")).expect("first folder");
        fs::create_dir(temporary.path().join("folder-b")).expect("second folder");
        fs::write(temporary.path().join("tiny-a.txt"), b"one").expect("first file");
        fs::write(temporary.path().join("tiny-b.txt"), b"two").expect("second file");
        let grouping = DirectoryGrouping::Type;
        let mut entries = floe_core::enumerate_directory(temporary.path())
            .expect("enumeration")
            .entries()
            .to_vec();
        DirectorySort::new(SortColumn::Name, SortDirection::Ascending)
            .with_grouping(grouping)
            .sort_entries(&mut entries);

        let mut preferences = ViewPreferences::default();
        preferences.sort = preferences.sort.with_grouping(grouping);
        let icon_style = Rc::new(Cell::new(EntryIconStyle::default()));
        let panel = build_directory_panel(preferences, &DropDispatcher::default(), &icon_style);
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        for entry in &entries {
            store.append(&glib::BoxedAnyObject::new(std::sync::Arc::new(
                entry.clone(),
            )));
        }
        panel.selection.set_model(Some(&store));
        panel.grouped_grid.rebuild();

        let sections = panel.grouped_grid.state.sections.clone();
        assert_eq!(sections.observe_children().n_items(), 2);
        let first_section = sections
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("group is a vertical section");
        let first_header = first_section
            .first_child()
            .and_downcast::<gtk::Button>()
            .expect("spanning header is outside the item grid");
        let first_grid = first_header
            .next_sibling()
            .and_downcast::<gtk::GridView>()
            .expect("section body follows its header");
        assert!(first_header.hexpands());
        assert!(first_header.is_focusable());
        assert_eq!(first_header.accessible_role(), gtk::AccessibleRole::Button);
        assert!(first_header.has_css_class("floe-grid-section-header"));
        assert!(first_grid.has_css_class("floe-grid-section-body"));

        let slice = first_grid
            .model()
            .and_downcast::<SelectionSlice>()
            .expect("section uses a selection slice");
        assert!(slice.select_item(0, true));
        assert!(panel.selection.is_selected(slice.start()));
        let local = slice
            .item(0)
            .and_downcast::<glib::BoxedAnyObject>()
            .expect("local item");
        let global = panel
            .selection
            .item(slice.start())
            .and_downcast::<glib::BoxedAnyObject>()
            .expect("global item");
        assert_eq!(
            local.borrow::<std::sync::Arc<DirectoryEntry>>().path(),
            global.borrow::<std::sync::Arc<DirectoryEntry>>().path()
        );

        let collapsed_label = first_header.tooltip_text().expect("group label");
        panel
            .collapsed_groups
            .borrow_mut()
            .insert(collapsed_label.to_string());
        panel.grouped_grid.refresh_collapsed_groups();
        let first_section = panel
            .grouped_grid
            .state
            .sections
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("rebuilt first section");
        let first_body = first_section
            .last_child()
            .and_downcast::<gtk::GridView>()
            .expect("first body");
        let second_body = first_section
            .next_sibling()
            .and_downcast::<gtk::Box>()
            .and_then(|section| section.last_child())
            .and_downcast::<gtk::GridView>()
            .expect("second body");
        assert!(!first_body.is_visible());
        assert!(second_body.is_visible());

        panel.grouped_grid.set_single_click_activate(true);
        assert!(
            panel
                .grouped_grid
                .state
                .section_grids
                .borrow()
                .iter()
                .all(gtk::GridView::is_single_click_activate)
        );
        let larger = GridSize::from_index(GRID_SIZES.len().saturating_sub(1))
            .expect("last configured grid size");
        panel.grouped_grid.set_grid_size(larger);
        assert_eq!(panel.grouped_grid.state.grid_size.get(), larger);

        let alternate_surface_store = gio::ListStore::new::<glib::BoxedAnyObject>();
        alternate_surface_store.append(&glib::BoxedAnyObject::new(String::from(
            "content-search-row",
        )));
        panel.selection.set_model(Some(&alternate_surface_store));
        panel.grouped_grid.rebuild();
        assert_eq!(
            panel
                .grouped_grid
                .state
                .sections
                .observe_children()
                .n_items(),
            0,
            "shared search models must not be interpreted as directory entries",
        );
        // This low-level component fixture intentionally has no application
        // window to own and tear down the manually parented popovers. The
        // native smoke below covers ordinary full-window shutdown.
        std::mem::forget(panel);
    }

    #[test]
    #[ignore = "requires graphical GTK session; run documented GTK component gate"]
    fn phase_testing_gtk_phase_20b2_appearance_click_accessibility_contract() {
        gtk::init().expect("GTK component gate requires an available display");
        adw::init().expect("libadwaita must initialize in GTK component gate");
        let application = adw::Application::builder()
            .application_id("io.github.rodriguezcappsec.Floe.Phase20B2ComponentTest")
            .build();
        application
            .register(None::<&gio::Cancellable>)
            .expect("component-test application must register before creating window");
        let mut preferences = ViewPreferences::default();
        preferences.color_scheme = ColorSchemePreference::Dark;
        preferences.click_policy = ClickPolicy::Single;
        preferences.font_family = Some("Sans".to_owned());
        preferences.font_scale_percent = 150;
        preferences.reduced_motion = true;
        let widgets = build(
            &application,
            &[],
            Appearance::for_preset(AppearancePreset::Native),
            preferences.clone(),
        );
        widgets.apply_appearance_preferences(&preferences);
        widgets.apply_click_policy(preferences.click_policy);

        assert_eq!(
            adw::StyleManager::default().color_scheme(),
            adw::ColorScheme::ForceDark
        );
        assert!(widgets.window.has_css_class("floe-reduced-motion"));
        assert!(widgets.list_view.is_single_click_activate());
        assert!(widgets.grid_view.is_single_click_activate());
        assert!(widgets.search_results_view.is_single_click_activate());
        widgets.window.close();
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::Default);
    }

    #[test]
    #[ignore = "requires graphical GTK session; run documented GTK component gate"]
    fn phase_23_reliability_window_transient_teardown() {
        gtk::init().expect("GTK component gate requires an available display");
        adw::init().expect("libadwaita must initialize in the GTK component gate");
        let application = adw::Application::builder()
            .application_id("io.github.rodriguezcappsec.Floe.WindowTeardownTest")
            .build();
        application
            .register(None::<&gio::Cancellable>)
            .expect("component-test application must register before creating a window");
        let widgets = build(
            &application,
            &[],
            Appearance::for_preset(AppearancePreset::Native),
            ViewPreferences::default(),
        );

        for popover in [
            widgets.location_suggestions.upcast_ref::<gtk::Widget>(),
            widgets.list_context_menu.upcast_ref(),
            widgets.grid_context_menu.upcast_ref(),
            widgets.search_context_menu.upcast_ref(),
            widgets.list_background_menu.upcast_ref(),
            widgets.grid_background_menu.upcast_ref(),
        ] {
            assert!(popover.parent().is_some());
        }

        widgets.prepare_for_window_close();

        for popover in [
            widgets.location_suggestions.upcast_ref::<gtk::Widget>(),
            widgets.list_context_menu.upcast_ref(),
            widgets.grid_context_menu.upcast_ref(),
            widgets.search_context_menu.upcast_ref(),
            widgets.list_background_menu.upcast_ref(),
            widgets.grid_background_menu.upcast_ref(),
        ] {
            assert!(popover.parent().is_none());
        }
        widgets.window.close();
    }

    #[test]
    #[ignore = "requires graphical GTK session; run documented GTK component gate"]
    fn phase_testing_gtk_phase_20b2a_window_size_restore_contract() {
        gtk::init().expect("GTK component gate requires an available display");
        adw::init().expect("libadwaita must initialize in GTK component gate");
        let application = adw::Application::builder()
            .application_id("io.github.rodriguezcappsec.Floe.WindowSizeComponentTest")
            .build();
        application
            .register(None::<&gio::Cancellable>)
            .expect("component-test application must register before creating window");
        let preferences = ViewPreferences::parse("window-size=1460x880\n");

        let widgets = build(
            &application,
            &[],
            Appearance::for_preset(AppearancePreset::Native),
            preferences,
        );
        assert_eq!(widgets.window.default_width(), 1460);
        assert_eq!(widgets.window.default_height(), 880);
        widgets.window.close();
    }

    #[test]
    fn phase_13a_filter_exposes_three_visible_matching_modes() {
        assert_eq!(FOLDER_FILTER_MODES, ["Text", "Glob", "Regex"]);
        assert_eq!(FOLDER_FILTER_MODE_HELP.len(), FOLDER_FILTER_MODES.len());
        assert_eq!(
            FOLDER_FILTER_MODE_SUMMARIES.len(),
            FOLDER_FILTER_MODES.len()
        );
        assert!(FOLDER_FILTER_MODE_HELP[0].contains("Match case"));
        assert!(FOLDER_FILTER_MODE_HELP[1].contains("* matches any characters"));
        assert!(FOLDER_FILTER_MODE_HELP[1].contains("*.pdf"));
        assert!(FOLDER_FILTER_MODE_HELP[1].contains("? matches one character"));
        assert!(FOLDER_FILTER_MODE_HELP[2].contains("regular-expression"));
        assert_eq!(folder_filter_mode_index("Glob"), Some(1));
        assert_eq!(folder_filter_mode_index("Unknown"), None);
    }

    #[test]
    fn phase_13c_advanced_filter_controls_are_plain_language_and_bounded() {
        assert_eq!(
            ADVANCED_TYPE_FILTERS,
            ["Any type", "Files", "Folders", "Symbolic links", "Other"]
        );
        assert_eq!(ADVANCED_OWNER_FILTERS, ["Anyone", "Me"]);
        assert_eq!(
            ADVANCED_HIDDEN_FILTERS,
            ["Current hidden setting", "Include hidden", "Hidden only"]
        );
        assert!(ADVANCED_SIZE_FILTERS.contains(&"Empty"));
        assert!(ADVANCED_SIZE_FILTERS.contains(&"Over 100 MB"));
        assert!(ADVANCED_DATE_FILTERS.contains(&"Last 24 hours"));
        assert!(ADVANCED_DATE_FILTERS.contains(&"Last 7 days"));
        assert!(FOLDER_FILTER_MODE_HELP[0].contains("Match case"));
    }

    fn collect_menu_actions(model: &gio::MenuModel, actions: &mut Vec<String>) {
        for index in 0..model.n_items() {
            if let Some(action) = model
                .item_attribute_value(index, "action", None)
                .and_then(|value| value.str().map(str::to_owned))
            {
                actions.push(action);
            }
            for link in ["section", "submenu"] {
                if let Some(child) = model.item_link(index, link) {
                    collect_menu_actions(&child, actions);
                }
            }
        }
    }

    fn menu_actions(model: &gio::MenuModel) -> Vec<String> {
        let mut actions = Vec::new();
        collect_menu_actions(model, &mut actions);
        actions
    }

    fn menu_labels(model: &gio::MenuModel) -> Vec<String> {
        (0..model.n_items())
            .filter_map(|index| {
                model
                    .item_attribute_value(index, "label", None)
                    .and_then(|value| value.str().map(str::to_owned))
            })
            .collect()
    }

    fn collect_all_menu_labels(model: &gio::MenuModel, labels: &mut Vec<String>) {
        for index in 0..model.n_items() {
            if let Some(label) = model
                .item_attribute_value(index, "label", None)
                .and_then(|value| value.str().map(str::to_owned))
            {
                labels.push(label);
            }
            for link in ["section", "submenu"] {
                if let Some(child) = model.item_link(index, link) {
                    collect_all_menu_labels(&child, labels);
                }
            }
        }
    }

    fn all_menu_labels(model: &gio::MenuModel) -> Vec<String> {
        let mut labels = Vec::new();
        collect_all_menu_labels(model, &mut labels);
        labels
    }

    #[test]
    fn phase_20b1_sort_ui_contains_requested_stateful_options() {
        let model = build_sort_by_menu_model();
        let model = model.upcast_ref::<gio::MenuModel>();
        let labels = all_menu_labels(model);
        for required in [
            "Name",
            "Natural Name",
            "Size",
            "Modified",
            "Created",
            "Accessed",
            "Type",
            "Rating",
            "Tags",
            "Comment",
            "Document",
            "Image",
            "Audio",
            "Video",
            "Other",
            "Ascending / Oldest First",
            "Descending / Newest First",
            "Folders First",
            "Hidden Files Last",
        ] {
            assert!(
                labels.iter().any(|label| label == required),
                "missing {required}"
            );
        }

        let actions = menu_actions(model);
        assert_eq!(
            actions
                .iter()
                .filter(|action| action.as_str() == "win.sort-column")
                .count(),
            34
        );
        assert_eq!(
            actions
                .iter()
                .filter(|action| action.as_str() == "win.sort-direction")
                .count(),
            2
        );
        assert!(actions.contains(&"win.folders-first".to_owned()));
        assert!(actions.contains(&"win.hidden-last".to_owned()));
        assert!(actions.contains(&"win.cancel-metadata-sort".to_owned()));
        assert!(actions.contains(&"win.clear-metadata-sort-cache".to_owned()));
        assert!(
            !labels
                .iter()
                .any(|label| label.contains("metadata index required"))
        );
    }

    #[test]
    fn phase_20b1a_ui_exposes_real_advanced_metadata_actions() {
        let model = build_sort_by_menu_model();
        let actions = menu_actions(model.upcast_ref());
        for column in [
            SortColumn::DocumentWordCount,
            SortColumn::ImageDimensions,
            SortColumn::AudioArtist,
            SortColumn::VideoDuration,
            SortColumn::LinkDestination,
            SortColumn::Owner,
        ] {
            assert!(actions.iter().any(|action| action == "win.sort-column"));
            assert!(SortColumn::from_persisted(column.persisted()).is_some());
        }
    }

    fn max_menu_depth(model: &gio::MenuModel) -> usize {
        let child_depth = (0..model.n_items())
            .flat_map(|index| {
                ["section", "submenu"]
                    .into_iter()
                    .filter_map(move |link| model.item_link(index, link))
            })
            .map(|child| max_menu_depth(&child))
            .max()
            .unwrap_or(0);
        child_depth + 1
    }

    #[test]
    fn header_options_are_task_grouped_and_preserve_nested_actions() {
        let create = gio::Menu::new();
        create.append(Some("New"), Some("win.test-create"));
        let open_inspect = gio::Menu::new();
        open_inspect.append(Some("Open"), Some("win.test-open"));
        let file_operations = gio::Menu::new();
        let transfer = gio::Menu::new();
        transfer.append(Some("Copy"), Some("win.test-transfer"));
        file_operations.append_submenu(Some("Transfer"), &transfer);
        let view_layout = gio::Menu::new();
        view_layout.append(Some("View"), Some("win.test-view"));
        let tools_safety = gio::Menu::new();
        tools_safety.append(Some("Tool"), Some("win.test-tool"));
        let utility = gio::Menu::new();
        utility.append(Some("Settings"), Some("win.test-settings"));

        let model = organize_header_options_menu(
            &create,
            &open_inspect,
            &file_operations,
            &view_layout,
            &tools_safety,
            &utility,
        );

        assert_eq!(model.n_items(), 6, "root remains compact");
        assert_eq!(
            menu_labels(model.upcast_ref()),
            [
                "Create",
                "Open & Inspect",
                "File Operations",
                "View & Layout",
                "Tools & Safety",
            ]
        );
        let actions = menu_actions(model.upcast_ref());
        assert_eq!(actions.len(), 6);
        let unique = actions.iter().collect::<HashSet<_>>();
        assert_eq!(unique.len(), actions.len(), "actions remain unique");
        for expected in [
            "win.test-create",
            "win.test-open",
            "win.test-transfer",
            "win.test-view",
            "win.test-tool",
            "win.test-settings",
        ] {
            assert!(actions.iter().any(|action| action == expected));
        }
        assert!(max_menu_depth(model.upcast_ref()) <= 4);
    }

    #[test]
    fn split_pane_updates_preserve_the_focused_active_child_when_opening_or_closing() {
        assert_eq!(
            split_pane_update(
                Some(SplitPaneLayout::ActiveOnly),
                SplitPaneLayout::ActiveThenInactive,
            ),
            SplitPaneUpdate::AddInactiveEnd
        );
        assert_eq!(
            split_pane_update(
                Some(SplitPaneLayout::ActiveThenInactive),
                SplitPaneLayout::ActiveOnly,
            ),
            SplitPaneUpdate::RemoveInactiveEnd
        );
    }

    #[test]
    fn split_pane_updates_reparent_only_for_real_side_changes() {
        assert_eq!(
            split_pane_layout(false, SplitSide::Secondary),
            SplitPaneLayout::ActiveOnly
        );
        assert_eq!(
            split_pane_update(
                Some(SplitPaneLayout::ActiveThenInactive),
                SplitPaneLayout::InactiveThenActive,
            ),
            SplitPaneUpdate::Rebuild
        );
        assert_eq!(
            split_pane_update(
                Some(SplitPaneLayout::InactiveThenActive),
                SplitPaneLayout::InactiveThenActive,
            ),
            SplitPaneUpdate::NoChange
        );
    }

    #[test]
    fn phase_12f_context_menu_defaults_expose_archives_without_a_menu_wall() {
        let model = build_configured_file_context_menu_model(ContextMenuPreferences::default());
        let actions = menu_actions(model.upcast_ref());

        for action in [
            "win.extract-here",
            "win.extract-to",
            "win.compress",
            "win.batch-rename",
            "win.inspect-privacy-safety",
            "win.scan-threats",
            "win.create-sanitized-copy",
            "win.context-menu-settings",
        ] {
            assert!(
                actions.iter().any(|candidate| candidate == action),
                "{action}"
            );
        }
        assert!(!actions.iter().any(|action| action == "win.checksum"));
        assert!(!actions.iter().any(|action| action == "win.copy-path"));
        assert!(
            model.n_items() <= 9,
            "root stays grouped into compact sections"
        );
    }

    #[test]
    fn phase_12f_context_menu_fixed_actions_survive_empty_optional_preferences() {
        let empty = ContextMenuPreferences::empty();
        let file = build_configured_file_context_menu_model(empty);
        let file_actions = menu_actions(file.upcast_ref());
        for action in [
            "win.open",
            "win.copy",
            "win.cut",
            "win.rename",
            "win.trash",
            "win.permanent-delete",
            "win.properties",
            "win.context-menu-settings",
        ] {
            assert!(file_actions.iter().any(|candidate| candidate == action));
        }
        for action in [
            "win.extract-here",
            "win.checksum",
            "win.open-terminal",
            "win.open-opposite-pane",
        ] {
            assert!(!file_actions.iter().any(|candidate| candidate == action));
        }

        let background = build_configured_background_context_menu_model(empty);
        let background_actions = menu_actions(background.upcast_ref());
        assert!(
            background_actions
                .iter()
                .any(|action| action == "win.new-folder")
        );
        assert!(
            background_actions
                .iter()
                .any(|action| action == "win.context-menu-settings")
        );
        assert!(
            !background_actions
                .iter()
                .any(|action| action == "win.open-terminal")
        );
        assert!(
            !background_actions
                .iter()
                .any(|action| action == "win.toggle-split")
        );
    }

    #[test]
    fn phase_18x_protected_folder_actions_are_fixed_in_file_and_background_contexts() {
        let file = build_configured_file_context_menu_model(ContextMenuPreferences::empty());
        let background =
            build_configured_background_context_menu_model(ContextMenuPreferences::empty());
        for actions in [
            menu_actions(file.upcast_ref()),
            menu_actions(background.upcast_ref()),
        ] {
            for required in [
                "win.protect-folder",
                "win.unprotect-folder",
                "win.protected-folders",
            ] {
                assert!(
                    actions.iter().any(|action| action == required),
                    "{required}"
                );
            }
        }
    }

    #[test]
    fn phase_14b_ui_administrator_action_is_reachable_from_folder_contexts() {
        let file = build_configured_file_context_menu_model(ContextMenuPreferences::empty());
        let background =
            build_configured_background_context_menu_model(ContextMenuPreferences::empty());
        for actions in [
            menu_actions(file.upcast_ref()),
            menu_actions(background.upcast_ref()),
        ] {
            assert!(
                actions
                    .iter()
                    .any(|action| action == "win.open-as-administrator")
            );
        }
        assert!(crate::command_registry::command("win.open-as-administrator").is_some());
        assert!(crate::command_registry::command("win.return-standard-access").is_some());
    }

    #[test]
    fn phase_12f_context_menu_custom_groups_are_deterministic_and_deduplicated() {
        let preferences =
            ContextMenuPreferences::parse("checksums,copy-details,archives,checksums,unknown");
        let first = build_configured_file_context_menu_model(preferences);
        let second = build_configured_file_context_menu_model(preferences);
        let first_actions = menu_actions(first.upcast_ref());
        let second_actions = menu_actions(second.upcast_ref());
        assert_eq!(first_actions, second_actions);
        assert_eq!(
            first_actions
                .iter()
                .filter(|action| action.as_str() == "win.checksum")
                .count(),
            1
        );
        assert!(first_actions.iter().any(|action| action == "win.copy-uri"));
        assert!(first_actions.iter().any(|action| action == "win.compress"));
    }

    #[test]
    fn phase_6n_actions_keep_restore_and_irreversible_trash_actions_distinct() {
        assert_eq!(
            TRASH_CONTEXT_ACTIONS,
            [
                ("Restore", "win.restore"),
                ("Calculate Checksums…", "win.checksum"),
                ("Delete Permanently…", "win.permanent-delete"),
                ("Properties", "win.properties"),
            ]
        );
        assert!(FILE_CONTEXT_ACTIONS.contains(&("Delete Permanently…", "win.permanent-delete")));
        assert!(
            TRASH_CONTEXT_ACTIONS
                .iter()
                .all(|(label, _)| !label.to_ascii_lowercase().contains("secure"))
        );
    }

    #[test]
    fn properties_context_menu_models_expose_actual_action_in_final_section() {
        for menu in [
            build_file_context_menu_model(),
            build_trash_context_menu_model(),
        ] {
            let final_section = menu
                .item_link(menu.n_items() - 1, "section")
                .expect("Properties should be in a separated final section");
            assert_eq!(final_section.n_items(), 1);

            let action = final_section
                .item_attribute_value(0, "action", None)
                .and_then(|value| value.str().map(str::to_owned));
            assert_eq!(action.as_deref(), Some("win.properties"));
        }
    }

    #[test]
    fn phase_6p_ui_operation_history_remains_reachable_after_island_hides() {
        assert_eq!(
            OPERATION_HISTORY_MENU_ITEM,
            ("Operation History", "win.operation-history")
        );
    }

    #[test]
    fn phase_6k2_operation_island_layout_keeps_every_child_within_content_width() {
        let layout = OperationIslandLayout::CURRENT;

        assert!(layout.outer_width >= 320);
        assert_eq!(layout.inset, 12);
        assert_eq!(layout.content_width(), 316);
        assert!(layout.child_minimums_fit());
        assert!(layout.cancel_min_width <= layout.content_width());
        assert!(layout.action_min_width <= layout.content_width());
    }

    #[test]
    fn phase_6k2_operation_island_layout_keeps_recovery_actions_reachable() {
        let layout = OperationIslandLayout::CURRENT;

        assert!(layout.action_min_width >= 72);
        assert!(layout.content_width() - layout.action_min_width >= 0);
        assert_eq!(
            OPERATION_ISLAND_STRUCTURE,
            [
                OperationIslandRow::TitleAndCancel,
                OperationIslandRow::Detail,
                OperationIslandRow::Progress,
                OperationIslandRow::RecoveryActions,
            ]
        );
    }

    #[test]
    fn phase_6k2_sidebar_density_uses_consistent_related_and_section_rhythm() {
        let compact = sidebar_density_metrics(SidebarDensity::Compact);
        let balanced = sidebar_density_metrics(SidebarDensity::Balanced);
        let comfortable = sidebar_density_metrics(SidebarDensity::Comfortable);

        assert_eq!(compact.row_gap, 2);
        assert_eq!(balanced.row_gap, 2);
        assert_eq!(comfortable.row_gap, 2);
        assert_eq!(compact.section_gap, 4);
        assert_eq!(balanced.section_gap, 8);
        assert_eq!(comfortable.section_gap, 12);
        assert!(compact.outer_margin < balanced.outer_margin);
        assert!(balanced.outer_margin < comfortable.outer_margin);
        assert_eq!(
            sidebar_density_class(SidebarDensity::Compact),
            "sidebar-compact"
        );
        assert_eq!(
            SIDEBAR_DENSITY_MENU_ITEMS,
            [
                ("Compact", "win.sidebar-density::compact"),
                ("Balanced", "win.sidebar-density::balanced"),
                ("Comfortable", "win.sidebar-density::comfortable"),
            ]
        );
        assert_eq!(
            RESET_SIDEBAR_WIDTH_MENU_ITEM,
            ("Reset Sidebar Width", "win.reset-sidebar-width")
        );
        assert_eq!(
            OPERATION_HISTORY_MENU_ITEM,
            ("Operation History", "win.operation-history")
        );
    }

    #[test]
    fn phase_6k2_sidebar_width_restores_clamped_value_or_appearance_default() {
        let appearance_default = 168;
        let with_width = |width| {
            let mut preferences = ViewPreferences::default();
            preferences.sidebar_width = Some(width);
            preferences
        };

        assert_eq!(
            initial_sidebar_width(with_width(312), appearance_default),
            312
        );
        assert_eq!(
            initial_sidebar_width(with_width(1), appearance_default),
            i32::from(SIDEBAR_WIDTH_MIN)
        );
        assert_eq!(
            initial_sidebar_width(ViewPreferences::default(), appearance_default),
            appearance_default
        );
        assert_eq!(sidebar_pane_resize_policy(), (false, true));
    }

    #[test]
    fn phase_20b2a_window_size_policy_restores_or_uses_default() {
        assert_eq!(
            initial_window_size(&ViewPreferences::default()),
            WindowSize::default()
        );

        let preferences = ViewPreferences::parse("window-size=1600x960\n");
        let restored = initial_window_size(&preferences);
        assert_eq!((restored.width(), restored.height()), (1600, 960));
    }

    #[test]
    fn phase_6a_columns_have_stable_scannable_semantics() {
        assert_eq!(
            LIST_COLUMN_LABELS,
            ["Name", "Type", "Size", "Modified", "Extension"]
        );
    }

    #[test]
    fn phase_6b_sort_heading_text_exposes_direction_without_color() {
        assert_eq!(
            sort_heading_text(SortColumn::Name, Some(SortDirection::Ascending)),
            "Name ↑"
        );
        assert_eq!(
            sort_heading_text(SortColumn::Modified, Some(SortDirection::Descending)),
            "Modified ↓"
        );
        assert_eq!(sort_heading_text(SortColumn::Size, None), "Size");
    }

    #[test]
    fn phase_6c_presentation_deduplicates_pending_and_bounds_completed_cache() {
        let presentation =
            ThumbnailPresentation::new(Rc::new(Cell::new(EntryIconStyle::FloeColor)));
        let pending_key = ThumbnailKey::for_test(PathBuf::from("/virtual/pending.png"), 1);
        {
            let mut state = presentation.state.borrow_mut();
            state.enqueue(pending_key.clone());
            state.enqueue(pending_key);
            assert_eq!(state.pending.len(), 1);
            assert_eq!(state.requests.len(), 1);

            for index in 0..=THUMBNAIL_CACHE_CAPACITY {
                state.insert_completed(
                    ThumbnailKey::for_test(
                        PathBuf::from(format!("/virtual/{index}.png")),
                        index as u64,
                    ),
                    CachedThumbnail::Fallback,
                );
            }
            assert_eq!(state.completed.len(), THUMBNAIL_CACHE_CAPACITY);
            assert!(
                !state
                    .completed
                    .contains_key(&ThumbnailKey::for_test(PathBuf::from("/virtual/0.png"), 0))
            );
        }
    }

    #[test]
    fn phase_6a_kind_labels_distinguish_links_without_color_or_icons() {
        assert_eq!(entry_type_label(EntryKind::Directory), "Folder");
        assert_eq!(entry_type_label(EntryKind::RegularFile), "File");
        assert_eq!(
            entry_type_label(EntryKind::SymbolicLink {
                target_is_directory: true,
            }),
            "Folder link"
        );
        assert_eq!(
            entry_type_label(EntryKind::SymbolicLink {
                target_is_directory: false,
            }),
            "File link"
        );
        assert_eq!(entry_type_label(EntryKind::Other), "Special");
    }

    #[test]
    fn phase_6a_size_formatting_is_compact_at_unit_boundaries() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(1_000), "1.0 KB");
        assert_eq!(format_size(1_500_000), "1.5 MB");
        assert_eq!(format_size(u64::MAX), "18.4 EB");
    }

    #[test]
    fn phase_6a_modified_formatting_handles_epoch_and_pre_epoch_times() {
        let rendered = format_modified(UNIX_EPOCH).expect("Unix epoch should be representable");
        assert!(!rendered.trim().is_empty());

        let pre_epoch = UNIX_EPOCH
            .checked_sub(Duration::from_secs(1))
            .expect("one second before epoch should be representable");
        let rendered = format_modified(pre_epoch).expect("pre-epoch times should be representable");
        assert!(!rendered.trim().is_empty());
    }

    #[test]
    fn phase_6j_secondary_click_preserves_or_retargets_selection() {
        assert_eq!(
            context_selection_for_secondary(true),
            ContextSelection::Preserve
        );
        assert_eq!(
            context_selection_for_secondary(false),
            ContextSelection::SelectOnly
        );
    }

    #[test]
    fn phase_6j_background_context_menu_exposes_only_directory_actions() {
        assert_eq!(
            BACKGROUND_CONTEXT_ACTIONS,
            [
                ("New Folder…", "win.new-folder"),
                ("New Empty File…", "win.new-empty-file"),
                ("New From Template…", "win.new-from-template"),
                ("Paste", "win.paste"),
                ("Select All", "win.select-all"),
                ("Invert Selection", "win.invert-selection"),
                ("Refresh", "win.refresh"),
                ("Edit Location", "win.location"),
                ("Open Terminal Here", "win.open-terminal"),
                ("Check for Duplicates…", "win.check-duplicates"),
                ("Customize Context Menus…", "win.context-menu-settings"),
                ("Protect Folder", "win.protect-folder"),
                ("Unprotect Folder", "win.unprotect-folder"),
                ("Protected Folders…", "win.protected-folders"),
            ]
        );
        assert!(BACKGROUND_CONTEXT_ACTIONS.iter().all(|(_, action)| {
            !matches!(
                *action,
                "win.open" | "win.open-with" | "win.rename" | "win.trash"
            )
        }));
    }

    #[test]
    fn phase_5c_context_menu_reuses_complete_existing_action_mapping() {
        assert_eq!(
            FILE_CONTEXT_ACTIONS,
            [
                ("Open", "win.open"),
                ("Open With…", "win.open-with"),
                ("Copy", "win.copy"),
                ("Copy and Verify…", "win.copy-and-verify"),
                (
                    "Verified Removable Transfer…",
                    "win.verified-removable-transfer",
                ),
                ("Cut", "win.cut"),
                ("Duplicate", "win.duplicate"),
                ("Rename…", "win.rename"),
                ("Create Symbolic Link…", "win.create-symbolic-link"),
                ("Create Hard Link…", "win.create-hard-link"),
                ("Reveal Link Target", "win.reveal-link-target"),
                ("Copy Name", "win.copy-name"),
                ("Copy Path", "win.copy-path"),
                ("Copy Relative Path", "win.copy-relative-path"),
                ("Copy URI", "win.copy-uri"),
                ("Calculate Checksums…", "win.checksum"),
                ("Check for Duplicates…", "win.check-duplicates"),
                ("Move to Trash", "win.trash"),
                ("Delete Permanently…", "win.permanent-delete"),
                ("Properties", "win.properties"),
                ("Open Terminal Here", "win.open-terminal"),
                ("Extract Here", "win.extract-here"),
                ("Extract To…", "win.extract-to"),
                ("Compress…", "win.compress"),
                ("Batch Rename…", "win.batch-rename"),
                ("Undo Last Batch Rename", "win.undo-batch-rename"),
                ("Customize Context Menus…", "win.context-menu-settings"),
                ("Reveal in Folder", "win.reveal-in-folder"),
                ("Protect Folder", "win.protect-folder"),
                ("Unprotect Folder", "win.unprotect-folder"),
                ("Protected Folders…", "win.protected-folders"),
                ("Audit Permissions…", "win.audit-permissions"),
            ]
        );
    }

    #[test]
    fn phase_5c_context_selection_rejects_unbound_virtualized_rows() {
        assert!(is_bound_list_position(0));
        assert!(is_bound_list_position(42));
        assert!(!is_bound_list_position(gtk::INVALID_LIST_POSITION));
    }

    #[test]
    fn phase_5f_conflict_surface_has_only_non_overwriting_decisions() {
        assert_eq!(
            CONFLICT_DECISION_LABELS,
            ["Keep Existing", "Retry with New Name"]
        );
        assert!(
            CONFLICT_DECISION_LABELS
                .iter()
                .all(|label| !label.contains("Overwrite") && !label.contains("Apply to All"))
        );
    }

    #[test]
    fn phase_6k_sidebar_is_compact_and_bookmark_removal_is_explicit() {
        let compact_width = std::hint::black_box(SIDEBAR_COMPACT_MIN_WIDTH);
        assert!((124..=144).contains(&compact_width));
        assert!(!bookmark_actions_enabled(false, false));
        assert!(!bookmark_actions_enabled(true, true));
        assert!(bookmark_actions_enabled(true, false));
        let raw = PathBuf::from("/bookmarks/one");
        let paths = vec![raw.clone(), PathBuf::from("/bookmarks/two")];
        assert_eq!(
            bookmark_paths_after_remove(&paths, 0),
            Some(vec![paths[1].clone()])
        );
        assert_eq!(bookmark_paths_after_remove(&paths, 2), None);
        assert_eq!(paths[0], raw, "removal policy must not rewrite exact paths");
    }

    #[test]
    fn phase_6k_sidebar_device_policy_mounts_or_navigates_local_storage() {
        let unavailable =
            DeviceActionStatus::Unavailable(crate::devices::DeviceActionUnavailable::NotSupported);
        let mountable = device_row_policy_for(
            DeviceMountState::Unmounted,
            DeviceRootKind::None,
            DeviceActions {
                mount: DeviceActionStatus::Available,
                unmount: unavailable,
                eject: DeviceActionStatus::Available,
            },
            None,
        );
        assert_eq!(mountable.status, "Unmounted");
        assert_eq!(mountable.activation, DeviceActivation::Mount);
        assert!(mountable.can_eject);

        let root = PathBuf::from("/run/media/example");
        let mounted = device_row_policy_for(
            DeviceMountState::Mounted,
            DeviceRootKind::Local,
            DeviceActions {
                mount: DeviceActionStatus::Unavailable(
                    crate::devices::DeviceActionUnavailable::AlreadyMounted,
                ),
                unmount: DeviceActionStatus::Available,
                eject: DeviceActionStatus::Available,
            },
            Some(&root),
        );
        assert_eq!(mounted.activation, DeviceActivation::Navigate(root));
        assert!(mounted.can_unmount);
        assert!(mounted.can_eject);
    }

    #[test]
    fn phase_6k_sidebar_keeps_remote_and_busy_devices_honestly_unavailable() {
        let remote = device_row_policy_for(
            DeviceMountState::Mounted,
            DeviceRootKind::NonLocal,
            DeviceActions {
                mount: DeviceActionStatus::Unavailable(
                    crate::devices::DeviceActionUnavailable::AlreadyMounted,
                ),
                unmount: DeviceActionStatus::Available,
                eject: DeviceActionStatus::Unavailable(
                    crate::devices::DeviceActionUnavailable::NotSupported,
                ),
            },
            None,
        );
        assert_eq!(remote.status, "Remote");
        assert!(matches!(
            remote.activation,
            DeviceActivation::Unavailable(_)
        ));

        let busy = device_row_policy_for(
            DeviceMountState::Unmounted,
            DeviceRootKind::None,
            DeviceActions {
                mount: DeviceActionStatus::Busy,
                unmount: DeviceActionStatus::Unavailable(
                    crate::devices::DeviceActionUnavailable::NotMounted,
                ),
                eject: DeviceActionStatus::Busy,
            },
            None,
        );
        assert_eq!(busy.status, "Mounting");
        assert!(matches!(busy.activation, DeviceActivation::Unavailable(_)));
        assert!(!busy.can_eject);

        let unavailable = device_row_policy_for(
            DeviceMountState::Unmounted,
            DeviceRootKind::None,
            DeviceActions {
                mount: DeviceActionStatus::Unavailable(
                    crate::devices::DeviceActionUnavailable::NotSupported,
                ),
                unmount: DeviceActionStatus::Unavailable(
                    crate::devices::DeviceActionUnavailable::NotMounted,
                ),
                eject: DeviceActionStatus::Unavailable(
                    crate::devices::DeviceActionUnavailable::NotSupported,
                ),
            },
            None,
        );
        assert_eq!(unavailable.status, "Unavailable");
    }

    #[test]
    fn phase_6t_density_maps_to_stable_shared_list_and_grid_classes() {
        assert_eq!(
            file_view_density_class(FileViewDensity::Compact),
            "view-compact"
        );
        assert_eq!(
            file_view_density_class(FileViewDensity::Comfortable),
            "view-comfortable"
        );
        assert_eq!(
            file_view_density_class(FileViewDensity::Spacious),
            "view-spacious"
        );
    }

    #[test]
    fn phase_10f_advanced_columns_present_bounded_lazy_values_truthfully() {
        let details = MetadataDetails {
            mime_type: Some("audio/flac".to_owned()),
            created: None,
            accessed: None,
            unix_mode: Some(0o100644),
            owner: Some(1000),
            group: Some(1000),
            link_target: None,
            image_dimensions: crate::inspector::ImageDimensionFacts::NotImage,
            advanced: crate::advanced_metadata::AdvancedMetadataState::Present(
                crate::advanced_metadata::AdvancedMetadata {
                    exif: None,
                    media: Some(crate::advanced_metadata::MediaMetadata {
                        duration: Some(Duration::from_secs(3_725)),
                        artist: Some("Floe Artist".to_owned()),
                        album: Some("Floe Album".to_owned()),
                        track: Some(3),
                        track_total: Some(12),
                        ..crate::advanced_metadata::MediaMetadata::default()
                    }),
                },
            ),
        };
        assert_eq!(
            advanced_column_texts(&details),
            ["", "1:02:05", "Floe Artist", "Floe Album", "3/12"]
        );

        let limited = MetadataDetails {
            advanced: crate::advanced_metadata::AdvancedMetadataState::LimitExceeded,
            ..details
        };
        assert_eq!(advanced_column_texts(&limited)[1], "Limited");
        assert!(ListColumn::OPTIONAL.contains(&ListColumn::Duration));
    }

    #[test]
    fn phase_10f_advanced_metadata_ui_uses_explicit_non_verdict_states() {
        let malformed = MetadataDetails {
            mime_type: Some("audio/mpeg".to_owned()),
            created: None,
            accessed: None,
            unix_mode: None,
            owner: None,
            group: None,
            link_target: None,
            image_dimensions: crate::inspector::ImageDimensionFacts::NotImage,
            advanced: crate::advanced_metadata::AdvancedMetadataState::Malformed(
                "invalid frame".to_owned(),
            ),
        };
        let columns = advanced_column_texts(&malformed);
        assert_eq!(columns[1], "Malformed");
        assert!(columns.iter().all(|value| {
            !value.contains("safe") && !value.contains("malicious") && !value.contains("verified")
        }));
        assert_eq!(format_media_duration(Duration::from_secs(65)), "1:05");
    }

    #[test]
    fn grid_grouping_exposes_the_same_visible_boundaries_as_list_grouping() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        fs::create_dir(temporary.path().join("archive.2024")).expect("dotted folder");
        fs::create_dir(temporary.path().join("projects")).expect("plain folder");
        fs::write(temporary.path().join("main.rs"), b"fn main() {}").expect("Rust fixture");
        fs::write(temporary.path().join("notes.txt"), b"notes").expect("text fixture");
        let mut entries = floe_core::enumerate_directory(temporary.path())
            .expect("enumeration")
            .entries()
            .to_vec();
        let grouping = DirectoryGrouping::Extension;
        DirectorySort::new(SortColumn::Name, SortDirection::Ascending)
            .with_grouping(grouping)
            .sort_entries(&mut entries);

        let labels = entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                visible_group_label(grouping, entry, index.checked_sub(1).map(|i| &entries[i]))
            })
            .collect::<Vec<_>>();
        assert_eq!(labels, ["Folders", ".rs", ".txt"]);
        assert!(
            entries.iter().all(|entry| {
                visible_group_label(DirectoryGrouping::None, entry, None).is_none()
            })
        );
    }

    #[test]
    fn phase_6t_columns_have_keyboard_action_parity_and_extension_sort_name() {
        for column in ListColumn::ALL {
            assert_eq!(
                column_resize_action(column, false),
                format!("win.narrow-{}", column.persisted())
            );
            assert_eq!(
                column_resize_action(column, true),
                format!("win.widen-{}", column.persisted())
            );
        }
        assert_eq!(
            sort_action_name(SortColumn::Extension),
            "win.sort-extension"
        );
        assert_eq!(
            sort_heading_text(SortColumn::Extension, Some(SortDirection::Ascending)),
            "Extension ↑"
        );
    }

    #[test]
    fn phase_6q_ui_menus_expose_complete_create_link_and_copy_action_mapping() {
        for required in [
            ("Duplicate", "win.duplicate"),
            ("Create Symbolic Link…", "win.create-symbolic-link"),
            ("Create Hard Link…", "win.create-hard-link"),
            ("Reveal Link Target", "win.reveal-link-target"),
            ("Copy Name", "win.copy-name"),
            ("Copy Path", "win.copy-path"),
            ("Copy Relative Path", "win.copy-relative-path"),
            ("Copy URI", "win.copy-uri"),
        ] {
            assert!(FILE_CONTEXT_ACTIONS.contains(&required));
        }
        for required in [
            ("New Folder…", "win.new-folder"),
            ("New Empty File…", "win.new-empty-file"),
            ("New From Template…", "win.new-from-template"),
        ] {
            assert!(BACKGROUND_CONTEXT_ACTIONS.contains(&required));
        }
        assert!(
            FILE_CONTEXT_ACTIONS
                .iter()
                .all(|(_, action)| action.starts_with("win."))
        );
        assert!(
            BACKGROUND_CONTEXT_ACTIONS
                .iter()
                .all(|(_, action)| action.starts_with("win."))
        );
    }

    #[test]
    fn phase_10c_properties_ui_is_discoverable_read_only_and_selection_aware() {
        assert!(FILE_CONTEXT_ACTIONS.contains(&("Properties", "win.properties")));
        assert!(TRASH_CONTEXT_ACTIONS.contains(&("Properties", "win.properties")));
        let presentation = crate::properties::PropertiesPresentation {
            title: "2 Items".to_owned(),
            general: vec![crate::properties::PropertyRow {
                label: "Metadata",
                value: "Differing values are not merged".to_owned(),
            }],
            filesystem: vec![crate::properties::PropertyRow {
                label: "Read-only",
                value: "Unknown".to_owned(),
            }],
            selection_count: 2,
            open_with_available: false,
            checksum_available: false,
            permissions: crate::properties::PermissionDefaults {
                targets: std::sync::Arc::from([
                    std::path::PathBuf::from("/tmp/a"),
                    std::path::PathBuf::from("/tmp/b"),
                ]),
                common_file_mode: Some(0o644),
                common_directory_mode: None,
                common_uid: Some(1000),
                common_gid: Some(1000),
                has_files: true,
                has_directories: false,
                editable: true,
            },
            permission_audit: crate::properties::PermissionAuditPresentation::default(),
        };
        assert_eq!(presentation.selection_count, 2);
        assert!(!presentation.open_with_available);
        assert!(presentation.general[0].value.contains("not merged"));
    }

    #[test]
    #[ignore = "requires graphical GTK session; run documented GTK component gate"]
    fn phase_testing_gtk_phase_18r_permission_ui_is_accessible_and_explicit() {
        gtk::init().expect("GTK component gate requires available display");
        adw::init().expect("libadwaita must initialize in GTK component gate");
        let presentation = crate::properties::PermissionAuditPresentation {
            summary: vec![crate::properties::PropertyRow {
                label: "Findings",
                value: "1 reviewed finding · 1 high-attention".to_owned(),
            }],
            details: "private.key\n  mode 0644 (rw-r--r--)\n  Limitations: not a complete security verdict."
                .to_owned(),
            fix: Some(crate::properties::PermissionFixPresentation {
                path: std::path::PathBuf::from("/tmp/private.key"),
                identity: floe_core::PermissionAuditIdentity {
                    device: 1,
                    inode: 2,
                    size: 3,
                    mode: 0o100644,
                    uid: 1000,
                    gid: 1000,
                    modified_seconds: 4,
                    modified_nanoseconds: 5,
                    changed_seconds: 6,
                    changed_nanoseconds: 7,
                },
                object_kind: floe_core::PermissionObjectKind::RegularFile,
                original_mode: 0o644,
                proposed_mode: 0o600,
                reasons: "Sensitive-looking file has group or other access".to_owned(),
            }),
        };
        let widgets = build_permission_audit_dialog(&presentation);
        assert_eq!(widgets.dialog.title().as_str(), "Permission Audit");
        assert!(widgets.fix_button.is_sensitive());
        assert_eq!(
            widgets.fix_button.accessible_role(),
            gtk::AccessibleRole::Button
        );
        assert_eq!(
            widgets.close_button.accessible_role(),
            gtk::AccessibleRole::Button
        );
    }

    #[test]
    #[ignore = "requires graphical GTK session; run documented GTK component gate"]
    fn phase_testing_gtk_background_feedback_surface_is_persistent_and_semantic() {
        gtk::init().expect("GTK component gate requires available display");
        adw::init().expect("libadwaita must initialize in GTK component gate");
        let display = gtk::gdk::Display::default().expect("GTK display must be available");
        crate::iconography::register(&display);
        let application = adw::Application::builder()
            .application_id("io.github.rodriguezcappsec.Floe.BackgroundFeedbackTest")
            .build();
        application
            .register(None::<&gio::Cancellable>)
            .expect("component-test application must register before creating window");
        let widgets = build(
            &application,
            &[],
            Appearance::for_preset(AppearancePreset::Native),
            ViewPreferences::default(),
        );
        assert_eq!(
            widgets.background_feedback_revealer.accessible_role(),
            gtk::AccessibleRole::Group
        );
        assert!(!widgets.background_feedback_revealer.reveals_child());
        assert!(widgets.background_feedback_list.first_child().is_none());

        let row = gtk::Label::new(Some("Scanning locally with ClamAV"));
        widgets.background_feedback_list.append(&row);
        widgets.background_feedback_revealer.set_reveal_child(true);
        assert!(widgets.background_feedback_revealer.reveals_child());
        assert!(widgets.background_feedback_list.first_child().is_some());
    }

    #[test]
    #[ignore = "requires graphical GTK session; run documented GTK component gate"]
    fn phase_18v_ui_gtk_result_dialog_has_semantic_controls() {
        gtk::init().expect("GTK component gate requires available display");
        adw::init().expect("libadwaita must initialize in GTK component gate");
        let presentation = crate::verified_copy_executor::VerifiedCopyPresentation {
            title: "Copy retained without verification".to_owned(),
            detail: "The destination remains unverified.".to_owned(),
            notice: "SHA-256 equality does not prove authenticity.",
            retry_enabled: true,
        };
        let widgets = build_verified_copy_result_dialog(&presentation);
        assert_eq!(widgets.dialog.title(), "Copy and Verify Result");
        assert_eq!(
            widgets.retry_button.accessible_role(),
            gtk::AccessibleRole::Button
        );
        assert_eq!(
            widgets.close_button.accessible_role(),
            gtk::AccessibleRole::Button
        );
        assert!(widgets.retry_button.is_visible());
    }

    #[test]
    fn phase_18w_ui_exposes_verified_removable_action_without_replacing_copy() {
        assert!(FILE_CONTEXT_ACTIONS.contains(&(
            "Verified Removable Transfer…",
            "win.verified-removable-transfer",
        )));
        assert!(FILE_CONTEXT_ACTIONS.contains(&("Copy", "win.copy")));
        assert!(FILE_CONTEXT_ACTIONS.contains(&("Copy and Verify…", "win.copy-and-verify")));
    }
}
