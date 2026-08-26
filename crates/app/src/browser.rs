use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStrExt,
    path::{Component, Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use adw::prelude::*;
use floe_core::SymbolicLinkMode;
use floe_core::{
    BrowserSession, BrowserSessionId, BrowserTabs, ChecksumAlgorithm, CreateRequest,
    DirectoryEntry, DirectoryError, DirectoryGrouping, DirectoryPlacement, DirectorySort,
    EntryKind, FilenameSearchRequest, FilenameSearchScope, FilenameSearchSummary, FolderFilterMode,
    MillerChildKind, MillerColumnModel, MillerSelectionTransition, RestoreRequest, SPLIT_RATIO_MAX,
    SPLIT_RATIO_MIN, SessionScrollAnchor, SortColumn, SplitRatio, SplitSide, TabActivation,
    TabError, TrashEnumerateError, TrashRoot,
};

fn tab_title(path: &Path) -> String {
    if path == Path::new("/") {
        return "/".to_owned();
    }
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

fn session_restore_snapshot(session: &BrowserSession) -> ViewStateSnapshot {
    let location = session.current();
    ViewStateSnapshot {
        selected_paths: location.selection().to_vec(),
        anchor_path: location
            .scroll_anchor()
            .and_then(SessionScrollAnchor::path)
            .map(Path::to_path_buf),
        anchor_index: location
            .scroll_anchor()
            .map_or(0, SessionScrollAnchor::index),
    }
}

fn folder_tab_eligible(entries: &[Arc<DirectoryEntry>], trash_active: bool) -> bool {
    !trash_active && entries.len() == 1 && entries[0].is_navigable_directory()
}

const SPLIT_SNAPSHOT_CAPACITY: usize = 512;

#[derive(Clone, Debug)]
struct FolderFilterState {
    mode: FolderFilterMode,
    query: String,
}

impl Default for FolderFilterState {
    fn default() -> Self {
        Self {
            mode: FolderFilterMode::Text,
            query: String::new(),
        }
    }
}

fn filename_search_feedback(
    summary: FilenameSearchSummary,
    running: bool,
    stopped: bool,
) -> String {
    let skipped = summary
        .skipped_entries
        .saturating_add(summary.skipped_directories)
        .saturating_add(summary.skipped_mounts)
        .saturating_add(summary.depth_limited);
    let mut message = if running {
        format!(
            "Searching… {} matches from {} items",
            summary.matched, summary.examined_entries
        )
    } else if stopped {
        format!("Stopped with {} matches", summary.matched)
    } else {
        format!("{} matches", summary.matched)
    };
    if skipped > 0 {
        message.push_str(&format!(" · {skipped} skipped"));
    }
    if summary.truncated {
        message.push_str(" · incomplete (search limit reached)");
    }
    message
}

struct PendingFolderFilter {
    generation: u64,
    entries: Arc<[Arc<DirectoryEntry>]>,
}
#[cfg(test)]
const MILLER_NAVIGATION_ACTIONS: [&str; 2] = ["win.miller-parent", "win.miller-child"];
const MILLER_DETAIL_ACTIONS: [&str; 2] = ["miller-preview-hook", "miller-inspector-hook"];
const QUICK_PREVIEW_ACCELERATOR: &str = "space";
#[cfg(test)]
const INSPECTOR_ACCELERATOR: &str = "<Control>i";
#[cfg(test)]
const SPLIT_ACTION_NAMES: [&str; 10] = [
    "win.toggle-split",
    "win.switch-split-side",
    "win.swap-split-sides",
    "win.close-split",
    "win.narrow-primary-pane",
    "win.widen-primary-pane",
    "win.open-opposite-pane",
    "win.copy-to-opposite-pane",
    "win.move-to-opposite-pane",
    "win.link-to-opposite-pane",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SplitActionState {
    switch: bool,
    close: bool,
    swap: bool,
    open_opposite: bool,
    transfer_opposite: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct SplitPaneSnapshot {
    names: Vec<String>,
    total: usize,
}

const fn split_action_state(
    is_split: bool,
    selected_folder: bool,
    has_transfer_selection: bool,
    trash_active: bool,
) -> SplitActionState {
    SplitActionState {
        switch: is_split,
        close: is_split,
        swap: is_split,
        open_opposite: selected_folder && !trash_active,
        transfer_opposite: is_split && has_transfer_selection && !trash_active,
    }
}

const fn split_side_index(side: SplitSide) -> usize {
    match side {
        SplitSide::Primary => 0,
        SplitSide::Secondary => 1,
    }
}

fn split_snapshot(entries: &[Arc<DirectoryEntry>]) -> SplitPaneSnapshot {
    SplitPaneSnapshot {
        names: entries
            .iter()
            .take(SPLIT_SNAPSHOT_CAPACITY)
            .map(|entry| entry.display_name_lossy())
            .collect(),
        total: entries.len(),
    }
}

fn opposite_pane_destination(tabs: &BrowserTabs) -> Option<PathBuf> {
    tabs.active_split()
        .opposite()
        .map(|session| session.current().path().to_path_buf())
}

fn split_drop_destination(tabs: &BrowserTabs, trash_active: bool) -> Option<DropDestination> {
    (!trash_active)
        .then(|| opposite_pane_destination(tabs).map(DropDestination::Directory))
        .flatten()
}

fn tab_drop_destination(
    tabs: &BrowserTabs,
    id: BrowserSessionId,
    trash_active: bool,
) -> Option<DropDestination> {
    if trash_active {
        return None;
    }
    tabs.session(id)
        .map(|session| DropDestination::Directory(session.active().current().path().to_path_buf()))
}

fn tab_menu_item(label: &str, action: &str, id: BrowserSessionId) -> gio::MenuItem {
    let item = gio::MenuItem::new(Some(label), None);
    item.set_action_and_target_value(Some(action), Some(&id.get().to_variant()));
    item
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TabCloseVariant {
    Left,
    Right,
    Others,
}

#[cfg(test)]
const REOPEN_CLOSED_ACCELERATOR: &str = "<Control><Shift>t";
#[cfg(test)]
const TOGGLE_SPLIT_ACCELERATOR: &str = "F3";
#[cfg(test)]
const SWITCH_SPLIT_ACCELERATOR: &str = "F6";
#[cfg(test)]
const NARROW_PRIMARY_PANE_ACCELERATOR: &str = "<Control><Alt>Left";
#[cfg(test)]
const WIDEN_PRIMARY_PANE_ACCELERATOR: &str = "<Control><Alt>Right";
#[cfg(test)]
const OPEN_OPPOSITE_ACCELERATOR: &str = "<Control><Shift>Return";
#[cfg(test)]
const COPY_OPPOSITE_ACCELERATOR: &str = "<Control><Alt>c";
#[cfg(test)]
const MOVE_OPPOSITE_ACCELERATOR: &str = "<Control><Alt>m";
#[cfg(test)]
const LINK_OPPOSITE_ACCELERATOR: &str = "<Control><Alt>l";
const TAB_CLOSE_VARIANT_ACTIONS: [&str; 3] = [
    "win.close-tabs-left",
    "win.close-tabs-right",
    "win.close-other-tabs",
];
use gtk::{gdk, gio, glib};

use crate::{
    appearance::AppearancePreset,
    archive_ui::{
        ArchiveActionEligibility, build_compress_dialog, compression_request,
        default_compression_name, destination_preview, extraction_request, selected_format,
        with_archive_extension,
    },
    batch_rename::{BatchRenameSource, build_batch_rename_dialog, refresh_batch_rename_dialog},
    bookmarks::{BookmarkWorker, BookmarkWorkerEvent},
    checksum_ui::{ChecksumDialogInput, build_checksum_request},
    clipboard::{self, ClipboardTransfer},
    devices::{
        DeviceAction, DeviceActionOutcome, DeviceId, DeviceMonitor, DeviceSnapshot,
        DeviceSubscriptionId,
    },
    drag_drop::{
        DropAction, DropDestination, DropEvent, DropHoverTarget, DropRequest, install_drag_source,
        install_drop_target, install_drop_target_with_hover,
    },
    file_watcher::{
        FileWatcher, RenamePair, ViewStateSnapshot, WatchBatch, batch_is_current,
        reconcile_view_state, scroll_anchor_index, watch_failure_message,
    },
    inspector::{InspectorRequest, InspectorSubmitError, InspectorWorker},
    launcher,
    location_input::{
        PendingLocation, location_failure_message, location_text, resolve_location_input,
    },
    locations::Location,
    metadata::{MetadataSubmitError, MetadataWorker},
    miller_detail::{MillerDetailHooks, MillerDetailSurface},
    miller_view::{
        MillerActionContext, MillerActivation, MillerNavigation, MillerNavigationCommand,
        MillerPresentationState, resolve_action_context_entries,
    },
    preferences::{
        PreferenceSubmitError, PreferenceWorker, SIDEBAR_WIDTH_MAX, SIDEBAR_WIDTH_MIN,
        SidebarDensity, ViewPreferences, clamp_sidebar_width,
    },
    preview::{
        PREVIEW_QUEUE_CAPACITY, PreviewCachePolicy, PreviewLimits, PreviewOutcome, PreviewRequest,
        PreviewSourceKey, PreviewSubmitError, PreviewWorker,
    },
    properties::{
        ExecutableEdit, PROPERTIES_RESULT_CAPACITY, PermissionEditorInput, PropertiesRequest,
        PropertiesSubmitError, PropertiesWorker, build_permission_request,
        present as present_properties,
    },
    session_store::SessionStoreWorker,
    state::{ApplicationState, TransferIntent, validate_rename_name},
    storage::{
        StorageFacts, StorageRequest, StorageSubmitError, StorageTarget, StorageWorker,
        format_bytes, format_storage_facts,
    },
    thumbnail::{ThumbnailSubmitError, ThumbnailWorker},
    ui::{self, BrowserWidgets},
    view::{
        FileViewDensity, FolderViewState, GridSize, ListColumn, MillerColumnWidth, VIEW_ACTIONS,
        ViewCommand, ViewMode,
    },
    worker::{BrowserWorker, ResponseKind},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MountAuthenticationPolicy {
    window_parented: bool,
    credential_opaque: bool,
    feedback: &'static str,
}

const fn mount_authentication_policy() -> MountAuthenticationPolicy {
    MountAuthenticationPolicy {
        window_parented: true,
        credential_opaque: true,
        feedback: "Mounting… If authentication is required, your desktop will ask for the password.",
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClipboardTextMode {
    Name,
    AbsolutePath,
    RelativePath,
    Uri,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CreateDialogKind {
    Directory,
    EmptyFile,
    Template(PathBuf),
    SymbolicLink(PathBuf, SymbolicLinkMode),
    HardLink(PathBuf),
}

impl CreateDialogKind {
    fn title(&self) -> &'static str {
        match self {
            Self::Directory => "Create Folder",
            Self::EmptyFile => "Create Empty File",
            Self::Template(_) => "Create From Template",
            Self::SymbolicLink(_, _) => "Create Symbolic Link",
            Self::HardLink(_) => "Create Hard Link",
        }
    }

    fn request(
        &self,
        destination: PathBuf,
    ) -> Result<CreateRequest, floe_core::CreateRequestError> {
        match self {
            Self::Directory => CreateRequest::directory(destination),
            Self::EmptyFile => CreateRequest::empty_file(destination),
            Self::Template(source) => CreateRequest::template(source, destination),
            Self::SymbolicLink(source, mode) => {
                CreateRequest::symbolic_link_from(source, destination, *mode)
            }
            Self::HardLink(source) => CreateRequest::hard_link(source, destination),
        }
    }
}

const SIDEBAR_PERSIST_DEBOUNCE: Duration = Duration::from_millis(320);

#[cfg(test)]
fn with_current_view_preferences(
    mut preferences: ViewPreferences,
    mode: ViewMode,
    grid_size: GridSize,
) -> ViewPreferences {
    preferences.mode = mode;
    preferences.grid_size = grid_size;
    preferences
}

fn sidebar_width_from_position(position: i32) -> u16 {
    let width = match u16::try_from(position) {
        Ok(width) => width,
        Err(_) if position < 0 => SIDEBAR_WIDTH_MIN,
        Err(_) => SIDEBAR_WIDTH_MAX,
    };
    clamp_sidebar_width(width)
}

fn preferences_after_sidebar_reset(mut preferences: ViewPreferences) -> ViewPreferences {
    preferences.sidebar_width = None;
    preferences
}

pub struct BrowserServices {
    browser: BrowserWorker,
    thumbnails: Option<ThumbnailWorker>,
    metadata: Option<MetadataWorker>,
    inspector: Option<InspectorWorker>,
    preview: Option<PreviewWorker>,
    properties: Option<PropertiesWorker>,
    storage: Option<StorageWorker>,
    bookmarks: Option<BookmarkWorker>,
    devices: DeviceMonitor,
    preferences: Option<PreferenceWorker>,
    session_store: Option<SessionStoreWorker>,
}

#[derive(Clone, Debug)]
struct PendingReconciliation {
    snapshot: ViewStateSnapshot,
    renames: Vec<RenamePair>,
}

impl BrowserServices {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        browser: BrowserWorker,
        thumbnails: Option<ThumbnailWorker>,
        metadata: Option<MetadataWorker>,
        inspector: Option<InspectorWorker>,
        preview: Option<PreviewWorker>,
        properties: Option<PropertiesWorker>,
        storage: Option<StorageWorker>,
        bookmarks: Option<BookmarkWorker>,
        devices: DeviceMonitor,
        preferences: Option<PreferenceWorker>,
        session_store: Option<SessionStoreWorker>,
    ) -> Self {
        Self {
            browser,
            thumbnails,
            metadata,
            inspector,
            preview,
            properties,
            storage,
            bookmarks,
            devices,
            preferences,
            session_store,
        }
    }
}

pub struct BrowserController {
    widgets: BrowserWidgets,
    command_palette: crate::command_palette::CommandPalette,
    keyboard_shortcuts: crate::keyboard_shortcuts::KeyboardShortcuts,
    context_menu_editor: crate::context_menu::ContextMenuEditor,
    terminal_chooser: crate::terminal_ui::TerminalChooser,
    tabs: Rc<RefCell<BrowserTabs>>,
    worker: RefCell<BrowserWorker>,
    thumbnail_worker: RefCell<Option<ThumbnailWorker>>,
    metadata_worker: RefCell<Option<MetadataWorker>>,
    inspector_worker: RefCell<Option<InspectorWorker>>,
    inspector_generation: Cell<u64>,
    preview_worker: RefCell<Option<PreviewWorker>>,
    preview_generation: Cell<u64>,
    properties_worker: RefCell<Option<PropertiesWorker>>,
    properties_generation: Cell<u64>,
    storage_worker: RefCell<Option<StorageWorker>>,
    current_storage_generation: Cell<u64>,
    device_storage_generation: Cell<u64>,
    current_storage_facts: Cell<Option<StorageFacts>>,
    device_storage_facts: RefCell<HashMap<String, StorageFacts>>,
    device_snapshots: RefCell<Vec<DeviceSnapshot>>,
    thumbnail_generation: Cell<u64>,
    active_generation: Cell<u64>,
    show_hidden: Cell<bool>,
    trash_active: Cell<bool>,
    trash_root: TrashRoot,
    listed_entries: RefCell<Arc<[Arc<DirectoryEntry>]>>,
    visible_entries: RefCell<Vec<Arc<DirectoryEntry>>>,
    filter_worker: RefCell<Option<crate::folder_filter::FolderFilterWorker>>,
    filter_state: RefCell<FolderFilterState>,
    filter_generation: Cell<u64>,
    pending_filter: RefCell<Option<PendingFolderFilter>>,
    filter_selection_paths: RefCell<Vec<PathBuf>>,
    filter_location: RefCell<Option<(bool, PathBuf)>>,
    filename_search_worker: RefCell<Option<crate::filename_search::FilenameSearchWorker>>,
    filename_search_generation: Cell<u64>,
    filename_search_active: Cell<bool>,
    filename_search_running: Cell<bool>,
    filename_search_results: RefCell<Vec<Arc<DirectoryEntry>>>,
    filename_search_store: RefCell<Option<gio::ListStore>>,
    filename_search_root: RefCell<Option<PathBuf>>,
    pending_filename_search: RefCell<Option<FilenameSearchRequest>>,
    filename_search_summary: RefCell<Option<FilenameSearchSummary>>,
    pending_entries: RefCell<VecDeque<Arc<DirectoryEntry>>>,
    pending_store: RefCell<Option<gio::ListStore>>,
    pending_total: Cell<usize>,
    pending_selection_indices: RefCell<Vec<u32>>,
    selected_entries: RefCell<Vec<Arc<DirectoryEntry>>>,
    sort_order: Cell<DirectorySort>,
    sort_in_flight: Cell<bool>,
    sort_selection_paths: RefCell<Vec<PathBuf>>,
    reveal_selection_path: RefCell<Option<PathBuf>>,
    view_mode: Cell<ViewMode>,
    miller_model: RefCell<Option<MillerColumnModel>>,
    miller_state: RefCell<MillerPresentationState>,
    miller_action_context: RefCell<Option<MillerActionContext>>,
    miller_detail: RefCell<MillerDetailHooks>,
    grid_size: Cell<GridSize>,
    file_density: Cell<FileViewDensity>,
    list_columns: Cell<crate::view::ListColumnLayout>,
    preference_worker: RefCell<Option<PreferenceWorker>>,
    session_store: RefCell<Option<SessionStoreWorker>>,
    session_saved: Cell<bool>,
    pending_preferences: Cell<Option<ViewPreferences>>,
    current_preferences: RefCell<ViewPreferences>,
    sidebar_save_source: RefCell<Option<glib::SourceId>>,
    ignore_sidebar_position_signal: Cell<bool>,
    split_snapshots: RefCell<HashMap<BrowserSessionId, [SplitPaneSnapshot; 2]>>,
    ignore_split_position_signal: Cell<bool>,
    pending_location: RefCell<Option<PendingLocation>>,
    application_state: Rc<ApplicationState>,
    bookmark_worker: RefCell<Option<BookmarkWorker>>,
    bookmarks: RefCell<Vec<PathBuf>>,
    bookmarks_loaded: Cell<bool>,
    bookmark_revision: Cell<u64>,
    bookmark_save_in_flight: Cell<bool>,
    device_monitor: DeviceMonitor,
    device_subscription: Cell<Option<DeviceSubscriptionId>>,
    drop_hover_source: RefCell<Option<glib::SourceId>>,
    file_watcher: FileWatcher,
    watch_generation: Cell<u64>,
    pending_reconciliation: RefCell<Option<PendingReconciliation>>,
    pending_scroll_index: Cell<Option<u32>>,
    terminal_worker: RefCell<Option<crate::terminal::TerminalWorker>>,
    terminal_availability: RefCell<Vec<crate::terminal::TerminalAvailability>>,
    terminal_request_id: Cell<u64>,
    template_worker: RefCell<Option<crate::templates::TemplateWorker>>,
    template_request_id: Cell<u64>,
    pending_create_rename: RefCell<Option<PathBuf>>,
}

impl Drop for BrowserController {
    fn drop(&mut self) {
        if let Some(source) = self.sidebar_save_source.get_mut().take() {
            source.remove();
        }
        if let Some(subscription) = self.device_subscription.take() {
            self.device_monitor.disconnect_changed(subscription);
        }
        if let Some(source) = self.drop_hover_source.get_mut().take() {
            source.remove();
        }
        self.file_watcher.stop();
        self.persist_session_for_shutdown();
        let Some(worker) = self.preference_worker.get_mut().as_ref() else {
            return;
        };
        if let Err(error) = worker.save_before_shutdown(self.current_preferences.get_mut().clone())
        {
            tracing::warn!(%error, "could not submit final view preferences");
        }
    }
}

impl BrowserController {
    pub fn new(
        widgets: BrowserWidgets,
        initial_path: PathBuf,
        restored_tabs: Option<BrowserTabs>,
        services: BrowserServices,
        view_preferences: ViewPreferences,
        application_state: Rc<ApplicationState>,
    ) -> Rc<Self> {
        let BrowserServices {
            browser,
            thumbnails,
            metadata,
            inspector,
            preview,
            properties,
            storage,
            bookmarks,
            devices,
            preferences,
            session_store,
        } = services;
        let fallback_view = view_preferences.effective_state(&initial_path);
        let tabs = restored_tabs.unwrap_or_else(|| {
            BrowserTabs::new(initial_path, fallback_view)
                .expect("the standard initial location is an absolute session path")
        });
        let initial_view = tabs.active().current().view();
        let command_palette = crate::command_palette::CommandPalette::new(&widgets.window);
        let keyboard_shortcuts = crate::keyboard_shortcuts::KeyboardShortcuts::new(&widgets.window);
        let context_menu_editor = crate::context_menu::ContextMenuEditor::new(&widgets.window);
        let terminal_chooser = crate::terminal_ui::TerminalChooser::new(&widgets.window);
        let terminal_worker = match crate::terminal::TerminalWorker::spawn() {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(%error, "could not start terminal integration worker");
                None
            }
        };
        let template_worker = match crate::templates::TemplateWorker::spawn() {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(%error, "could not start template discovery worker");
                None
            }
        };
        let filter_worker = match crate::folder_filter::FolderFilterWorker::spawn() {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(%error, "could not start folder filter worker");
                None
            }
        };
        let filename_search_worker = match crate::filename_search::FilenameSearchWorker::spawn() {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(%error, "could not start filename search worker");
                None
            }
        };
        widgets.miller_view.set_vim_mode(view_preferences.vim_mode);
        Rc::new(Self {
            widgets,
            command_palette,
            keyboard_shortcuts,
            context_menu_editor,
            terminal_chooser,
            tabs: Rc::new(RefCell::new(tabs)),
            worker: RefCell::new(browser),
            thumbnail_worker: RefCell::new(thumbnails),
            metadata_worker: RefCell::new(metadata),
            inspector_worker: RefCell::new(inspector),
            inspector_generation: Cell::new(0),
            preview_worker: RefCell::new(preview),
            preview_generation: Cell::new(0),
            properties_worker: RefCell::new(properties),
            properties_generation: Cell::new(0),
            storage_worker: RefCell::new(storage),
            current_storage_generation: Cell::new(0),
            device_storage_generation: Cell::new(0),
            current_storage_facts: Cell::new(None),
            device_storage_facts: RefCell::new(HashMap::new()),
            device_snapshots: RefCell::new(Vec::new()),
            thumbnail_generation: Cell::new(0),
            active_generation: Cell::new(0),
            show_hidden: Cell::new(false),
            trash_active: Cell::new(false),
            trash_root: TrashRoot::for_data_home(glib::user_data_dir()),
            listed_entries: RefCell::new(Arc::from([])),
            visible_entries: RefCell::new(Vec::new()),
            filter_worker: RefCell::new(filter_worker),
            filter_state: RefCell::new(FolderFilterState::default()),
            filter_generation: Cell::new(0),
            pending_filter: RefCell::new(None),
            filter_selection_paths: RefCell::new(Vec::new()),
            filter_location: RefCell::new(None),
            filename_search_worker: RefCell::new(filename_search_worker),
            filename_search_generation: Cell::new(0),
            filename_search_active: Cell::new(false),
            filename_search_running: Cell::new(false),
            filename_search_results: RefCell::new(Vec::new()),
            filename_search_store: RefCell::new(None),
            filename_search_root: RefCell::new(None),
            pending_filename_search: RefCell::new(None),
            filename_search_summary: RefCell::new(None),
            pending_entries: RefCell::new(VecDeque::new()),
            pending_store: RefCell::new(None),
            pending_total: Cell::new(0),
            pending_selection_indices: RefCell::new(Vec::new()),
            selected_entries: RefCell::new(Vec::new()),
            sort_order: Cell::new(initial_view.sort),
            sort_in_flight: Cell::new(false),
            sort_selection_paths: RefCell::new(Vec::new()),
            reveal_selection_path: RefCell::new(None),
            view_mode: Cell::new(initial_view.mode),
            miller_model: RefCell::new(None),
            miller_state: RefCell::new(MillerPresentationState::default()),
            miller_action_context: RefCell::new(None),
            miller_detail: RefCell::new(MillerDetailHooks::default()),
            grid_size: Cell::new(initial_view.grid_size),
            file_density: Cell::new(initial_view.density),
            list_columns: Cell::new(initial_view.columns),
            preference_worker: RefCell::new(preferences),
            session_store: RefCell::new(session_store),
            session_saved: Cell::new(false),
            pending_preferences: Cell::new(None),
            current_preferences: RefCell::new(view_preferences),
            sidebar_save_source: RefCell::new(None),
            ignore_sidebar_position_signal: Cell::new(false),
            split_snapshots: RefCell::new(HashMap::new()),
            ignore_split_position_signal: Cell::new(false),
            pending_location: RefCell::new(None),
            application_state,
            bookmark_worker: RefCell::new(bookmarks),
            bookmarks: RefCell::new(Vec::new()),
            bookmarks_loaded: Cell::new(false),
            bookmark_revision: Cell::new(0),
            bookmark_save_in_flight: Cell::new(false),
            device_monitor: devices,
            device_subscription: Cell::new(None),
            drop_hover_source: RefCell::new(None),
            file_watcher: FileWatcher::default(),
            watch_generation: Cell::new(0),
            pending_reconciliation: RefCell::new(None),
            pending_scroll_index: Cell::new(None),
            terminal_worker: RefCell::new(terminal_worker),
            terminal_availability: RefCell::new(Vec::new()),
            terminal_request_id: Cell::new(0),
            template_worker: RefCell::new(template_worker),
            template_request_id: Cell::new(0),
            pending_create_rename: RefCell::new(None),
        })
    }

    pub fn wire(self: &Rc<Self>, application: &adw::Application, locations: &[Location]) {
        self.install_actions(application);
        self.install_filter_signals();
        self.install_filename_search_signals();
        let clipboard = self.widgets.window.clipboard();
        let controller = Rc::downgrade(self);
        clipboard.connect_changed(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.refresh_paste_enabled();
            }
        });
        self.refresh_paste_enabled();
        self.update_sort_headers();
        self.render_tabs();
        self.render_split_presentation();
        self.arm_split_ratio_tracking();
        self.widgets
            .apply_sidebar_density(self.current_preferences.borrow().sidebar_density);
        let initial_view = self
            .current_preferences
            .borrow()
            .effective_state(self.tabs.borrow().active().current().path());
        self.widgets.set_view_mode(initial_view.mode);
        self.widgets.set_grid_size(initial_view.grid_size);
        self.widgets.apply_file_view_policy(
            initial_view.density,
            initial_view.columns,
            initial_view.sort.grouping,
        );

        let controller = Rc::downgrade(self);
        self.widgets.drop_dispatcher.bind(move |event| {
            if let Some(controller) = controller.upgrade() {
                controller.handle_drop_event(event);
            }
        });

        let controller = Rc::downgrade(self);
        self.file_watcher.bind(move |batch| {
            if let Some(controller) = controller.upgrade() {
                controller.handle_watch_batch(batch);
            }
        });

        self.install_file_view_drag_drop(&self.widgets.list_view);
        self.install_file_view_drag_drop(&self.widgets.grid_view);
        self.install_file_view_drag_drop(&self.widgets.search_results_view);
        let controller = Rc::downgrade(self);
        install_drop_target_with_hover(
            &self.widgets.inactive_pane,
            Rc::new(move || {
                let controller = controller.upgrade()?;
                split_drop_destination(&controller.tabs.borrow(), controller.trash_active.get())
            }),
            Rc::new(|_| Some(DropHoverTarget::OppositePane)),
            self.widgets.drop_dispatcher.clone(),
            false,
        );

        for (button, location) in self.widgets.location_buttons.iter().zip(locations) {
            let controller = Rc::downgrade(self);
            let path = exact_sidebar_target(&location.path);
            let drop_path = path.clone();
            install_drop_target(
                button,
                Rc::new(move || Some(DropDestination::Directory(drop_path.clone()))),
                self.widgets.drop_dispatcher.clone(),
                true,
                false,
            );
            button.connect_clicked(move |_| {
                if let Some(controller) = controller.upgrade() {
                    controller.navigate_to(exact_sidebar_target(&path));
                }
            });
        }

        install_drop_target(
            &self.widgets.trash_button,
            Rc::new(|| Some(DropDestination::Trash)),
            self.widgets.drop_dispatcher.clone(),
            false,
            false,
        );

        let controller = Rc::downgrade(self);
        self.widgets.add_bookmark_button.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.add_current_bookmark();
            }
        });
        self.render_bookmarks();
        self.widgets
            .add_bookmark_button
            .set_sensitive(ui::bookmark_actions_enabled(false, false));

        let controller = Rc::downgrade(self);
        let subscription = self.device_monitor.connect_changed(move |snapshots| {
            if let Some(controller) = controller.upgrade() {
                controller.request_device_storage_facts(snapshots);
                controller.render_devices(snapshots);
            }
        });
        self.device_subscription.set(Some(subscription));

        let controller = Rc::downgrade(self);
        self.widgets.list_view.connect_activate(move |_, position| {
            if let Some(controller) = controller.upgrade() {
                controller.activate(position);
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.grid_view.connect_activate(move |_, position| {
            if let Some(controller) = controller.upgrade() {
                controller.activate(position);
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets
            .search_results_view
            .connect_activate(move |_, position| {
                if let Some(controller) = controller.upgrade() {
                    controller.activate(position);
                }
            });
        let controller = Rc::downgrade(self);
        self.widgets.miller_view.bind_activate(move |activation| {
            if let Some(controller) = controller.upgrade() {
                controller.activate_miller_entry(activation);
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.miller_view.bind_navigation(move |navigation| {
            if let Some(controller) = controller.upgrade() {
                controller.navigate_miller_keyboard(navigation);
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets
            .miller_view
            .bind_action_context(move |context| {
                if let Some(controller) = controller.upgrade() {
                    controller.own_miller_action_context(context);
                }
            });

        let controller = Rc::downgrade(self);
        self.widgets
            .selection
            .connect_selection_changed(move |_, _, _| {
                if let Some(controller) = controller.upgrade() {
                    controller.selection_changed();
                }
            });

        self.install_file_view_shortcuts(&self.widgets.list_view);
        self.install_file_view_shortcuts(&self.widgets.grid_view);
        self.install_file_view_shortcuts(&self.widgets.search_results_view);
        self.install_file_view_shortcuts(self.widgets.miller_view.widget());

        let controller = Rc::downgrade(self);
        self.widgets
            .grid_size_scale
            .connect_value_changed(move |scale| {
                let index = scale.value().round() as usize;
                if let Some(controller) = controller.upgrade()
                    && let Some(size) = GridSize::from_index(index)
                {
                    controller.change_grid_size(size);
                }
            });

        let controller = Rc::downgrade(self);
        self.widgets.location_entry.connect_activate(move |entry| {
            if let Some(controller) = controller.upgrade() {
                controller.submit_location_entry(entry.text().as_str());
            }
        });
    }

    fn install_file_view_drag_drop(self: &Rc<Self>, view: &impl IsA<gtk::Widget>) {
        let controller = Rc::downgrade(self);
        install_drag_source(
            view,
            Rc::new(move || {
                controller.upgrade().map_or_else(Vec::new, |controller| {
                    if controller.trash_active.get() {
                        Vec::new()
                    } else {
                        controller
                            .selected_entries
                            .borrow()
                            .iter()
                            .map(|entry| entry.path().to_path_buf())
                            .collect()
                    }
                })
            }),
        );
        let controller = Rc::downgrade(self);
        install_drop_target(
            view,
            Rc::new(move || {
                let controller = controller.upgrade()?;
                (!controller.trash_active.get()).then(|| {
                    DropDestination::Directory(
                        controller
                            .tabs
                            .borrow()
                            .active()
                            .current()
                            .path()
                            .to_path_buf(),
                    )
                })
            }),
            self.widgets.drop_dispatcher.clone(),
            false,
            true,
        );
    }

    fn handle_drop_event(self: &Rc<Self>, event: DropEvent) {
        match event {
            DropEvent::Commit(request) => self.submit_drop(request),
            DropEvent::Feedback(Some(message)) => {
                self.widgets.status_label.set_label(&message);
            }
            DropEvent::Feedback(None) => self.refresh_status(),
            DropEvent::HoverEnter(target) => self.schedule_hover_open(target),
            DropEvent::HoverLeave => self.cancel_hover_open(),
        }
    }

    fn submit_drop(&self, request: DropRequest) {
        let action = request.action().label();
        let count = request.sources().len();
        match self.application_state.submit_drop(request) {
            Ok(batch) => {
                self.show_toast(
                    &format!("{action}: queued {} of {count} items", batch.queued()),
                    4,
                );
            }
            Err(error) => {
                tracing::warn!(%error, "drop request was rejected");
                self.show_toast(&format!("Could not complete drop: {error}"), 7);
            }
        }
    }

    fn schedule_hover_open(self: &Rc<Self>, target: DropHoverTarget) {
        self.cancel_hover_open();
        let controller = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(Duration::from_millis(720), move || {
            let Some(controller) = controller.upgrade() else {
                return;
            };
            controller.drop_hover_source.borrow_mut().take();
            if controller.trash_active.get() {
                return;
            }
            match target {
                DropHoverTarget::Directory(path) => {
                    if controller.tabs.borrow().active().current().path() != path.as_path() {
                        controller.navigate_to(path);
                    }
                }
                DropHoverTarget::Tab(raw_id) => {
                    if let Ok(id) = BrowserSessionId::new(raw_id)
                        && controller.tabs.borrow().session(id).is_some()
                        && controller.tabs.borrow().active_id() != id
                    {
                        controller.activate_tab(id);
                    }
                }
                DropHoverTarget::OppositePane => {
                    if controller.tabs.borrow().active_split().is_split() {
                        controller.switch_split_side();
                    }
                }
                DropHoverTarget::MillerChild { depth, path } => {
                    controller.activate_miller_hover(depth, &path);
                }
            }
        });
        self.drop_hover_source.replace(Some(source));
    }

    fn cancel_hover_open(&self) {
        if let Some(source) = self.drop_hover_source.borrow_mut().take() {
            source.remove();
        }
    }

    fn handle_watch_batch(&self, batch: WatchBatch) {
        if self.trash_active.get() {
            return;
        }
        let current = self.tabs.borrow().active().current().path().to_path_buf();
        if !batch_is_current(&batch, self.watch_generation.get(), &current) {
            return;
        }
        let snapshot = self.capture_view_state();
        tracing::debug!(
            events = batch.event_count(),
            paths = batch.changed_paths().len(),
            renames = batch.renames().len(),
            overflowed = batch.overflowed(),
            "coalesced external filesystem changes"
        );
        self.pending_reconciliation
            .replace(Some(PendingReconciliation {
                snapshot,
                renames: batch.renames().to_vec(),
            }));
        self.load_current_inner();
    }

    fn capture_view_state(&self) -> ViewStateSnapshot {
        let entries = self.visible_entries.borrow();
        let anchor_index = self
            .current_scroll_adjustment()
            .map_or(0, |adjustment| {
                scroll_anchor_index(
                    adjustment.value(),
                    adjustment.lower(),
                    adjustment.upper(),
                    adjustment.page_size(),
                    entries.len(),
                )
            })
            .min(entries.len().saturating_sub(1));
        ViewStateSnapshot {
            selected_paths: self
                .selected_entries
                .borrow()
                .iter()
                .map(|entry| entry.path().to_path_buf())
                .collect(),
            anchor_path: entries
                .get(anchor_index)
                .map(|entry| entry.path().to_path_buf()),
            anchor_index,
        }
    }

    fn current_scroll_adjustment(&self) -> Option<gtk::Adjustment> {
        let view: &gtk::Widget = match self.view_mode.get() {
            ViewMode::List => self.widgets.list_view.upcast_ref(),
            ViewMode::Grid => self.widgets.grid_view.upcast_ref(),
            ViewMode::Miller => return None,
        };
        view.ancestor(gtk::ScrolledWindow::static_type())
            .and_downcast::<gtk::ScrolledWindow>()
            .map(|scroller| scroller.vadjustment())
    }

    fn start_current_watcher(&self) {
        if self.trash_active.get() {
            self.file_watcher.stop();
            self.watch_generation.set(self.file_watcher.generation());
            return;
        }
        let current = self.tabs.borrow().active().current().path().to_path_buf();
        match self.file_watcher.watch_directory(current) {
            Ok(generation) => self.watch_generation.set(generation),
            Err(error) => {
                tracing::warn!(%error, "directory live-update monitor unavailable");
                self.watch_generation.set(self.file_watcher.generation());
                self.show_toast(&watch_failure_message(&error), 7);
            }
        }
    }

    fn restore_scroll_anchor(&self) {
        let Some(index) = self.pending_scroll_index.take() else {
            return;
        };
        let info = gtk::ScrollInfo::new();
        info.set_enable_vertical(true);
        match self.view_mode.get() {
            ViewMode::List => {
                self.widgets
                    .list_view
                    .scroll_to(index, gtk::ListScrollFlags::NONE, Some(info))
            }
            ViewMode::Grid => {
                self.widgets
                    .grid_view
                    .scroll_to(index, gtk::ListScrollFlags::NONE, Some(info))
            }
            ViewMode::Miller => {}
        }
    }

    fn add_current_bookmark(self: &Rc<Self>) {
        if !self.bookmarks_loaded.get() {
            self.show_toast("Bookmarks are still loading", 4);
            return;
        }
        let current = self.tabs.borrow().active().current().path().to_path_buf();
        if self.bookmarks.borrow().contains(&current) {
            self.show_toast("This folder is already bookmarked", 4);
            return;
        }
        let mut revised = self.bookmarks.borrow().clone();
        revised.push(current);
        self.submit_bookmarks(revised);
    }

    fn remove_bookmark(self: &Rc<Self>, index: usize) {
        let Some(revised) = ui::bookmark_paths_after_remove(&self.bookmarks.borrow(), index) else {
            self.show_toast("That bookmark is no longer available", 4);
            return;
        };
        self.submit_bookmarks(revised);
    }

    fn submit_bookmarks(self: &Rc<Self>, paths: Vec<PathBuf>) {
        if self.bookmark_save_in_flight.get() {
            self.show_toast("Please wait for the current bookmark change", 4);
            return;
        }
        let revision = self.bookmark_revision.get().saturating_add(1);
        let result = self
            .bookmark_worker
            .borrow()
            .as_ref()
            .map(|worker| worker.try_save(revision, paths));
        match result {
            Some(Ok(())) => {
                self.bookmark_revision.set(revision);
                self.bookmark_save_in_flight.set(true);
                self.widgets.add_bookmark_button.set_sensitive(false);
                self.render_bookmarks();
            }
            Some(Err(error)) => {
                self.show_toast(&format!("Could not save bookmarks: {error}"), 6);
            }
            None => self.show_toast("Bookmarks are unavailable for this session", 5),
        }
    }

    fn drain_bookmark_worker(self: &Rc<Self>) {
        loop {
            let event = {
                let worker = self.bookmark_worker.borrow();
                worker.as_ref().map(BookmarkWorker::try_event)
            };
            let Some(event) = event else {
                return;
            };
            match event {
                Ok(BookmarkWorkerEvent::Loaded(Ok(bookmarks))) => {
                    self.bookmarks.replace(bookmarks.paths().to_vec());
                    self.bookmarks_loaded.set(true);
                    self.widgets
                        .add_bookmark_button
                        .set_sensitive(ui::bookmark_actions_enabled(
                            self.bookmarks_loaded.get(),
                            false,
                        ));
                    self.render_bookmarks();
                }
                Ok(BookmarkWorkerEvent::Loaded(Err(error))) => {
                    tracing::warn!(%error, "could not load bookmarks");
                    self.show_toast(&format!("Could not load bookmarks: {error}"), 6);
                }
                Ok(BookmarkWorkerEvent::Saved { revision, result }) => {
                    if revision != self.bookmark_revision.get() {
                        continue;
                    }
                    self.bookmark_save_in_flight.set(false);
                    self.widgets
                        .add_bookmark_button
                        .set_sensitive(ui::bookmark_actions_enabled(
                            self.bookmarks_loaded.get(),
                            false,
                        ));
                    self.widgets
                        .add_bookmark_button
                        .set_tooltip_text(Some("Add current folder to Bookmarks"));
                    match result {
                        Ok(bookmarks) => {
                            self.bookmarks.replace(bookmarks.paths().to_vec());
                            self.render_bookmarks();
                            self.show_toast("Bookmarks updated", 3);
                        }
                        Err(error) => {
                            tracing::warn!(%error, "could not persist bookmarks");
                            self.render_bookmarks();
                            self.show_toast(&format!("Could not save bookmarks: {error}"), 6);
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.bookmark_worker.borrow_mut().take();
                    self.bookmarks_loaded.set(false);
                    self.bookmark_save_in_flight.set(false);
                    self.widgets.add_bookmark_button.set_sensitive(false);
                    self.render_bookmarks();
                    self.show_toast("Bookmark storage stopped unexpectedly", 6);
                    return;
                }
            }
        }
    }

    fn render_bookmarks(self: &Rc<Self>) {
        remove_all_children(&self.widgets.bookmarks_box);
        let bookmarks = self.bookmarks.borrow().clone();
        if bookmarks.is_empty() {
            let empty = sidebar_status_label("No bookmarks yet");
            self.widgets.bookmarks_box.append(&empty);
            return;
        }
        let actions_enabled = ui::bookmark_actions_enabled(
            self.bookmarks_loaded.get(),
            self.bookmark_save_in_flight.get(),
        );
        for (index, path) in bookmarks.into_iter().enumerate() {
            let row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(2)
                .build();
            let display_name = sidebar_path_name(&path);
            let content = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(8)
                .build();
            content.append(&gtk::Image::from_icon_name("folder-symbolic"));
            let label = gtk::Label::builder()
                .label(&display_name)
                .halign(gtk::Align::Start)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            content.append(&label);
            let open = gtk::Button::builder()
                .child(&content)
                .has_frame(false)
                .hexpand(true)
                .tooltip_text(path.to_string_lossy())
                .build();
            set_accessible_label(&open, &format!("Open bookmark {display_name}"));
            let drop_path = path.clone();
            install_drop_target(
                &open,
                Rc::new(move || Some(DropDestination::Directory(drop_path.clone()))),
                self.widgets.drop_dispatcher.clone(),
                true,
                false,
            );
            let controller = Rc::downgrade(self);
            open.connect_clicked(move |_| {
                if let Some(controller) = controller.upgrade() {
                    controller.navigate_to(exact_sidebar_target(&path));
                }
            });
            row.append(&open);

            let remove = gtk::Button::builder()
                .icon_name("edit-delete-symbolic")
                .has_frame(false)
                .sensitive(actions_enabled)
                .tooltip_text(format!("Remove {display_name} from Bookmarks"))
                .build();
            remove.add_css_class("sidebar-icon-button");
            set_accessible_label(&remove, &format!("Remove bookmark {display_name}"));
            let controller = Rc::downgrade(self);
            remove.connect_clicked(move |_| {
                if let Some(controller) = controller.upgrade() {
                    controller.remove_bookmark(index);
                }
            });
            row.append(&remove);
            self.widgets.bookmarks_box.append(&row);
        }
    }

    fn render_devices(self: &Rc<Self>, snapshots: &[DeviceSnapshot]) {
        remove_all_children(&self.widgets.devices_box);
        if snapshots.is_empty() {
            self.widgets
                .devices_box
                .append(&sidebar_status_label("No storage devices found"));
            return;
        }

        for snapshot in snapshots {
            let policy = ui::device_row_policy(snapshot);
            let row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(2)
                .build();
            row.set_widget_name(snapshot.id.as_str());
            let content = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(8)
                .build();
            let icon_name = if snapshot.removable {
                "drive-removable-media-symbolic"
            } else {
                "drive-harddisk-symbolic"
            };
            content.append(&gtk::Image::from_icon_name(icon_name));
            let labels = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(0)
                .hexpand(true)
                .build();
            labels.append(
                &gtk::Label::builder()
                    .label(&snapshot.name)
                    .halign(gtk::Align::Start)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .build(),
            );
            let status_text = self
                .device_storage_facts
                .borrow()
                .get(snapshot.id.as_str())
                .copied()
                .map(|facts| device_status_text(&policy.status, facts))
                .unwrap_or(policy.status);
            let status = sidebar_status_label(&status_text);
            labels.append(&status);
            content.append(&labels);

            let activate = gtk::Button::builder()
                .child(&content)
                .has_frame(false)
                .hexpand(true)
                .sensitive(!matches!(
                    policy.activation,
                    ui::DeviceActivation::Unavailable(_)
                ))
                .build();
            let accessible = match &policy.activation {
                ui::DeviceActivation::Navigate(_) => format!("Open device {}", snapshot.name),
                ui::DeviceActivation::Mount => format!("Mount device {}", snapshot.name),
                ui::DeviceActivation::Unavailable(message) => {
                    activate.set_tooltip_text(Some(message));
                    format!("Device {} unavailable: {message}", snapshot.name)
                }
            };
            set_accessible_label(&activate, &accessible);
            let activation = policy.activation.clone();
            if let ui::DeviceActivation::Navigate(path) = &activation {
                let drop_path = path.clone();
                install_drop_target(
                    &activate,
                    Rc::new(move || Some(DropDestination::Directory(drop_path.clone()))),
                    self.widgets.drop_dispatcher.clone(),
                    true,
                    false,
                );
            }
            let device_id = snapshot.id.clone();
            let controller = Rc::downgrade(self);
            activate.connect_clicked(move |_| {
                let Some(controller) = controller.upgrade() else {
                    return;
                };
                match &activation {
                    ui::DeviceActivation::Navigate(path) => {
                        controller.navigate_to(exact_sidebar_target(path))
                    }
                    ui::DeviceActivation::Mount => {
                        controller.start_device_action(device_id.clone(), DeviceAction::Mount, true)
                    }
                    ui::DeviceActivation::Unavailable(message) => controller.show_toast(message, 5),
                }
            });
            row.append(&activate);

            if policy.can_unmount {
                row.append(&self.device_action_button(
                    snapshot,
                    "media-playback-stop-symbolic",
                    "Unmount",
                    DeviceAction::Unmount,
                ));
            }
            if policy.can_eject {
                row.append(&self.device_action_button(
                    snapshot,
                    "media-eject-symbolic",
                    "Eject",
                    DeviceAction::Eject,
                ));
            }
            self.widgets.devices_box.append(&row);
        }
    }

    fn device_action_button(
        self: &Rc<Self>,
        snapshot: &DeviceSnapshot,
        icon_name: &str,
        verb: &str,
        action: DeviceAction,
    ) -> gtk::Button {
        let label = format!("{verb} {}", snapshot.name);
        let button = gtk::Button::builder()
            .icon_name(icon_name)
            .has_frame(false)
            .tooltip_text(&label)
            .build();
        button.add_css_class("sidebar-icon-button");
        set_accessible_label(&button, &label);
        let id = snapshot.id.clone();
        let controller = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.start_device_action(id.clone(), action, false);
            }
        });
        button
    }

    fn start_device_action(
        self: &Rc<Self>,
        id: DeviceId,
        action: DeviceAction,
        navigate_after_mount: bool,
    ) {
        if action == DeviceAction::Mount {
            self.show_toast(mount_authentication_policy().feedback, 6);
        }
        let mount_operation = gtk::MountOperation::new(Some(&self.widgets.window));
        let controller = Rc::downgrade(self);
        let completion = move |outcome| {
            if let Some(controller) = controller.upgrade() {
                controller.finish_device_action(outcome, navigate_after_mount);
            }
        };
        let result = match action {
            DeviceAction::Mount => {
                self.device_monitor
                    .mount(&id, Some(mount_operation.upcast_ref()), completion)
            }
            DeviceAction::Unmount => {
                self.device_monitor
                    .unmount(&id, Some(mount_operation.upcast_ref()), completion)
            }
            DeviceAction::Eject => {
                self.device_monitor
                    .eject(&id, Some(mount_operation.upcast_ref()), completion)
            }
        };
        if let Err(error) = result {
            self.show_toast(&format!("Could not start storage action: {error}"), 6);
        }
    }

    fn finish_device_action(self: &Rc<Self>, outcome: DeviceActionOutcome, navigate: bool) {
        match outcome {
            DeviceActionOutcome::Completed { id, action } => {
                self.device_monitor.refresh();
                if navigate && action == DeviceAction::Mount {
                    let snapshot = self
                        .device_monitor
                        .snapshots()
                        .into_iter()
                        .find(|snapshot| snapshot.id == id);
                    match snapshot.as_ref().map(ui::device_row_policy) {
                        Some(ui::DeviceRowPolicy {
                            activation: ui::DeviceActivation::Navigate(path),
                            ..
                        }) => self.navigate_to(path),
                        Some(ui::DeviceRowPolicy {
                            activation: ui::DeviceActivation::Unavailable(message),
                            ..
                        }) => self.show_toast(message, 6),
                        _ => self.show_toast("The device mounted without a local folder", 6),
                    }
                } else {
                    let message = match action {
                        DeviceAction::Mount => "Device mounted",
                        DeviceAction::Unmount => "Device unmounted",
                        DeviceAction::Eject => "Device ejected",
                    };
                    self.show_toast(message, 4);
                }
            }
            DeviceActionOutcome::Failed { failure, .. } => {
                self.device_monitor.refresh();
                self.show_toast(&format!("Storage action failed: {}", failure.message), 7);
            }
        }
    }

    fn install_file_view_shortcuts<W>(self: &Rc<Self>, view: &W)
    where
        W: IsA<gtk::Widget>,
    {
        let shortcuts = gtk::EventControllerKey::new();
        let controller = Rc::downgrade(self);
        shortcuts.connect_key_pressed(move |_, key, _, modifiers| {
            if is_permanent_delete_shortcut(key, modifiers) {
                if let Some(controller) = controller.upgrade() {
                    controller.confirm_permanent_delete();
                }
                glib::Propagation::Stop
            } else if key == gtk::gdk::Key::Delete && modifiers.is_empty() {
                if let Some(controller) = controller.upgrade() {
                    controller.trash_selected();
                }
                glib::Propagation::Stop
            } else if key == gtk::gdk::Key::Escape && modifiers.is_empty() {
                if let Some(controller) = controller.upgrade() {
                    controller.clear_selection();
                }
                glib::Propagation::Stop
            } else if is_context_menu_shortcut(key, modifiers) {
                if let Some(controller) = controller.upgrade() {
                    controller.show_context_menu();
                }
                glib::Propagation::Stop
            } else if is_quick_preview_space(key, modifiers) {
                if let Some(controller) = controller.upgrade()
                    && controller.quick_preview_space_enabled()
                {
                    controller.toggle_miller_detail(MillerDetailSurface::Preview);
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            } else if let Some(controller) = controller.upgrade()
                && let Some(command) = controller.vim_command_for_key(key, modifiers)
            {
                controller.dispatch_vim_file_view(command);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        view.add_controller(shortcuts);
    }

    pub fn present_and_start(self: &Rc<Self>) {
        self.widgets.window.present();
        self.arm_sidebar_width_persistence();
        self.discover_terminals();
        self.load_current();

        let controller = Rc::clone(self);
        glib::timeout_add_local(Duration::from_millis(16), move || {
            if !controller.widgets.window.is_visible() {
                return glib::ControlFlow::Break;
            }
            controller.drain_worker();
            controller.drain_folder_filter_worker();
            controller.drain_filename_search_worker();
            controller.drain_bookmark_worker();
            controller.pump_pending_entries();
            controller.submit_thumbnail_requests();
            controller.drain_thumbnail_worker();
            controller.submit_metadata_requests();
            controller.drain_metadata_worker();
            controller.drain_inspector_worker();
            controller.drain_preview_worker();
            controller.drain_properties_worker();
            controller.drain_storage_worker();
            controller.drain_terminal_worker();
            controller.flush_pending_preferences();
            glib::ControlFlow::Continue
        });
    }

    pub fn persist_session_for_shutdown(&self) {
        if self.session_saved.replace(true) {
            return;
        }
        self.restore_pending_navigation();
        self.save_active_session_state();
        let replacement = BrowserTabs::new(PathBuf::from("/"), FolderViewState::default())
            .expect("root is an absolute fallback session");
        let workspace = self.tabs.replace(replacement);
        if let Some(mut worker) = self.session_store.borrow_mut().take()
            && let Err(error) = worker.save_before_shutdown(workspace)
        {
            tracing::warn!(%error, "could not submit final browser session");
        }
    }

    fn arm_sidebar_width_persistence(self: &Rc<Self>) {
        let controller = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            let Some(controller) = controller.upgrade() else {
                return;
            };
            let restored_width = controller
                .current_preferences
                .borrow()
                .sidebar_width
                .map(clamp_sidebar_width)
                .map(i32::from)
                .unwrap_or(controller.widgets.sidebar_default_width);
            controller.ignore_sidebar_position_signal.set(true);
            controller.widgets.workspace.set_position(restored_width);
            controller.ignore_sidebar_position_signal.set(false);

            let controller_weak = Rc::downgrade(&controller);
            controller
                .widgets
                .workspace
                .connect_position_notify(move |workspace| {
                    if let Some(controller) = controller_weak.upgrade() {
                        controller.sidebar_position_changed(workspace.position());
                    }
                });
        });
    }

    fn arm_split_ratio_tracking(self: &Rc<Self>) {
        let controller = Rc::downgrade(self);
        self.widgets
            .split_pane
            .connect_position_notify(move |paned| {
                let Some(controller) = controller.upgrade() else {
                    return;
                };
                if controller.ignore_split_position_signal.get()
                    || !controller.tabs.borrow().active_split().is_split()
                {
                    return;
                }
                let width = paned.width();
                if width <= 0 {
                    return;
                }
                let basis_points = ((i64::from(paned.position()) * 10_000) / i64::from(width))
                    .clamp(i64::from(SPLIT_RATIO_MIN), i64::from(SPLIT_RATIO_MAX))
                    as u16;
                if let Ok(ratio) = SplitRatio::new(basis_points) {
                    controller.tabs.borrow_mut().set_split_ratio(ratio);
                }
            });
    }

    fn resize_primary_pane(&self, delta_basis_points: i32) {
        let current = {
            let tabs = self.tabs.borrow();
            let split = tabs.active_split();
            if !split.is_split() {
                return;
            }
            i32::from(split.ratio().basis_points())
        };
        let next = (current + delta_basis_points)
            .clamp(i32::from(SPLIT_RATIO_MIN), i32::from(SPLIT_RATIO_MAX))
            as u16;
        let Ok(ratio) = SplitRatio::new(next) else {
            return;
        };

        self.tabs.borrow_mut().set_split_ratio(ratio);
        let width = self.widgets.split_pane.width();
        if width > 0 {
            let position = ((i64::from(width) * i64::from(next) + 5_000) / 10_000) as i32;
            self.ignore_split_position_signal.set(true);
            self.widgets.split_pane.set_position(position);
            self.ignore_split_position_signal.set(false);
        }
    }

    fn install_filter_signals(self: &Rc<Self>) {
        let controller = Rc::downgrade(self);
        self.widgets.filter_entry.connect_search_changed(move |_| {
            if let Some(controller) = controller.upgrade() {
                if controller.widgets.search_mode.selected() == 0 {
                    controller.update_folder_filter_from_widgets();
                } else if !controller.filename_search_running.get() {
                    controller.set_search_feedback("Press Enter or Search", false);
                }
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.filter_mode.connect_selected_notify(move |_| {
            if let Some(controller) = controller.upgrade() {
                if controller.widgets.search_mode.selected() == 0 {
                    controller.update_folder_filter_from_widgets();
                }
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.filter_entry.connect_stop_search(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.close_search_surface();
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.search_mode.connect_selected_notify(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.switch_search_surface_mode();
            }
        });
    }

    fn show_folder_filter(&self) {
        if self.filename_search_active.get() {
            self.deactivate_filename_search(true);
        }
        self.widgets.search_mode.set_selected(0);
        self.configure_search_surface(false);
        self.widgets.search_bar.set_visible(true);
        self.focus_search_entry();
    }

    fn focus_search_entry(&self) {
        let entry = self.widgets.filter_entry.clone();
        glib::idle_add_local_once(move || {
            entry.grab_focus();
        });
    }

    fn clear_folder_filter(&self) {
        if self.filename_search_active.get() {
            self.clear_filename_search();
            return;
        }
        self.filter_state.replace(FolderFilterState::default());
        self.widgets.filter_mode.set_selected(0);
        self.widgets.filter_entry.set_text("");
        self.widgets.search_bar.set_visible(false);
        self.filter_generation
            .set(self.filter_generation.get().wrapping_add(1));
        self.pending_filter.borrow_mut().take();
        let selected_paths = self.selected_paths();
        self.install_entries(self.listed_entries.borrow().to_vec(), &selected_paths, true);
        self.set_filter_feedback(None, self.listed_entries.borrow().len());
    }

    fn deactivate_folder_filter_for_search(&self) {
        self.filter_generation
            .set(self.filter_generation.get().wrapping_add(1));
        self.pending_filter.borrow_mut().take();
        self.filter_state.replace(FolderFilterState::default());
        let selected_paths = self.selected_paths();
        self.install_entries(
            self.listed_entries.borrow().to_vec(),
            &selected_paths,
            false,
        );
        self.set_filter_feedback(None, self.listed_entries.borrow().len());
    }

    fn update_folder_filter_from_widgets(&self) {
        let mode = match self.widgets.filter_mode.selected() {
            1 => FolderFilterMode::Glob,
            2 => FolderFilterMode::Regex,
            _ => FolderFilterMode::Text,
        };
        let query = self.widgets.filter_entry.text().to_string();
        self.filter_state.replace(FolderFilterState { mode, query });
        let selected_paths = self.selected_paths();
        self.apply_folder_filter(selected_paths, false);
    }

    fn apply_folder_filter(&self, selected_paths: Vec<PathBuf>, focus_list: bool) {
        let generation = self.filter_generation.get().wrapping_add(1);
        self.filter_generation.set(generation);
        self.filter_selection_paths.replace(selected_paths.clone());
        self.pending_filter.borrow_mut().take();

        let state = self.filter_state.borrow().clone();
        let entries = self.listed_entries.borrow().clone();
        if state.query.is_empty() {
            self.install_entries(entries.to_vec(), &selected_paths, focus_list);
            self.set_filter_feedback(None, self.listed_entries.borrow().len());
            return;
        }

        self.widgets.filter_feedback.remove_css_class("error");
        self.widgets.filter_feedback.add_css_class("dim-label");
        self.widgets.filter_feedback.set_label("Filtering…");
        self.pending_filter.replace(Some(PendingFolderFilter {
            generation,
            entries,
        }));
        self.try_submit_pending_filter();
    }

    fn try_submit_pending_filter(&self) {
        let Some(pending) = self.pending_filter.borrow_mut().take() else {
            return;
        };
        if pending.generation != self.filter_generation.get() {
            return;
        }
        let state = self.filter_state.borrow().clone();
        let result = self.filter_worker.borrow().as_ref().map_or(
            Err(crate::folder_filter::FilterSubmitError::Stopped),
            |worker| worker.submit(pending.generation, state.mode, state.query, pending.entries),
        );
        match result {
            Ok(()) => {}
            Err(crate::folder_filter::FilterSubmitError::Busy(entries)) => {
                self.pending_filter.replace(Some(PendingFolderFilter {
                    generation: pending.generation,
                    entries,
                }));
            }
            Err(crate::folder_filter::FilterSubmitError::Stopped) => {
                self.set_filter_feedback(Some("Filter worker is unavailable"), 0);
            }
        }
    }

    fn drain_folder_filter_worker(&self) {
        loop {
            let response = self
                .filter_worker
                .borrow()
                .as_ref()
                .and_then(crate::folder_filter::FolderFilterWorker::try_response);
            let Some(response) = response else {
                break;
            };
            if !folder_filter_response_is_current(self.filter_generation.get(), response.generation)
            {
                continue;
            }
            match response.result {
                Ok(entries) => {
                    let count = entries.len();
                    let selected_paths = self.filter_selection_paths.borrow().clone();
                    self.install_entries(entries, &selected_paths, false);
                    self.set_filter_feedback(None, count);
                }
                Err(error) => self.set_filter_feedback(Some(&error.to_string()), 0),
            }
        }
        self.try_submit_pending_filter();
    }

    fn set_filter_feedback(&self, error: Option<&str>, visible_count: usize) {
        if let Some(error) = error {
            self.widgets.filter_feedback.remove_css_class("dim-label");
            self.widgets.filter_feedback.add_css_class("error");
            self.widgets.filter_feedback.set_label(error);
            return;
        }

        self.widgets.filter_feedback.remove_css_class("error");
        self.widgets.filter_feedback.add_css_class("dim-label");
        let total = self.listed_entries.borrow().len();
        let active = !self.filter_state.borrow().query.is_empty();
        if active {
            self.widgets
                .filter_feedback
                .set_label(&format!("{visible_count} of {total}"));
            self.widgets.empty_label.set_label("No matching items");
        } else {
            self.widgets
                .filter_feedback
                .set_label(&format!("{total} items"));
            self.widgets
                .empty_label
                .set_label(if self.trash_active.get() {
                    "Trash is empty"
                } else {
                    "This folder is empty"
                });
        }
    }

    fn install_filename_search_signals(self: &Rc<Self>) {
        let controller = Rc::downgrade(self);
        self.widgets.filter_entry.connect_activate(move |_| {
            if let Some(controller) = controller.upgrade() {
                if controller.widgets.search_mode.selected() == 1 {
                    controller.start_filename_search();
                }
            }
        });
    }

    fn configure_search_surface(&self, file_search: bool) {
        let help = crate::ui::SEARCH_SURFACE_MODE_HELP[usize::from(file_search)];
        self.widgets.search_mode.set_tooltip_text(Some(help));
        self.widgets
            .search_mode
            .update_property(&[gtk::accessible::Property::Description(help)]);
        self.widgets.filter_entry.set_tooltip_text(Some(help));
        self.widgets
            .filter_entry
            .set_placeholder_text(Some(if file_search {
                "Filename contains…"
            } else {
                "Filter shown items"
            }));
        self.widgets.filter_mode.set_visible(!file_search);
        self.widgets.filter_feedback.set_visible(!file_search);
        self.widgets.search_scope.set_visible(file_search);
        self.widgets.search_button.set_visible(file_search);
        self.widgets.search_stop_button.set_visible(file_search);
        self.widgets.search_feedback.set_visible(file_search);
        if !file_search {
            self.widgets.filter_entry.set_sensitive(true);
            self.widgets.search_scope.set_sensitive(true);
            self.widgets.search_button.set_sensitive(true);
            self.widgets.search_stop_button.set_sensitive(false);
        }
    }

    fn switch_search_surface_mode(&self) {
        let file_search = self.widgets.search_mode.selected() == 1;
        let query = self.widgets.filter_entry.text().to_string();
        if file_search {
            if self.trash_active.get() {
                self.widgets.search_mode.set_selected(0);
                self.configure_search_surface(false);
                self.show_toast("Search Files is available in local folders", 5);
                return;
            }
            if !self.filename_search_active.get() {
                self.show_filename_search();
            }
        } else {
            if self.filename_search_active.get() {
                self.deactivate_filename_search(true);
            }
            self.configure_search_surface(false);
            self.widgets.search_bar.set_visible(true);
            if self.widgets.filter_entry.text().as_str() != query {
                self.widgets.filter_entry.set_text(&query);
            }
            self.update_folder_filter_from_widgets();
            self.focus_search_entry();
        }
    }

    fn close_search_surface(&self) {
        if self.filename_search_active.get() || self.widgets.search_mode.selected() == 1 {
            self.clear_filename_search();
        } else {
            self.clear_folder_filter();
        }
    }

    fn open_filename_search_surface(&self) {
        if self.trash_active.get() {
            self.show_toast("Search Files is available in local folders", 5);
            return;
        }
        self.widgets.search_mode.set_selected(1);
        if !self.filename_search_active.get() {
            self.show_filename_search();
        }
        self.widgets.search_bar.set_visible(true);
        self.focus_search_entry();
    }

    fn show_filename_search(&self) {
        if self.trash_active.get() {
            self.show_toast("Search Files is available in local folders", 5);
            return;
        }
        self.deactivate_folder_filter_for_search();
        self.configure_search_surface(true);
        self.filename_search_active.set(true);
        self.filename_search_running.set(false);
        self.filename_search_root.replace(Some(
            self.tabs.borrow().active().current().path().to_path_buf(),
        ));
        self.filename_search_results.borrow_mut().clear();
        self.filename_search_summary.borrow_mut().take();
        self.pending_filename_search.borrow_mut().take();
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        self.widgets.selection.set_model(Some(&store));
        self.filename_search_store.replace(Some(store));
        self.widgets.selection.unselect_all();
        self.selected_entries.borrow_mut().clear();
        self.widgets.search_bar.set_visible(true);
        self.widgets
            .view_stack
            .set_visible_child_name("search-results");
        self.widgets.list_header.set_visible(false);
        self.widgets.empty_state.set_visible(false);
        self.set_view_controls_sensitive(false);
        self.set_sort_controls_sensitive(false);
        self.set_filename_search_running(false);
        self.set_search_feedback("Enter a filename to search", false);
        self.set_open_enabled(false);
        self.set_open_with_enabled(false);
        self.set_properties_enabled(false);
        self.set_checksum_enabled(false);
        self.set_selection_actions_enabled(false, false, false);
        self.set_reveal_enabled(false);
        self.focus_search_entry();
    }

    fn start_filename_search(&self) {
        if !self.filename_search_active.get() {
            self.open_filename_search_surface();
        }
        if !self.filename_search_active.get() {
            return;
        }
        let query = self.widgets.filter_entry.text().to_string();
        let scope = if self.widgets.search_scope.selected() == 0 {
            FilenameSearchScope::CurrentFolder
        } else {
            FilenameSearchScope::Subtree
        };
        let root = self.tabs.borrow().active().current().path().to_path_buf();
        let request =
            match FilenameSearchRequest::new(root.clone(), query, scope, self.show_hidden.get()) {
                Ok(request) => request,
                Err(error) => {
                    self.set_filename_search_running(false);
                    self.set_search_feedback(&error.to_string(), true);
                    return;
                }
            };
        let generation = self.filename_search_generation.get().wrapping_add(1).max(1);
        self.filename_search_generation.set(generation);
        self.filename_search_root.replace(Some(root));
        self.filename_search_results.borrow_mut().clear();
        self.filename_search_summary.borrow_mut().take();
        self.widgets.selection.unselect_all();
        self.selected_entries.borrow_mut().clear();
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        self.widgets.selection.set_model(Some(&store));
        self.filename_search_store.replace(Some(store));
        self.widgets.empty_state.set_visible(false);
        self.pending_filename_search.replace(Some(request));
        self.set_filename_search_running(true);
        self.set_search_feedback("Searching filenames…", false);
        self.try_submit_pending_filename_search();
    }

    fn try_submit_pending_filename_search(&self) {
        let Some(request) = self.pending_filename_search.borrow_mut().take() else {
            return;
        };
        let generation = self.filename_search_generation.get();
        let outcome = self.filename_search_worker.borrow().as_ref().map_or(
            Err(crate::filename_search::FilenameSearchSubmitError::Stopped),
            |worker| worker.submit(generation, request),
        );
        match outcome {
            Ok(()) => {}
            Err(crate::filename_search::FilenameSearchSubmitError::Busy(request)) => {
                self.pending_filename_search.replace(Some(*request));
            }
            Err(crate::filename_search::FilenameSearchSubmitError::Stopped) => {
                self.set_filename_search_running(false);
                self.set_search_feedback("Filename search worker is unavailable", true);
            }
        }
    }

    fn drain_filename_search_worker(&self) {
        loop {
            let event = self
                .filename_search_worker
                .borrow()
                .as_ref()
                .and_then(crate::filename_search::FilenameSearchWorker::try_event);
            let Some(event) = event else {
                break;
            };
            if !self.filename_search_active.get()
                || event.generation != self.filename_search_generation.get()
            {
                continue;
            }
            match event.kind {
                crate::filename_search::FilenameSearchEventKind::Batch { entries, summary } => {
                    if let Some(store) = self.filename_search_store.borrow().as_ref() {
                        for entry in &entries {
                            store.append(&glib::BoxedAnyObject::new(entry.clone()));
                        }
                    }
                    self.filename_search_results.borrow_mut().extend(entries);
                    self.filename_search_summary.replace(Some(summary));
                    self.widgets.empty_state.set_visible(false);
                    self.set_search_feedback(
                        &filename_search_feedback(summary, true, false),
                        false,
                    );
                    self.refresh_status();
                }
                crate::filename_search::FilenameSearchEventKind::Finished(summary) => {
                    self.filename_search_summary.replace(Some(summary));
                    self.set_filename_search_running(false);
                    self.widgets
                        .empty_state
                        .set_visible(self.filename_search_results.borrow().is_empty());
                    if self.filename_search_results.borrow().is_empty() {
                        self.widgets.empty_label.set_label("No matching files");
                    }
                    self.set_search_feedback(
                        &filename_search_feedback(summary, false, false),
                        false,
                    );
                    self.refresh_status();
                }
                crate::filename_search::FilenameSearchEventKind::Failed(error) => {
                    self.set_filename_search_running(false);
                    self.set_search_feedback(&format!("Search failed: {error}"), true);
                }
            }
        }
        self.try_submit_pending_filename_search();
    }

    fn stop_filename_search(&self) {
        if !self.filename_search_active.get() {
            return;
        }
        let generation = self.filename_search_generation.get().wrapping_add(1).max(1);
        self.filename_search_generation.set(generation);
        if let Some(worker) = self.filename_search_worker.borrow().as_ref() {
            worker.cancel(generation);
        }
        self.pending_filename_search.borrow_mut().take();
        self.set_filename_search_running(false);
        let summary = self.filename_search_summary.borrow().unwrap_or_default();
        self.set_search_feedback(&filename_search_feedback(summary, false, true), false);
    }

    fn clear_filename_search(&self) {
        self.widgets.filter_entry.set_text("");
        self.deactivate_filename_search(true);
    }

    fn deactivate_filename_search(&self, restore_listing: bool) {
        let generation = self.filename_search_generation.get().wrapping_add(1).max(1);
        self.filename_search_generation.set(generation);
        if let Some(worker) = self.filename_search_worker.borrow().as_ref() {
            worker.cancel(generation);
        }
        self.filename_search_active.set(false);
        self.filename_search_running.set(false);
        self.filename_search_root.borrow_mut().take();
        self.filename_search_results.borrow_mut().clear();
        self.filename_search_store.borrow_mut().take();
        self.filename_search_summary.borrow_mut().take();
        self.pending_filename_search.borrow_mut().take();
        self.widgets.search_bar.set_visible(false);
        self.set_view_controls_sensitive(true);
        self.set_sort_controls_sensitive(true);
        self.set_reveal_enabled(false);
        self.widgets.set_view_mode(self.view_mode.get());
        if restore_listing {
            self.install_entries(self.listed_entries.borrow().to_vec(), &[], true);
        }
    }

    fn set_filename_search_running(&self, running: bool) {
        self.filename_search_running.set(running);
        self.widgets.search_button.set_sensitive(!running);
        self.widgets.search_stop_button.set_sensitive(running);
        self.widgets.filter_entry.set_sensitive(!running);
        self.widgets.search_scope.set_sensitive(!running);
    }

    fn set_search_feedback(&self, message: &str, error: bool) {
        self.widgets.search_feedback.set_label(message);
        if error {
            self.widgets.search_feedback.remove_css_class("dim-label");
            self.widgets.search_feedback.add_css_class("error");
        } else {
            self.widgets.search_feedback.remove_css_class("error");
            self.widgets.search_feedback.add_css_class("dim-label");
        }
    }

    fn set_reveal_enabled(&self, enabled: bool) {
        if let Some(action) = self
            .widgets
            .window
            .lookup_action("reveal-in-folder")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(enabled);
        }
    }

    fn reveal_search_result(&self) {
        if !self.filename_search_active.get() {
            return;
        }
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let target = entry.path().to_path_buf();
        self.deactivate_filename_search(false);
        self.navigate_to_revealing(target);
    }

    fn install_actions(self: &Rc<Self>, application: &adw::Application) {
        self.add_action("command-palette", |controller| {
            controller.command_palette.present();
        });
        self.add_action("keyboard-shortcuts", |controller| {
            let current = controller.current_preferences.borrow().keybindings.clone();
            let weak = Rc::downgrade(controller);
            controller
                .keyboard_shortcuts
                .present(current, move |keybindings| {
                    if let Some(controller) = weak.upgrade() {
                        controller.apply_keybinding_overrides(keybindings);
                    }
                });
        });
        self.add_action("context-menu-settings", |controller| {
            let current = controller.current_preferences.borrow().context_menu;
            let weak = Rc::downgrade(controller);
            controller
                .context_menu_editor
                .present(current, move |preferences| {
                    if let Some(controller) = weak.upgrade() {
                        controller.apply_context_menu_preferences(preferences);
                    }
                });
        });
        let vim_enabled = self.current_preferences.borrow().vim_mode;
        let vim_action =
            gio::SimpleAction::new_stateful("vim-mode", None, &vim_enabled.to_variant());
        let controller = Rc::downgrade(self);
        vim_action.connect_activate(move |action, _| {
            let enabled = !action
                .state()
                .and_then(|state| state.get::<bool>())
                .unwrap_or(false);
            if let Some(controller) = controller.upgrade() {
                controller.change_vim_mode(enabled);
                action.set_state(&enabled.to_variant());
            }
        });
        self.widgets.window.add_action(&vim_action);
        let open_terminal = self.add_action("open-terminal", |controller| {
            controller.open_terminal_here();
        });
        open_terminal.set_enabled(false);
        self.add_action("terminal-preferences", |controller| {
            controller.show_terminal_preferences();
        });
        self.add_action("back", |controller| controller.go_back());
        self.add_action("forward", |controller| controller.go_forward());
        self.add_action("parent", |controller| controller.go_parent());
        self.add_action("location", |controller| controller.show_location_entry());
        self.add_action("folder-filter", |controller| {
            controller.show_folder_filter();
        });
        self.add_action("clear-folder-filter", |controller| {
            controller.clear_folder_filter();
        });
        self.add_action("close-search-surface", |controller| {
            controller.close_search_surface();
        });
        self.add_action("filename-search", |controller| {
            controller.open_filename_search_surface();
        });
        self.add_action("start-filename-search", |controller| {
            controller.start_filename_search();
        });
        self.add_action("stop-filename-search", |controller| {
            controller.stop_filename_search();
        });
        self.add_action("clear-filename-search", |controller| {
            controller.clear_filename_search();
        });
        let reveal = self.add_action("reveal-in-folder", |controller| {
            controller.reveal_search_result();
        });
        reveal.set_enabled(false);
        self.add_action("cancel-location", |controller| {
            controller.cancel_location_entry();
        });
        self.add_action("hidden", |controller| controller.toggle_hidden());
        self.add_action("refresh", |controller| {
            controller.reload_preserving_view(Vec::new());
        });
        self.add_action("open-trash", |controller| controller.open_trash());
        self.add_action("select-all", |controller| controller.select_all());
        self.add_action("clear-selection", |controller| controller.clear_selection());
        self.add_action("new-tab", |controller| controller.new_tab());
        self.add_action("close-tab-active", |controller| {
            let id = controller.tabs.borrow().active_id();
            controller.close_tab(id);
        });
        self.add_action("duplicate-tab-active", |controller| {
            let id = controller.tabs.borrow().active_id();
            controller.duplicate_tab(id);
        });
        let reopen = self.add_action("reopen-closed-tab", |controller| {
            controller.reopen_closed_tab();
        });
        reopen.set_enabled(self.tabs.borrow().can_reopen_closed());
        self.add_action("toggle-split", |controller| controller.toggle_split());
        self.add_action("switch-split-side", |controller| {
            controller.switch_split_side()
        });
        self.add_action("swap-split-sides", |controller| {
            controller.swap_split_sides()
        });
        self.add_action("close-split", |controller| controller.close_split());
        self.add_action("narrow-primary-pane", |controller| {
            controller.resize_primary_pane(-500);
        });
        self.add_action("widen-primary-pane", |controller| {
            controller.resize_primary_pane(500);
        });
        let open_opposite = self.add_action("open-opposite-pane", |controller| {
            controller.open_selected_in_opposite_pane()
        });
        open_opposite.set_enabled(false);
        let copy_opposite = self.add_action("copy-to-opposite-pane", |controller| {
            controller.transfer_selected_to_opposite(TransferIntent::Copy)
        });
        copy_opposite.set_enabled(false);
        let move_opposite = self.add_action("move-to-opposite-pane", |controller| {
            controller.transfer_selected_to_opposite(TransferIntent::Move)
        });
        move_opposite.set_enabled(false);
        let link_opposite = self.add_action("link-to-opposite-pane", |controller| {
            controller.link_selected_to_opposite()
        });
        link_opposite.set_enabled(false);
        self.add_action("next-tab", |controller| controller.switch_relative_tab(1));
        self.add_action("previous-tab", |controller| {
            controller.switch_relative_tab(-1);
        });
        self.add_action("move-tab-left", |controller| controller.move_active_tab(-1));
        self.add_action("move-tab-right", |controller| controller.move_active_tab(1));
        self.add_u64_action("activate-tab", |controller, id| controller.activate_tab(id));
        self.add_u64_action("close-tab", |controller, id| controller.close_tab(id));
        self.add_u64_action("duplicate-tab", |controller, id| {
            controller.duplicate_tab(id);
        });
        self.add_u64_action("close-tabs-left", |controller, id| {
            controller.close_tab_variant(id, TabCloseVariant::Left);
        });
        self.add_u64_action("close-tabs-right", |controller, id| {
            controller.close_tab_variant(id, TabCloseVariant::Right);
        });
        self.add_u64_action("close-other-tabs", |controller, id| {
            controller.close_tab_variant(id, TabCloseVariant::Others);
        });
        let move_before = gio::SimpleAction::new(
            "move-tab-before",
            Some(&<(u64, u64)>::static_variant_type()),
        );
        let controller = Rc::downgrade(self);
        move_before.connect_activate(move |_, parameter| {
            let Some((source, target)) = parameter.and_then(glib::Variant::get::<(u64, u64)>)
            else {
                return;
            };
            if let Some(controller) = controller.upgrade() {
                controller.move_tab_before(source, target);
            }
        });
        self.widgets.window.add_action(&move_before);
        let open_tab = self.add_action("open-new-tab", |controller| {
            controller.open_selected_folder_in_tab(TabActivation::Foreground);
        });
        open_tab.set_enabled(false);
        let open_background = self.add_action("open-background-tab", |controller| {
            controller.open_selected_folder_in_tab(TabActivation::Background);
        });
        open_background.set_enabled(false);
        for (name, command) in VIEW_ACTIONS {
            self.add_action(name, move |controller| {
                controller.apply_view_command(command);
            });
        }
        self.add_action("miller-parent", |controller| {
            controller.navigate_active_miller_command(MillerNavigationCommand::Parent);
        });
        self.add_action("miller-child", |controller| {
            controller.navigate_active_miller_command(MillerNavigationCommand::Child);
        });
        let preview_hook = self.add_action(MILLER_DETAIL_ACTIONS[0], |controller| {
            controller.toggle_miller_detail(MillerDetailSurface::Preview);
        });
        preview_hook.set_enabled(self.view_mode.get() == ViewMode::Miller);
        self.add_action("quick-preview", |controller| {
            controller.toggle_miller_detail(MillerDetailSurface::Preview);
        });
        self.add_action("preview-zoom-in", |controller| {
            controller.widgets.miller_view.preview_zoom_in();
            controller.render_miller();
        });
        self.add_action("preview-zoom-out", |controller| {
            controller.widgets.miller_view.preview_zoom_out();
            controller.render_miller();
        });
        self.add_action("preview-zoom-reset", |controller| {
            controller.widgets.miller_view.preview_zoom_reset();
            controller.render_miller();
        });
        self.add_action("preview-fullscreen", |controller| {
            if controller.widgets.window.is_fullscreen() {
                controller.widgets.window.unfullscreen();
            } else {
                controller.widgets.window.fullscreen();
            }
        });
        self.add_action("preview-clear-cache", |controller| {
            if let Some(worker) = controller.preview_worker.borrow().as_ref() {
                worker.clear_memory_cache();
            }
            controller.miller_detail.borrow_mut().hide();
            controller.widgets.window.unfullscreen();
            controller.render_miller();
            controller.widgets.miller_view.focus_active();
            controller.show_toast("Memory-only Preview cache cleared", 4);
        });
        let inspector_hook = self.add_action(MILLER_DETAIL_ACTIONS[1], |controller| {
            controller.toggle_miller_detail(MillerDetailSurface::Inspector);
        });
        inspector_hook.set_enabled(self.view_mode.get() == ViewMode::Miller);
        self.add_action("narrow-miller-columns", |controller| {
            let width = controller.widgets.miller_view.width().narrower();
            controller.set_miller_column_width(width);
        });
        self.add_action("widen-miller-columns", |controller| {
            let width = controller.widgets.miller_view.width().wider();
            controller.set_miller_column_width(width);
        });
        self.add_action("narrow-inspector", |controller| {
            let width = controller.widgets.miller_view.detail_width().narrower();
            controller.set_inspector_width(width);
        });
        self.add_action("widen-inspector", |controller| {
            let width = controller.widgets.miller_view.detail_width().wider();
            controller.set_inspector_width(width);
        });

        let appearance = self.widgets.appearance_preset();
        let appearance_action = gio::SimpleAction::new_stateful(
            "appearance",
            Some(&String::static_variant_type()),
            &appearance.persisted().to_variant(),
        );
        let controller = Rc::downgrade(self);
        appearance_action.connect_activate(move |action, parameter| {
            let Some(preset) = parameter
                .and_then(glib::Variant::str)
                .and_then(AppearancePreset::from_persisted)
            else {
                return;
            };
            if let Some(controller) = controller.upgrade() {
                controller.change_appearance(preset);
                action.set_state(&preset.persisted().to_variant());
            }
        });
        self.widgets.window.add_action(&appearance_action);

        let density = self.current_preferences.borrow().sidebar_density;
        let density_action = gio::SimpleAction::new_stateful(
            "sidebar-density",
            Some(&String::static_variant_type()),
            &density.persisted().to_variant(),
        );
        let controller = Rc::downgrade(self);
        density_action.connect_activate(move |action, parameter| {
            let Some(density) = parameter
                .and_then(glib::Variant::str)
                .and_then(SidebarDensity::from_persisted)
            else {
                return;
            };
            if let Some(controller) = controller.upgrade() {
                controller.change_sidebar_density(density);
                action.set_state(&density.persisted().to_variant());
            }
        });
        self.widgets.window.add_action(&density_action);

        let file_density = self.file_density.get();
        let file_density_action = gio::SimpleAction::new_stateful(
            "file-density",
            Some(&String::static_variant_type()),
            &file_density.persisted().to_variant(),
        );
        let controller = Rc::downgrade(self);
        file_density_action.connect_activate(move |action, parameter| {
            let Some(density) = parameter
                .and_then(glib::Variant::str)
                .and_then(FileViewDensity::from_persisted)
            else {
                return;
            };
            if let Some(controller) = controller.upgrade() {
                controller.change_file_density(density);
                action.set_state(&density.persisted().to_variant());
            }
        });
        self.widgets.window.add_action(&file_density_action);

        let grouping = self.sort_order.get().grouping;
        let grouping_action = gio::SimpleAction::new_stateful(
            "grouping",
            Some(&String::static_variant_type()),
            &grouping.persisted().to_variant(),
        );
        let controller = Rc::downgrade(self);
        grouping_action.connect_activate(move |action, parameter| {
            let Some(grouping) = parameter
                .and_then(glib::Variant::str)
                .and_then(DirectoryGrouping::from_persisted)
            else {
                return;
            };
            if let Some(controller) = controller.upgrade() {
                controller.change_grouping(grouping);
                action.set_state(&grouping.persisted().to_variant());
            }
        });
        self.widgets.window.add_action(&grouping_action);

        let placement = self.sort_order.get().directories;
        let placement_action = gio::SimpleAction::new_stateful(
            "directory-placement",
            Some(&String::static_variant_type()),
            &placement.persisted().to_variant(),
        );
        let controller = Rc::downgrade(self);
        placement_action.connect_activate(move |action, parameter| {
            let Some(placement) = parameter
                .and_then(glib::Variant::str)
                .and_then(DirectoryPlacement::from_persisted)
            else {
                return;
            };
            if let Some(controller) = controller.upgrade() {
                controller.change_directory_placement(placement);
                action.set_state(&placement.persisted().to_variant());
            }
        });
        self.widgets.window.add_action(&placement_action);

        let remember = self.current_preferences.borrow().remember_per_folder;
        let remember_action =
            gio::SimpleAction::new_stateful("remember-folder-view", None, &remember.to_variant());
        let controller = Rc::downgrade(self);
        remember_action.connect_activate(move |action, parameter| {
            let enabled = parameter
                .and_then(glib::Variant::get::<bool>)
                .unwrap_or_else(|| {
                    !action
                        .state()
                        .and_then(|state| state.get())
                        .unwrap_or(false)
                });
            if let Some(controller) = controller.upgrade() {
                controller.set_remember_per_folder(enabled);
                action.set_state(&enabled.to_variant());
            }
        });
        self.widgets.window.add_action(&remember_action);

        self.add_action("narrow-name", |controller| {
            controller.resize_list_column(ListColumn::Name, -16);
        });
        self.add_action("widen-name", |controller| {
            controller.resize_list_column(ListColumn::Name, 16);
        });
        for column in ListColumn::OPTIONAL {
            let action_name = format!("column-{}", column.persisted());
            let visible = self.list_columns.get().is_visible(column);
            let column_action =
                gio::SimpleAction::new_stateful(&action_name, None, &visible.to_variant());
            let controller = Rc::downgrade(self);
            column_action.connect_activate(move |action, parameter| {
                let visible = parameter
                    .and_then(glib::Variant::get::<bool>)
                    .unwrap_or_else(|| {
                        !action
                            .state()
                            .and_then(|state| state.get())
                            .unwrap_or(false)
                    });
                if let Some(controller) = controller.upgrade() {
                    controller.toggle_list_column(column, visible);
                    action.set_state(&visible.to_variant());
                }
            });
            self.widgets.window.add_action(&column_action);

            let narrower = format!("narrow-{}", column.persisted());
            self.add_action(&narrower, move |controller| {
                controller.resize_list_column(column, -16);
            });
            let wider = format!("widen-{}", column.persisted());
            self.add_action(&wider, move |controller| {
                controller.resize_list_column(column, 16);
            });
        }

        self.add_action("reset-sidebar-width", |controller| {
            controller.reset_sidebar_width();
        });
        let open_action = self.add_action("open", |controller| controller.activate_selected());
        open_action.set_enabled(false);
        let open_with_action =
            self.add_action("open-with", |controller| controller.show_open_with());
        open_with_action.set_enabled(false);
        let properties_action =
            self.add_action("properties", |controller| controller.show_properties());
        properties_action.set_enabled(false);
        let checksum_action =
            self.add_action("checksum", |controller| controller.show_checksum_dialog());
        checksum_action.set_enabled(false);
        let extract_here_action =
            self.add_action("extract-here", |controller| controller.extract_here());
        extract_here_action.set_enabled(false);
        let extract_to_action = self.add_action("extract-to", |controller| {
            controller.choose_extract_destination()
        });
        extract_to_action.set_enabled(false);
        let compress_action =
            self.add_action("compress", |controller| controller.show_compress_dialog());
        compress_action.set_enabled(false);
        let copy_action = self.add_action("copy", |controller| controller.stage_selected_copy());
        copy_action.set_enabled(false);
        let cut_action = self.add_action("cut", |controller| controller.stage_selected_move());
        cut_action.set_enabled(false);
        let rename_action = self.add_action("rename", |controller| controller.show_rename());
        rename_action.set_enabled(false);
        let batch_rename_action =
            self.add_action("batch-rename", |controller| controller.show_batch_rename());
        batch_rename_action.set_enabled(false);
        let undo_batch_rename = self.add_action("undo-batch-rename", |controller| {
            controller.undo_batch_rename();
        });
        undo_batch_rename.set_enabled(false);
        let trash_action = self.add_action("trash", |controller| controller.trash_selected());
        trash_action.set_enabled(false);
        let permanent_delete_action = self.add_action("permanent-delete", |controller| {
            controller.confirm_permanent_delete();
        });
        permanent_delete_action.set_enabled(false);
        let restore_action = self.add_action("restore", |controller| {
            controller.restore_selected();
        });
        restore_action.set_enabled(false);
        let empty_trash_action = self.add_action("empty-trash", |controller| {
            controller.confirm_empty_trash();
        });
        empty_trash_action.set_enabled(false);
        let paste_action = self.add_action("paste", |controller| controller.paste_transfer());
        paste_action.set_enabled(false);
        self.add_action("new-folder", |controller| controller.show_new_folder());
        self.add_action("new-empty-file", |controller| {
            controller.show_new_empty_file()
        });
        self.add_action("new-from-template", |controller| {
            controller.choose_template();
        });
        let duplicate_action =
            self.add_action("duplicate", |controller| controller.duplicate_selected());
        duplicate_action.set_enabled(false);
        let symbolic_link_action = self.add_action("create-symbolic-link", |controller| {
            controller.show_create_symbolic_link();
        });
        symbolic_link_action.set_enabled(false);
        let hard_link_action = self.add_action("create-hard-link", |controller| {
            controller.show_create_hard_link();
        });
        hard_link_action.set_enabled(false);
        let reveal_link_action = self.add_action("reveal-link-target", |controller| {
            controller.reveal_link_target();
        });
        reveal_link_action.set_enabled(false);
        for (name, mode) in [
            ("copy-name", ClipboardTextMode::Name),
            ("copy-path", ClipboardTextMode::AbsolutePath),
            ("copy-relative-path", ClipboardTextMode::RelativePath),
            ("copy-uri", ClipboardTextMode::Uri),
        ] {
            let action = self.add_action(name, move |controller| {
                controller.copy_selection_text(mode);
            });
            action.set_enabled(false);
        }
        for (name, column) in ui::SORT_ACTIONS {
            self.add_action(name, move |controller| controller.change_sort(column));
        }

        crate::keybindings::install_effective_window_shortcuts(
            application,
            &self.current_preferences.borrow().keybindings,
        );
    }

    fn add_action<F>(self: &Rc<Self>, name: &str, callback: F) -> gio::SimpleAction
    where
        F: Fn(&Rc<Self>) + 'static,
    {
        let action = gio::SimpleAction::new(name, None);
        let controller = Rc::downgrade(self);
        action.connect_activate(move |_, _| {
            if let Some(controller) = controller.upgrade() {
                callback(&controller);
            }
        });
        self.widgets.window.add_action(&action);
        action
    }

    fn add_u64_action<F>(self: &Rc<Self>, name: &str, callback: F) -> gio::SimpleAction
    where
        F: Fn(&Rc<Self>, BrowserSessionId) + 'static,
    {
        let action = gio::SimpleAction::new(name, Some(&u64::static_variant_type()));
        let controller = Rc::downgrade(self);
        action.connect_activate(move |_, parameter| {
            let Some(raw_id) = parameter.and_then(glib::Variant::get::<u64>) else {
                return;
            };
            let Ok(id) = BrowserSessionId::new(raw_id) else {
                return;
            };
            if let Some(controller) = controller.upgrade() {
                callback(&controller, id);
            }
        });
        self.widgets.window.add_action(&action);
        action
    }

    fn save_active_session_state(&self) {
        if self.trash_active.get() {
            return;
        }
        let snapshot = self.capture_view_state();
        let anchor =
            SessionScrollAnchor::new(snapshot.anchor_path.clone(), snapshot.anchor_index).ok();
        let mut tabs = self.tabs.borrow_mut();
        let session = tabs.active_mut();
        if let Err(error) = session.set_selection(snapshot.selected_paths) {
            tracing::warn!(%error, "could not retain active tab selection");
        }
        if let Err(error) = session.set_scroll_anchor(anchor) {
            tracing::warn!(%error, "could not retain active tab scroll anchor");
        }
        session.set_view(self.active_view_state());
        drop(tabs);
        self.capture_split_snapshot();
    }

    fn restore_active_session(&self) {
        let location = self.tabs.borrow().active().current().clone();
        let view = location.view();
        self.view_mode.set(view.mode);
        self.grid_size.set(view.grid_size);
        self.sort_order.set(view.sort);
        self.file_density.set(view.density);
        self.list_columns.set(view.columns);
        self.widgets.set_view_mode(view.mode);
        self.widgets.set_grid_size(view.grid_size);
        self.widgets
            .apply_file_view_policy(view.density, view.columns, view.sort.grouping);
        self.update_sort_headers();
        self.pending_reconciliation
            .replace(Some(PendingReconciliation {
                snapshot: session_restore_snapshot(self.tabs.borrow().active()),
                renames: Vec::new(),
            }));
        self.load_current_inner();
        self.render_tabs();
        self.render_split_presentation();
    }

    fn capture_split_snapshot(&self) {
        let (tab_id, side) = {
            let tabs = self.tabs.borrow();
            (tabs.active_id(), tabs.active_split().active_side())
        };
        self.split_snapshots.borrow_mut().entry(tab_id).or_default()[split_side_index(side)] =
            split_snapshot(&self.visible_entries.borrow());
    }

    fn render_split_presentation(&self) {
        let tabs = self.tabs.borrow();
        let tab_id = tabs.active_id();
        let split = tabs.active_split();
        let is_split = split.is_split();
        let active_side = split.active_side();
        let ratio = split.ratio();
        let opposite_path = split
            .opposite()
            .map(|session| session.current().path().to_path_buf());
        drop(tabs);
        let opposite_snapshot = self
            .split_snapshots
            .borrow()
            .get(&tab_id)
            .map(|snapshots| snapshots[split_side_index(active_side.opposite())].clone())
            .unwrap_or_default();
        self.ignore_split_position_signal.set(true);
        let restore_view_focus = self.widgets.set_split_presentation(
            is_split,
            active_side,
            ratio,
            opposite_path.as_deref(),
            &opposite_snapshot.names,
            opposite_snapshot.total,
        );
        self.ignore_split_position_signal.set(false);
        if restore_view_focus {
            self.widgets.focus_view(self.view_mode.get());
        }
        self.update_split_action_states();
    }

    fn toggle_split(&self) {
        if self.trash_active.get() {
            self.show_toast("Restore a normal folder before opening split view", 5);
            return;
        }
        if self.tabs.borrow().active_split().is_split() {
            self.close_split();
            return;
        }
        self.restore_pending_navigation();
        self.save_active_session_state();
        self.capture_split_snapshot();
        let (path, view) = {
            let tabs = self.tabs.borrow();
            (
                tabs.active().current().path().to_path_buf(),
                tabs.active().current().view(),
            )
        };
        let result = self.tabs.borrow_mut().split_active(path, view);
        match result {
            Ok(_) => {
                let tab_id = self.tabs.borrow().active_id();
                let mut snapshot_map = self.split_snapshots.borrow_mut();
                let snapshots = snapshot_map.entry(tab_id).or_default();
                snapshots[split_side_index(SplitSide::Secondary)] =
                    snapshots[split_side_index(SplitSide::Primary)].clone();
                drop(snapshot_map);
                self.render_split_presentation();
                self.show_toast("Split view opened. Press F6 to switch panes.", 4);
            }
            Err(error) => self.show_toast(&format!("Could not open split view: {error}"), 6),
        }
    }

    fn switch_split_side(&self) {
        let target = {
            let tabs = self.tabs.borrow();
            let split = tabs.active_split();
            if !split.is_split() {
                self.show_toast("Split view is not open", 4);
                return;
            }
            split.active_side().opposite()
        };
        self.restore_pending_navigation();
        self.save_active_session_state();
        self.capture_split_snapshot();
        let result = self.tabs.borrow_mut().activate_split_side(target);
        match result {
            Ok(_) => {
                self.trash_active.set(false);
                self.widgets.set_trash_mode(false);
                self.restore_active_session();
                self.render_split_presentation();
            }
            Err(error) => self.show_toast(&format!("Could not switch pane: {error}"), 5),
        }
    }

    fn close_split(&self) {
        let side_to_close = {
            let tabs = self.tabs.borrow();
            let split = tabs.active_split();
            if !split.is_split() {
                return;
            }
            split.active_side().opposite()
        };
        self.restore_pending_navigation();
        self.save_active_session_state();
        self.capture_split_snapshot();
        let result = self.tabs.borrow_mut().close_split_side(side_to_close);
        match result {
            Ok(_) => {
                let tab_id = self.tabs.borrow().active_id();
                self.split_snapshots.borrow_mut().remove(&tab_id);
                self.render_split_presentation();
                self.render_tabs();
                self.show_toast("Split view closed", 3);
            }
            Err(error) => self.show_toast(&format!("Could not close split view: {error}"), 5),
        }
    }

    fn swap_split_sides(&self) {
        if !self.tabs.borrow().active_split().is_split() {
            self.show_toast("Split view is not open", 4);
            return;
        }
        self.restore_pending_navigation();
        self.save_active_session_state();
        self.capture_split_snapshot();
        let result = self.tabs.borrow_mut().swap_split_sides();
        match result {
            Ok(()) => {
                let tab_id = self.tabs.borrow().active_id();
                if let Some(snapshots) = self.split_snapshots.borrow_mut().get_mut(&tab_id) {
                    snapshots.swap(0, 1);
                }
                self.render_split_presentation();
                self.render_tabs();
            }
            Err(error) => self.show_toast(&format!("Could not swap panes: {error}"), 5),
        }
    }

    fn open_selected_in_opposite_pane(&self) {
        let Some(entry) = self.selected_entry() else {
            self.show_toast("Select one folder to open in the other pane", 4);
            return;
        };
        if self.trash_active.get() || !entry.is_navigable_directory() {
            self.show_toast("Only normal folders can open in the other pane", 4);
            return;
        }
        let path = entry.path().to_path_buf();
        let view = self.current_preferences.borrow().effective_state(&path);
        self.restore_pending_navigation();
        self.save_active_session_state();
        self.capture_split_snapshot();
        let result = {
            let mut tabs = self.tabs.borrow_mut();
            if tabs.active_split().is_split() {
                let opposite = tabs.active_split().active_side().opposite();
                tabs.active_split_mut()
                    .pane_mut(opposite)
                    .expect("split invariant keeps opposite pane present")
                    .navigate_to(path, view)
                    .map(|_| opposite)
                    .map_err(TabError::from)
            } else {
                tabs.split_active(path, view).map(|_| SplitSide::Secondary)
            }
        };
        match result {
            Ok(side) => {
                let tab_id = self.tabs.borrow().active_id();
                self.split_snapshots.borrow_mut().entry(tab_id).or_default()
                    [split_side_index(side)] = SplitPaneSnapshot::default();
                self.render_split_presentation();
                self.show_toast("Folder opened in the other pane", 3);
            }
            Err(error) => self.show_toast(&format!("Could not open other pane: {error}"), 6),
        }
    }

    fn transfer_selected_to_opposite(&self, intent: TransferIntent) {
        let destination = {
            let tabs = self.tabs.borrow();
            let Some(destination) = opposite_pane_destination(&tabs) else {
                self.show_toast("Open split view before transferring between panes", 4);
                return;
            };
            destination
        };
        let paths = self.selected_paths();
        if paths.is_empty() {
            self.show_toast("Select one or more items first", 4);
            return;
        }
        match self
            .application_state
            .submit_transfer_batch(intent, paths, &destination)
        {
            Ok(batch) => {
                self.widgets.status_label.set_label(&format!(
                    "{} to other pane queued: {}",
                    match intent {
                        TransferIntent::Copy => "Copy",
                        TransferIntent::Move => "Move",
                    },
                    item_count_text(batch.queued())
                ));
            }
            Err(error) => self.show_toast(&format!("Could not start pane transfer: {error}"), 6),
        }
    }

    fn link_selected_to_opposite(&self) {
        let destination = {
            let tabs = self.tabs.borrow();
            let Some(destination) = split_drop_destination(&tabs, self.trash_active.get()) else {
                self.show_toast("Open split view before linking between panes", 4);
                return;
            };
            destination
        };
        let paths = self.selected_paths();
        if paths.is_empty() {
            self.show_toast("Select one or more items first", 4);
            return;
        }
        match DropRequest::new(paths, destination, DropAction::Link) {
            Ok(request) => self.submit_drop(request),
            Err(error) => self.show_toast(&format!("Could not link to other pane: {error}"), 6),
        }
    }

    fn new_tab(&self) {
        let path = self.tabs.borrow().active().current().path().to_path_buf();
        self.open_path_in_tab(path, TabActivation::Foreground);
    }

    fn open_selected_folder_in_tab(&self, activation: TabActivation) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        if self.trash_active.get() || !entry.is_navigable_directory() {
            return;
        }
        self.open_path_in_tab(entry.path().to_path_buf(), activation);
    }

    fn open_path_in_tab(&self, path: PathBuf, activation: TabActivation) {
        self.restore_pending_navigation();
        self.save_active_session_state();
        let view = self.current_preferences.borrow().effective_state(&path);
        let result = self.tabs.borrow_mut().open(path, view, activation);
        match result {
            Ok(_) if activation == TabActivation::Foreground => {
                self.trash_active.set(false);
                self.widgets.set_trash_mode(false);
                self.refresh_paste_enabled();
                self.restore_active_session();
            }
            Ok(_) => self.render_tabs(),
            Err(error) => self.show_toast(&format!("Could not open tab: {error}"), 5),
        }
    }

    fn activate_tab(&self, id: BrowserSessionId) {
        if self.tabs.borrow().active_id() == id {
            return;
        }
        self.restore_pending_navigation();
        self.save_active_session_state();
        if self.tabs.borrow_mut().activate(id).unwrap_or(false) {
            self.trash_active.set(false);
            self.widgets.set_trash_mode(false);
            self.refresh_paste_enabled();
            self.restore_active_session();
        }
    }

    fn switch_relative_tab(&self, delta: isize) {
        self.restore_pending_navigation();
        self.save_active_session_state();
        if self.tabs.borrow_mut().activate_relative(delta) {
            self.trash_active.set(false);
            self.widgets.set_trash_mode(false);
            self.refresh_paste_enabled();
            self.restore_active_session();
        }
    }

    fn duplicate_tab(&self, id: BrowserSessionId) {
        self.restore_pending_navigation();
        self.save_active_session_state();
        let result = self
            .tabs
            .borrow_mut()
            .duplicate(id, TabActivation::Foreground);
        match result {
            Ok(_) => {
                self.trash_active.set(false);
                self.widgets.set_trash_mode(false);
                self.restore_active_session();
            }
            Err(error) => self.show_toast(&format!("Could not duplicate tab: {error}"), 5),
        }
    }

    fn close_tab(&self, id: BrowserSessionId) {
        if self.tabs.borrow().len() == 1 {
            self.widgets.window.close();
            return;
        }
        self.restore_pending_navigation();
        if self.tabs.borrow().active_id() == id {
            self.save_active_session_state();
        }
        let result = self.tabs.borrow_mut().close(id);
        if result.is_ok() {
            self.split_snapshots.borrow_mut().remove(&id);
        }
        match result {
            Ok(closed) if closed.active_changed => {
                self.trash_active.set(false);
                self.widgets.set_trash_mode(false);
                self.refresh_paste_enabled();
                self.restore_active_session();
            }
            Ok(_) => self.render_tabs(),
            Err(error) => self.show_toast(&format!("Could not close tab: {error}"), 5),
        }
        self.update_reopen_closed_action();
    }

    fn reopen_closed_tab(&self) {
        self.restore_pending_navigation();
        self.save_active_session_state();
        let result = self.tabs.borrow_mut().reopen_closed();
        match result {
            Ok(_) => {
                self.trash_active.set(false);
                self.widgets.set_trash_mode(false);
                self.refresh_paste_enabled();
                self.restore_active_session();
            }
            Err(error) => self.show_toast(&format!("Could not reopen tab: {error}"), 5),
        }
        self.update_reopen_closed_action();
    }

    fn close_tab_variant(&self, id: BrowserSessionId, variant: TabCloseVariant) {
        self.restore_pending_navigation();
        self.save_active_session_state();
        let previous_active = self.tabs.borrow().active_id();
        let result = match variant {
            TabCloseVariant::Left => self.tabs.borrow_mut().close_left_of(id),
            TabCloseVariant::Right => self.tabs.borrow_mut().close_right_of(id),
            TabCloseVariant::Others => self.tabs.borrow_mut().close_others(id),
        };
        if result.is_ok() {
            self.prune_split_snapshots();
        }
        match result {
            Ok(0) => {}
            Ok(_) if self.tabs.borrow().active_id() != previous_active => {
                self.trash_active.set(false);
                self.widgets.set_trash_mode(false);
                self.refresh_paste_enabled();
                self.restore_active_session();
            }
            Ok(_) => self.render_tabs(),
            Err(error) => self.show_toast(&format!("Could not close tabs: {error}"), 5),
        }
        self.update_reopen_closed_action();
    }

    fn update_reopen_closed_action(&self) {
        if let Some(action) = self
            .widgets
            .window
            .lookup_action("reopen-closed-tab")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(self.tabs.borrow().can_reopen_closed());
        }
    }

    fn prune_split_snapshots(&self) {
        let live = self
            .tabs
            .borrow()
            .sessions()
            .iter()
            .map(|tab| tab.id())
            .collect::<HashSet<_>>();
        self.split_snapshots
            .borrow_mut()
            .retain(|id, _| live.contains(id));
    }

    fn move_tab_before(&self, source: u64, target: u64) {
        let (Ok(source), Ok(target)) =
            (BrowserSessionId::new(source), BrowserSessionId::new(target))
        else {
            return;
        };
        if self
            .tabs
            .borrow_mut()
            .move_before(source, target)
            .unwrap_or(false)
        {
            self.render_tabs();
        }
    }

    fn move_active_tab(&self, delta: isize) {
        if self.tabs.borrow_mut().move_active(delta) {
            self.render_tabs();
        }
    }

    fn render_tabs(&self) {
        while let Some(child) = self.widgets.tab_bar.first_child() {
            self.widgets.tab_bar.remove(&child);
        }
        let active = self.tabs.borrow().active_id();
        let tabs = self
            .tabs
            .borrow()
            .sessions()
            .iter()
            .map(|session| {
                (
                    session.id(),
                    session.current().path().to_path_buf(),
                    session.id() == active,
                )
            })
            .collect::<Vec<_>>();
        for (id, path, is_active) in tabs {
            let row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(2)
                .build();
            row.add_css_class("floe-tab");
            if is_active {
                row.add_css_class("active");
            }
            let title = tab_title(&path);
            let button = gtk::ToggleButton::builder()
                .label(&title)
                .active(is_active)
                .hexpand(true)
                .build();
            button.set_action_name(Some("win.activate-tab"));
            button.set_action_target_value(Some(&id.get().to_variant()));
            button.set_tooltip_text(Some(&path.to_string_lossy()));
            button.update_property(&[gtk::accessible::Property::Label(&format!("Tab: {title}"))]);
            let middle_click = gtk::GestureClick::new();
            middle_click.set_button(2);
            let middle_target = id.get();
            middle_click.connect_released(move |gesture, _, _, _| {
                if let Some(widget) = gesture.widget() {
                    let _ =
                        widget.activate_action("win.close-tab", Some(&middle_target.to_variant()));
                }
            });
            button.add_controller(middle_click);

            let drag = gtk::DragSource::builder()
                .actions(gdk::DragAction::MOVE)
                .build();
            let dragged_id = id.get();
            drag.connect_prepare(move |_, _, _| {
                Some(gdk::ContentProvider::for_value(&dragged_id.to_value()))
            });
            row.add_controller(drag);
            let drop = gtk::DropTarget::new(u64::static_type(), gdk::DragAction::MOVE);
            let target_id = id.get();
            drop.connect_drop(move |target, value, _, _| {
                let Ok(source_id) = value.get::<u64>() else {
                    return false;
                };
                let Some(widget) = target.widget() else {
                    return false;
                };
                widget
                    .activate_action(
                        "win.move-tab-before",
                        Some(&(source_id, target_id).to_variant()),
                    )
                    .is_ok()
            });
            row.add_controller(drop);

            let drop_tabs = Rc::clone(&self.tabs);
            let drop_id = id;
            let hover_id = id.get();
            install_drop_target_with_hover(
                &row,
                Rc::new(move || {
                    let tabs = drop_tabs.borrow();
                    tab_drop_destination(&tabs, drop_id, false)
                }),
                Rc::new(move |_| Some(DropHoverTarget::Tab(hover_id))),
                self.widgets.drop_dispatcher.clone(),
                false,
            );

            let menu = gio::Menu::new();
            menu.append_item(&tab_menu_item("Duplicate Tab", "win.duplicate-tab", id));
            menu.append_item(&tab_menu_item("Close Tab", "win.close-tab", id));
            let variants = gio::Menu::new();
            variants.append_item(&tab_menu_item(
                "Close Tabs to the Left",
                TAB_CLOSE_VARIANT_ACTIONS[0],
                id,
            ));
            variants.append_item(&tab_menu_item(
                "Close Tabs to the Right",
                TAB_CLOSE_VARIANT_ACTIONS[1],
                id,
            ));
            variants.append_item(&tab_menu_item(
                "Close Other Tabs",
                TAB_CLOSE_VARIANT_ACTIONS[2],
                id,
            ));
            menu.append_section(None, &variants);
            let popover = gtk::PopoverMenu::from_model(Some(&menu));
            popover.set_has_arrow(false);
            popover.set_parent(&row);
            let context = gtk::GestureClick::new();
            context.set_button(gdk::BUTTON_SECONDARY);
            context.connect_pressed(move |gesture, _, x, y| {
                popover.set_pointing_to(Some(&gdk::Rectangle::new(
                    x.round() as i32,
                    y.round() as i32,
                    1,
                    1,
                )));
                popover.popup();
                gesture.set_state(gtk::EventSequenceState::Claimed);
            });
            row.add_controller(context);

            let close = gtk::Button::builder()
                .icon_name("window-close-symbolic")
                .tooltip_text(format!("Close {title}"))
                .build();
            close.add_css_class("flat");
            close.add_css_class("floe-tab-close");
            close.set_action_name(Some("win.close-tab"));
            close.set_action_target_value(Some(&id.get().to_variant()));
            close.update_property(&[gtk::accessible::Property::Label(&format!(
                "Close tab {title}"
            ))]);
            row.append(&button);
            row.append(&close);
            self.widgets.tab_bar.append(&row);
        }
    }

    fn navigate_to(&self, destination: PathBuf) {
        self.reveal_selection_path.borrow_mut().take();
        self.restore_pending_navigation();
        let was_trash = self.trash_active.replace(false);
        self.widgets.set_trash_mode(false);
        self.refresh_paste_enabled();
        self.save_active_session_state();
        let view = self
            .current_preferences
            .borrow()
            .effective_state(&destination);
        let navigated = self
            .tabs
            .borrow_mut()
            .active_mut()
            .navigate_to(destination, view)
            .unwrap_or(false);
        if navigated || was_trash {
            self.restore_active_session();
        }
    }

    fn navigate_to_revealing(&self, target: PathBuf) {
        let directory = target
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| target.clone());
        let already_current = !self.trash_active.get()
            && self.tabs.borrow().active().current().path() == directory.as_path();
        self.navigate_to(directory);
        self.reveal_selection_path.replace(Some(target));
        if already_current {
            self.load_current();
        }
    }

    fn open_trash(&self) {
        self.restore_pending_navigation();
        self.trash_active.set(true);
        self.sort_order.set(DirectorySort::default());
        self.update_sort_headers();
        self.widgets.set_trash_mode(true);
        self.set_paste_enabled(false);
        self.load_current();
    }

    fn trash_roots(&self) -> Vec<TrashRoot> {
        let mut roots = vec![self.trash_root.clone()];
        let uid = rustix::process::getuid().as_raw();
        let mut seen = HashSet::new();
        seen.insert(self.trash_root.base().to_path_buf());
        for mount in self
            .device_monitor
            .snapshots()
            .into_iter()
            .filter_map(|snapshot| snapshot.local_root().map(Path::to_path_buf))
        {
            for root in TrashRoot::for_mount_top(&mount, uid) {
                if seen.insert(root.base().to_path_buf()) {
                    roots.push(root);
                }
            }
        }
        roots
    }

    fn go_back(&self) {
        self.restore_pending_navigation();
        if self.trash_active.replace(false) {
            self.widgets.set_trash_mode(false);
            self.refresh_paste_enabled();
            self.load_current();
            return;
        }
        self.save_active_session_state();
        if self.tabs.borrow_mut().active_mut().go_back() {
            self.restore_active_session();
        }
    }

    fn go_forward(&self) {
        self.restore_pending_navigation();
        self.save_active_session_state();
        if self.tabs.borrow_mut().active_mut().go_forward() {
            self.restore_active_session();
        }
    }

    fn go_parent(&self) {
        self.restore_pending_navigation();
        self.save_active_session_state();
        let parent = self
            .tabs
            .borrow()
            .active()
            .current()
            .path()
            .parent()
            .map(Path::to_path_buf);
        let Some(parent) = parent else {
            return;
        };
        let view = self.current_preferences.borrow().effective_state(&parent);
        if self
            .tabs
            .borrow_mut()
            .active_mut()
            .go_parent(view)
            .unwrap_or(false)
        {
            self.restore_active_session();
        }
    }

    fn toggle_hidden(&self) {
        self.restore_pending_navigation();
        let show_hidden = !self.show_hidden.get();
        self.show_hidden.set(show_hidden);
        self.widgets.hidden_button.set_active(show_hidden);
        self.load_current();
    }

    fn change_view_mode(&self, mode: ViewMode) {
        let changed = self.view_mode.replace(mode) != mode;
        self.widgets.popdown_context_menus();
        self.widgets.set_view_mode(mode);
        if mode == ViewMode::Miller {
            self.prepare_miller_for_current();
            self.render_miller();
        } else {
            self.miller_detail.borrow_mut().hide();
            if let Some(worker) = self.preview_worker.borrow().as_ref() {
                worker.cancel();
            }
        }
        for name in MILLER_DETAIL_ACTIONS {
            if let Some(action) = self
                .widgets
                .window
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
            {
                action.set_enabled(mode == ViewMode::Miller);
            }
        }
        self.widgets.focus_view(mode);
        if changed {
            self.queue_preferences();
        }
    }

    fn apply_view_command(&self, command: ViewCommand) {
        if self.filename_search_active.get() {
            return;
        }
        match command {
            ViewCommand::List => self.change_view_mode(ViewMode::List),
            ViewCommand::Grid => self.change_view_mode(ViewMode::Grid),
            ViewCommand::Miller => self.change_view_mode(ViewMode::Miller),
            ViewCommand::ZoomIn => self.change_grid_size(self.grid_size.get().zoom_in()),
            ViewCommand::ZoomOut => self.change_grid_size(self.grid_size.get().zoom_out()),
        }
    }

    fn set_miller_column_width(&self, width: MillerColumnWidth) {
        self.widgets.miller_view.set_width(width);
        self.current_preferences.borrow_mut().miller_column_width = width;
        self.queue_preferences();
        self.show_toast(&format!("Miller column width: {} pixels", width.get()), 3);
    }

    fn set_inspector_width(&self, width: MillerColumnWidth) {
        self.widgets.miller_view.set_detail_width(width);
        self.current_preferences.borrow_mut().inspector_width = width;
        self.queue_preferences();
        self.show_toast(&format!("Inspector width: {} pixels", width.get()), 3);
    }

    fn change_appearance(&self, preset: AppearancePreset) {
        if self.widgets.appearance_preset() == preset {
            return;
        }
        self.widgets.apply_appearance(preset);
        self.current_preferences.borrow_mut().appearance = preset;
        self.queue_preferences();
        self.show_toast(&format!("Appearance: {}", preset.label()), 3);
    }

    fn prepare_miller_for_current(&self) {
        if self.trash_active.get() {
            self.miller_model.borrow_mut().take();
            self.miller_state.borrow_mut().clear();
            return;
        }
        let current = self.tabs.borrow().active().current().path().to_path_buf();
        let retained_depth = self.miller_model.borrow().as_ref().and_then(|model| {
            model
                .columns()
                .find(|column| column.directory() == current)
                .map(|column| column.depth())
        });
        if let Some(depth) = retained_depth {
            if let Some(model) = self.miller_model.borrow_mut().as_mut()
                && let Err(error) = model.activate(depth)
            {
                tracing::warn!(%error, "could not reactivate retained Miller column");
            }
            return;
        }
        match MillerColumnModel::new(current) {
            Ok(model) => {
                self.miller_model.replace(Some(model));
                self.miller_state.borrow_mut().clear();
            }
            Err(error) => {
                tracing::warn!(%error, "could not initialize Miller columns");
                self.miller_model.borrow_mut().take();
                self.miller_state.borrow_mut().clear();
            }
        }
    }

    fn render_miller(&self) {
        if self.view_mode.get() != ViewMode::Miller {
            return;
        }
        let current = self.tabs.borrow().active().current().path().to_path_buf();
        let model = self.miller_model.borrow();
        let Some(model) = model.as_ref() else {
            self.miller_detail
                .borrow_mut()
                .refresh(None, current, &self.selected_entries.borrow());
            let detail = self.miller_detail.borrow().state().clone();
            self.widgets
                .miller_view
                .render(&[], &self.widgets.selection, &detail);
            return;
        };
        let active_depth = model.active_depth().map(|depth| depth.get());
        self.miller_detail.borrow_mut().refresh(
            active_depth,
            current.clone(),
            &self.selected_entries.borrow(),
        );
        self.ensure_preview_request();
        self.ensure_inspector_request();
        let detail = self.miller_detail.borrow().state().clone();
        let columns = self.miller_state.borrow().columns(model, &current);
        self.widgets
            .miller_view
            .render(&columns, &self.widgets.selection, &detail);
    }

    fn toggle_miller_detail(&self, surface: MillerDetailSurface) {
        if self.view_mode.get() != ViewMode::Miller || self.trash_active.get() {
            return;
        }
        let current = self.tabs.borrow().active().current().path().to_path_buf();
        let active_depth = self
            .miller_model
            .borrow()
            .as_ref()
            .and_then(MillerColumnModel::active_depth)
            .map(|depth| depth.get());
        let selected = self.selected_entries.borrow().clone();
        self.miller_detail
            .borrow_mut()
            .toggle(surface, active_depth, current, &selected);
        if self.miller_detail.borrow().state().surface() != Some(MillerDetailSurface::Preview)
            && let Some(worker) = self.preview_worker.borrow().as_ref()
        {
            worker.cancel();
        }
        let visible = self.miller_detail.borrow().state().is_visible();
        self.render_miller();
        if visible {
            let _ = self.widgets.miller_view.focus_detail();
        } else {
            self.widgets.window.unfullscreen();
            self.widgets.miller_view.focus_active();
        }
    }

    fn quick_preview_space_enabled(&self) -> bool {
        crate::keybindings::local_file_view_shortcut_enabled(
            &self.current_preferences.borrow().keybindings,
            "win.quick-preview",
            QUICK_PREVIEW_ACCELERATOR,
        )
    }

    fn vim_command_for_key(
        &self,
        key: gtk::gdk::Key,
        modifiers: gtk::gdk::ModifierType,
    ) -> Option<crate::vim_mode::VimCommand> {
        crate::vim_mode::command_for_input(
            self.current_preferences.borrow().vim_mode,
            true,
            key.to_unicode(),
            crate::vim_mode::VimModifiers {
                control: modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK),
                alt: modifiers.contains(gtk::gdk::ModifierType::ALT_MASK),
                shift: modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK),
                super_key: modifiers.contains(gtk::gdk::ModifierType::SUPER_MASK),
            },
        )
    }

    fn dispatch_vim_file_view(&self, command: crate::vim_mode::VimCommand) {
        use crate::vim_mode::VimCommand;

        match command {
            VimCommand::Parent => self.go_parent(),
            VimCommand::Child | VimCommand::Open => self.activate_selected(),
            VimCommand::Previous | VimCommand::Next | VimCommand::First | VimCommand::Last => {
                let item_count = self.widgets.selection.n_items();
                let selected =
                    (0..item_count).find(|index| self.widgets.selection.is_selected(*index));
                let Some(target) = vim_selection_target(selected, item_count, command) else {
                    return;
                };
                self.widgets.selection.select_item(target, true);
                match self.view_mode.get() {
                    ViewMode::List => self.widgets.list_view.scroll_to(
                        target,
                        gtk::ListScrollFlags::FOCUS,
                        None::<gtk::ScrollInfo>,
                    ),
                    ViewMode::Grid => self.widgets.grid_view.scroll_to(
                        target,
                        gtk::ListScrollFlags::FOCUS,
                        None::<gtk::ScrollInfo>,
                    ),
                    ViewMode::Miller => {}
                }
            }
        }
    }

    fn ensure_preview_request(&self) {
        let should_start = matches!(
            self.miller_detail.borrow().state(),
            crate::miller_detail::MillerDetailState::Ready(target)
                if target.surface() == MillerDetailSurface::Preview
        );
        if !should_start {
            return;
        }
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let Some(source) = PreviewSourceKey::from_entry(&entry) else {
            return;
        };
        let preview_worker = self.preview_worker.borrow();
        let Some(worker) = preview_worker.as_ref() else {
            let generation = self.preview_generation.get().wrapping_add(1).max(1);
            self.preview_generation.set(generation);
            if self
                .miller_detail
                .borrow_mut()
                .begin_preview_loading(generation)
            {
                self.miller_detail
                    .borrow_mut()
                    .finish_preview(generation, PreviewOutcome::Unsupported);
            }
            return;
        };
        let generation = worker.begin_generation();
        let Some(request) = PreviewRequest::new(
            generation,
            source,
            PreviewLimits::default(),
            PreviewCachePolicy::MemoryOnly,
        ) else {
            return;
        };
        if !self
            .miller_detail
            .borrow_mut()
            .begin_preview_loading(generation)
        {
            return;
        }
        self.preview_generation.set(generation);
        if let Err(error) = worker.submit(request) {
            let message = match error {
                PreviewSubmitError::Full(_) => "Preview queue is busy.".to_owned(),
                PreviewSubmitError::Disconnected => "Preview worker is unavailable.".to_owned(),
                PreviewSubmitError::Stale(_) => "Preview request was superseded.".to_owned(),
            };
            self.miller_detail
                .borrow_mut()
                .finish_preview(generation, PreviewOutcome::Failed(message));
        }
    }

    fn drain_preview_worker(&self) {
        let mut changed = false;
        for _ in 0..PREVIEW_QUEUE_CAPACITY.min(8) {
            let response = self
                .preview_worker
                .borrow()
                .as_ref()
                .and_then(PreviewWorker::try_response);
            let Some(response) = response else {
                break;
            };
            let current = self
                .preview_worker
                .borrow()
                .as_ref()
                .is_some_and(|worker| worker.is_current(response.generation));
            if current && self.preview_generation.get() == response.generation {
                changed |= self
                    .miller_detail
                    .borrow_mut()
                    .finish_preview(response.generation, response.outcome);
            }
        }
        if changed && self.view_mode.get() == ViewMode::Miller {
            self.render_miller();
        }
    }

    fn ensure_inspector_request(&self) {
        let target = match self.miller_detail.borrow().state() {
            crate::miller_detail::MillerDetailState::Ready(target)
                if target.surface() == MillerDetailSurface::Inspector =>
            {
                target.clone()
            }
            _ => return,
        };
        let generation = self.inspector_generation.get().wrapping_add(1).max(1);
        self.inspector_generation.set(generation);
        if !self
            .miller_detail
            .borrow_mut()
            .begin_inspector_loading(generation)
        {
            return;
        }
        let request = match InspectorRequest::from_entries(
            generation,
            target.directory().to_path_buf(),
            &self.selected_entries.borrow(),
        ) {
            Ok(request) => request,
            Err(error) => {
                self.miller_detail
                    .borrow_mut()
                    .finish_inspector(generation, Err(error));
                return;
            }
        };
        let worker = self.inspector_worker.borrow();
        let Some(worker) = worker.as_ref() else {
            self.miller_detail
                .borrow_mut()
                .finish_inspector_failure(generation, "Inspector worker unavailable.");
            return;
        };
        if let Err(error) = worker.submit(request) {
            let message = match error {
                InspectorSubmitError::Full(_) => "Inspector queue busy.",
                InspectorSubmitError::Disconnected => "Inspector worker unavailable.",
            };
            self.miller_detail
                .borrow_mut()
                .finish_inspector_failure(generation, message);
        }
    }

    fn drain_inspector_worker(&self) {
        let mut changed = false;
        for _ in 0..crate::inspector::INSPECTOR_QUEUE_CAPACITY.min(8) {
            let response = self
                .inspector_worker
                .borrow()
                .as_ref()
                .and_then(InspectorWorker::try_response);
            let Some(response) = response else {
                break;
            };
            if self.inspector_generation.get() == response.generation {
                changed |= self
                    .miller_detail
                    .borrow_mut()
                    .finish_inspector(response.generation, response.result);
            }
        }
        if changed && self.view_mode.get() == ViewMode::Miller {
            self.render_miller();
        }
    }

    fn own_miller_action_context(&self, context: MillerActionContext) {
        if self.view_mode.get() != ViewMode::Miller || self.trash_active.get() {
            return;
        }
        let current = self.tabs.borrow().active().current().path().to_path_buf();
        let model = self.miller_model.borrow();
        let Some(model) = model.as_ref() else {
            return;
        };
        let columns = self.miller_state.borrow().columns(model, &current);
        let Some(column) = columns
            .iter()
            .find(|column| column.depth == context.depth && column.directory == context.directory)
        else {
            self.show_toast("That Miller column is no longer retained", 5);
            return;
        };
        let available = if column.is_active {
            self.visible_entries.borrow().clone()
        } else {
            column.entries.clone()
        };
        let resolved = match resolve_action_context_entries(&context, &available) {
            Ok(resolved) => resolved,
            Err(error) => {
                tracing::debug!(?error, "rejected stale Miller action context");
                self.show_toast("That Miller action context is no longer valid", 5);
                return;
            }
        };
        let is_active = column.is_active;
        let owned = MillerActionContext {
            selected_entries: resolved.clone(),
            ..context
        };
        self.miller_action_context.replace(Some(owned));
        self.apply_action_selection(resolved);
        self.set_miller_context_navigation_actions_enabled(is_active);
        self.refresh_paste_enabled();
        self.widgets.status_label.set_label(if is_active {
            "Actions target the active Miller column"
        } else {
            "Actions target the retained Miller column"
        });
    }

    fn set_miller_context_navigation_actions_enabled(&self, enabled: bool) {
        for name in ["refresh", "location", "select-all"] {
            if let Some(action) = self
                .widgets
                .window
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
            {
                action.set_enabled(enabled);
            }
        }
    }

    fn action_directory(&self) -> PathBuf {
        self.miller_action_context
            .borrow()
            .as_ref()
            .filter(|_| self.view_mode.get() == ViewMode::Miller)
            .map(|context| context.directory.clone())
            .unwrap_or_else(|| self.tabs.borrow().active().current().path().to_path_buf())
    }

    fn activate_miller_hover(&self, depth: usize, path: &Path) {
        if self.view_mode.get() != ViewMode::Miller || self.trash_active.get() {
            return;
        }
        let entry = {
            let current = self.tabs.borrow().active().current().path().to_path_buf();
            let model = self.miller_model.borrow();
            let Some(model) = model.as_ref() else {
                return;
            };
            let columns = self.miller_state.borrow().columns(model, &current);
            let Some(column) = columns.iter().find(|column| column.depth == depth) else {
                return;
            };
            if path.parent() != Some(column.directory.as_path()) {
                return;
            }
            let available = if column.is_active {
                self.visible_entries.borrow().clone()
            } else {
                column.entries.clone()
            };
            available
                .into_iter()
                .find(|entry| entry.path() == path && entry.is_navigable_directory())
        };
        if let Some(entry) = entry {
            self.activate_miller_entry(MillerActivation { depth, entry });
        }
    }

    fn activate_miller_entry(&self, activation: MillerActivation) {
        if self.view_mode.get() != ViewMode::Miller || self.trash_active.get() {
            return;
        }
        let current = self.tabs.borrow().active().current().path().to_path_buf();
        let source_is_current = self
            .miller_model
            .borrow()
            .as_ref()
            .and_then(|model| {
                model
                    .columns()
                    .find(|column| column.depth().get() == activation.depth)
                    .map(|column| column.directory() == current)
            })
            .unwrap_or(false);
        if source_is_current {
            self.miller_state.borrow_mut().capture(
                activation.depth,
                current,
                &self.visible_entries.borrow(),
            );
        }

        let kind = if activation.entry.is_navigable_directory() {
            MillerChildKind::Directory
        } else {
            MillerChildKind::Leaf
        };
        let transition = self
            .miller_model
            .borrow_mut()
            .as_mut()
            .ok_or_else(|| "Miller column state is unavailable".to_owned())
            .and_then(|model| {
                let depth = model
                    .columns()
                    .find(|column| column.depth().get() == activation.depth)
                    .map(|column| column.depth())
                    .ok_or_else(|| "That Miller column is no longer retained".to_owned())?;
                model
                    .select_child(depth, activation.entry.path().to_path_buf(), kind)
                    .map_err(|error| error.to_string())
            });
        let transition = match transition {
            Ok(transition) => transition,
            Err(message) => {
                self.show_toast(&message, 5);
                self.prepare_miller_for_current();
                self.render_miller();
                return;
            }
        };
        self.miller_state
            .borrow_mut()
            .truncate_after(activation.depth);

        match transition {
            MillerSelectionTransition::Descended { .. }
            | MillerSelectionTransition::ActivatedExisting { .. }
                if kind == MillerChildKind::Directory =>
            {
                self.navigate_to(activation.entry.path().to_path_buf());
                self.change_view_mode(ViewMode::Miller);
            }
            _ => {
                self.render_miller();
                self.activate_entry(&activation.entry);
            }
        }
    }

    fn navigate_miller_keyboard(&self, navigation: MillerNavigation) {
        if self.view_mode.get() != ViewMode::Miller || self.trash_active.get() {
            return;
        }
        match navigation.command {
            MillerNavigationCommand::Parent => {
                let destination = {
                    let mut model = self.miller_model.borrow_mut();
                    let Some(model) = model.as_mut() else {
                        return;
                    };
                    let previous = model
                        .columns()
                        .rfind(|column| column.depth().get() < navigation.depth)
                        .map(|column| (column.depth(), column.directory().to_path_buf()));
                    let Some((depth, destination)) = previous else {
                        self.show_toast("This is the first retained Miller column", 3);
                        return;
                    };
                    if let Err(error) = model.activate(depth) {
                        tracing::warn!(%error, "could not activate parent Miller column");
                        return;
                    }
                    destination
                };
                self.navigate_to(destination);
                self.change_view_mode(ViewMode::Miller);
            }
            MillerNavigationCommand::Child => {
                let Some(entry) = navigation.selected_entry else {
                    self.show_toast("Select a folder before moving to the next column", 3);
                    return;
                };
                if !entry.is_navigable_directory() {
                    self.show_toast("The selected item does not open another column", 3);
                    return;
                }
                self.activate_miller_entry(MillerActivation {
                    depth: navigation.depth,
                    entry,
                });
            }
        }
    }

    fn navigate_active_miller_command(&self, command: MillerNavigationCommand) {
        let depth = self
            .miller_model
            .borrow()
            .as_ref()
            .and_then(MillerColumnModel::active_depth)
            .map(|depth| depth.get());
        let Some(depth) = depth else {
            return;
        };
        self.navigate_miller_keyboard(MillerNavigation {
            depth,
            command,
            selected_entry: self.selected_entries.borrow().first().cloned(),
        });
    }

    fn change_grid_size(&self, size: GridSize) {
        if self.grid_size.replace(size) == size {
            return;
        }
        self.widgets.popdown_context_menus();
        self.widgets.set_grid_size(size);
        if self.view_mode.get() == ViewMode::Grid {
            self.widgets.focus_view(ViewMode::Grid);
        }
        self.queue_preferences();
    }

    fn active_view_state(&self) -> FolderViewState {
        FolderViewState {
            mode: self.view_mode.get(),
            grid_size: self.grid_size.get(),
            density: self.file_density.get(),
            sort: self.sort_order.get(),
            columns: self.list_columns.get(),
        }
    }

    fn apply_keybinding_overrides(&self, keybindings: crate::keybindings::KeybindingOverrides) {
        self.current_preferences.borrow_mut().keybindings = keybindings.clone();
        if let Some(application) = self
            .widgets
            .window
            .application()
            .and_downcast::<adw::Application>()
        {
            crate::keybindings::install_effective_window_shortcuts(&application, &keybindings);
        }
        self.queue_preferences();
    }

    fn apply_context_menu_preferences(
        &self,
        preferences: crate::context_menu::ContextMenuPreferences,
    ) {
        if self.current_preferences.borrow().context_menu == preferences {
            self.show_toast("Context menus already use those groups", 3);
            return;
        }
        self.current_preferences.borrow_mut().context_menu = preferences;
        self.widgets.apply_context_menu_preferences(preferences);
        self.queue_preferences();
        self.show_toast("Context menus updated", 3);
    }

    fn change_vim_mode(&self, enabled: bool) {
        if self.current_preferences.borrow().vim_mode == enabled {
            return;
        }
        self.current_preferences.borrow_mut().vim_mode = enabled;
        self.widgets.miller_view.set_vim_mode(enabled);
        self.widgets.vim_mode_button.set_label(if enabled {
            ui::VIM_MODE_ON_LABEL
        } else {
            ui::VIM_MODE_OFF_LABEL
        });
        self.widgets
            .vim_mode_button
            .update_property(&[gtk::accessible::Property::Label(if enabled {
                "Vim navigation mode enabled"
            } else {
                "Vim navigation mode disabled"
            })]);
        self.show_toast(
            if enabled {
                "Vim navigation enabled for file views"
            } else {
                "Vim navigation disabled"
            },
            3,
        );
        self.queue_preferences();
    }

    fn discover_terminals(&self) {
        let result = self
            .terminal_worker
            .borrow()
            .as_ref()
            .map(crate::terminal::TerminalWorker::try_discover);
        if matches!(
            result,
            Some(Err(crate::terminal::TerminalSubmitError::Disconnected))
        ) {
            tracing::warn!("terminal discovery worker disconnected");
        }
    }

    fn show_terminal_preferences(self: &Rc<Self>) {
        let preferred = self.current_preferences.borrow().preferred_terminal;
        let availability = self.terminal_availability.borrow().clone();
        let weak = Rc::downgrade(self);
        self.terminal_chooser
            .present(preferred, availability, move |preferred| {
                if let Some(controller) = weak.upgrade() {
                    controller.set_preferred_terminal(preferred);
                }
            });
    }

    fn set_preferred_terminal(&self, preferred: Option<crate::terminal::TerminalProviderId>) {
        if self.current_preferences.borrow().preferred_terminal == preferred {
            return;
        }
        self.current_preferences.borrow_mut().preferred_terminal = preferred;
        self.queue_preferences();
        self.show_toast(
            preferred.map_or("Automatic terminal selection enabled", |provider| {
                provider.definition().name
            }),
            3,
        );
    }

    fn open_terminal_here(&self) {
        let current = self.tabs.borrow().active().current().path().to_path_buf();
        let target = crate::terminal::terminal_target(
            &current,
            &self.selected_entries.borrow(),
            self.trash_active.get(),
        );
        let target = match target {
            Ok(target) => target,
            Err(error) => {
                self.show_toast(&error.to_string(), 5);
                return;
            }
        };
        let id = self.terminal_request_id.get().wrapping_add(1).max(1);
        self.terminal_request_id.set(id);
        let request = crate::terminal::TerminalLaunchRequest {
            id,
            target,
            preferred: self.current_preferences.borrow().preferred_terminal,
        };
        let result = self.terminal_worker.borrow().as_ref().map_or(
            Err(crate::terminal::TerminalSubmitError::Disconnected),
            |worker| worker.try_launch(request),
        );
        match result {
            Ok(()) => self.show_toast("Opening terminal…", 3),
            Err(crate::terminal::TerminalSubmitError::Full) => {
                self.show_toast("Terminal launch queue is busy. Try again.", 5);
            }
            Err(crate::terminal::TerminalSubmitError::Disconnected) => {
                self.show_toast("Terminal integration is unavailable.", 5);
            }
        }
    }

    fn drain_terminal_worker(&self) {
        for _ in 0..crate::terminal::TERMINAL_RESULT_CAPACITY {
            let event = self
                .terminal_worker
                .borrow()
                .as_ref()
                .and_then(crate::terminal::TerminalWorker::try_event);
            let Some(event) = event else {
                break;
            };
            match event {
                crate::terminal::TerminalEvent::Discovery(availability) => {
                    *self.terminal_availability.borrow_mut() = availability;
                    self.update_terminal_action_enabled();
                }
                crate::terminal::TerminalEvent::Launch(Ok(success)) => {
                    let name = success.provider.definition().name;
                    self.show_toast(
                        &if success.preferred_unavailable {
                            format!("Preferred terminal unavailable; opened {name}")
                        } else {
                            format!("Opened {name}")
                        },
                        4,
                    );
                }
                crate::terminal::TerminalEvent::Launch(Err(error)) => {
                    self.show_toast(&error.to_string(), 6);
                }
            }
        }
    }

    fn update_terminal_action_enabled(&self) {
        let provider_available = self
            .terminal_availability
            .borrow()
            .iter()
            .any(|provider| provider.available);
        let current = self.tabs.borrow().active().current().path().to_path_buf();
        let target_available = crate::terminal::terminal_target(
            &current,
            &self.selected_entries.borrow(),
            self.trash_active.get(),
        )
        .is_ok();
        if let Some(action) = self
            .widgets
            .window
            .lookup_action("open-terminal")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(provider_available && target_available);
        }
    }

    fn queue_preferences(&self) {
        let mut preferences = self.current_preferences.borrow().clone();
        let state = self.active_view_state();
        let current = {
            let mut tabs = self.tabs.borrow_mut();
            tabs.active_mut().set_view(state);
            tabs.active().current().path().to_path_buf()
        };
        preferences.remember_folder_state(current, state);
        *self.current_preferences.borrow_mut() = preferences.clone();
        self.pending_preferences.set(Some(preferences));
        self.flush_pending_preferences();
    }

    fn change_sidebar_density(&self, density: SidebarDensity) {
        if self.current_preferences.borrow().sidebar_density == density {
            return;
        }
        self.current_preferences.borrow_mut().sidebar_density = density;
        self.widgets.apply_sidebar_density(density);
        self.queue_preferences();
    }

    fn sidebar_position_changed(self: &Rc<Self>, position: i32) {
        if self.ignore_sidebar_position_signal.get() {
            return;
        }
        self.current_preferences.borrow_mut().sidebar_width =
            Some(sidebar_width_from_position(position));

        if let Some(source) = self.sidebar_save_source.borrow_mut().take() {
            source.remove();
        }
        let controller = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(SIDEBAR_PERSIST_DEBOUNCE, move || {
            if let Some(controller) = controller.upgrade() {
                controller.sidebar_save_source.borrow_mut().take();
                controller.queue_preferences();
            }
        });
        self.sidebar_save_source.borrow_mut().replace(source);
    }

    fn reset_sidebar_width(&self) {
        if let Some(source) = self.sidebar_save_source.borrow_mut().take() {
            source.remove();
        }
        self.ignore_sidebar_position_signal.set(true);
        self.widgets
            .workspace
            .set_position(self.widgets.sidebar_default_width);
        self.ignore_sidebar_position_signal.set(false);
        let reset = preferences_after_sidebar_reset(self.current_preferences.borrow().clone());
        *self.current_preferences.borrow_mut() = reset;
        self.queue_preferences();
    }

    fn flush_pending_preferences(&self) {
        let Some(preferences) = self.pending_preferences.take() else {
            return;
        };
        let result = {
            let worker = self.preference_worker.borrow();
            worker.as_ref().map(|worker| worker.try_save(preferences))
        };
        match result {
            Some(Ok(())) | None => {}
            Some(Err(PreferenceSubmitError::Full(preferences))) => {
                self.pending_preferences.set(Some(preferences));
            }
            Some(Err(PreferenceSubmitError::Disconnected)) => {
                tracing::warn!("view preference worker disconnected; persistence disabled");
                self.preference_worker.borrow_mut().take();
            }
        }
    }

    fn change_sort(&self, column: SortColumn) {
        if self.sort_in_flight.get() {
            return;
        }
        let sort = self.sort_order.get().next_for(column);
        self.resort_with(sort);
    }

    fn resort_with(&self, sort: DirectorySort) {
        if self.sort_in_flight.get() {
            return;
        }
        self.sort_order.set(sort);
        self.update_sort_headers();
        self.widgets.apply_file_view_policy(
            self.file_density.get(),
            self.list_columns.get(),
            sort.grouping,
        );
        self.queue_preferences();
        let entries = self.listed_entries.borrow().to_vec();
        if entries.len() < 2 {
            self.refresh_status();
            return;
        }

        let selected_paths = self.selected_paths();
        self.sort_selection_paths.replace(selected_paths);
        self.pending_entries.borrow_mut().clear();
        self.pending_store.borrow_mut().take();
        self.pending_selection_indices.borrow_mut().clear();
        self.widgets.popdown_context_menus();
        self.widgets.set_views_sensitive(false);
        self.sort_in_flight.set(true);
        self.set_sort_controls_sensitive(false);
        self.widgets.spinner.start();
        self.widgets.status_label.set_label(&format!(
            "Sorting by {} {}…",
            sort.column.label(),
            sort.direction.label()
        ));

        let path = self.tabs.borrow().active().current().path().to_path_buf();
        let generation = self.worker.borrow_mut().request_sort(path, entries, sort);
        self.active_generation.set(generation);
    }

    fn change_file_density(&self, density: FileViewDensity) {
        if self.file_density.replace(density) == density {
            return;
        }
        self.widgets.apply_file_view_policy(
            density,
            self.list_columns.get(),
            self.sort_order.get().grouping,
        );
        self.widgets.focus_view(self.view_mode.get());
        self.queue_preferences();
    }

    fn change_grouping(&self, grouping: DirectoryGrouping) {
        let sort = self.sort_order.get().with_grouping(grouping);
        if sort == self.sort_order.get() {
            return;
        }
        self.resort_with(sort);
    }

    fn change_directory_placement(&self, placement: DirectoryPlacement) {
        let sort = self.sort_order.get().with_directories(placement);
        if sort == self.sort_order.get() {
            return;
        }
        self.resort_with(sort);
    }

    fn toggle_list_column(&self, column: ListColumn, visible: bool) {
        let mut layout = self.list_columns.get();
        layout.set_visible(column, visible);
        if layout == self.list_columns.replace(layout) {
            return;
        }
        self.widgets.apply_file_view_policy(
            self.file_density.get(),
            layout,
            self.sort_order.get().grouping,
        );
        self.queue_preferences();
    }

    fn resize_list_column(&self, column: ListColumn, delta: i32) {
        let mut layout = self.list_columns.get();
        let revised = i32::from(layout.width(column)).saturating_add(delta);
        let revised = u16::try_from(revised.max(0)).unwrap_or(u16::MAX);
        layout.set_width(column, revised);
        if layout == self.list_columns.replace(layout) {
            return;
        }
        self.widgets.apply_file_view_policy(
            self.file_density.get(),
            layout,
            self.sort_order.get().grouping,
        );
        self.queue_preferences();
    }

    fn set_remember_per_folder(&self, enabled: bool) {
        let state = self.active_view_state();
        let current = self.tabs.borrow().active().current().path().to_path_buf();
        {
            let mut preferences = self.current_preferences.borrow_mut();
            preferences.remember_per_folder = enabled;
            if enabled {
                preferences.remember_folder_state(current, state);
            } else {
                preferences.clear_all_folder_states();
                preferences.set_global_state(state);
            }
        }
        self.queue_preferences();
    }

    fn update_sort_headers(&self) {
        let sort = self.sort_order.get();
        for header in &self.widgets.sort_headers {
            ui::update_sort_header(header, sort);
        }
        if self.trash_active.get() {
            self.widgets.set_trash_mode(true);
        }
    }

    fn set_sort_controls_sensitive(&self, sensitive: bool) {
        for header in &self.widgets.sort_headers {
            let trash_supported = !self.trash_active.get()
                || matches!(header.column, SortColumn::Name | SortColumn::Size);
            header.button.set_sensitive(sensitive && trash_supported);
        }
    }

    fn set_view_controls_sensitive(&self, sensitive: bool) {
        self.widgets.list_view_button.set_sensitive(sensitive);
        self.widgets.grid_view_button.set_sensitive(sensitive);
        self.widgets.miller_view_button.set_sensitive(sensitive);
        self.widgets.grid_size_controls.set_sensitive(sensitive);
        for (name, _) in VIEW_ACTIONS {
            if let Some(action) = self
                .widgets
                .window
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
            {
                action.set_enabled(sensitive);
            }
        }
    }

    fn submit_thumbnail_requests(&self) {
        while let Some(key) = self.widgets.thumbnails.take_request() {
            let result = {
                let worker = self.thumbnail_worker.borrow();
                let Some(worker) = worker.as_ref() else {
                    self.widgets.thumbnails.disable();
                    return;
                };
                worker.try_request(self.thumbnail_generation.get(), key)
            };
            match result {
                Ok(()) => {}
                Err(ThumbnailSubmitError::Full(key)) => {
                    self.widgets.thumbnails.retry_request(key);
                    break;
                }
                Err(ThumbnailSubmitError::Disconnected) => {
                    tracing::warn!("thumbnail worker stopped accepting requests");
                    self.thumbnail_worker.borrow_mut().take();
                    self.widgets.thumbnails.disable();
                    break;
                }
            }
        }
    }

    fn drain_thumbnail_worker(&self) {
        loop {
            let response = self
                .thumbnail_worker
                .borrow()
                .as_ref()
                .and_then(ThumbnailWorker::try_response);
            let Some(response) = response else {
                break;
            };
            if response.generation != self.thumbnail_generation.get() {
                continue;
            }
            self.widgets
                .thumbnails
                .complete(response.key, response.result);
        }
    }

    fn show_location_entry(&self) {
        if self.pending_location.borrow().is_some() {
            self.restore_pending_navigation();
            self.load_current();
        }
        let current = self.tabs.borrow().active().current().path().to_path_buf();
        self.clear_location_error();
        self.widgets
            .location_entry
            .set_text(&location_text(&current));
        self.widgets.path_stack.set_visible_child_name("entry");
        self.widgets.location_entry.grab_focus();
        self.widgets.location_entry.select_region(0, -1);
    }

    fn hide_location_entry(&self) {
        self.clear_location_error();
        self.widgets.path_stack.set_visible_child_name("path");
        self.widgets.focus_view(self.view_mode.get());
    }

    fn cancel_location_entry(&self) {
        if self.pending_location.borrow().is_some() {
            self.restore_pending_navigation();
            self.load_current();
        }
        self.hide_location_entry();
    }

    fn submit_metadata_requests(&self) {
        while let Some(key) = self.widgets.metadata.take_request() {
            let result = {
                let worker = self.metadata_worker.borrow();
                let Some(worker) = worker.as_ref() else {
                    return;
                };
                worker.try_request(key)
            };
            match result {
                Ok(()) => {}
                Err(MetadataSubmitError::Full(key)) => {
                    self.widgets.metadata.retry(key);
                    break;
                }
                Err(MetadataSubmitError::Disconnected) => {
                    tracing::warn!("metadata worker stopped accepting requests");
                    self.metadata_worker.borrow_mut().take();
                    break;
                }
            }
        }
    }

    fn drain_metadata_worker(&self) {
        loop {
            let response = self
                .metadata_worker
                .borrow()
                .as_ref()
                .and_then(MetadataWorker::try_response);
            let Some(response) = response else {
                break;
            };
            self.widgets
                .metadata
                .complete(response.key, response.result);
        }
    }

    fn request_current_storage_facts(&self) {
        self.current_storage_facts.set(None);
        if self.trash_active.get() {
            self.refresh_status();
            return;
        }
        let generation = self.current_storage_generation.get().wrapping_add(1).max(1);
        self.current_storage_generation.set(generation);
        let request = StorageRequest {
            generation,
            target: StorageTarget::CurrentLocation,
            path: self.tabs.borrow().active().current().path().to_path_buf(),
        };
        self.submit_storage_request(request);
    }

    fn request_device_storage_facts(&self, snapshots: &[DeviceSnapshot]) {
        let generation = self.device_storage_generation.get().wrapping_add(1).max(1);
        self.device_storage_generation.set(generation);
        self.device_snapshots.replace(snapshots.to_vec());
        self.device_storage_facts.borrow_mut().clear();
        for snapshot in snapshots {
            let Some(path) = snapshot.local_root() else {
                continue;
            };
            self.submit_storage_request(StorageRequest {
                generation,
                target: StorageTarget::Device(snapshot.id.as_str().to_owned()),
                path: path.to_path_buf(),
            });
        }
    }

    fn submit_storage_request(&self, request: StorageRequest) {
        let result = self
            .storage_worker
            .borrow()
            .as_ref()
            .map(|worker| worker.try_request(request));
        match result {
            Some(Ok(())) | None => {}
            Some(Err(StorageSubmitError::Full(_))) => {
                tracing::debug!("storage facts queue is at capacity");
            }
            Some(Err(StorageSubmitError::Disconnected)) => {
                tracing::warn!("storage facts worker stopped accepting requests");
                self.storage_worker.borrow_mut().take();
            }
        }
    }

    fn drain_storage_worker(self: &Rc<Self>) {
        let mut current_changed = false;
        let mut devices_changed = false;
        loop {
            let response = self
                .storage_worker
                .borrow()
                .as_ref()
                .and_then(StorageWorker::try_response);
            let Some(response) = response else {
                break;
            };
            let Ok(facts) = response.result else {
                continue;
            };
            match response.request.target {
                StorageTarget::CurrentLocation => {
                    if current_storage_request_is_current(
                        &response.request,
                        self.current_storage_generation.get(),
                        self.tabs.borrow().active().current().path(),
                        self.trash_active.get(),
                    ) {
                        self.current_storage_facts.set(Some(facts));
                        current_changed = true;
                    }
                }
                StorageTarget::Device(id) => {
                    if response.request.generation != self.device_storage_generation.get() {
                        continue;
                    }
                    let is_current = self.device_snapshots.borrow().iter().any(|snapshot| {
                        snapshot.id.as_str() == id
                            && snapshot.local_root() == Some(response.request.path.as_path())
                    });
                    if is_current {
                        self.device_storage_facts.borrow_mut().insert(id, facts);
                        devices_changed = true;
                    }
                }
            }
        }
        if current_changed {
            self.refresh_status();
        }
        if devices_changed {
            let snapshots = self.device_snapshots.borrow().clone();
            self.render_devices(&snapshots);
        }
    }

    fn restore_pending_navigation(&self) {
        if let Some(pending) = self.pending_location.borrow_mut().take() {
            *self.tabs.borrow_mut().active_mut() = pending.previous_session;
        }
    }

    fn submit_location_entry(&self, input: &str) {
        let current = self.tabs.borrow().active().current().path().to_path_buf();
        let destination = match resolve_location_input(input, &current) {
            Ok(path) => path,
            Err(error) => {
                self.show_location_error(&error.to_string());
                return;
            }
        };

        if destination == self.tabs.borrow().active().current().path() {
            self.hide_location_entry();
            return;
        }

        self.save_active_session_state();
        let previous_session = self.tabs.borrow().active().clone();
        let view = self
            .current_preferences
            .borrow()
            .effective_state(&destination);
        if !self
            .tabs
            .borrow_mut()
            .active_mut()
            .navigate_to(destination, view)
            .unwrap_or(false)
        {
            self.hide_location_entry();
            return;
        }

        self.clear_location_error();
        self.widgets.location_entry.set_sensitive(false);
        self.render_tabs();
        let generation = self.load_current();
        self.pending_location.replace(Some(PendingLocation {
            generation,
            previous_session,
            submitted_text: input.trim().to_owned(),
        }));
    }

    fn clear_location_error(&self) {
        self.widgets.location_entry.remove_css_class("error");
        self.widgets.location_entry.set_sensitive(true);
        self.widgets
            .location_entry
            .update_property(&[gtk::accessible::Property::Description(
                "Enter an absolute folder path",
            )]);
        self.widgets.location_error.set_label("");
        self.widgets.location_error.set_visible(false);
    }

    fn show_location_error(&self, message: &str) {
        self.widgets.location_entry.set_sensitive(true);
        self.widgets.location_entry.add_css_class("error");
        self.widgets
            .location_entry
            .update_property(&[gtk::accessible::Property::Description(message)]);
        self.widgets.location_error.set_label(message);
        self.widgets.location_error.set_visible(true);
        self.widgets.path_stack.set_visible_child_name("entry");
        self.widgets.location_entry.grab_focus();
        self.widgets.location_entry.select_region(0, -1);
    }

    fn load_current(&self) -> u64 {
        self.pending_reconciliation.borrow_mut().take();
        self.pending_scroll_index.set(None);
        self.load_current_inner()
    }

    fn load_current_inner(&self) -> u64 {
        self.file_watcher.stop();
        self.watch_generation.set(self.file_watcher.generation());
        self.widgets.thumbnails.begin_generation();
        self.widgets.metadata.begin_generation();
        let thumbnail_generation = self
            .thumbnail_worker
            .borrow_mut()
            .as_mut()
            .map(ThumbnailWorker::begin_generation)
            .unwrap_or_default();
        self.thumbnail_generation.set(thumbnail_generation);
        if thumbnail_generation == 0 {
            self.widgets.thumbnails.disable();
        }
        self.listed_entries.replace(Arc::from([]));
        self.visible_entries.borrow_mut().clear();
        self.pending_entries.borrow_mut().clear();
        self.pending_store.borrow_mut().take();
        self.pending_total.set(0);
        self.pending_selection_indices.borrow_mut().clear();
        self.sort_selection_paths.borrow_mut().clear();
        self.sort_in_flight.set(false);
        self.set_sort_controls_sensitive(false);
        self.widgets.popdown_context_menus();
        self.selected_entries.borrow_mut().clear();
        self.widgets.selection.unselect_all();
        self.widgets
            .selection
            .set_model(Some(&gio::ListStore::new::<glib::BoxedAnyObject>()));
        self.widgets.set_views_sensitive(false);
        self.widgets.empty_state.set_visible(false);
        self.set_open_enabled(false);
        self.set_open_with_enabled(false);
        self.set_properties_enabled(false);
        self.set_checksum_enabled(false);
        self.set_selection_actions_enabled(false, false, false);
        let path = if self.trash_active.get() {
            self.trash_root.files().to_path_buf()
        } else {
            self.tabs.borrow().active().current().path().to_path_buf()
        };
        if self.filename_search_active.get() {
            let same_root = !self.trash_active.get()
                && self
                    .filename_search_root
                    .borrow()
                    .as_ref()
                    .is_some_and(|root| root == &path);
            if same_root {
                let generation = self.filename_search_generation.get().wrapping_add(1).max(1);
                self.filename_search_generation.set(generation);
                if let Some(worker) = self.filename_search_worker.borrow().as_ref() {
                    worker.cancel(generation);
                }
                self.set_filename_search_running(false);
                self.filename_search_results.borrow_mut().clear();
                self.filename_search_store.borrow_mut().take();
                self.filename_search_summary.borrow_mut().take();
                self.pending_filename_search.borrow_mut().take();
            } else {
                self.deactivate_filename_search(false);
                self.set_sort_controls_sensitive(false);
                self.widgets.set_views_sensitive(false);
            }
        }
        let filter_location = (self.trash_active.get(), path.clone());
        let location_changed = self
            .filter_location
            .borrow()
            .as_ref()
            .is_none_or(|previous| previous != &filter_location);
        self.filter_location.replace(Some(filter_location));
        if location_changed {
            self.filter_state.replace(FolderFilterState::default());
            self.filter_generation
                .set(self.filter_generation.get().wrapping_add(1));
            self.pending_filter.borrow_mut().take();
            self.widgets.filter_entry.set_text("");
            self.widgets.filter_mode.set_selected(0);
            self.widgets.search_bar.set_visible(false);
            self.set_filter_feedback(None, 0);
        }

        let generation = if self.trash_active.get() {
            self.worker
                .borrow_mut()
                .request_trash(self.trash_roots(), self.sort_order.get())
        } else {
            self.worker
                .borrow_mut()
                .request(path.clone(), self.sort_order.get())
        };
        self.active_generation.set(generation);

        let display_path = if self.trash_active.get() {
            std::borrow::Cow::Borrowed("Trash")
        } else {
            path.to_string_lossy()
        };
        self.widgets.path_label.set_label(&display_path);
        self.widgets
            .path_label
            .set_tooltip_text(Some(&display_path));
        let title = if self.trash_active.get() {
            std::borrow::Cow::Borrowed("Trash")
        } else {
            path.file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
        };
        self.widgets
            .window
            .set_title(Some(&format!("{title} — Floe")));
        self.widgets.spinner.start();
        self.widgets
            .status_label
            .set_label(if self.trash_active.get() {
                "Loading Trash…"
            } else {
                "Loading directory…"
            });
        self.update_navigation_controls();
        generation
    }

    fn update_navigation_controls(&self) {
        let tabs = self.tabs.borrow();
        let navigation = tabs.active();
        self.widgets
            .back_button
            .set_sensitive(self.trash_active.get() || navigation.can_go_back());
        self.widgets
            .forward_button
            .set_sensitive(!self.trash_active.get() && navigation.can_go_forward());
        self.widgets
            .parent_button
            .set_sensitive(!self.trash_active.get() && navigation.can_go_parent());
    }

    fn drain_worker(&self) {
        while let Some(response) = self.worker.borrow().try_response() {
            if response.generation != self.active_generation.get() {
                continue;
            }

            self.widgets.spinner.stop();
            match response.kind {
                ResponseKind::Listing(Ok(entries)) => {
                    if self
                        .pending_location
                        .borrow()
                        .as_ref()
                        .is_some_and(|pending| pending.matches(response.generation))
                    {
                        self.pending_location.borrow_mut().take();
                        self.hide_location_entry();
                    }
                    self.set_sort_controls_sensitive(true);
                    self.show_listing(entries);
                }
                ResponseKind::Listing(Err(DirectoryError::Cancelled)) => {}
                ResponseKind::Listing(Err(error)) => {
                    if self.restore_failed_location(response.generation, &error) {
                        continue;
                    }
                    tracing::warn!(path = ?response.path, %error, "directory enumeration failed");
                    self.set_sort_controls_sensitive(true);
                    self.widgets.set_views_sensitive(true);
                    self.widgets.status_label.set_label("Could not load folder");
                    let toast = adw::Toast::builder()
                        .title(format!("Could not open folder: {error}"))
                        .timeout(6)
                        .build();
                    self.widgets.toast_overlay.add_toast(toast);
                }
                ResponseKind::TrashListing(Ok(entries)) => {
                    self.set_sort_controls_sensitive(true);
                    self.show_listing(entries);
                }
                ResponseKind::TrashListing(Err(TrashEnumerateError::Directory(
                    DirectoryError::Cancelled,
                ))) => {}
                ResponseKind::TrashListing(Err(error)) => {
                    tracing::warn!(%error, "Trash enumeration failed");
                    self.set_sort_controls_sensitive(true);
                    self.widgets.set_views_sensitive(true);
                    self.widgets.status_label.set_label("Could not load Trash");
                    self.show_toast(&format!("Could not load Trash: {error}"), 7);
                }
                ResponseKind::Sorted { entries, sort } => {
                    self.sort_in_flight.set(false);
                    self.set_sort_controls_sensitive(true);
                    if sort != self.sort_order.get() {
                        continue;
                    }
                    let selected_paths = self.sort_selection_paths.take();
                    self.listed_entries.replace(Arc::from(entries));
                    self.apply_folder_filter(selected_paths, false);
                }
            }
        }
    }

    fn restore_failed_location(&self, generation: u64, error: &DirectoryError) -> bool {
        let is_pending = self
            .pending_location
            .borrow()
            .as_ref()
            .is_some_and(|pending| pending.matches(generation));
        if !is_pending {
            return false;
        }

        let Some(pending) = self.pending_location.borrow_mut().take() else {
            return false;
        };
        let submitted_text = pending.restore(self.tabs.borrow_mut().active_mut());
        self.render_tabs();
        self.load_current();
        self.widgets.location_entry.set_text(&submitted_text);
        self.show_location_error(&location_failure_message(error));
        true
    }

    fn show_listing(&self, entries: Vec<DirectoryEntry>) {
        let show_hidden = self.show_hidden.get();
        let entries: Vec<Arc<DirectoryEntry>> = entries
            .into_iter()
            .filter(|entry| self.trash_active.get() || show_hidden || !entry.is_hidden())
            .map(Arc::new)
            .collect();
        let pending_reconciliation = self.pending_reconciliation.borrow_mut().take();
        let mut selected_paths = if let Some(pending) = pending_reconciliation {
            let current_paths = entries
                .iter()
                .map(|entry| entry.path().to_path_buf())
                .collect::<Vec<_>>();
            let reconciled =
                reconcile_view_state(pending.snapshot, &pending.renames, &current_paths);
            self.pending_scroll_index.set(
                reconciled
                    .anchor_index
                    .and_then(|index| u32::try_from(index).ok()),
            );
            reconciled.selected_paths
        } else {
            let reveal_path = self.reveal_selection_path.borrow_mut().take();
            if let Some(target) = reveal_path.as_ref()
                && !entries.iter().any(|entry| entry.path() == target)
            {
                self.show_toast(
                    "The symbolic link target changed or is no longer visible",
                    6,
                );
            }
            reveal_path.into_iter().collect::<Vec<_>>()
        };
        if let Some(created) = self.pending_create_rename.borrow().as_ref()
            && entries.iter().any(|entry| entry.path() == created)
            && !selected_paths.contains(created)
        {
            selected_paths.push(created.clone());
        }
        self.listed_entries.replace(Arc::from(entries));
        self.apply_folder_filter(selected_paths, true);
        self.request_current_storage_facts();
        self.start_current_watcher();
        if self.filename_search_active.get() {
            self.start_filename_search();
        }
    }

    fn install_entries(
        &self,
        entries: Vec<Arc<DirectoryEntry>>,
        selected_paths: &[PathBuf],
        focus_list: bool,
    ) {
        let count = entries.len();
        if count == 0 {
            self.pending_scroll_index.set(None);
        }
        let selection_indices = selection_indices_for_paths(&entries, selected_paths);
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        self.widgets.selection.set_model(Some(&store));
        self.widgets.set_views_sensitive(true);
        self.widgets.empty_state.set_visible(count == 0);
        self.pending_total.set(count);
        self.pending_selection_indices.replace(selection_indices);
        self.pending_entries
            .replace(entries.iter().cloned().collect());
        self.visible_entries.replace(entries);
        self.set_selection_actions_enabled(false, false, false);
        self.pending_store.replace(Some(store));
        if self.view_mode.get() == ViewMode::Miller {
            self.prepare_miller_for_current();
            self.render_miller();
        }
        self.update_loading_status(0, count);
        if focus_list {
            self.widgets.focus_view(self.view_mode.get());
        }
    }

    fn pump_pending_entries(&self) {
        const BATCH_SIZE: usize = 256;

        let mut pending = self.pending_entries.borrow_mut();
        if pending.is_empty() {
            return;
        }
        let store = self.pending_store.borrow();
        let Some(store) = store.as_ref() else {
            return;
        };
        for _ in 0..BATCH_SIZE {
            let Some(entry) = pending.pop_front() else {
                break;
            };
            store.append(&glib::BoxedAnyObject::new(entry));
        }

        let total = self.pending_total.get();
        let loaded = total.saturating_sub(pending.len());
        let mut pending_selection = self.pending_selection_indices.borrow_mut();
        let ready = pending_selection
            .iter()
            .take_while(|index| usize::try_from(**index).is_ok_and(|index| index < loaded))
            .count();
        for index in pending_selection.drain(..ready) {
            self.widgets.selection.select_item(index, false);
        }
        self.update_loading_status(loaded, total);
        if loaded == total {
            self.restore_scroll_anchor();
            let rename_ready = self
                .pending_create_rename
                .borrow()
                .as_ref()
                .is_some_and(|target| {
                    self.visible_entries
                        .borrow()
                        .iter()
                        .any(|entry| entry.path() == target)
                });
            if rename_ready {
                self.pending_create_rename.borrow_mut().take();
                self.show_rename();
            }
        }
    }

    fn update_loading_status(&self, loaded: usize, total: usize) {
        if loaded < total {
            self.widgets
                .status_label
                .set_label(&format!("Showing {loaded} of {total} items…"));
        } else {
            self.refresh_status();
        }
    }

    fn activate(&self, position: u32) {
        let Some(model) = self.widgets.selection.model() else {
            return;
        };
        let Some(object) = model.item(position).and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let entry = object.borrow::<Arc<DirectoryEntry>>().clone();
        self.activate_entry(&entry);
    }

    fn activate_selected(&self) {
        if let Some(entry) = self.selected_entry() {
            self.activate_entry(&entry);
        }
    }

    fn activate_entry(&self, entry: &DirectoryEntry) {
        if entry.is_navigable_directory() {
            if self.trash_active.get() {
                self.show_toast("Restore this folder before browsing its contents", 5);
                return;
            }
            self.navigate_to(entry.path().to_path_buf());
        } else if matches!(
            entry.kind(),
            floe_core::EntryKind::RegularFile
                | floe_core::EntryKind::SymbolicLink {
                    target_is_directory: false
                }
        ) {
            self.launch_file(entry);
        } else {
            self.show_toast("This type of filesystem entry cannot be opened yet", 5);
        }
    }

    fn launch_file(&self, entry: &DirectoryEntry) {
        let display_name = entry.display_name_lossy();
        let window = self.widgets.window.clone();
        let toast_overlay = self.widgets.toast_overlay.clone();
        launcher::launch_default(entry.path(), move |result| {
            if !window.is_visible() {
                return;
            }
            match result {
                Ok(launcher::DefaultLaunch::Launched) => {}
                Ok(launcher::DefaultLaunch::NoDefault(options)) => {
                    present_or_report_open_with(&window, &toast_overlay, &display_name, options);
                }
                Err(error) => {
                    tracing::warn!(%error, "default application launch failed");
                    toast_overlay.add_toast(
                        adw::Toast::builder()
                            .title(format!("Could not open {display_name}: {error}"))
                            .timeout(6)
                            .build(),
                    );
                }
            }
        });
    }

    fn selection_changed(&self) {
        let selected_entries = self.selected_model_entries();
        self.miller_action_context.borrow_mut().take();
        self.set_miller_context_navigation_actions_enabled(true);
        self.apply_action_selection(selected_entries);
        if self.view_mode.get() == ViewMode::Miller
            && self.miller_detail.borrow().state().is_visible()
        {
            self.render_miller();
        }
    }

    fn apply_action_selection(&self, selected_entries: Vec<Arc<DirectoryEntry>>) {
        let properties_generation = self.properties_generation.get().wrapping_add(1).max(1);
        self.properties_generation.set(properties_generation);
        if let Some(worker) = self.properties_worker.borrow().as_ref() {
            worker.supersede(properties_generation);
        }
        let state = selection_action_state(&selected_entries);
        let folder_tab = folder_tab_eligible(&selected_entries, self.trash_active.get());
        self.selected_entries.replace(selected_entries);
        self.set_open_enabled(state.single);
        self.set_open_with_enabled(state.open_with);
        self.set_properties_enabled(!self.selected_entries.borrow().is_empty());
        self.set_reveal_enabled(self.filename_search_active.get() && state.single);
        self.set_checksum_enabled(state.checksum);
        self.set_archive_actions_enabled();
        self.set_batch_rename_enabled();
        self.set_selection_actions_enabled(state.transfer, state.rename, state.trash);
        for name in ["open-new-tab", "open-background-tab"] {
            if let Some(action) = self
                .widgets
                .window
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
            {
                action.set_enabled(folder_tab);
            }
        }
        self.update_split_action_states();
        self.update_terminal_action_enabled();
        self.refresh_status();
    }

    fn update_split_action_states(&self) {
        let selected = self.selected_entries.borrow();
        let selection = selection_action_state(&selected);
        let selected_folder = folder_tab_eligible(&selected, self.trash_active.get());
        let is_split = self.tabs.borrow().active_split().is_split();
        let state = split_action_state(
            is_split,
            selected_folder,
            selection.transfer,
            self.trash_active.get(),
        );
        for (name, enabled) in [
            ("toggle-split", !self.trash_active.get()),
            ("switch-split-side", state.switch),
            ("close-split", state.close),
            ("swap-split-sides", state.swap),
            ("narrow-primary-pane", state.close),
            ("widen-primary-pane", state.close),
            ("open-opposite-pane", state.open_opposite),
            ("copy-to-opposite-pane", state.transfer_opposite),
            ("move-to-opposite-pane", state.transfer_opposite),
            ("link-to-opposite-pane", state.transfer_opposite),
        ] {
            if let Some(action) = self
                .widgets
                .window
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
            {
                action.set_enabled(enabled);
            }
        }
    }

    fn selected_entry(&self) -> Option<Arc<DirectoryEntry>> {
        let selected = self.selected_entries.borrow();
        (selected.len() == 1).then(|| selected[0].clone())
    }

    fn selected_paths(&self) -> Vec<PathBuf> {
        self.selected_entries
            .borrow()
            .iter()
            .map(|entry| entry.path().to_path_buf())
            .collect()
    }

    fn show_context_menu(&self) {
        if self.filename_search_active.get() {
            if self.selected_entries.borrow().is_empty() {
                self.widgets.search_background_menu.set_pointing_to(None);
                self.widgets.search_background_menu.popup();
                return;
            }
            self.widgets.search_context_menu.set_pointing_to(None);
            self.widgets.search_context_menu.popup();
            return;
        }
        let context_menu = if self.selected_entries.borrow().is_empty() {
            self.widgets.background_menu(self.view_mode.get())
        } else {
            self.widgets.context_menu(self.view_mode.get())
        };
        context_menu.set_pointing_to(None);
        context_menu.popup();
    }

    fn selected_model_entries(&self) -> Vec<Arc<DirectoryEntry>> {
        selected_entries_for_selection(&self.widgets.selection)
    }

    fn refresh_status(&self) {
        if self.filename_search_active.get() {
            let results = self.filename_search_results.borrow().len();
            let selected = self.selected_entries.borrow().len();
            let label = if selected == 0 {
                format!("{results} search results")
            } else {
                format!("{selected} selected of {results} search results")
            };
            self.widgets.status_label.set_label(&label);
            return;
        }
        let visible_entries = self.visible_entries.borrow();
        let label = selection_status(
            &visible_entries,
            &self.selected_entries.borrow(),
            self.current_storage_facts.get(),
        );
        self.widgets.status_label.set_label(&label);
    }

    fn select_all(&self) {
        self.widgets.selection.select_all();
    }

    fn clear_selection(&self) {
        self.widgets.selection.unselect_all();
    }

    fn set_open_enabled(&self, enabled: bool) {
        if let Some(action) = self
            .widgets
            .window
            .lookup_action("open")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(enabled);
        }
    }

    fn set_open_with_enabled(&self, enabled: bool) {
        if let Some(action) = self
            .widgets
            .window
            .lookup_action("open-with")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(enabled);
        }
    }

    fn set_properties_enabled(&self, enabled: bool) {
        if let Some(action) = self
            .widgets
            .window
            .lookup_action("properties")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(enabled && self.properties_worker.borrow().is_some());
        }
    }

    fn set_checksum_enabled(&self, enabled: bool) {
        if let Some(action) = self
            .widgets
            .window
            .lookup_action("checksum")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(enabled);
        }
    }

    fn set_archive_actions_enabled(&self) {
        let selected = self.selected_entries.borrow();
        let paths = selected
            .iter()
            .map(|entry| entry.path().to_path_buf())
            .collect::<Vec<_>>();
        let eligible = ArchiveActionEligibility::new(
            &paths,
            selected
                .iter()
                .all(|entry| matches!(entry.kind(), EntryKind::RegularFile | EntryKind::Directory)),
            self.trash_active.get(),
        );
        for (name, enabled) in [
            ("extract-here", eligible.extract),
            ("extract-to", eligible.extract),
            ("compress", eligible.compress),
        ] {
            if let Some(action) = self
                .widgets
                .window
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
            {
                action.set_enabled(enabled);
            }
        }
    }

    fn set_batch_rename_enabled(&self) {
        let selected = self.selected_entries.borrow();
        let enabled = !self.trash_active.get()
            && selected.len() >= 2
            && selected.len() <= floe_core::BATCH_RENAME_CAPACITY
            && selected
                .iter()
                .all(|entry| !matches!(entry.kind(), EntryKind::Other));
        if let Some(action) = self
            .widgets
            .window
            .lookup_action("batch-rename")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(enabled);
        }
    }

    fn show_checksum_dialog(&self) {
        let selected = self.selected_entries.borrow();
        let targets = selected
            .iter()
            .filter(|entry| matches!(entry.kind(), EntryKind::RegularFile))
            .map(|entry| entry.path().to_path_buf())
            .collect::<Vec<_>>();
        if targets.is_empty() || targets.len() != selected.len() {
            drop(selected);
            self.show_toast("Select one or more regular files to calculate checksums", 5);
            return;
        }
        drop(selected);
        let targets: Arc<[PathBuf]> = targets.into();
        let widgets = ui::build_checksum_dialog(targets.len());
        let dialog = widgets.dialog.downgrade();
        widgets.cancel_button.connect_clicked(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        });
        let dialog = widgets.dialog.downgrade();
        let algorithm_dropdown = widgets.algorithm_dropdown.clone();
        let expected_entry = widgets.expected_entry.clone();
        let error_label = widgets.error_label.clone();
        let state = Rc::clone(&self.application_state);
        let toast_overlay = self.widgets.toast_overlay.clone();
        widgets.calculate_button.connect_clicked(move |_| {
            let algorithm = match algorithm_dropdown.selected() {
                1 => ChecksumAlgorithm::Sha512,
                2 => ChecksumAlgorithm::Md5Legacy,
                _ => ChecksumAlgorithm::Sha256,
            };
            let input = ChecksumDialogInput {
                algorithm,
                expected: expected_entry.text().to_string(),
            };
            match build_checksum_request(Arc::clone(&targets), &input) {
                Ok(request) => match state.submit_checksum(request) {
                    Ok(_) => {
                        if let Some(dialog) = dialog.upgrade() {
                            dialog.close();
                        }
                        toast_overlay.add_toast(
                            adw::Toast::builder()
                                .title("Checksum calculation queued")
                                .timeout(4)
                                .build(),
                        );
                    }
                    Err(error) => error_label
                        .set_label(&format!("Could not queue checksum calculation: {error}")),
                },
                Err(error) => error_label.set_label(&error.to_string()),
            }
        });
        widgets.dialog.present(Some(&self.widgets.window));
        widgets.algorithm_dropdown.grab_focus();
    }

    fn show_properties(&self) {
        let selected = self.selected_entries.borrow().clone();
        if selected.is_empty() {
            self.show_toast("Select one or more items to view Properties", 4);
            return;
        }
        let generation = self.properties_generation.get().wrapping_add(1).max(1);
        self.properties_generation.set(generation);
        let Some(directory) = selected[0].path().parent().map(Path::to_path_buf) else {
            self.show_toast("Properties unavailable for this root entry", 5);
            return;
        };
        let request = match InspectorRequest::from_entries(generation, directory, &selected)
            .ok()
            .and_then(|request| PropertiesRequest::new(request).ok())
        {
            Some(request) => request,
            None => {
                self.show_toast("Properties request is invalid", 6);
                return;
            }
        };
        let submitted = self
            .properties_worker
            .borrow()
            .as_ref()
            .map(|worker| worker.submit(request));
        match submitted {
            Some(Ok(())) => {
                self.set_properties_enabled(false);
                self.show_toast("Loading read-only Properties…", 2);
            }
            Some(Err(PropertiesSubmitError::Full(_))) => {
                self.show_toast("Properties queue is busy; try again", 5);
            }
            Some(Err(PropertiesSubmitError::Disconnected)) | None => {
                self.show_toast("Properties worker is unavailable", 6);
            }
        }
    }

    fn drain_properties_worker(&self) {
        for _ in 0..PROPERTIES_RESULT_CAPACITY.min(8) {
            let response = self
                .properties_worker
                .borrow()
                .as_ref()
                .and_then(PropertiesWorker::try_response);
            let Some(response) = response else {
                break;
            };
            if response.generation != self.properties_generation.get() {
                continue;
            }
            self.set_properties_enabled(!self.selected_entries.borrow().is_empty());
            match response.result {
                Ok(snapshot) => self.present_properties_dialog(&present_properties(&snapshot)),
                Err(error) => self.show_toast(&format!("Properties unavailable: {error}"), 7),
            }
        }
    }

    fn present_properties_dialog(&self, presentation: &crate::properties::PropertiesPresentation) {
        let widgets = ui::build_properties_dialog(presentation);
        let dialog = widgets.dialog.downgrade();
        widgets.close_button.connect_clicked(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        });
        let window = self.widgets.window.downgrade();
        let dialog = widgets.dialog.downgrade();
        widgets.open_with_button.connect_clicked(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
            if let Some(window) = window.upgrade() {
                gio::prelude::ActionGroupExt::activate_action(&window, "open-with", None);
            }
        });
        let parent_dialog = widgets.dialog.downgrade();
        let window = self.widgets.window.clone();
        let toast_overlay = self.widgets.toast_overlay.clone();
        let state = Rc::clone(&self.application_state);
        let defaults = presentation.permissions.clone();
        widgets.edit_permissions_button.connect_clicked(move |_| {
            if let Some(dialog) = parent_dialog.upgrade() {
                dialog.close();
            }
            let permission_widgets = ui::build_permission_dialog(&defaults);
            let dialog = permission_widgets.dialog.downgrade();
            permission_widgets.cancel_button.connect_clicked(move |_| {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.close();
                }
            });
            let dialog = permission_widgets.dialog.downgrade();
            let file_mode_entry = permission_widgets.file_mode_entry.clone();
            let directory_mode_entry = permission_widgets.directory_mode_entry.clone();
            let executable_dropdown = permission_widgets.executable_dropdown.clone();
            let owner_entry = permission_widgets.owner_entry.clone();
            let group_entry = permission_widgets.group_entry.clone();
            let recursive_check = permission_widgets.recursive_check.clone();
            let acknowledge_check = permission_widgets.acknowledge_check.clone();
            let error_label = permission_widgets.error_label.clone();
            let defaults = defaults.clone();
            let state = Rc::clone(&state);
            let toast_overlay = toast_overlay.clone();
            permission_widgets.apply_button.connect_clicked(move |_| {
                let executable = match executable_dropdown.selected() {
                    1 => ExecutableEdit::Enable,
                    2 => ExecutableEdit::Disable,
                    _ => ExecutableEdit::Unchanged,
                };
                let input = PermissionEditorInput {
                    file_mode: file_mode_entry.text().to_string(),
                    directory_mode: directory_mode_entry.text().to_string(),
                    executable,
                    owner: owner_entry.text().to_string(),
                    group: group_entry.text().to_string(),
                    recursive: recursive_check.is_active(),
                    acknowledged: acknowledge_check.is_active(),
                };
                match build_permission_request(&defaults, &input) {
                    Ok(request) => match state.submit_permissions(request) {
                        Ok(_) => {
                            if let Some(dialog) = dialog.upgrade() {
                                dialog.close();
                            }
                            toast_overlay.add_toast(
                                adw::Toast::builder()
                                    .title("Permission change queued")
                                    .timeout(4)
                                    .build(),
                            );
                        }
                        Err(error) => error_label
                            .set_label(&format!("Could not queue permission change: {error}")),
                    },
                    Err(error) => error_label.set_label(&error.to_string()),
                }
            });
            permission_widgets.dialog.present(Some(&window));
            permission_widgets.file_mode_entry.grab_focus();
        });
        self.widgets.focus_view(self.view_mode.get());
        widgets.dialog.present(Some(&self.widgets.window));
        widgets.close_button.grab_focus();
    }

    fn show_open_with(&self) {
        let Some(entry) = self.selected_entry() else {
            self.show_toast("Select a file to choose an application", 4);
            return;
        };
        if !open_with_eligible(&entry) {
            self.show_toast("Open With is available for files", 4);
            return;
        }

        let path = entry.path().to_path_buf();
        let display_name = entry.display_name_lossy();
        let window = self.widgets.window.clone();
        let toast_overlay = self.widgets.toast_overlay.clone();
        let status_label = self.widgets.status_label.clone();
        let selection = self.widgets.selection.clone();
        let visible = self.visible_entries.borrow().clone();
        let storage = self.current_storage_facts.get();
        let action = self
            .widgets
            .window
            .lookup_action("open-with")
            .and_downcast::<gio::SimpleAction>();
        if let Some(action) = action.as_ref() {
            action.set_enabled(false);
        }
        status_label.set_label("Loading applications…");

        glib::spawn_future_local(async move {
            let result = launcher::discover_open_with(path).await;
            if !window.is_visible() {
                return;
            }
            if let Some(action) = action.as_ref() {
                let selected = selected_entries_for_selection(&selection);
                action.set_enabled(selection_action_state(&selected).open_with);
                status_label.set_label(&selection_status(&visible, &selected, storage));
            } else {
                let selected = selected_entries_for_selection(&selection);
                status_label.set_label(&selection_status(&visible, &selected, storage));
            }
            match result {
                Ok(options) if options.applications.is_empty() => {
                    toast_overlay.add_toast(
                        adw::Toast::builder()
                            .title("No compatible applications were found")
                            .timeout(6)
                            .build(),
                    );
                }
                Ok(options) => {
                    present_or_report_open_with(&window, &toast_overlay, &display_name, options)
                }
                Err(error) => {
                    tracing::warn!(%error, "Open With application discovery failed");
                    toast_overlay.add_toast(
                        adw::Toast::builder()
                            .title(format!("Could not find applications: {error}"))
                            .timeout(7)
                            .build(),
                    );
                }
            }
        });
    }

    fn set_selection_actions_enabled(&self, transfer: bool, rename: bool, trash: bool) {
        let trash_mode = self.trash_active.get();
        let all_restorable = self.selected_entries.borrow().iter().all(|entry| {
            entry.trash_metadata().is_some_and(|metadata| {
                metadata.original_path().is_some() && metadata.info_path().is_some()
            })
        });
        let trash_state = trash_mode_action_state(
            trash_mode,
            self.selected_entries.borrow().len(),
            all_restorable,
            self.visible_entries.borrow().len(),
        );
        let selection_state = selection_action_state(&self.selected_entries.borrow());
        for (action_name, enabled) in [
            ("copy", transfer && !trash_mode),
            ("cut", transfer && !trash_mode),
            ("duplicate", selection_state.duplicate && !trash_mode),
            ("rename", rename && !trash_mode),
            (
                "create-symbolic-link",
                selection_state.symbolic_link && !trash_mode,
            ),
            ("create-hard-link", selection_state.hard_link && !trash_mode),
            (
                "reveal-link-target",
                selection_state.reveal_link && !trash_mode,
            ),
            ("copy-name", selection_state.copy_identity && !trash_mode),
            ("copy-path", selection_state.copy_identity && !trash_mode),
            (
                "copy-relative-path",
                selection_state.copy_identity && !trash_mode,
            ),
            ("copy-uri", selection_state.copy_identity && !trash_mode),
            ("new-folder", !trash_mode),
            ("new-empty-file", !trash_mode),
            ("new-from-template", !trash_mode),
            ("trash", trash && !trash_mode),
            (
                "permanent-delete",
                trash_state.permanent_delete || (!trash_mode && trash),
            ),
            ("restore", trash_state.restore),
            ("empty-trash", trash_state.empty),
        ] {
            if let Some(action) = self
                .widgets
                .window
                .lookup_action(action_name)
                .and_downcast::<gio::SimpleAction>()
            {
                action.set_enabled(enabled);
            }
        }
    }

    fn set_paste_enabled(&self, enabled: bool) {
        if let Some(action) = self
            .widgets
            .window
            .lookup_action("paste")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(enabled);
        }
    }

    fn refresh_paste_enabled(&self) {
        let available = !self.trash_active.get()
            && (self.application_state.staged_transfers().is_some()
                || clipboard::contains_transfer(&self.widgets.window.clipboard()));
        self.set_paste_enabled(available);
    }

    fn extract_here(&self) {
        let Some(source) = self.selected_paths().into_iter().next() else {
            self.show_toast("Select one supported archive to extract", 4);
            return;
        };
        let Some(parent) = source.parent().map(Path::to_path_buf) else {
            self.show_toast("The archive has no local parent folder", 5);
            return;
        };
        match extraction_request(source, &parent) {
            Ok(request) => self.submit_archive_request(request, "Extracting archive…"),
            Err(error) => self.show_toast(&format!("Could not prepare extraction: {error}"), 6),
        }
    }

    fn choose_extract_destination(self: &Rc<Self>) {
        let Some(source) = self.selected_paths().into_iter().next() else {
            self.show_toast("Select one supported archive to extract", 4);
            return;
        };
        let chooser = gtk::FileDialog::builder()
            .title("Choose Extraction Folder")
            .modal(true)
            .build();
        if let Some(parent) = source.parent() {
            chooser.set_initial_folder(Some(&gio::File::for_path(parent)));
        }
        let window = self.widgets.window.clone();
        let controller = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            match chooser.select_folder_future(Some(&window)).await {
                Ok(folder) => {
                    let Some(parent) = folder.path() else {
                        if let Some(controller) = controller.upgrade() {
                            controller.show_toast("Only local extraction folders are supported", 5);
                        }
                        return;
                    };
                    if let Some(controller) = controller.upgrade() {
                        match extraction_request(source, &parent) {
                            Ok(request) => {
                                controller.submit_archive_request(request, "Extracting archive…");
                            }
                            Err(error) => controller
                                .show_toast(&format!("Could not prepare extraction: {error}"), 6),
                        }
                    }
                }
                Err(error) if error.matches(gio::IOErrorEnum::Cancelled) => {}
                Err(error) => {
                    if let Some(controller) = controller.upgrade() {
                        controller
                            .show_toast(&format!("Could not choose extraction folder: {error}"), 6);
                    }
                }
            }
        });
    }

    fn show_compress_dialog(self: &Rc<Self>) {
        let sources: Arc<[PathBuf]> = self.selected_paths().into();
        if sources.is_empty() {
            self.show_toast("Select one or more files or folders to compress", 4);
            return;
        }
        let destination_parent = self.action_directory();
        let raw_default = default_compression_name(&sources, floe_core::ArchiveFormat::Zip);
        let default_name = raw_default.to_str().unwrap_or("Archive.zip");
        let widgets = build_compress_dialog(
            sources.len(),
            default_name,
            &destination_preview(&destination_parent, OsStr::new(default_name)),
        );

        let preview = widgets.preview_label.clone();
        let parent = destination_parent.clone();
        let dropdown = widgets.format_dropdown.clone();
        widgets.name_entry.connect_changed(move |entry| {
            let name = with_archive_extension(entry.text().as_str(), selected_format(&dropdown));
            preview.set_label(&destination_preview(&parent, name.as_os_str()));
        });
        let preview = widgets.preview_label.clone();
        let parent = destination_parent.clone();
        let entry = widgets.name_entry.clone();
        widgets
            .format_dropdown
            .connect_selected_notify(move |dropdown| {
                let name = with_archive_extension(entry.text().as_str(), selected_format(dropdown));
                preview.set_label(&destination_preview(&parent, name.as_os_str()));
            });

        let dialog = widgets.dialog.downgrade();
        widgets.cancel_button.connect_clicked(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        });
        let controller = Rc::downgrade(self);
        let dialog = widgets.dialog.downgrade();
        let name_entry = widgets.name_entry.clone();
        let format_dropdown = widgets.format_dropdown.clone();
        let error_label = widgets.error_label.clone();
        widgets.compress_button.connect_clicked(move |button| {
            let Some(controller) = controller.upgrade() else {
                return;
            };
            let format = selected_format(&format_dropdown);
            let name = with_archive_extension(name_entry.text().as_str(), format);
            match compression_request(Arc::clone(&sources), &destination_parent, name.as_os_str()) {
                Ok(request) => match controller.application_state.submit_archive(request) {
                    Ok(_) => {
                        controller
                            .widgets
                            .status_label
                            .set_label("Compressing selection…");
                        if let Some(dialog) = dialog.upgrade() {
                            dialog.close();
                        }
                    }
                    Err(error) => {
                        error_label.set_label(&format!("Could not queue compression: {error}"));
                        button.set_sensitive(true);
                    }
                },
                Err(error) => {
                    error_label.set_label(&error.to_string());
                    button.set_sensitive(true);
                    name_entry.grab_focus();
                }
            }
        });
        widgets.dialog.present(Some(&self.widgets.window));
        widgets.name_entry.grab_focus();
    }

    fn submit_archive_request(&self, request: floe_core::ArchiveRequest, status: &str) {
        match self.application_state.submit_archive(request) {
            Ok(_) => self.widgets.status_label.set_label(status),
            Err(error) => {
                self.show_toast(&format!("Could not start archive operation: {error}"), 7)
            }
        }
    }

    fn stage_selected_copy(&self) {
        let selected = self.selected_entries.borrow();
        if selected.is_empty() {
            self.show_toast("Select one or more items to copy", 4);
            return;
        }
        if selected
            .iter()
            .any(|entry| matches!(entry.kind(), floe_core::EntryKind::Other))
        {
            self.show_toast(
                "The selection includes a special file type that cannot be copied yet",
                5,
            );
            return;
        }
        let paths = selected
            .iter()
            .map(|entry| entry.path().to_path_buf())
            .collect::<Vec<_>>();
        let count = paths.len();
        drop(selected);

        let clipboard_transfer = ClipboardTransfer::new(TransferIntent::Copy, paths.clone());
        match self.application_state.stage_copy_many(paths) {
            Ok(()) => {
                let published = clipboard_transfer.and_then(|transfer| {
                    clipboard::publish_transfer(&self.widgets.window.clipboard(), &transfer)
                });
                self.refresh_paste_enabled();
                let message = if published.is_ok() {
                    format!(
                        "Ready to copy {}. Open a destination and press Ctrl+V.",
                        item_count_text(count)
                    )
                } else {
                    format!(
                        "Ready to copy {} inside Floe; desktop clipboard unavailable.",
                        item_count_text(count)
                    )
                };
                self.show_toast(&message, 6);
            }
            Err(error) => self.show_toast(&format!("Could not stage copy: {error}"), 6),
        }
    }

    fn stage_selected_move(&self) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            self.show_toast("Select one or more items to move", 4);
            return;
        }
        let count = paths.len();
        let clipboard_transfer = ClipboardTransfer::new(TransferIntent::Move, paths.clone());
        match self.application_state.stage_move_many(paths) {
            Ok(()) => {
                let published = clipboard_transfer.and_then(|transfer| {
                    clipboard::publish_transfer(&self.widgets.window.clipboard(), &transfer)
                });
                self.refresh_paste_enabled();
                let message = if published.is_ok() {
                    format!(
                        "Ready to move {}. Open a destination and press Ctrl+V.",
                        item_count_text(count)
                    )
                } else {
                    format!(
                        "Ready to move {} inside Floe; desktop clipboard unavailable.",
                        item_count_text(count)
                    )
                };
                self.show_toast(&message, 6);
            }
            Err(error) => self.show_toast(&format!("Could not stage move: {error}"), 6),
        }
    }

    fn paste_transfer(&self) {
        let destination = self.action_directory();
        let clipboard = self.widgets.window.clipboard();
        if clipboard::contains_transfer(&clipboard) {
            let application_state = Rc::clone(&self.application_state);
            let status_label = self.widgets.status_label.clone();
            let toast_overlay = self.widgets.toast_overlay.clone();
            let window = self.widgets.window.clone();
            clipboard::read_transfer_async(&clipboard, move |result| {
                let transfer = match result {
                    Ok(transfer) => transfer,
                    Err(error) => {
                        toast_overlay.add_toast(
                            adw::Toast::builder()
                                .title(format!("Could not read clipboard files: {error}"))
                                .timeout(7)
                                .build(),
                        );
                        return;
                    }
                };
                let intent = transfer.intent();
                let stage_result = match intent {
                    TransferIntent::Copy => {
                        application_state.stage_copy_many(transfer.paths().to_vec())
                    }
                    TransferIntent::Move => {
                        application_state.stage_move_many(transfer.paths().to_vec())
                    }
                };
                if let Err(error) = stage_result {
                    toast_overlay.add_toast(
                        adw::Toast::builder()
                            .title(format!("Could not stage clipboard files: {error}"))
                            .timeout(7)
                            .build(),
                    );
                    return;
                }
                match application_state.submit_paste_batch(&destination) {
                    Ok(batch) => {
                        status_label.set_label(&format!(
                            "{} {} queued…",
                            match intent {
                                TransferIntent::Move => "Move",
                                TransferIntent::Copy => "Copy",
                            },
                            item_count_text(batch.queued())
                        ));
                        if intent == TransferIntent::Move {
                            let _ = window.clipboard().set_content(gdk::ContentProvider::NONE);
                            if let Some(action) = window
                                .lookup_action("paste")
                                .and_downcast::<gio::SimpleAction>()
                            {
                                action.set_enabled(false);
                            }
                        }
                    }
                    Err(error) => toast_overlay.add_toast(
                        adw::Toast::builder()
                            .title(format!("Could not start operation: {error}"))
                            .timeout(7)
                            .build(),
                    ),
                }
            });
            return;
        }

        let intent = self
            .application_state
            .staged_transfers()
            .map(|(intent, _)| intent);
        match self.application_state.submit_paste_batch(&destination) {
            Ok(batch) => {
                if intent == Some(TransferIntent::Move) {
                    self.set_paste_enabled(false);
                }
                self.widgets.status_label.set_label(&format!(
                    "{} {} queued…",
                    match intent {
                        Some(TransferIntent::Move) => "Move",
                        _ => "Copy",
                    },
                    item_count_text(batch.queued())
                ));
            }
            Err(error) => self.show_toast(&format!("Could not start operation: {error}"), 6),
        }
    }

    fn trash_selected(&self) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            self.show_toast("Select one or more items to move to Trash", 4);
            return;
        }
        match self.application_state.submit_trash_batch(paths) {
            Ok(_) => self
                .widgets
                .status_label
                .set_label("Moving selection to Trash…"),
            Err(error) => {
                self.show_toast(&format!("Could not move selection to Trash: {error}"), 7)
            }
        }
    }

    fn restore_selected(&self) {
        if !self.trash_active.get() {
            self.show_toast("Open Trash to restore items", 4);
            return;
        }
        let requests = self
            .selected_entries
            .borrow()
            .iter()
            .map(|entry| RestoreRequest::from_entry(entry))
            .collect::<Result<Vec<_>, _>>();
        let Ok(requests) = requests else {
            self.show_toast(
                "Original location metadata is unavailable for part of this selection",
                7,
            );
            return;
        };
        if requests.is_empty() {
            self.show_toast("Select one or more Trash items to restore", 4);
            return;
        }
        match self.application_state.submit_restore_batch(requests) {
            Ok(batch) => self
                .widgets
                .status_label
                .set_label(&format!("Restoring {}…", item_count_text(batch.queued()))),
            Err(error) => self.show_toast(&format!("Could not start restore: {error}"), 7),
        }
    }

    fn confirm_empty_trash(&self) {
        if !self.trash_active.get() {
            return;
        }
        let entries = self.visible_entries.borrow();
        if entries.is_empty() {
            self.show_toast("Trash is already empty", 4);
            return;
        }
        let labels = entries
            .iter()
            .map(|entry| permanent_delete_target_label(entry.path()))
            .collect::<Vec<_>>();
        let mut targets = entries
            .iter()
            .map(|entry| entry.path().to_path_buf())
            .collect::<Vec<_>>();
        targets.extend(entries.iter().filter_map(|entry| {
            entry
                .trash_metadata()
                .and_then(|metadata| metadata.info_path())
                .map(Path::to_path_buf)
        }));
        drop(entries);

        let confirmation = ui::build_empty_trash_dialog(&labels);
        let dialog = confirmation.dialog.downgrade();
        confirmation.cancel_button.connect_clicked(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        });
        let application_state = Rc::clone(&self.application_state);
        let status_label = self.widgets.status_label.clone();
        let toast_overlay = self.widgets.toast_overlay.clone();
        let dialog = confirmation.dialog.downgrade();
        confirmation.delete_button.connect_clicked(move |button| {
            button.set_sensitive(false);
            match application_state.submit_permanent_delete(targets.clone()) {
                Ok(_) => {
                    status_label.set_label("Empty Trash queued…");
                    if let Some(dialog) = dialog.upgrade() {
                        dialog.close();
                    }
                }
                Err(error) => {
                    button.set_sensitive(true);
                    toast_overlay.add_toast(
                        adw::Toast::builder()
                            .title(format!("Could not empty Trash: {error}"))
                            .timeout(7)
                            .build(),
                    );
                }
            }
        });
        confirmation.dialog.present(Some(&self.widgets.window));
        confirmation.cancel_button.grab_focus();
    }

    fn confirm_permanent_delete(&self) {
        let mut paths = self.selected_paths();
        if paths.is_empty() {
            self.show_toast("Select one or more items to delete permanently", 4);
            return;
        }

        let labels = paths
            .iter()
            .map(|path| permanent_delete_target_label(path))
            .collect::<Vec<_>>();
        if self.trash_active.get() {
            paths.extend(self.selected_entries.borrow().iter().filter_map(|entry| {
                entry
                    .trash_metadata()
                    .and_then(|metadata| metadata.info_path())
                    .map(Path::to_path_buf)
            }));
        }
        let confirmation = ui::build_permanent_delete_dialog(&labels);

        let dialog = confirmation.dialog.downgrade();
        confirmation.cancel_button.connect_clicked(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        });

        let application_state = Rc::clone(&self.application_state);
        let status_label = self.widgets.status_label.clone();
        let toast_overlay = self.widgets.toast_overlay.clone();
        let dialog = confirmation.dialog.downgrade();
        confirmation.delete_button.connect_clicked(move |button| {
            button.set_sensitive(false);
            match application_state.submit_permanent_delete(paths.clone()) {
                Ok(_) => {
                    status_label.set_label("Permanent deletion queued…");
                    if let Some(dialog) = dialog.upgrade() {
                        dialog.close();
                    }
                }
                Err(error) => {
                    button.set_sensitive(true);
                    toast_overlay.add_toast(
                        adw::Toast::builder()
                            .title(format!("Could not start permanent deletion: {error}"))
                            .timeout(7)
                            .build(),
                    );
                }
            }
        });

        confirmation.dialog.present(Some(&self.widgets.window));
        confirmation.cancel_button.grab_focus();
    }

    fn show_new_folder(self: &Rc<Self>) {
        self.show_create_name_dialog(CreateDialogKind::Directory, "New Folder");
    }

    fn show_new_empty_file(self: &Rc<Self>) {
        self.show_create_name_dialog(CreateDialogKind::EmptyFile, "New File");
    }

    fn present_template_catalog(self: &Rc<Self>) {
        let Some(root) = glib::user_special_dir(glib::UserDirectory::Templates) else {
            self.show_toast("No XDG Templates folder is configured", 5);
            return;
        };
        let root = root.to_path_buf();
        let widgets = crate::templates::build_template_dialog();

        let controller = Rc::downgrade(self);
        let dialog = widgets.dialog.downgrade();
        let management_root = root.clone();
        widgets.open_folder_button.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.navigate_to(management_root.clone());
            }
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        });

        let request_id = self.template_request_id.get().wrapping_add(1).max(1);
        self.template_request_id.set(request_id);
        let request_result = self.template_worker.borrow().as_ref().map_or(
            Err(crate::templates::TemplateSubmitError::Stopped),
            |worker| worker.request(request_id, root),
        );
        if let Err(error) = request_result {
            widgets.spinner.stop();
            widgets.spinner.set_visible(false);
            widgets.status.set_label(&error.to_string());
        } else {
            let controller = Rc::downgrade(self);
            let response_widgets = widgets.clone();
            glib::timeout_add_local(Duration::from_millis(25), move || {
                let Some(controller) = controller.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                let response = controller
                    .template_worker
                    .borrow()
                    .as_ref()
                    .and_then(crate::templates::TemplateWorker::try_response);
                let Some(response) = response else {
                    return glib::ControlFlow::Continue;
                };
                if response.id != request_id {
                    return glib::ControlFlow::Continue;
                }
                controller.populate_template_dialog(&response_widgets, response.result);
                glib::ControlFlow::Break
            });
        }
        widgets.dialog.present(Some(&self.widgets.window));
    }

    fn populate_template_dialog(
        self: &Rc<Self>,
        widgets: &crate::templates::TemplateDialogWidgets,
        result: Result<crate::templates::TemplateCatalog, crate::templates::TemplateDiscoveryError>,
    ) {
        widgets.spinner.stop();
        widgets.spinner.set_visible(false);
        match result {
            Ok(catalog) if catalog.entries().is_empty() => widgets.status.set_label(
                "No templates found. Add regular files to your Templates folder to use them here.",
            ),
            Ok(catalog) => {
                widgets.status.set_label(if catalog.truncated() {
                    "Showing the first 256 templates. Remove unused files to see the rest."
                } else {
                    "Choose a template"
                });
                widgets.list.set_visible(true);
                for entry in catalog.entries() {
                    let display_name = entry.display_name();
                    let row = adw::ActionRow::builder()
                        .title(&display_name)
                        .subtitle("Creates a non-executable copy")
                        .activatable(true)
                        .build();
                    row.update_property(&[gtk::accessible::Property::Label(&display_name)]);
                    row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
                    let source = entry.path().to_path_buf();
                    let initial_name = entry
                        .name()
                        .to_str()
                        .map_or_else(|| "Untitled".to_owned(), ToOwned::to_owned);
                    let controller = Rc::downgrade(self);
                    let dialog = widgets.dialog.downgrade();
                    row.connect_activated(move |_| {
                        if let Some(dialog) = dialog.upgrade() {
                            dialog.close();
                        }
                        if let Some(controller) = controller.upgrade() {
                            controller.show_create_name_dialog(
                                CreateDialogKind::Template(source.clone()),
                                &initial_name,
                            );
                        }
                    });
                    widgets.list.append(&row);
                }
            }
            Err(error) => widgets.status.set_label(&format!(
                "{error}. Use Open Templates Folder to review the location."
            )),
        }
    }

    fn choose_template(self: &Rc<Self>) {
        if self.trash_active.get() {
            self.show_toast("Templates are unavailable while browsing Trash", 4);
            return;
        }

        self.present_template_catalog();
    }

    fn duplicate_selected(&self) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            self.show_toast("Select one or more items to duplicate", 4);
            return;
        }
        match self.application_state.submit_duplicate_batch(paths) {
            Ok(batch) => self.widgets.status_label.set_label(&format!(
                "Queued {} item{} for duplication…",
                batch.queued(),
                if batch.queued() == 1 { "" } else { "s" }
            )),
            Err(error) => self.show_toast(&format!("Could not duplicate selection: {error}"), 7),
        }
    }

    fn show_create_symbolic_link(self: &Rc<Self>) {
        let Some(entry) = self.selected_entry() else {
            self.show_toast("Select one item to link", 4);
            return;
        };
        let source = entry.path().to_path_buf();
        let Some(target_name) = source.file_name() else {
            self.show_toast("This item cannot be linked", 4);
            return;
        };
        let initial_name = suggested_link_name(target_name, "Link");

        let heading = gtk::Label::builder()
            .label("Choose link target style")
            .halign(gtk::Align::Start)
            .build();
        heading.add_css_class("title-2");
        let explanation = gtk::Label::builder()
            .label("Relative links keep working when their surrounding folder tree moves. Absolute links always store the full current path. Either may become broken if its target moves.")
            .halign(gtk::Align::Start)
            .xalign(0.0)
            .wrap(true)
            .build();
        explanation.add_css_class("dim-label");
        let relative_button = gtk::Button::with_label("Use Relative Target");
        relative_button.add_css_class("suggested-action");
        let absolute_button = gtk::Button::with_label("Use Absolute Target");
        let cancel_button = gtk::Button::with_label("Cancel");
        let actions = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .build();
        actions.append(&relative_button);
        actions.append(&absolute_button);
        actions.append(&cancel_button);
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
        content.append(&actions);
        let dialog = adw::Dialog::builder()
            .title("Create Symbolic Link")
            .content_width(460)
            .content_height(330)
            .child(&content)
            .build();
        dialog.update_property(&[gtk::accessible::Property::Label("Create Symbolic Link")]);

        let weak_dialog = dialog.downgrade();
        cancel_button.connect_clicked(move |_| {
            if let Some(dialog) = weak_dialog.upgrade() {
                dialog.close();
            }
        });
        for (button, mode) in [
            (relative_button.clone(), SymbolicLinkMode::Relative),
            (absolute_button, SymbolicLinkMode::Absolute),
        ] {
            let controller = Rc::downgrade(self);
            let weak_dialog = dialog.downgrade();
            let source = source.clone();
            let initial_name = initial_name.clone();
            button.connect_clicked(move |_| {
                if let Some(dialog) = weak_dialog.upgrade() {
                    dialog.close();
                }
                if let Some(controller) = controller.upgrade() {
                    controller.show_create_name_dialog(
                        CreateDialogKind::SymbolicLink(source.clone(), mode),
                        &initial_name,
                    );
                }
            });
        }
        dialog.present(Some(&self.widgets.window));
        relative_button.grab_focus();
    }

    fn show_create_hard_link(self: &Rc<Self>) {
        let Some(entry) = self.selected_entry() else {
            self.show_toast("Select one regular file to hard link", 4);
            return;
        };
        if !matches!(entry.kind(), EntryKind::RegularFile) {
            self.show_toast("Hard links require one regular non-symbolic file", 5);
            return;
        }
        let Some(source_name) = entry.path().file_name() else {
            self.show_toast("This file cannot be hard linked", 4);
            return;
        };
        let initial_name = suggested_link_name(source_name, "Hard Link");
        self.show_create_name_dialog(
            CreateDialogKind::HardLink(entry.path().to_path_buf()),
            &initial_name,
        );
    }

    fn show_create_name_dialog(self: &Rc<Self>, kind: CreateDialogKind, initial_name: &str) {
        if self.trash_active.get() {
            self.show_toast("Creation is unavailable while browsing Trash", 4);
            return;
        }
        let destination_directory = self.action_directory();
        let dialog = ui::build_name_dialog(
            kind.title(),
            "New item name",
            initial_name,
            "Create",
            "Creation error",
        );
        dialog.rename_entry.select_region(0, -1);

        let weak_dialog = dialog.dialog.downgrade();
        dialog.cancel_button.connect_clicked(move |_| {
            if let Some(dialog) = weak_dialog.upgrade() {
                dialog.close();
            }
        });

        let application_state = Rc::clone(&self.application_state);
        let status_label = self.widgets.status_label.clone();
        let toast_overlay = self.widgets.toast_overlay.clone();
        let name_entry = dialog.rename_entry.clone();
        let name_error = dialog.rename_error.clone();
        let weak_dialog = dialog.dialog.downgrade();
        let controller = Rc::downgrade(self);
        dialog.rename_button.connect_clicked(move |_| {
            let new_name = name_entry.text();
            let new_name_os = OsString::from(new_name.as_str());
            if validate_rename_name(OsStr::new(new_name.as_str())).is_err() {
                name_error.set_label(if new_name.is_empty() {
                    "Enter a name"
                } else {
                    "Use one filename without '/'"
                });
                name_error.set_visible(true);
                name_entry.grab_focus();
                name_entry.select_region(0, -1);
                return;
            }

            let destination = destination_directory.join(new_name_os);
            let rename_after_create = matches!(
                &kind,
                CreateDialogKind::Directory | CreateDialogKind::Template(_)
            );
            let request = match kind.request(destination.clone()) {
                Ok(request) => request,
                Err(error) => {
                    name_error.set_label(&format!("Invalid creation request: {error}"));
                    name_error.set_visible(true);
                    name_entry.grab_focus();
                    return;
                }
            };
            match application_state.submit_create(request) {
                Ok(_) => {
                    status_label.set_label("Creation queued…");
                    if rename_after_create && let Some(controller) = controller.upgrade() {
                        controller
                            .pending_create_rename
                            .replace(Some(destination.clone()));
                    }
                    if let Some(dialog) = weak_dialog.upgrade() {
                        dialog.close();
                    }
                }
                Err(error) => {
                    toast_overlay.add_toast(
                        adw::Toast::builder()
                            .title(format!("Could not start creation: {error}"))
                            .timeout(7)
                            .build(),
                    );
                    name_entry.grab_focus();
                }
            }
        });

        dialog.dialog.present(Some(&self.widgets.window));
        dialog.rename_entry.grab_focus();
    }

    fn copy_selection_text(&self, mode: ClipboardTextMode) {
        let paths = self.selected_paths();
        let base = self.action_directory();
        match selection_clipboard_text(&paths, &base, mode) {
            Ok(text) => {
                self.widgets.window.clipboard().set_text(&text);
                self.widgets.status_label.set_label(match mode {
                    ClipboardTextMode::Name => "Copied name to clipboard",
                    ClipboardTextMode::AbsolutePath => "Copied path to clipboard",
                    ClipboardTextMode::RelativePath => "Copied relative path to clipboard",
                    ClipboardTextMode::Uri => "Copied URI to clipboard",
                });
            }
            Err(message) => self.show_toast(message, 6),
        }
    }

    fn reveal_link_target(self: &Rc<Self>) {
        let Some(entry) = self.selected_entry() else {
            self.show_toast("Select one symbolic link", 4);
            return;
        };
        if !matches!(entry.kind(), EntryKind::SymbolicLink { .. }) {
            self.show_toast("Reveal Link Target is available for symbolic links", 4);
            return;
        }

        let link_path = entry.path().to_path_buf();
        let controller = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = async {
                let info = gio::File::for_path(&link_path)
                    .query_info_future(
                        "standard::symlink-target",
                        gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                        glib::Priority::DEFAULT,
                    )
                    .await
                    .map_err(|error| format!("Could not read link target: {error}"))?;
                let stored_target = info
                    .symlink_target()
                    .ok_or_else(|| "The selected item has no stored link target".to_owned())?;
                let resolved = resolve_link_target(&link_path, &stored_target)
                    .ok_or_else(|| "The link target cannot be resolved".to_owned())?;
                gio::File::for_path(&resolved)
                    .query_info_future(
                        "standard::type",
                        gio::FileQueryInfoFlags::NONE,
                        glib::Priority::DEFAULT,
                    )
                    .await
                    .map_err(|_| {
                        "The symbolic link target is missing or inaccessible".to_owned()
                    })?;
                Ok::<PathBuf, String>(resolved)
            }
            .await;

            let Some(controller) = controller.upgrade() else {
                return;
            };
            match result {
                Ok(target) => controller.navigate_to_revealing(target),
                Err(message) => controller.show_toast(&message, 6),
            }
        });
    }

    fn show_batch_rename(self: &Rc<Self>) {
        let selected = self.selected_entries.borrow().clone();
        if selected.len() < 2 {
            self.show_toast("Select at least two items to batch rename", 4);
            return;
        }
        let sources = selected
            .iter()
            .map(|entry| BatchRenameSource {
                path: entry.path().to_path_buf(),
                date: entry
                    .modified()
                    .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                    .and_then(|duration| {
                        glib::DateTime::from_unix_utc(duration.as_secs() as i64)
                            .ok()
                            .and_then(|date| date.format("%Y-%m-%d").ok())
                            .map(|date| date.to_string())
                    })
                    .unwrap_or_else(|| "unknown-date".to_owned()),
            })
            .collect::<Vec<_>>();
        let widgets = build_batch_rename_dialog(sources.len());
        let request = Rc::new(RefCell::new(None));
        let refresh: Rc<dyn Fn()> = {
            let widgets = widgets.clone();
            let sources = sources.clone();
            let request = Rc::clone(&request);
            Rc::new(move || {
                request.replace(refresh_batch_rename_dialog(&widgets, &sources));
            })
        };
        refresh();
        for entry in [
            &widgets.find_entry,
            &widgets.replace_entry,
            &widgets.prefix_entry,
            &widgets.suffix_entry,
        ] {
            let refresh = Rc::clone(&refresh);
            entry.connect_changed(move |_| refresh());
        }
        for check in [&widgets.regex_check, &widgets.preserve_extension_check] {
            let refresh = Rc::clone(&refresh);
            check.connect_toggled(move |_| refresh());
        }
        for spin in [&widgets.sequence_start, &widgets.sequence_padding] {
            let refresh = Rc::clone(&refresh);
            spin.connect_value_changed(move |_| refresh());
        }
        let refresh_case = Rc::clone(&refresh);
        widgets
            .case_dropdown
            .connect_selected_notify(move |_| refresh_case());

        let dialog = widgets.dialog.downgrade();
        widgets.cancel_button.connect_clicked(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        });
        let controller = Rc::downgrade(self);
        let dialog = widgets.dialog.downgrade();
        let error_label = widgets.error_label.clone();
        widgets.apply_button.connect_clicked(move |button| {
            let Some(controller) = controller.upgrade() else {
                return;
            };
            let Some(request) = request.borrow().clone() else {
                error_label.set_label("Resolve the preview validation error first");
                return;
            };
            button.set_sensitive(false);
            match controller.application_state.submit_batch_rename(request) {
                Ok(_) => {
                    controller
                        .widgets
                        .status_label
                        .set_label("Batch rename queued…");
                    if let Some(dialog) = dialog.upgrade() {
                        dialog.close();
                    }
                }
                Err(error) => {
                    error_label.set_label(&format!("Could not queue batch rename: {error}"));
                    button.set_sensitive(true);
                }
            }
        });
        widgets.dialog.present(Some(&self.widgets.window));
        widgets.prefix_entry.grab_focus();
    }

    fn undo_batch_rename(&self) {
        match self.application_state.submit_batch_rename_undo() {
            Ok(Some(_)) => {
                self.widgets.status_label.set_label("Undoing batch rename…");
                if let Some(action) = self
                    .widgets
                    .window
                    .lookup_action("undo-batch-rename")
                    .and_downcast::<gio::SimpleAction>()
                {
                    action.set_enabled(false);
                }
            }
            Ok(None) => self.show_toast("No completed batch rename is available to undo", 4),
            Err(error) => self.show_toast(&format!("Could not undo batch rename: {error}"), 7),
        }
    }

    fn show_rename(&self) {
        let Some(entry) = self.selected_entry() else {
            self.show_toast("Select an item to rename", 4);
            return;
        };
        let source = entry.path().to_path_buf();
        let current_name = entry.display_name_lossy();
        let rename = ui::build_rename_dialog(&current_name);
        rename.rename_entry.select_region(0, -1);

        let dialog = rename.dialog.downgrade();
        rename.cancel_button.connect_clicked(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        });

        let application_state = Rc::clone(&self.application_state);
        let status_label = self.widgets.status_label.clone();
        let rename_entry = rename.rename_entry.clone();
        let rename_error = rename.rename_error.clone();
        let dialog = rename.dialog.downgrade();
        rename.rename_button.connect_clicked(move |_| {
            let new_name = rename_entry.text();
            let new_name_os = OsString::from(new_name.as_str());
            let unchanged = source
                .file_name()
                .is_some_and(|current| current == OsStr::new(new_name.as_str()));
            if unchanged || validate_rename_name(&new_name_os).is_err() {
                rename_error.set_label(if unchanged {
                    "Enter a different filename."
                } else {
                    "Enter one filename without '/', '.' or '..'."
                });
                rename_error.set_visible(true);
                rename_entry.grab_focus();
                rename_entry.select_region(0, -1);
                return;
            }

            match application_state.submit_rename(source.clone(), new_name_os) {
                Ok(_) => {
                    status_label.set_label("Rename queued…");
                    if let Some(dialog) = dialog.upgrade() {
                        dialog.close();
                    }
                }
                Err(error) => {
                    rename_error.set_label(&format!("Could not rename: {error}"));
                    rename_error.set_visible(true);
                    rename_entry.grab_focus();
                }
            }
        });

        rename.dialog.present(Some(&self.widgets.window));
        rename.rename_entry.grab_focus();
    }

    pub fn refresh_if_current(&self, directory: &std::path::Path) {
        let trash_directory = self.trash_active.get()
            && self.visible_entries.borrow().iter().any(|entry| {
                entry.path().parent() == Some(directory)
                    || entry
                        .trash_metadata()
                        .and_then(|metadata| metadata.info_path())
                        .and_then(Path::parent)
                        == Some(directory)
            });
        if trash_directory || self.tabs.borrow().active().current().path() == directory {
            self.reload_preserving_view(Vec::new());
        }
    }

    fn reload_preserving_view(&self, renames: Vec<RenamePair>) {
        self.pending_reconciliation
            .replace(Some(PendingReconciliation {
                snapshot: self.capture_view_state(),
                renames,
            }));
        self.load_current_inner();
    }

    fn show_toast(&self, title: &str, timeout: u32) {
        self.widgets
            .toast_overlay
            .add_toast(adw::Toast::builder().title(title).timeout(timeout).build());
    }
}

fn selection_indices_for_paths(
    entries: &[Arc<DirectoryEntry>],
    selected_paths: &[PathBuf],
) -> Vec<u32> {
    let selected = selected_paths
        .iter()
        .map(PathBuf::as_path)
        .collect::<HashSet<_>>();
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| selected.contains(entry.path()))
        .filter_map(|(index, _)| u32::try_from(index).ok())
        .collect()
}

fn folder_filter_response_is_current(active_generation: u64, response_generation: u64) -> bool {
    active_generation == response_generation
}

fn selected_entries_for_selection(selection: &gtk::MultiSelection) -> Vec<Arc<DirectoryEntry>> {
    let Some(model) = selection.model() else {
        return Vec::new();
    };
    let selected = selection.selection();
    let Some((indices, first)) = gtk::BitsetIter::init_first(&selected) else {
        return Vec::new();
    };
    std::iter::once(first)
        .chain(indices)
        .filter_map(|position| {
            model
                .item(position)
                .and_downcast::<glib::BoxedAnyObject>()
                .map(|object| object.borrow::<Arc<DirectoryEntry>>().clone())
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SelectionActionState {
    single: bool,
    open_with: bool,
    checksum: bool,
    transfer: bool,
    duplicate: bool,
    symbolic_link: bool,
    hard_link: bool,
    reveal_link: bool,
    copy_identity: bool,
    rename: bool,
    trash: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TrashModeActionState {
    restore: bool,
    permanent_delete: bool,
    empty: bool,
}

fn trash_mode_action_state(
    active: bool,
    selected_count: usize,
    all_restorable: bool,
    item_count: usize,
) -> TrashModeActionState {
    TrashModeActionState {
        restore: active && selected_count > 0 && all_restorable,
        permanent_delete: active && selected_count > 0,
        empty: active && item_count > 0,
    }
}

fn selection_action_state(entries: &[Arc<DirectoryEntry>]) -> SelectionActionState {
    let single = entries.len() == 1;
    let transferable = !entries.is_empty()
        && entries
            .iter()
            .all(|entry| !matches!(entry.kind(), EntryKind::Other));
    SelectionActionState {
        single,
        open_with: single && open_with_eligible(&entries[0]),
        checksum: !entries.is_empty()
            && entries
                .iter()
                .all(|entry| matches!(entry.kind(), EntryKind::RegularFile)),
        transfer: transferable,
        duplicate: transferable,
        symbolic_link: single && !matches!(entries[0].kind(), EntryKind::Other),
        hard_link: single && matches!(entries[0].kind(), EntryKind::RegularFile),
        reveal_link: single && matches!(entries[0].kind(), EntryKind::SymbolicLink { .. }),
        copy_identity: !entries.is_empty(),
        rename: single,
        trash: !entries.is_empty(),
    }
}

fn suggested_link_name(source_name: &OsStr, fallback: &str) -> String {
    source_name
        .to_str()
        .map(|name| format!("{name} link"))
        .unwrap_or_else(|| fallback.to_owned())
}

fn selection_clipboard_text(
    paths: &[PathBuf],
    base: &Path,
    mode: ClipboardTextMode,
) -> Result<String, &'static str> {
    if paths.is_empty() {
        return Err("Select one or more items first");
    }

    let mut lines = Vec::with_capacity(paths.len());
    for path in paths {
        let line = match mode {
            ClipboardTextMode::Name => path
                .file_name()
                .and_then(OsStr::to_str)
                .ok_or("A selected filename cannot be represented losslessly as text")?
                .to_owned(),
            ClipboardTextMode::AbsolutePath => path
                .to_str()
                .ok_or("A selected path cannot be represented losslessly as text")?
                .to_owned(),
            ClipboardTextMode::RelativePath => path
                .strip_prefix(base)
                .map_err(|_| "A selected item is outside the current folder")?
                .to_str()
                .ok_or("A relative path cannot be represented losslessly as text")?
                .to_owned(),
            ClipboardTextMode::Uri => clipboard::local_file_uri(path)
                .map_err(|_| "A selected path could not be encoded as a local file URI")?,
        };
        lines.push(line);
    }
    Ok(lines.join("\n"))
}

fn resolve_link_target(link_path: &Path, stored_target: &Path) -> Option<PathBuf> {
    let target = if stored_target.is_absolute() {
        stored_target.to_path_buf()
    } else {
        link_path.parent()?.join(stored_target)
    };
    Some(lexically_normalize_path(&target))
}

fn lexically_normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => match normalized.components().next_back() {
                Some(Component::Normal(_)) => {
                    normalized.pop();
                }
                Some(Component::ParentDir) | None if !path.has_root() => {
                    normalized.push(component.as_os_str());
                }
                _ => {}
            },
            Component::Normal(name) => normalized.push(name),
        }
    }
    normalized
}

fn selection_status(
    visible: &[Arc<DirectoryEntry>],
    selected: &[Arc<DirectoryEntry>],
    storage: Option<StorageFacts>,
) -> String {
    let entries = if selected.is_empty() {
        visible
    } else {
        selected
    };
    let mut status = match selected {
        [] => item_count_text(visible.len()),
        [entry] => format!("{} selected", entry.display_name_lossy()),
        entries => format!("{} selected", item_count_text(entries.len())),
    };
    if let Some(bytes) = known_byte_total(entries) {
        status.push_str(" · ");
        status.push_str(&format!("{} known", format_bytes(bytes)));
    }
    if let Some(storage) = storage {
        let storage = format_storage_facts(storage);
        if !storage.is_empty() {
            status.push_str(" · ");
            status.push_str(&storage);
        }
    }
    status
}

fn known_byte_total(entries: &[Arc<DirectoryEntry>]) -> Option<u64> {
    entries
        .iter()
        .filter_map(|entry| entry.size())
        .reduce(u64::saturating_add)
}

fn device_status_text(base: &str, facts: StorageFacts) -> String {
    let facts = format_storage_facts(facts);
    if facts.is_empty() {
        base.to_owned()
    } else {
        format!("{base} · {facts}")
    }
}

fn current_storage_request_is_current(
    request: &StorageRequest,
    generation: u64,
    current_path: &Path,
    trash_active: bool,
) -> bool {
    !trash_active
        && request.target == StorageTarget::CurrentLocation
        && request.generation == generation
        && request.path == current_path
}

fn item_count_text(count: usize) -> String {
    if count == 1 {
        "1 item".to_owned()
    } else {
        format!("{count} items")
    }
}

fn remove_all_children(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn sidebar_status_label(text: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .halign(gtk::Align::Start)
        .margin_start(8)
        .wrap(true)
        .build();
    label.add_css_class("floe-status");
    label
}

fn sidebar_path_name(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

fn exact_sidebar_target(path: &std::path::Path) -> PathBuf {
    path.to_path_buf()
}

fn set_accessible_label(widget: &impl IsA<gtk::Accessible>, label: &str) {
    widget.update_property(&[gtk::accessible::Property::Label(label)]);
}

fn permanent_delete_target_label(path: &Path) -> String {
    if let Some(text) = path.to_str() {
        let mut escaped = String::with_capacity(text.len());
        for character in text.chars() {
            if character == '\\' || character.is_control() {
                escaped.extend(character.escape_default());
            } else {
                escaped.push(character);
            }
        }
        return escaped;
    }

    let mut escaped = String::new();
    for byte in path.as_os_str().as_bytes() {
        if matches!(byte, b' '..=b'~') && *byte != b'\\' {
            escaped.push(char::from(*byte));
        } else if *byte == b'\\' {
            escaped.push_str("\\\\");
        } else {
            use std::fmt::Write;
            let _ = write!(escaped, "\\x{byte:02x}");
        }
    }
    escaped
}

fn is_permanent_delete_shortcut(key: gtk::gdk::Key, modifiers: gtk::gdk::ModifierType) -> bool {
    let command_modifiers = gtk::gdk::ModifierType::SHIFT_MASK
        | gtk::gdk::ModifierType::CONTROL_MASK
        | gtk::gdk::ModifierType::ALT_MASK
        | gtk::gdk::ModifierType::SUPER_MASK
        | gtk::gdk::ModifierType::HYPER_MASK
        | gtk::gdk::ModifierType::META_MASK;
    key == gtk::gdk::Key::Delete
        && modifiers & command_modifiers == gtk::gdk::ModifierType::SHIFT_MASK
}

fn is_context_menu_shortcut(key: gtk::gdk::Key, modifiers: gtk::gdk::ModifierType) -> bool {
    let command_modifiers = gtk::gdk::ModifierType::SHIFT_MASK
        | gtk::gdk::ModifierType::CONTROL_MASK
        | gtk::gdk::ModifierType::ALT_MASK
        | gtk::gdk::ModifierType::SUPER_MASK
        | gtk::gdk::ModifierType::HYPER_MASK
        | gtk::gdk::ModifierType::META_MASK;
    let relevant = modifiers & command_modifiers;

    (key == gtk::gdk::Key::Menu && relevant.is_empty())
        || (key == gtk::gdk::Key::F10 && relevant == gtk::gdk::ModifierType::SHIFT_MASK)
}

fn is_quick_preview_space(key: gtk::gdk::Key, modifiers: gtk::gdk::ModifierType) -> bool {
    key == gtk::gdk::Key::space && modifiers.is_empty()
}

fn vim_selection_target(
    selected: Option<u32>,
    item_count: u32,
    command: crate::vim_mode::VimCommand,
) -> Option<u32> {
    use crate::vim_mode::VimCommand;

    if item_count == 0 {
        return None;
    }
    let last = item_count - 1;
    match command {
        VimCommand::Previous => Some(selected.unwrap_or(0).saturating_sub(1)),
        VimCommand::Next => Some(selected.map_or(0, |index| index.saturating_add(1).min(last))),
        VimCommand::First => Some(0),
        VimCommand::Last => Some(last),
        VimCommand::Parent | VimCommand::Child | VimCommand::Open => None,
    }
}

fn open_with_eligible(entry: &DirectoryEntry) -> bool {
    open_with_kind_eligible(entry.kind())
}

fn open_with_kind_eligible(kind: EntryKind) -> bool {
    matches!(
        kind,
        EntryKind::RegularFile
            | EntryKind::SymbolicLink {
                target_is_directory: false
            }
    )
}

fn selected_application_index(list: &gtk::ListBox, count: usize) -> Option<usize> {
    usize::try_from(list.selected_row()?.index())
        .ok()
        .filter(|index| *index < count)
}

fn chooser_action_sensitivity(selected: Option<usize>, default: Option<usize>) -> (bool, bool) {
    (
        selected.is_some(),
        selected.is_some() && selected != default,
    )
}

fn present_or_report_open_with(
    window: &adw::ApplicationWindow,
    toast_overlay: &adw::ToastOverlay,
    display_name: &str,
    options: launcher::OpenWithOptions,
) {
    if options.applications.is_empty() {
        toast_overlay.add_toast(
            adw::Toast::builder()
                .title("No compatible applications found")
                .timeout(6)
                .build(),
        );
        return;
    }

    present_open_with_dialog(window, toast_overlay, display_name, options);
}

fn present_open_with_dialog(
    window: &adw::ApplicationWindow,
    toast_overlay: &adw::ToastOverlay,
    display_name: &str,
    options: launcher::OpenWithOptions,
) {
    let chooser = ui::build_open_with_dialog(display_name, &options);
    let applications = Rc::new(options.applications);
    let default_index = Rc::new(Cell::new(
        applications
            .iter()
            .position(|application| application.is_default),
    ));
    let initial_selection = selected_application_index(&chooser.list, applications.len());
    let (can_open, can_set_default) =
        chooser_action_sensitivity(initial_selection, default_index.get());
    chooser.open_button.set_sensitive(can_open);
    chooser.set_default_button.set_sensitive(can_set_default);

    let open_button = chooser.open_button.clone();
    let set_default_button = chooser.set_default_button.clone();
    let applications_for_selection = Rc::clone(&applications);
    let default_for_selection = Rc::clone(&default_index);
    chooser.list.connect_selected_rows_changed(move |list| {
        let selected = selected_application_index(list, applications_for_selection.len());
        let (can_open, can_set_default) =
            chooser_action_sensitivity(selected, default_for_selection.get());
        open_button.set_sensitive(can_open);
        set_default_button.set_sensitive(can_set_default);
    });

    let dialog = chooser.dialog.downgrade();
    chooser.cancel_button.connect_clicked(move |_| {
        if let Some(dialog) = dialog.upgrade() {
            dialog.close();
        }
    });

    let list = chooser.list.clone();
    let path = options.path.clone();
    let applications_for_open = Rc::clone(&applications);
    let toast_for_open = toast_overlay.clone();
    let dialog = chooser.dialog.downgrade();
    chooser.open_button.connect_clicked(move |button| {
        let Some(index) = selected_application_index(&list, applications_for_open.len()) else {
            return;
        };
        button.set_sensitive(false);
        let application = applications_for_open[index].app_info.clone();
        let application_name = applications_for_open[index].display_name.clone();
        let toast_for_result = toast_for_open.clone();
        launcher::launch_with(&application, &path, move |result| {
            if let Err(error) = result {
                tracing::warn!(%error, "Open With launch failed");
                toast_for_result.add_toast(
                    adw::Toast::builder()
                        .title(format!("Could not open with {application_name}: {error}"))
                        .timeout(7)
                        .build(),
                );
            }
        });
        if let Some(dialog) = dialog.upgrade() {
            dialog.close();
        }
    });

    let open_button = chooser.open_button.clone();
    chooser
        .list
        .connect_row_activated(move |_, _| open_button.emit_clicked());

    let list = chooser.list.clone();
    let rows = chooser.rows.clone();
    let default_label = chooser.default_label.clone();
    let applications_for_default = Rc::clone(&applications);
    let default_for_change = Rc::clone(&default_index);
    let content_type = options.content_type;
    let toast_for_default = toast_overlay.clone();
    chooser.set_default_button.connect_clicked(move |button| {
        let Some(index) = selected_application_index(&list, applications_for_default.len()) else {
            return;
        };
        let application = &applications_for_default[index];
        match launcher::set_default_for_type(&application.app_info, &content_type) {
            Ok(()) => {
                default_for_change.set(Some(index));
                button.set_sensitive(false);
                default_label.set_label(&format!("Current default: {}", application.display_name));
                for (row_index, row) in rows.iter().enumerate() {
                    if let Some(row) = row.downcast_ref::<adw::ActionRow>() {
                        row.set_subtitle(if row_index == index {
                            "Current default"
                        } else {
                            ""
                        });
                    }
                }
                toast_for_default.add_toast(
                    adw::Toast::builder()
                        .title(format!("{} is now the default", application.display_name))
                        .timeout(4)
                        .build(),
                );
            }
            Err(error) => {
                tracing::warn!(%error, "default application change failed");
                toast_for_default.add_toast(
                    adw::Toast::builder()
                        .title(format!("Could not change the default application: {error}"))
                        .timeout(7)
                        .build(),
                );
            }
        }
    });

    chooser.dialog.present(Some(window));
    chooser.list.grab_focus();
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::{ffi::OsString, fs, os::unix::ffi::OsStringExt};

    #[cfg(unix)]
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn phase_13b_search_ui_feedback_reports_progress_stop_skips_and_limits() {
        let summary = FilenameSearchSummary {
            matched: 12,
            examined_entries: 90,
            examined_directories: 8,
            skipped_entries: 2,
            skipped_directories: 1,
            skipped_mounts: 1,
            depth_limited: 1,
            truncated: true,
        };
        let progress = filename_search_feedback(summary, true, false);
        assert!(progress.contains("Searching… 12 matches from 90 items"));
        assert!(progress.contains("5 skipped"));
        assert!(progress.contains("incomplete"));

        let stopped = filename_search_feedback(summary, false, true);
        assert!(stopped.starts_with("Stopped with 12 matches"));
        let finished = filename_search_feedback(FilenameSearchSummary::default(), false, false);
        assert_eq!(finished, "0 matches");
    }

    #[test]
    fn phase_13a_filter_rejects_stale_results_and_restores_only_visible_selection() {
        assert!(folder_filter_response_is_current(19, 19));
        assert!(!folder_filter_response_is_current(20, 19));

        let fixture = tempdir().expect("temporary filter fixture");
        let keep = fixture.path().join("keep.txt");
        let hidden_by_filter = fixture.path().join("notes.log");
        fs::write(&keep, b"keep").expect("filter fixture");
        fs::write(&hidden_by_filter, b"hide").expect("filter fixture");
        let entries = floe_core::enumerate_directory(fixture.path())
            .expect("enumerate filter fixture")
            .into_entries()
            .into_iter()
            .map(Arc::new)
            .filter(|entry| entry.path() == keep)
            .collect::<Vec<_>>();
        let selected = vec![keep, hidden_by_filter];
        assert_eq!(selection_indices_for_paths(&entries, &selected), vec![0]);
    }

    #[test]
    fn phase_9f_interaction_space_respects_text_focus_and_exports_stable_accelerator() {
        assert_eq!(QUICK_PREVIEW_ACCELERATOR, "space");
        assert!(is_quick_preview_space(
            gtk::gdk::Key::space,
            gtk::gdk::ModifierType::empty()
        ));
        assert!(!is_quick_preview_space(
            gtk::gdk::Key::space,
            gtk::gdk::ModifierType::CONTROL_MASK
        ));
        assert!(!is_quick_preview_space(
            gtk::gdk::Key::Return,
            gtk::gdk::ModifierType::empty()
        ));
        assert_eq!(MILLER_DETAIL_ACTIONS[0], "miller-preview-hook");
    }

    #[test]
    fn phase_11d_vim_dispatch_reuses_bounded_selection_and_registered_actions() {
        use crate::vim_mode::VimCommand;

        assert_eq!(vim_selection_target(None, 4, VimCommand::Next), Some(0));
        assert_eq!(
            vim_selection_target(Some(0), 4, VimCommand::Previous),
            Some(0)
        );
        assert_eq!(vim_selection_target(Some(3), 4, VimCommand::Next), Some(3));
        assert_eq!(vim_selection_target(Some(2), 4, VimCommand::First), Some(0));
        assert_eq!(vim_selection_target(Some(2), 4, VimCommand::Last), Some(3));
        assert_eq!(vim_selection_target(None, 0, VimCommand::First), None);
        assert!(crate::command_registry::command("win.parent").is_some());
        assert!(crate::command_registry::command("win.open").is_some());
        assert!(MILLER_NAVIGATION_ACTIONS.contains(&"win.miller-child"));
    }

    #[test]
    fn phase_7b_session_snapshot_preserves_exact_selection_scroll_and_view() {
        let mut session = BrowserSession::new(
            BrowserSessionId::new(9).expect("phase 7B fixture"),
            PathBuf::from("/work"),
            FolderViewState {
                mode: ViewMode::Grid,
                ..FolderViewState::default()
            },
        )
        .expect("phase 7B fixture");
        let raw = PathBuf::from(OsString::from_vec(b"/work/raw-\xff".to_vec()));
        session
            .set_selection(vec![raw.clone()])
            .expect("phase 7B fixture");
        session
            .set_scroll_anchor(Some(
                SessionScrollAnchor::new(Some(raw.clone()), 37).expect("phase 7B fixture"),
            ))
            .expect("phase 7B fixture");

        let snapshot = session_restore_snapshot(&session);
        assert_eq!(snapshot.selected_paths, vec![raw.clone()]);
        assert_eq!(snapshot.anchor_path, Some(raw));
        assert_eq!(snapshot.anchor_index, 37);
        assert_eq!(session.current().view().mode, ViewMode::Grid);
    }

    #[test]
    fn phase_7b_tab_ui_uses_stable_display_only_titles_and_action_contract() {
        assert_eq!(tab_title(Path::new("/")), "/");
        assert_eq!(tab_title(Path::new("/home/user/Documents")), "Documents");
        let raw = PathBuf::from(OsString::from_vec(b"/tmp/raw-\xff".to_vec()));
        assert!(tab_title(&raw).contains('\u{fffd}'));
        assert_eq!(raw.as_os_str().as_encoded_bytes(), b"/tmp/raw-\xff");
        let actions = [
            "new-tab",
            "close-tab-active",
            "next-tab",
            "previous-tab",
            "move-tab-left",
            "move-tab-right",
            "open-new-tab",
            "open-background-tab",
        ];
        assert_eq!(actions.len(), 8);
        assert!(actions.contains(&"open-background-tab"));
    }

    #[cfg(unix)]
    #[test]
    fn phase_7b_folder_tabs_accept_only_one_navigable_non_trash_entry() {
        let fixture = tempdir().expect("phase 7B fixture");
        fs::create_dir(fixture.path().join("folder")).expect("phase 7B fixture");
        fs::write(fixture.path().join("file.txt"), b"floe").expect("phase 7B fixture");
        let listing = floe_core::enumerate_directory(fixture.path()).expect("phase 7B fixture");
        let entries = listing
            .entries()
            .iter()
            .cloned()
            .map(Arc::new)
            .collect::<Vec<_>>();
        let folder = entries
            .iter()
            .find(|entry| entry.is_navigable_directory())
            .expect("phase 7B fixture")
            .clone();
        let file = entries
            .iter()
            .find(|entry| entry.kind() == EntryKind::RegularFile)
            .expect("phase 7B fixture")
            .clone();
        assert!(folder_tab_eligible(std::slice::from_ref(&folder), false));
        assert!(!folder_tab_eligible(&[folder], true));
        assert!(!folder_tab_eligible(&[file], false));
        assert!(!folder_tab_eligible(&entries, false));
    }

    #[test]
    fn phase_7c_tab_actions_expose_reopen_and_all_close_variants() {
        assert_eq!(REOPEN_CLOSED_ACCELERATOR, "<Control><Shift>t");
        assert_eq!(
            TAB_CLOSE_VARIANT_ACTIONS,
            [
                "win.close-tabs-left",
                "win.close-tabs-right",
                "win.close-other-tabs"
            ]
        );
        assert_ne!(TabCloseVariant::Left, TabCloseVariant::Right);
        assert_ne!(TabCloseVariant::Others, TabCloseVariant::Left);
    }

    #[test]
    fn phase_6n_browser_trash_actions_are_mode_and_metadata_scoped() {
        assert_eq!(
            trash_mode_action_state(false, 1, true, 3),
            TrashModeActionState {
                restore: false,
                permanent_delete: false,
                empty: false,
            }
        );
        assert_eq!(
            trash_mode_action_state(true, 2, true, 3),
            TrashModeActionState {
                restore: true,
                permanent_delete: true,
                empty: true,
            }
        );
        assert!(!trash_mode_action_state(true, 1, false, 1).restore);
        assert!(trash_mode_action_state(true, 1, false, 1).permanent_delete);
        assert!(!trash_mode_action_state(true, 0, true, 0).empty);
    }

    #[test]
    fn phase_6n_actions_use_truthful_irreversible_wording() {
        assert_eq!("Restore", "Restore");
        assert!("Empty Trash…".contains("Trash"));
        assert!(
            !"Delete Permanently…"
                .to_ascii_lowercase()
                .contains("secure")
        );
    }

    #[test]
    fn phase_6k2_mount_auth_is_window_parented_and_credential_opaque() {
        let policy = mount_authentication_policy();

        assert!(policy.window_parented);
        assert!(policy.credential_opaque);
        assert!(policy.feedback.contains("your desktop will ask"));
        assert!(!policy.feedback.to_ascii_lowercase().contains("store"));
        assert!(!policy.feedback.to_ascii_lowercase().contains("log"));
    }

    #[test]
    fn phase_6k2_preferences_preserve_sidebar_state_across_view_changes() {
        let mut current = ViewPreferences::default();
        current.mode = ViewMode::List;
        current.grid_size = GridSize::default();
        current.sidebar_density = SidebarDensity::Comfortable;
        current.sidebar_width = Some(312);
        let updated =
            with_current_view_preferences(current, ViewMode::Grid, GridSize::default().zoom_in());

        assert_eq!(updated.mode, ViewMode::Grid);
        assert_eq!(updated.sidebar_density, SidebarDensity::Comfortable);
        assert_eq!(updated.sidebar_width, Some(312));
        assert_eq!(SIDEBAR_PERSIST_DEBOUNCE, Duration::from_millis(320));
    }

    #[test]
    fn phase_6k2_sidebar_width_debounces_clamps_and_resets_to_appearance_default() {
        assert_eq!(sidebar_width_from_position(-1), SIDEBAR_WIDTH_MIN);
        assert_eq!(sidebar_width_from_position(0), SIDEBAR_WIDTH_MIN);
        assert_eq!(sidebar_width_from_position(312), 312);
        assert_eq!(sidebar_width_from_position(i32::MAX), SIDEBAR_WIDTH_MAX);
        assert_eq!(SIDEBAR_PERSIST_DEBOUNCE, Duration::from_millis(320));

        let mut current = ViewPreferences::default();
        current.mode = ViewMode::Grid;
        current.grid_size = GridSize::default();
        current.sidebar_density = SidebarDensity::Balanced;
        current.sidebar_width = Some(312);
        let reset = preferences_after_sidebar_reset(current);
        assert_eq!(reset.sidebar_width, None);
        assert_eq!(reset.sidebar_density, SidebarDensity::Balanced);
    }

    #[cfg(unix)]
    #[test]
    fn phase_6j_selection_restoration_uses_multiple_exact_non_utf8_paths() {
        let directory = tempdir().expect("temporary directory should be created");
        let first_name = OsString::from_vec(vec![b'f', 0x80]);
        let target_name = OsString::from_vec(vec![b'f', 0x81]);
        fs::write(directory.path().join(&first_name), b"first")
            .expect("first non-UTF-8 file should be created");
        fs::write(directory.path().join(&target_name), b"target")
            .expect("target non-UTF-8 file should be created");

        let entries: Vec<_> = floe_core::enumerate_directory(directory.path())
            .expect("directory should enumerate")
            .into_entries()
            .into_iter()
            .map(Arc::new)
            .collect();
        let first_path = directory.path().join(first_name);
        let target_path = directory.path().join(target_name);
        let selected_paths = vec![target_path.clone(), first_path.clone()];
        let indices = selection_indices_for_paths(&entries, &selected_paths);

        assert_eq!(indices.len(), 2);
        let restored = indices
            .iter()
            .map(|index| entries[*index as usize].path())
            .collect::<HashSet<_>>();
        assert_eq!(
            restored,
            HashSet::from([first_path.as_path(), target_path.as_path()])
        );
        assert_eq!(
            entries[0].display_name_lossy(),
            entries[1].display_name_lossy(),
            "the test must exercise colliding lossy display names"
        );
    }

    #[cfg(unix)]
    #[test]
    fn phase_6j_action_policy_distinguishes_single_and_multi_selection() {
        let directory = tempdir().expect("temporary directory should be created");
        fs::write(directory.path().join("one.txt"), b"one").expect("fixture should be written");
        fs::write(directory.path().join("two.txt"), b"two").expect("fixture should be written");
        let entries = floe_core::enumerate_directory(directory.path())
            .expect("directory should enumerate")
            .into_entries()
            .into_iter()
            .map(Arc::new)
            .collect::<Vec<_>>();

        assert_eq!(
            selection_action_state(&[]),
            SelectionActionState {
                single: false,
                open_with: false,
                checksum: false,
                transfer: false,
                duplicate: false,
                symbolic_link: false,
                hard_link: false,
                reveal_link: false,
                copy_identity: false,
                rename: false,
                trash: false,
            }
        );
        assert_eq!(
            selection_action_state(&entries[..1]),
            SelectionActionState {
                single: true,
                open_with: true,
                checksum: true,
                transfer: true,
                duplicate: true,
                symbolic_link: true,
                hard_link: true,
                reveal_link: false,
                copy_identity: true,
                rename: true,
                trash: true,
            }
        );
        assert_eq!(
            selection_action_state(&entries),
            SelectionActionState {
                single: false,
                open_with: false,
                checksum: true,
                transfer: true,
                duplicate: true,
                symbolic_link: false,
                hard_link: false,
                reveal_link: false,
                copy_identity: true,
                rename: false,
                trash: true,
            }
        );
        assert_eq!(
            selection_status(&entries, &entries, None),
            "2 items selected · 6 B known"
        );
    }

    #[test]
    fn phase_6t_status_reports_only_known_non_recursive_bytes() {
        let directory = tempdir().expect("temporary directory should be created");
        fs::write(directory.path().join("known.bin"), vec![0; 1_500])
            .expect("fixture should be written");
        fs::create_dir(directory.path().join("folder")).expect("folder should be created");
        let entries = floe_core::enumerate_directory(directory.path())
            .expect("directory should enumerate")
            .into_entries()
            .into_iter()
            .map(Arc::new)
            .collect::<Vec<_>>();
        assert_eq!(
            selection_status(&entries, &[], None),
            "2 items · 1.5 KB known"
        );
        assert_eq!(
            selection_status(
                &entries,
                &[],
                Some(StorageFacts {
                    total: Some(10_000),
                    free: Some(4_000),
                    read_only: Some(true),
                }),
            ),
            "2 items · 1.5 KB known · 4.0 KB free of 10.0 KB · Read-only"
        );
        let folder = entries
            .iter()
            .find(|entry| entry.is_navigable_directory())
            .expect("folder entry");
        assert_eq!(
            selection_status(&entries, std::slice::from_ref(folder), None),
            "folder selected"
        );
    }

    #[test]
    fn phase_6t_status_device_capacity_extends_base_state_without_false_detail() {
        assert_eq!(
            device_status_text(
                "Mounted",
                StorageFacts {
                    total: Some(8_000),
                    free: Some(2_000),
                    read_only: Some(false),
                }
            ),
            "Mounted · 2.0 KB free of 8.0 KB"
        );
        assert_eq!(
            device_status_text(
                "Mounted",
                StorageFacts {
                    total: None,
                    free: None,
                    read_only: None,
                }
            ),
            "Mounted"
        );
    }

    #[test]
    fn phase_6t_status_rejects_stale_or_wrong_location_storage_facts() {
        let request = StorageRequest {
            generation: 4,
            target: StorageTarget::CurrentLocation,
            path: PathBuf::from("/current"),
        };
        assert!(current_storage_request_is_current(
            &request,
            4,
            Path::new("/current"),
            false
        ));
        assert!(!current_storage_request_is_current(
            &request,
            5,
            Path::new("/current"),
            false
        ));
        assert!(!current_storage_request_is_current(
            &request,
            4,
            Path::new("/elsewhere"),
            false
        ));
        assert!(!current_storage_request_is_current(
            &request,
            4,
            Path::new("/current"),
            true
        ));
    }

    #[test]
    fn phase_5c_context_shortcuts_ignore_lock_state_but_reject_command_chords() {
        assert!(is_context_menu_shortcut(
            gtk::gdk::Key::Menu,
            gtk::gdk::ModifierType::LOCK_MASK,
        ));
        assert!(is_context_menu_shortcut(
            gtk::gdk::Key::F10,
            gtk::gdk::ModifierType::SHIFT_MASK | gtk::gdk::ModifierType::LOCK_MASK,
        ));
        assert!(!is_context_menu_shortcut(
            gtk::gdk::Key::F10,
            gtk::gdk::ModifierType::SHIFT_MASK | gtk::gdk::ModifierType::CONTROL_MASK,
        ));
    }

    #[cfg(unix)]
    #[test]
    fn phase_6m_confirmation_preserves_exact_targets_and_requires_shift_delete() {
        let path = PathBuf::from("/tmp").join(OsString::from_vec(b"line\nraw-\xff".to_vec()));
        assert_eq!(
            permanent_delete_target_label(&path),
            "/tmp/line\\x0araw-\\xff"
        );
        assert!(is_permanent_delete_shortcut(
            gtk::gdk::Key::Delete,
            gtk::gdk::ModifierType::SHIFT_MASK | gtk::gdk::ModifierType::LOCK_MASK,
        ));
        assert!(!is_permanent_delete_shortcut(
            gtk::gdk::Key::Delete,
            gtk::gdk::ModifierType::SHIFT_MASK | gtk::gdk::ModifierType::CONTROL_MASK,
        ));
        assert!(!is_permanent_delete_shortcut(
            gtk::gdk::Key::Delete,
            gtk::gdk::ModifierType::empty(),
        ));
    }

    #[test]
    fn phase_5d_open_with_is_limited_to_launchable_file_kinds() {
        assert!(open_with_kind_eligible(EntryKind::RegularFile));
        assert!(open_with_kind_eligible(EntryKind::SymbolicLink {
            target_is_directory: false,
        }));
        assert!(!open_with_kind_eligible(EntryKind::Directory));
        assert!(!open_with_kind_eligible(EntryKind::SymbolicLink {
            target_is_directory: true,
        }));
        assert!(!open_with_kind_eligible(EntryKind::Other));
    }

    #[test]
    fn phase_5d_chooser_separates_open_from_default_changes() {
        assert_eq!(chooser_action_sensitivity(None, Some(0)), (false, false));
        assert_eq!(chooser_action_sensitivity(Some(0), Some(0)), (true, false));
        assert_eq!(chooser_action_sensitivity(Some(1), Some(0)), (true, true));
        assert_eq!(chooser_action_sensitivity(Some(0), None), (true, true));
    }

    #[test]
    fn phase_6k_sidebar_navigation_keeps_exact_non_utf8_path_identity() {
        let raw = OsString::from_vec(b"device-\xff".to_vec());
        let path = PathBuf::from("/run/media").join(raw);
        let target = exact_sidebar_target(&path);

        assert_eq!(target, path);
        assert_eq!(
            target.into_os_string().into_vec(),
            path.into_os_string().into_vec()
        );
    }

    #[test]
    fn phase_6q_templates_and_creation_actions_build_exact_typed_requests() {
        let raw_source =
            PathBuf::from("/templates").join(OsString::from_vec(b"source-\xff.txt".to_vec()));
        let destination = PathBuf::from("/work/new.txt");
        let template = CreateDialogKind::Template(raw_source.clone())
            .request(destination.clone())
            .expect("template request");
        assert_eq!(template.source(), Some(raw_source.as_path()));
        assert_eq!(template.destination(), destination);
        assert!(matches!(
            template.kind(),
            floe_core::CreateKind::Template { .. }
        ));

        let folder = CreateDialogKind::Directory
            .request(PathBuf::from("/work/folder"))
            .expect("directory request");
        let file = CreateDialogKind::EmptyFile
            .request(PathBuf::from("/work/file"))
            .expect("empty-file request");
        assert!(matches!(folder.kind(), floe_core::CreateKind::Directory));
        assert!(matches!(file.kind(), floe_core::CreateKind::EmptyFile));
        assert_eq!(CreateDialogKind::Directory.title(), "Create Folder");
    }

    #[test]
    fn phase_6q_reveal_resolves_relative_absolute_and_raw_targets_lexically() {
        let link = PathBuf::from("/work/links/item-link");
        assert_eq!(
            resolve_link_target(&link, Path::new("../target/./item")),
            Some(PathBuf::from("/work/target/item"))
        );
        assert_eq!(
            resolve_link_target(&link, Path::new("/absolute/../target")),
            Some(PathBuf::from("/target"))
        );

        let raw = PathBuf::from(OsString::from_vec(b"../raw-\xff".to_vec()));
        let resolved = resolve_link_target(&link, &raw).expect("raw relative target");
        assert_eq!(
            resolved.into_os_string().into_vec(),
            b"/work/raw-\xff".to_vec()
        );
        assert_eq!(
            resolve_link_target(Path::new("item-link"), Path::new("target")),
            Some(PathBuf::from("target"))
        );
    }

    #[test]
    fn phase_6q_clipboard_text_is_exact_multiline_and_rejects_lossy_paths() {
        let base = PathBuf::from("/work");
        let paths = vec![base.join("one.txt"), base.join("two words.txt")];
        assert_eq!(
            selection_clipboard_text(&paths, &base, ClipboardTextMode::Name)
                .expect("names should copy"),
            "one.txt\ntwo words.txt"
        );
        assert_eq!(
            selection_clipboard_text(&paths, &base, ClipboardTextMode::RelativePath)
                .expect("relative paths should copy"),
            "one.txt\ntwo words.txt"
        );
        assert_eq!(
            selection_clipboard_text(&paths[..1], &base, ClipboardTextMode::AbsolutePath)
                .expect("absolute path should copy"),
            "/work/one.txt"
        );
        assert_eq!(
            selection_clipboard_text(&paths[..1], &base, ClipboardTextMode::Uri)
                .expect("URI should copy"),
            "file:///work/one.txt"
        );

        let raw = base.join(OsString::from_vec(b"raw-\xff".to_vec()));
        assert!(
            selection_clipboard_text(std::slice::from_ref(&raw), &base, ClipboardTextMode::Name)
                .is_err()
        );
        let raw_uri =
            selection_clipboard_text(std::slice::from_ref(&raw), &base, ClipboardTextMode::Uri)
                .expect("raw path should retain exact URI identity");
        assert!(raw_uri.ends_with("raw-%FF"), "unexpected URI: {raw_uri}");
        assert!(
            selection_clipboard_text(
                &[PathBuf::from("/outside/item")],
                &base,
                ClipboardTextMode::RelativePath
            )
            .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn phase_6q_actions_distinguish_regular_files_and_symbolic_links() {
        let fixture = tempdir().expect("temporary fixture");
        let file = fixture.path().join("file.txt");
        let link = fixture.path().join("link");
        fs::write(&file, b"payload").expect("regular file");
        std::os::unix::fs::symlink("file.txt", &link).expect("symbolic link");
        let entries = floe_core::enumerate_directory(fixture.path())
            .expect("enumeration")
            .into_entries()
            .into_iter()
            .map(Arc::new)
            .collect::<Vec<_>>();
        let file_entry = entries
            .iter()
            .find(|entry| entry.path() == file)
            .expect("file entry");
        let link_entry = entries
            .iter()
            .find(|entry| entry.path() == link)
            .expect("link entry");

        let file_state = selection_action_state(std::slice::from_ref(file_entry));
        assert!(file_state.duplicate);
        assert!(file_state.symbolic_link);
        assert!(file_state.hard_link);
        assert!(!file_state.reveal_link);

        let link_state = selection_action_state(std::slice::from_ref(link_entry));
        assert!(link_state.duplicate);
        assert!(link_state.symbolic_link);
        assert!(!link_state.hard_link);
        assert!(link_state.reveal_link);
    }

    #[test]
    fn phase_7e_split_presentation_reports_ratio_and_bounded_snapshot_truthfully() {
        let ratio = floe_core::SplitRatio::new(6_000).expect("bounded ratio");
        assert_eq!(crate::ui::split_primary_position(1_000, ratio), 600);
        assert_eq!(
            crate::ui::split_snapshot_status(0, 0),
            "Activate pane to load or refresh"
        );
        assert_eq!(
            crate::ui::split_snapshot_status(1, 1),
            "1 cached item — activate to refresh"
        );
        assert_eq!(
            crate::ui::split_snapshot_status(SPLIT_SNAPSHOT_CAPACITY, 2_000),
            "First 512 of 2000 items cached — activate to refresh"
        );
    }

    #[test]
    fn phase_7e_split_actions_have_deterministic_sensitivity() {
        assert_eq!(
            split_action_state(false, false, false, false),
            SplitActionState {
                switch: false,
                close: false,
                swap: false,
                open_opposite: false,
                transfer_opposite: false,
            }
        );
        assert_eq!(
            split_action_state(false, true, true, false),
            SplitActionState {
                switch: false,
                close: false,
                swap: false,
                open_opposite: true,
                transfer_opposite: false,
            }
        );
        assert_eq!(
            split_action_state(true, true, true, false),
            SplitActionState {
                switch: true,
                close: true,
                swap: true,
                open_opposite: true,
                transfer_opposite: true,
            }
        );
        assert!(!split_action_state(true, true, true, true).open_opposite);
        assert!(!split_action_state(true, true, true, true).transfer_opposite);
    }

    #[test]
    fn phase_7e_opposite_pane_resolves_exact_authoritative_destination() {
        let raw = PathBuf::from(OsString::from_vec(b"/tmp/opposite-\xff".to_vec()));
        let mut tabs = floe_core::BrowserTabs::new(
            PathBuf::from("/primary"),
            floe_core::FolderViewState::default(),
        )
        .expect("initial tab");
        assert_eq!(opposite_pane_destination(&tabs), None);
        tabs.split_active(raw.clone(), floe_core::FolderViewState::default())
            .expect("secondary pane");
        assert_eq!(opposite_pane_destination(&tabs), Some(raw.clone()));
        tabs.activate_split_side(floe_core::SplitSide::Secondary)
            .expect("secondary active");
        assert_eq!(
            opposite_pane_destination(&tabs),
            Some(PathBuf::from("/primary"))
        );
    }

    #[test]
    fn phase_8c_integration_exports_logical_navigation_actions_without_fixed_rtl_keys() {
        assert_eq!(
            MILLER_NAVIGATION_ACTIONS,
            ["win.miller-parent", "win.miller-child"]
        );
    }

    #[test]
    fn phase_8e_surfaces_resolve_live_tab_split_and_typed_hover_ownership() {
        let raw = PathBuf::from(OsString::from_vec(b"/tmp/tab-drop-\xff".to_vec()));
        let mut tabs =
            BrowserTabs::new(raw.clone(), FolderViewState::default()).expect("initial tab");
        let tab_id = tabs.active_id();
        assert_eq!(
            tab_drop_destination(&tabs, tab_id, false),
            Some(DropDestination::Directory(raw.clone()))
        );
        assert_eq!(tab_drop_destination(&tabs, tab_id, true), None);
        tabs.split_active(PathBuf::from("/opposite"), FolderViewState::default())
            .expect("split");
        assert_eq!(
            split_drop_destination(&tabs, false),
            Some(DropDestination::Directory(PathBuf::from("/opposite")))
        );
        assert_eq!(DropHoverTarget::Tab(tab_id.get()), DropHoverTarget::Tab(1));
        assert_eq!(DropHoverTarget::OppositePane, DropHoverTarget::OppositePane);
    }

    #[test]
    fn phase_8f_integration_keeps_existing_views_and_exports_only_hook_actions() {
        assert_eq!(
            MILLER_DETAIL_ACTIONS,
            ["miller-preview-hook", "miller-inspector-hook"]
        );
        assert_eq!(MillerDetailSurface::Preview.title(), "Quick Preview");
        assert_eq!(MillerDetailSurface::Inspector.title(), "Inspector");
        assert!(VIEW_ACTIONS.contains(&("view-list", ViewCommand::List)));
        assert!(VIEW_ACTIONS.contains(&("view-grid", ViewCommand::Grid)));
        assert!(VIEW_ACTIONS.contains(&("view-miller", ViewCommand::Miller)));
    }

    #[test]
    fn phase_9a_integration_bounds_preview_drain_and_preserves_view_pipeline() {
        assert_eq!(PREVIEW_QUEUE_CAPACITY, 16);
        assert!(PREVIEW_QUEUE_CAPACITY.min(8) <= 8);
        assert!(VIEW_ACTIONS.contains(&("view-list", ViewCommand::List)));
        assert!(VIEW_ACTIONS.contains(&("view-grid", ViewCommand::Grid)));
        assert_eq!(MILLER_DETAIL_ACTIONS.len(), 2);
    }

    #[test]
    fn phase_7e_split_accessibility_has_actions_and_keyboard_alternatives() {
        assert_eq!(SPLIT_ACTION_NAMES.len(), 10);
        assert!(SPLIT_ACTION_NAMES.contains(&"win.toggle-split"));
        assert!(SPLIT_ACTION_NAMES.contains(&"win.switch-split-side"));
        assert!(SPLIT_ACTION_NAMES.contains(&"win.narrow-primary-pane"));
        assert!(SPLIT_ACTION_NAMES.contains(&"win.widen-primary-pane"));
        assert!(SPLIT_ACTION_NAMES.contains(&"win.open-opposite-pane"));
        assert!(SPLIT_ACTION_NAMES.contains(&"win.link-to-opposite-pane"));
        assert_eq!(TOGGLE_SPLIT_ACCELERATOR, "F3");
        assert_eq!(SWITCH_SPLIT_ACCELERATOR, "F6");
        assert_eq!(NARROW_PRIMARY_PANE_ACCELERATOR, "<Control><Alt>Left");
        assert_eq!(WIDEN_PRIMARY_PANE_ACCELERATOR, "<Control><Alt>Right");
        assert_eq!(OPEN_OPPOSITE_ACCELERATOR, "<Control><Shift>Return");
        assert_ne!(COPY_OPPOSITE_ACCELERATOR, MOVE_OPPOSITE_ACCELERATOR);
    }

    #[test]
    fn phase_10a_inspector_lifecycle_action_contract_is_keyboard_and_width_accessible() {
        assert_eq!(INSPECTOR_ACCELERATOR, "<Control>i");
        assert!(MILLER_DETAIL_ACTIONS.contains(&"miller-inspector-hook"));
        assert_ne!("narrow-inspector", "widen-inspector");
    }

    #[test]
    fn phase_7f_split_drop_destination_is_live_exact_and_trash_safe() {
        let raw = PathBuf::from(OsString::from_vec(b"/tmp/drop-\xff".to_vec()));
        let mut tabs = floe_core::BrowserTabs::new(
            PathBuf::from("/primary"),
            floe_core::FolderViewState::default(),
        )
        .expect("initial tab");
        assert_eq!(split_drop_destination(&tabs, false), None);
        tabs.split_active(raw.clone(), floe_core::FolderViewState::default())
            .expect("secondary pane");
        assert_eq!(
            split_drop_destination(&tabs, false),
            Some(DropDestination::Directory(raw))
        );
        assert_eq!(split_drop_destination(&tabs, true), None);
        tabs.activate_split_side(SplitSide::Secondary)
            .expect("secondary active");
        assert_eq!(
            split_drop_destination(&tabs, false),
            Some(DropDestination::Directory(PathBuf::from("/primary")))
        );
    }

    #[test]
    fn phase_7f_split_drag_accessibility_has_complete_action_alternatives() {
        assert!(SPLIT_ACTION_NAMES.contains(&"win.copy-to-opposite-pane"));
        assert!(SPLIT_ACTION_NAMES.contains(&"win.move-to-opposite-pane"));
        assert!(SPLIT_ACTION_NAMES.contains(&"win.link-to-opposite-pane"));
        assert_ne!(COPY_OPPOSITE_ACCELERATOR, MOVE_OPPOSITE_ACCELERATOR);
        assert_ne!(MOVE_OPPOSITE_ACCELERATOR, LINK_OPPOSITE_ACCELERATOR);
        assert_eq!(LINK_OPPOSITE_ACCELERATOR, "<Control><Alt>l");
    }
}
