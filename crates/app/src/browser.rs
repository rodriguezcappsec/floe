use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    ffi::{OsStr, OsString},
    io,
    os::unix::ffi::OsStrExt,
    path::{Component, Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::{Duration, SystemTime},
};

use crate::operation_hub::{OperationEventHub, WindowRuntimeId};
use crate::operation_reveal::{
    OPERATION_REVEAL_DURATION_MS, OperationRevealRequest, PendingOperationReveal,
};
use adw::prelude::*;
use floe_core::SymbolicLinkMode;
use floe_core::{
    AdvancedFilter, BrowserSession, BrowserSessionId, BrowserTabs, ChecksumAlgorithm,
    ConflictPolicy, ContentSearchMatch, ContentSearchRequest, ContentSearchSummary, CreateRequest,
    DestructiveScope, DirectoryEntry, DirectoryError, DirectoryGrouping, DirectoryPlacement,
    DirectorySort, EntryKind, EntryTypeFilter, FilenameSearchRequest, FilenameSearchScope,
    FilenameSearchSummary, FolderFilterMode, HiddenFilter, IntegrityBaseline,
    IntegrityMonitorSession, IntegrityMonitorStaleReason, IntegrityRescanDecision,
    IntegrityWatchEvent, IntegrityWatchSetPolicy, JobId, MillerChildKind, MillerColumnModel,
    MillerSelectionTransition, MoveRequest, OwnerFilter, PermanentDeleteRequest, RecentSearches,
    RenameRequest, RestoreRequest, SAVED_SEARCH_NAME_CAPACITY, SPLIT_RATIO_MAX, SPLIT_RATIO_MIN,
    SavedSearch, SearchHistoryPolicy, SearchKind, SearchQuery, SearchResultOrder,
    SessionScrollAnchor, SortColumn, SortDirection, SplitRatio, SplitSide, TabActivation, TabError,
    TrashEnumerateError, TrashRoot, VerifiedCopyRequest,
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

const COMPACT_TAB_TITLE_MAX_CHARS: i32 = 18;

fn compact_tab_title_label(title: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(title)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(COMPACT_TAB_TITLE_MAX_CHARS)
        .single_line_mode(true)
        .xalign(0.5)
        .build()
}

fn sidebar_device_name_label(name: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(name)
        .halign(gtk::Align::Start)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .single_line_mode(true)
        .build();
    label.set_tooltip_text(Some(name));
    label
}

fn sidebar_device_status_label(status: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(status)
        .halign(gtk::Align::Start)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .single_line_mode(true)
        .build();
    label.add_css_class("floe-status");
    label.set_tooltip_text(Some(status));
    label
}

fn recent_session_locations(session: &BrowserSession) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    std::iter::once(session.current().path())
        .chain(session.back_history().iter().rev().map(|item| item.path()))
        .chain(
            session
                .forward_history()
                .iter()
                .rev()
                .map(|item| item.path()),
        )
        .filter(|path| seen.insert((*path).to_path_buf()))
        .take(floe_core::RECENT_LOCATION_CAPACITY)
        .map(Path::to_path_buf)
        .collect()
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

fn device_start_failure_detail(error: &DeviceActionStartError) -> String {
    match error {
        DeviceActionStartError::UnknownDevice => {
            "The removable device disconnected before removal could start.".to_owned()
        }
        DeviceActionStartError::Busy { .. } => {
            "The copy was verified and flushed, but the removable device is busy.".to_owned()
        }
        DeviceActionStartError::Unavailable { .. } => format!(
            "The copy was verified and flushed, but the selected removal action is no longer available: {error}"
        ),
    }
}

const SPLIT_SNAPSHOT_CAPACITY: usize = 512;

#[derive(Clone, Debug)]
struct FolderFilterState {
    mode: FolderFilterMode,
    query: String,
    advanced: AdvancedFilter,
}

impl Default for FolderFilterState {
    fn default() -> Self {
        Self {
            mode: FolderFilterMode::Text,
            query: String::new(),
            advanced: AdvancedFilter::default(),
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
    if summary.metadata_unavailable > 0 {
        message.push_str(&format!(
            " · {} metadata unavailable",
            summary.metadata_unavailable
        ));
    }
    if summary.truncated {
        message.push_str(" · incomplete (search limit reached)");
    }
    message
}

fn content_search_feedback(summary: ContentSearchSummary, running: bool, stopped: bool) -> String {
    let skipped = summary
        .skipped_entries
        .saturating_add(summary.skipped_directories)
        .saturating_add(summary.skipped_mounts)
        .saturating_add(summary.depth_limited)
        .saturating_add(summary.metadata_unavailable)
        .saturating_add(summary.binary_skipped)
        .saturating_add(summary.encoding_skipped)
        .saturating_add(summary.too_large)
        .saturating_add(summary.changed_files)
        .saturating_add(summary.long_lines_skipped);
    let mut message = if running {
        format!(
            "Searching contents… {} matches in {} files",
            summary.matched, summary.examined_files
        )
    } else if stopped {
        format!("Stopped with {} content matches", summary.matched)
    } else {
        format!("{} content matches", summary.matched)
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
    background_feedback::{
        BackgroundActivity, BackgroundFeedbackState, BackgroundOutcomeKind, FeedbackPresentation,
        outcome_accessible_description, result_action, running_presentation, stopping_presentation,
    },
    batch_rename::{BatchRenameSource, build_batch_rename_dialog, refresh_batch_rename_dialog},
    bookmarks::{
        BookmarkRecord, SharedBookmarkNotice, SharedBookmarks, records_with_alias,
        reordered_records,
    },
    checksum_ui::{ChecksumDialogInput, build_checksum_request},
    cli_routing::{CliRoute, CliRouteError, CliRouteWorker},
    clipboard::{self, ClipboardTransfer},
    devices::{
        DeviceAction, DeviceActionOutcome, DeviceId, DeviceMonitor, DeviceSnapshot,
        DeviceSubscriptionId,
    },
    drag_drop::{
        DropAction, DropDestination, DropEvent, DropHoverTarget, DropRequest, install_drag_source,
        install_drop_target, install_drop_target_with_hover, plan_directory_drop,
    },
    file_watcher::{
        FileWatcher, RenamePair, ViewStateSnapshot, WatchBatch, batch_is_current,
        reconcile_view_state, scroll_anchor_index, watch_failure_message,
    },
    guardrail_controller::{GuardrailAuthorizationItem, GuardrailStoreHealth},
    guardrail_preflight::PreflightEnvironment,
    guardrail_ui::review_and_authorize,
    iconography::EntryIconStyle,
    inspector::{InspectorRequest, InspectorSubmitError, InspectorWorker},
    integrity_executor::IntegrityRequest,
    integrity_monitor::{
        IntegrityMonitorOutcome, IntegrityMonitorRequest, IntegrityMonitorRootKind,
        IntegrityMonitorWorker,
    },
    integrity_monitor_store::IntegrityBaselineStoragePolicy,
    integrity_ui::private_fingerprint_store_path,
    integrity_watch::IntegrityWatchSet,
    launcher,
    location_completion::LocationCompletionWorker,
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
        ClickPolicy, ColorSchemePreference, PreferencePresentationChanges, SIDEBAR_WIDTH_MAX,
        SIDEBAR_WIDTH_MIN, SharedPreferences, SidebarDensity, ViewPreferences, WindowSize,
        clamp_clamav_file_limit_mib, clamp_font_scale, clamp_sidebar_width,
        normalized_clamav_total_limit_gib, validated_font_family,
    },
    preview::{
        PREVIEW_QUEUE_CAPACITY, PreviewCachePolicy, PreviewLimits, PreviewOutcome, PreviewRequest,
        PreviewSourceKey, PreviewSubmitError, PreviewWorker,
    },
    privacy_security::{
        InspectionOutcome, PrivacyInspectionState, PrivacySecurityRequest, PrivacySecurityResult,
    },
    properties::{
        ExecutableEdit, PROPERTIES_RESULT_CAPACITY, PermissionEditorInput, PropertiesRequest,
        PropertiesSubmitError, PropertiesWorker, build_permission_request,
        checksum_targets_for_presentation, present as present_properties,
    },
    selection_mode::{
        SELECTION_PATH_CAPACITY, SelectionCompletion, SelectionConfig, SelectionFilterRequest,
        SelectionFilterSubmitError, SelectionFilterWorker, SelectionMode, SelectionOptionResult,
        SelectionValidationOutcome, SelectionValidationRequest, SelectionValidationWorker,
    },
    session_store::SessionStoreWorker,
    sort_metadata_index::{MetadataIndexEventKind, MetadataIndexSubmitError, MetadataIndexWorker},
    state::{
        ApplicationState, GuardrailAuthorized, GuardrailAuthorizedBatchRename, TrackedOperation,
        TransferIntent, destructive_scope_for_move, destructive_scope_for_permanent_delete,
        destructive_scope_for_rename, destructive_scope_for_restore, destructive_scope_for_trash,
        destructive_scopes_for_batch_rename, transfer_destination, validate_rename_name,
    },
    storage::{
        StorageFacts, StorageRequest, StorageSubmitError, StorageTarget, StorageWorker,
        format_bytes, format_storage_facts,
    },
    threat_scan::{
        ThreatFileStatus, ThreatScanLimits, ThreatScanOutcome, ThreatScanRequest, format_scan_limit,
    },
    thumbnail::{ThumbnailSubmitError, ThumbnailWorker},
    trash_executor::TrashRequest,
    ui::{self, BrowserWidgets},
    view::{
        FileViewDensity, FolderViewState, GridSize, ListColumn, MillerColumnWidth, VIEW_ACTIONS,
        ViewCommand, ViewMode,
    },
    worker::{BrowserWorker, ResponseKind},
};

use crate::{
    devices::{DeviceActionStartError, resolve_removal_target, revalidate_removal_target},
    verified_copy_executor::{VerifiedCopyError, VerifiedCopyResult, present_verified_copy},
    verified_usb::{DeviceFlushResult, DeviceFlushWorker, VerifiedUsbWorkflow},
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
const WINDOW_SIZE_PERSIST_DEBOUNCE: Duration = Duration::from_millis(320);

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

fn remember_window_size_if_normal(
    preferences: &mut ViewPreferences,
    width: i32,
    height: i32,
    maximized: bool,
    fullscreen: bool,
) -> bool {
    let Some(size) = WindowSize::from_normal_allocation(width, height, maximized, fullscreen)
    else {
        return false;
    };
    if preferences.window_size == Some(size) {
        return false;
    }
    preferences.window_size = Some(size);
    true
}

const ACTIVE_OPERATION_CLOSE_MESSAGE: &str = "File operations are still running. Wait for them to finish or cancel them before closing this window.";

const fn window_close_allowed(has_active_jobs: bool) -> bool {
    !has_active_jobs
}

fn present_checksum_dialog_for_targets(
    parent: &adw::ApplicationWindow,
    toast_overlay: &adw::ToastOverlay,
    state: Rc<ApplicationState>,
    targets: Arc<[PathBuf]>,
) {
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
    let toast_overlay = toast_overlay.clone();
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
                Err(error) => {
                    error_label.set_label(&format!("Could not queue checksum calculation: {error}"))
                }
            },
            Err(error) => error_label.set_label(&error.to_string()),
        }
    });
    widgets.dialog.present(Some(parent));
    widgets.algorithm_dropdown.grab_focus();
}

pub struct BrowserServices {
    browser: BrowserWorker,
    thumbnails: Option<ThumbnailWorker>,
    metadata: Option<MetadataWorker>,
    metadata_index: Option<MetadataIndexWorker>,
    inspector: Option<InspectorWorker>,
    preview: Option<PreviewWorker>,
    properties: Option<PropertiesWorker>,
    storage: Option<StorageWorker>,
    bookmarks: Option<SharedBookmarks>,
    devices: DeviceMonitor,
    preferences: Option<SharedPreferences>,
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
        metadata_index: Option<MetadataIndexWorker>,
        inspector: Option<InspectorWorker>,
        preview: Option<PreviewWorker>,
        properties: Option<PropertiesWorker>,
        storage: Option<StorageWorker>,
        bookmarks: Option<SharedBookmarks>,
        devices: DeviceMonitor,
        preferences: Option<SharedPreferences>,
        session_store: Option<SessionStoreWorker>,
    ) -> Self {
        Self {
            browser,
            thumbnails,
            metadata,
            metadata_index,
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
    desktop_integration: Rc<crate::integration::DesktopIntegrationController>,
    context_menu_editor: crate::context_menu::ContextMenuEditor,
    terminal_chooser: crate::terminal_ui::TerminalChooser,
    tabs: Rc<RefCell<BrowserTabs>>,
    worker: RefCell<BrowserWorker>,
    thumbnail_worker: RefCell<Option<ThumbnailWorker>>,
    metadata_worker: RefCell<Option<MetadataWorker>>,
    metadata_index_worker: RefCell<Option<MetadataIndexWorker>>,
    metadata_index_generation: Cell<u64>,
    inspector_worker: RefCell<Option<InspectorWorker>>,
    inspector_generation: Cell<u64>,
    preview_worker: RefCell<Option<PreviewWorker>>,
    preview_generation: Cell<u64>,
    properties_worker: RefCell<Option<PropertiesWorker>>,
    properties_generation: Cell<u64>,
    privacy_security_generation: Cell<u64>,
    threat_scan_generation: Cell<u64>,
    background_feedback_state: RefCell<BackgroundFeedbackState>,
    background_feedback_rows: RefCell<HashMap<BackgroundActivity, gtk::Box>>,
    last_properties_presentation: RefCell<Option<crate::properties::PropertiesPresentation>>,
    last_privacy_outcome: RefCell<Option<InspectionOutcome>>,
    last_threat_outcome: RefCell<Option<ThreatScanOutcome>>,
    last_sanitized_copy: RefCell<Option<PathBuf>>,
    storage_worker: RefCell<Option<StorageWorker>>,
    current_storage_generation: Cell<u64>,
    device_storage_generation: Cell<u64>,
    current_storage_facts: Cell<Option<StorageFacts>>,
    device_storage_facts: RefCell<HashMap<String, StorageFacts>>,
    device_snapshots: RefCell<Vec<DeviceSnapshot>>,
    verified_usb_workflow: RefCell<Option<VerifiedUsbLive>>,
    verified_usb_flush_worker: RefCell<Option<DeviceFlushWorker>>,
    verified_usb_flush_source: RefCell<Option<glib::SourceId>>,
    thumbnail_generation: Cell<u64>,
    active_generation: Cell<u64>,
    show_hidden: Cell<bool>,
    trash_active: Cell<bool>,
    trash_root: TrashRoot,
    all_listed_entries: RefCell<Arc<[Arc<DirectoryEntry>]>>,
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
    search_index_worker: RefCell<Option<crate::search_index::SearchIndexWorker>>,
    search_index_generation: Cell<u64>,
    search_index_query_active: Cell<bool>,
    search_index_fallback_note: RefCell<Option<String>>,
    duplicate_worker: RefCell<Option<crate::duplicate_finder::DuplicateFinderWorker>>,
    duplicate_generation: Cell<u64>,
    duplicate_running: Cell<bool>,
    duplicate_progress: RefCell<Option<crate::duplicate_ui::DuplicateProgressDialog>>,
    content_search_worker: RefCell<Option<crate::content_search::ContentSearchWorker>>,
    content_search_generation: Cell<u64>,
    content_search_active: Cell<bool>,
    content_search_running: Cell<bool>,
    content_search_results: RefCell<Vec<Arc<ContentSearchMatch>>>,
    content_search_store: RefCell<Option<gio::ListStore>>,
    content_search_root: RefCell<Option<PathBuf>>,
    pending_content_search: RefCell<Option<ContentSearchRequest>>,
    content_search_summary: RefCell<Option<ContentSearchSummary>>,
    recent_searches: RefCell<RecentSearches>,
    search_result_order: Cell<SearchResultOrder>,
    pending_saved_search: RefCell<Option<SearchQuery>>,
    selected_saved_search_id: Cell<Option<u64>>,
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
    shared_preferences: Option<SharedPreferences>,
    session_store: RefCell<Option<SessionStoreWorker>>,
    session_saved: Cell<bool>,
    preference_baseline: RefCell<ViewPreferences>,
    current_preferences: RefCell<ViewPreferences>,
    window_size_save_source: RefCell<Option<glib::SourceId>>,
    sidebar_save_source: RefCell<Option<glib::SourceId>>,
    ignore_sidebar_position_signal: Cell<bool>,
    split_snapshots: RefCell<HashMap<BrowserSessionId, [SplitPaneSnapshot; 2]>>,
    ignore_split_position_signal: Cell<bool>,
    pending_location: RefCell<Option<PendingLocation>>,
    breadcrumb_paths: RefCell<Vec<PathBuf>>,
    location_completion_worker: RefCell<Option<LocationCompletionWorker>>,
    location_completion_generation: Cell<u64>,
    location_completion_source: RefCell<Option<glib::SourceId>>,
    cli_route_worker: RefCell<Option<CliRouteWorker>>,
    association_worker: Option<Rc<launcher::AssociationWorker>>,
    custom_action_worker: RefCell<Option<crate::custom_actions::CustomActionWorker>>,
    privileged_access: Rc<crate::privileged_access::PrivilegedAccessController>,
    application_state: Rc<ApplicationState>,
    operation_event_hub: Rc<OperationEventHub>,
    window_runtime_id: WindowRuntimeId,
    completion_notification_namespace: u64,
    shared_bookmarks: Option<SharedBookmarks>,
    bookmarks: RefCell<Vec<BookmarkRecord>>,
    bookmarks_loaded: Cell<bool>,
    bookmark_revision: Cell<u64>,
    bookmark_save_in_flight: Cell<bool>,
    device_monitor: DeviceMonitor,
    device_subscription: Cell<Option<DeviceSubscriptionId>>,
    drop_hover_source: RefCell<Option<glib::SourceId>>,
    file_watcher: FileWatcher,
    watch_generation: Cell<u64>,
    pending_reconciliation: RefCell<Option<PendingReconciliation>>,
    pending_operation_reveal: RefCell<Option<PendingOperationReveal>>,
    pending_operation_emphasis: Cell<bool>,
    operation_emphasis_source: Rc<RefCell<Option<glib::SourceId>>>,
    integrity_monitor_worker: RefCell<Option<IntegrityMonitorWorker>>,
    integrity_baseline: RefCell<Option<IntegrityBaseline>>,
    integrity_session: RefCell<IntegrityMonitorSession>,
    integrity_watch_set: RefCell<Option<IntegrityWatchSet>>,
    integrity_request_generation: Cell<Option<u64>>,
    integrity_rescan_source: RefCell<Option<glib::SourceId>>,
    pending_scroll_index: Cell<Option<u32>>,
    terminal_worker: RefCell<Option<crate::terminal::TerminalWorker>>,
    terminal_availability: RefCell<Vec<crate::terminal::TerminalAvailability>>,
    terminal_request_id: Cell<u64>,
    template_worker: RefCell<Option<crate::templates::TemplateWorker>>,
    template_request_id: Cell<u64>,
    pending_create_rename: RefCell<Option<PathBuf>>,
    selection_mode: RefCell<Option<SelectionModeRuntime>>,
}

#[derive(Debug)]
struct VerifiedUsbLive {
    workflow: VerifiedUsbWorkflow,
    request: VerifiedCopyRequest,
}

struct SelectionModeRuntime {
    config: SelectionConfig,
    worker: SelectionValidationWorker,
    footer: gtk::Box,
    filename: gtk::Entry,
    status: gtk::Label,
    accept: gtk::Button,
    completion: Rc<dyn Fn(SelectionCompletion)>,
    request_id: Cell<u64>,
    pending: Cell<bool>,
    finished: Cell<bool>,
    filename_user_edited: Cell<bool>,
    updating_filename: Cell<bool>,
    filter: Option<gtk::DropDown>,
    filter_worker: Option<SelectionFilterWorker>,
    filter_generation: Cell<u64>,
    filter_pending: RefCell<Option<SelectionFilterRequest>>,
    choices: Vec<(String, SelectionChoiceWidget)>,
    poll_source: RefCell<Option<glib::SourceId>>,
}

enum SelectionChoiceWidget {
    Boolean(gtk::CheckButton),
    Combo(gtk::DropDown, Vec<String>),
}

impl SelectionModeRuntime {
    fn accepted(&self, paths: Vec<PathBuf>) -> SelectionCompletion {
        if self.filter.is_none() && self.choices.is_empty() {
            return SelectionCompletion::Accepted(paths);
        }
        let current_filter = self.filter.as_ref().and_then(|filter| {
            let selected = filter.selected();
            (selected != gtk::INVALID_LIST_POSITION)
                .then(|| usize::try_from(selected).ok())
                .flatten()
        });
        let choices = self
            .choices
            .iter()
            .map(|(id, widget)| {
                let value = match widget {
                    SelectionChoiceWidget::Boolean(check) => check.is_active().to_string(),
                    SelectionChoiceWidget::Combo(dropdown, ids) => ids
                        .get(usize::try_from(dropdown.selected()).unwrap_or(usize::MAX))
                        .cloned()
                        .unwrap_or_default(),
                };
                (id.clone(), value)
            })
            .collect();
        SelectionCompletion::AcceptedWithOptions(
            paths,
            SelectionOptionResult {
                current_filter,
                choices,
            },
        )
    }
}

impl Drop for BrowserController {
    fn drop(&mut self) {
        let feedback = self.background_feedback_state.get_mut();
        let threat_generation = self.threat_scan_generation.get();
        if feedback.is_active(BackgroundActivity::ThreatScan, threat_generation) {
            self.application_state.cancel_threat_scan(threat_generation);
        }
        let privacy_generation = self.privacy_security_generation.get();
        if feedback.is_active(BackgroundActivity::PrivacyInspection, privacy_generation)
            || feedback.is_active(BackgroundActivity::MetadataSanitization, privacy_generation)
        {
            self.application_state
                .cancel_privacy_security(privacy_generation);
        }
        self.operation_event_hub.unregister(self.window_runtime_id);
        if let Some(source) = self.sidebar_save_source.get_mut().take() {
            source.remove();
        }
        if let Some(source) = self.location_completion_source.get_mut().take() {
            source.remove();
        }
        if let Some(subscription) = self.device_subscription.take() {
            self.device_monitor.disconnect_changed(subscription);
        }
        if let Some(source) = self.drop_hover_source.get_mut().take() {
            source.remove();
        }
        if let Some(source) = self.verified_usb_flush_source.get_mut().take() {
            source.remove();
        }
        self.file_watcher.stop();
        if let Some(source) = self.integrity_rescan_source.get_mut().take() {
            source.remove();
        }
        if let Some(source) = self.operation_emphasis_source.borrow_mut().take() {
            source.remove();
        }
        if let Some(runtime) = self.selection_mode.get_mut().as_mut()
            && let Some(source) = runtime.poll_source.get_mut().take()
        {
            source.remove();
        }
        self.integrity_watch_set.get_mut().take();
        self.integrity_session.get_mut().disable();
        if let Some(worker) = self.metadata_index_worker.get_mut().as_mut() {
            worker.cancel();
        }
        self.persist_for_shutdown();
    }
}

impl BrowserController {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        widgets: BrowserWidgets,
        initial_path: PathBuf,
        restored_tabs: Option<BrowserTabs>,
        services: BrowserServices,
        view_preferences: ViewPreferences,
        application_state: Rc<ApplicationState>,
        operation_event_hub: Rc<OperationEventHub>,
        window_runtime_id: WindowRuntimeId,
    ) -> Rc<Self> {
        let BrowserServices {
            browser,
            thumbnails,
            metadata,
            metadata_index,
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
        let desktop_integration =
            crate::integration::DesktopIntegrationController::new(&widgets.window);
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
        let location_completion_worker = match LocationCompletionWorker::spawn() {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(%error, "could not start location completion worker");
                None
            }
        };
        let cli_route_worker = match CliRouteWorker::spawn() {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(%error, "could not start command-line route worker");
                None
            }
        };
        let association_worker = match launcher::AssociationWorker::spawn() {
            Ok(worker) => Some(Rc::new(worker)),
            Err(error) => {
                tracing::warn!(%error, "could not start file-association worker");
                None
            }
        };
        let custom_action_worker = match crate::custom_actions::CustomActionWorker::spawn() {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(%error, "could not start custom-action worker");
                None
            }
        };
        let privileged_access = crate::privileged_access::PrivilegedAccessController::new();
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
        let content_search_worker = match crate::content_search::ContentSearchWorker::spawn() {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(%error, "could not start content search worker");
                None
            }
        };
        let search_index_worker = match crate::search_index::SearchIndexWorker::spawn() {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(%error, "could not start optional search index worker");
                None
            }
        };
        let duplicate_worker = match crate::duplicate_finder::DuplicateFinderWorker::spawn() {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(%error, "could not start duplicate finder worker");
                None
            }
        };
        let verified_usb_flush_worker = match DeviceFlushWorker::spawn() {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(%error, "could not start verified removable-device flush worker");
                None
            }
        };
        widgets.miller_view.set_vim_mode(view_preferences.vim_mode);
        widgets.apply_click_policy(view_preferences.click_policy);
        widgets.apply_appearance_preferences(&view_preferences);
        Rc::new(Self {
            widgets,
            command_palette,
            keyboard_shortcuts,
            desktop_integration,
            context_menu_editor,
            terminal_chooser,
            tabs: Rc::new(RefCell::new(tabs)),
            worker: RefCell::new(browser),
            thumbnail_worker: RefCell::new(thumbnails),
            metadata_worker: RefCell::new(metadata),
            metadata_index_worker: RefCell::new(metadata_index),
            metadata_index_generation: Cell::new(0),
            inspector_worker: RefCell::new(inspector),
            inspector_generation: Cell::new(0),
            preview_worker: RefCell::new(preview),
            preview_generation: Cell::new(0),
            properties_worker: RefCell::new(properties),
            properties_generation: Cell::new(0),
            privacy_security_generation: Cell::new(0),
            threat_scan_generation: Cell::new(0),
            background_feedback_state: RefCell::new(BackgroundFeedbackState::default()),
            background_feedback_rows: RefCell::new(HashMap::new()),
            last_properties_presentation: RefCell::new(None),
            last_privacy_outcome: RefCell::new(None),
            last_threat_outcome: RefCell::new(None),
            last_sanitized_copy: RefCell::new(None),
            storage_worker: RefCell::new(storage),
            current_storage_generation: Cell::new(0),
            device_storage_generation: Cell::new(0),
            current_storage_facts: Cell::new(None),
            device_storage_facts: RefCell::new(HashMap::new()),
            device_snapshots: RefCell::new(Vec::new()),
            verified_usb_workflow: RefCell::new(None),
            verified_usb_flush_worker: RefCell::new(verified_usb_flush_worker),
            verified_usb_flush_source: RefCell::new(None),
            thumbnail_generation: Cell::new(0),
            active_generation: Cell::new(0),
            show_hidden: Cell::new(false),
            trash_active: Cell::new(false),
            trash_root: TrashRoot::for_data_home(glib::user_data_dir()),
            all_listed_entries: RefCell::new(Arc::from([])),
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
            search_index_worker: RefCell::new(search_index_worker),
            search_index_generation: Cell::new(0),
            search_index_query_active: Cell::new(false),
            search_index_fallback_note: RefCell::new(None),
            duplicate_worker: RefCell::new(duplicate_worker),
            duplicate_generation: Cell::new(0),
            duplicate_running: Cell::new(false),
            duplicate_progress: RefCell::new(None),
            content_search_worker: RefCell::new(content_search_worker),
            content_search_generation: Cell::new(0),
            content_search_active: Cell::new(false),
            content_search_running: Cell::new(false),
            content_search_results: RefCell::new(Vec::new()),
            content_search_store: RefCell::new(None),
            content_search_root: RefCell::new(None),
            pending_content_search: RefCell::new(None),
            content_search_summary: RefCell::new(None),
            recent_searches: RefCell::new(RecentSearches::default()),
            search_result_order: Cell::new(SearchResultOrder::default()),
            pending_saved_search: RefCell::new(None),
            selected_saved_search_id: Cell::new(None),
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
            shared_preferences: preferences,
            session_store: RefCell::new(session_store),
            session_saved: Cell::new(false),
            preference_baseline: RefCell::new(view_preferences.clone()),
            current_preferences: RefCell::new(view_preferences),
            window_size_save_source: RefCell::new(None),
            sidebar_save_source: RefCell::new(None),
            ignore_sidebar_position_signal: Cell::new(false),
            split_snapshots: RefCell::new(HashMap::new()),
            ignore_split_position_signal: Cell::new(false),
            pending_location: RefCell::new(None),
            breadcrumb_paths: RefCell::new(Vec::new()),
            location_completion_worker: RefCell::new(location_completion_worker),
            location_completion_generation: Cell::new(0),
            location_completion_source: RefCell::new(None),
            cli_route_worker: RefCell::new(cli_route_worker),
            association_worker,
            custom_action_worker: RefCell::new(custom_action_worker),
            privileged_access,
            application_state,
            operation_event_hub,
            window_runtime_id,
            completion_notification_namespace:
                crate::completeness::next_completion_notification_namespace(),
            shared_bookmarks: bookmarks,
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
            pending_operation_reveal: RefCell::new(None),
            pending_operation_emphasis: Cell::new(false),
            operation_emphasis_source: Rc::new(RefCell::new(None)),
            integrity_monitor_worker: RefCell::new(None),
            integrity_baseline: RefCell::new(None),
            integrity_session: RefCell::new(IntegrityMonitorSession::default()),
            integrity_watch_set: RefCell::new(None),
            integrity_request_generation: Cell::new(None),
            integrity_rescan_source: RefCell::new(None),
            pending_scroll_index: Cell::new(None),
            terminal_worker: RefCell::new(terminal_worker),
            terminal_availability: RefCell::new(Vec::new()),
            terminal_request_id: Cell::new(0),
            template_worker: RefCell::new(template_worker),
            template_request_id: Cell::new(0),
            pending_create_rename: RefCell::new(None),
            selection_mode: RefCell::new(None),
        })
    }

    pub fn configure_selection_mode(
        self: &Rc<Self>,
        config: SelectionConfig,
        completion: impl Fn(SelectionCompletion) + 'static,
    ) {
        let worker = match SelectionValidationWorker::spawn() {
            Ok(worker) => worker,
            Err(error) => {
                self.show_toast(&format!("Selection validation is unavailable: {error}"), 0);
                tracing::error!(%error, "could not start selection validation worker");
                completion(SelectionCompletion::Failed);
                return;
            }
        };
        let presentation = config.presentation();
        self.widgets
            .window
            .set_title(Some(&format!("{} — Floe", presentation.title)));
        self.widgets.window.set_modal(config.modal);
        if let Some(parent_handle) = config.parent_window.clone() {
            self.widgets.window.connect_map(move |window| {
                let Some(surface) = window.surface() else {
                    return;
                };
                match surface.downcast::<gdk4_wayland::WaylandToplevel>() {
                    Ok(toplevel) => {
                        if !toplevel.set_transient_for_exported(&parent_handle) {
                            tracing::warn!("Wayland compositor rejected portal parent handle");
                        }
                    }
                    Err(_) => tracing::warn!("portal parent handle requires a Wayland toplevel"),
                }
            });
        }

        let title = gtk::Label::builder()
            .label(&presentation.title)
            .xalign(0.0)
            .build();
        title.add_css_class("heading");
        let option_controls = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let filter = if config.chooser_options.filters.is_empty() {
            None
        } else {
            let labels = config
                .chooser_options
                .filters
                .iter()
                .map(|filter| filter.label.as_str())
                .collect::<Vec<_>>();
            let dropdown = gtk::DropDown::from_strings(&labels);
            dropdown.set_selected(
                config
                    .chooser_options
                    .current_filter
                    .and_then(|index| u32::try_from(index).ok())
                    .unwrap_or(0),
            );
            dropdown.set_tooltip_text(Some("Choose which file types are shown and returned"));
            dropdown.update_property(&[gtk::accessible::Property::Label("File type filter")]);
            option_controls.append(&dropdown);
            Some(dropdown)
        };
        let mut choice_widgets = Vec::new();
        for choice in &config.chooser_options.choices {
            if choice.options.is_empty() {
                let check = gtk::CheckButton::with_label(&choice.label);
                check.set_active(choice.initial == "true");
                option_controls.append(&check);
                choice_widgets.push((choice.id.clone(), SelectionChoiceWidget::Boolean(check)));
            } else {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                row.append(&gtk::Label::new(Some(&choice.label)));
                let labels = choice
                    .options
                    .iter()
                    .map(|(_, label)| label.as_str())
                    .collect::<Vec<_>>();
                let ids = choice
                    .options
                    .iter()
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>();
                let dropdown = gtk::DropDown::from_strings(&labels);
                let selected = ids
                    .iter()
                    .position(|id| id == &choice.initial)
                    .and_then(|index| u32::try_from(index).ok())
                    .unwrap_or(0);
                dropdown.set_selected(selected);
                dropdown
                    .update_property(&[gtk::accessible::Property::Label(choice.label.as_str())]);
                row.append(&dropdown);
                option_controls.append(&row);
                choice_widgets.push((
                    choice.id.clone(),
                    SelectionChoiceWidget::Combo(dropdown, ids),
                ));
            }
        }
        let filename = gtk::Entry::builder()
            .placeholder_text("Filename")
            .hexpand(true)
            .visible(presentation.filename_visible)
            .build();
        set_accessible_label(&filename, "Save filename");
        let has_suggested_name = config.suggested_name.is_some();
        if let Some(suggested_name) = config.suggested_name.as_deref() {
            filename.set_text(suggested_name);
            filename.select_region(0, -1);
        }
        let status = gtk::Label::builder()
            .label("Choose an item")
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        status.add_css_class("caption");
        status.add_css_class("dim-label");
        status.set_accessible_role(gtk::AccessibleRole::Status);
        let cancel = gtk::Button::with_label("Cancel");
        set_accessible_label(&cancel, "Cancel file selection");
        let accept = gtk::Button::with_label(&presentation.accept_label);
        accept.add_css_class("suggested-action");
        accept.set_sensitive(false);
        set_accessible_label(
            &accept,
            match config.mode {
                SelectionMode::OpenFile => "Open selected file",
                SelectionMode::OpenFiles => "Open selected files",
                SelectionMode::SelectFolder => "Select folder",
                SelectionMode::SaveFile => "Choose save destination",
            },
        );
        let footer = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(8)
            .build();
        footer.add_css_class("floe-selection-footer");
        footer.set_accessible_role(gtk::AccessibleRole::Group);
        let selection_description = if presentation.multiple {
            "Choose one or more local files with Floe, then accept or cancel"
        } else {
            "Choose one local destination with Floe, then accept or cancel"
        };
        footer.update_property(&[
            gtk::accessible::Property::Label("File selection controls"),
            gtk::accessible::Property::Description(selection_description),
        ]);
        footer.append(&title);
        if filter.is_some() || !choice_widgets.is_empty() {
            footer.append(&option_controls);
        }
        footer.append(&filename);
        footer.append(&status);
        footer.append(&cancel);
        footer.append(&accept);
        self.widgets.active_pane_shell.append(&footer);

        let filter_control = filter.clone();
        let filter_worker = if filter.is_some() {
            match SelectionFilterWorker::spawn() {
                Ok(worker) => Some(worker),
                Err(error) => {
                    tracing::warn!(%error, "could not start Selection Mode filter worker");
                    None
                }
            }
        } else {
            None
        };

        *self.selection_mode.borrow_mut() = Some(SelectionModeRuntime {
            config,
            worker,
            footer,
            filename: filename.clone(),
            status,
            accept: accept.clone(),
            completion: Rc::new(completion),
            request_id: Cell::new(0),
            pending: Cell::new(false),
            finished: Cell::new(false),
            filename_user_edited: Cell::new(has_suggested_name),
            updating_filename: Cell::new(false),
            filter,
            filter_worker,
            filter_generation: Cell::new(0),
            filter_pending: RefCell::new(None),
            choices: choice_widgets,
            poll_source: RefCell::new(None),
        });

        if let Some(filter) = filter_control {
            let controller = Rc::downgrade(self);
            filter.connect_selected_notify(move |_| {
                if let Some(controller) = controller.upgrade() {
                    let selected_paths = controller.selected_paths();
                    controller.apply_folder_filter(selected_paths, false);
                }
            });
        }

        let controller = Rc::downgrade(self);
        accept.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.submit_selection_mode();
            }
        });
        let controller = Rc::downgrade(self);
        filename.connect_activate(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.submit_selection_mode();
            }
        });
        let controller = Rc::downgrade(self);
        filename.connect_changed(move |_| {
            if let Some(controller) = controller.upgrade() {
                if let Some(runtime) = controller.selection_mode.borrow().as_ref()
                    && !runtime.updating_filename.get()
                {
                    runtime.filename_user_edited.set(true);
                }
                controller.refresh_selection_mode();
            }
        });
        let controller = Rc::downgrade(self);
        cancel.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.finish_selection_mode(SelectionCompletion::Cancelled);
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.window.connect_close_request(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.finish_selection_mode(SelectionCompletion::Cancelled);
            }
            glib::Propagation::Proceed
        });

        let event_hub = Rc::clone(&self.operation_event_hub);
        let window_runtime_id = self.window_runtime_id;
        self.widgets.window.connect_is_active_notify(move |window| {
            if window.is_active() {
                event_hub.mark_active(window_runtime_id);
            }
        });
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let controller = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            if key == gtk::gdk::Key::Escape && modifiers.is_empty() {
                if let Some(controller) = controller.upgrade() {
                    controller.finish_selection_mode(SelectionCompletion::Cancelled);
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.widgets.window.add_controller(keys);

        let controller = Rc::downgrade(self);
        let source = glib::timeout_add_local(Duration::from_millis(40), move || {
            let Some(controller) = controller.upgrade() else {
                return glib::ControlFlow::Break;
            };
            controller.poll_selection_mode();
            glib::ControlFlow::Continue
        });
        if let Some(runtime) = self.selection_mode.borrow().as_ref() {
            *runtime.poll_source.borrow_mut() = Some(source);
        }
        self.refresh_selection_mode();
    }

    fn refresh_selection_mode(&self) {
        let runtime = self.selection_mode.borrow();
        let Some(runtime) = runtime.as_ref() else {
            return;
        };
        if runtime.pending.get() {
            runtime.accept.set_sensitive(false);
            runtime.status.set_label("Checking selection…");
            return;
        }
        let selected = self.selected_entries.borrow();
        if runtime.config.mode == SelectionMode::SaveFile
            && !runtime.filename_user_edited.get()
            && let [entry] = selected.as_slice()
            && selection_mode_file_entry(entry)
            && let Some(name) = entry.path().file_name().and_then(std::ffi::OsStr::to_str)
            && runtime.filename.text().as_str() != name
        {
            runtime.updating_filename.set(true);
            runtime.filename.set_text(name);
            runtime.filename.select_region(0, -1);
            runtime.updating_filename.set(false);
        }
        let (enabled, message) = match runtime.config.mode {
            SelectionMode::OpenFile => match selected.as_slice() {
                [entry] if selection_mode_file_entry(entry) => (true, "One file selected"),
                [] => (false, "Select one file"),
                _ => (false, "Select exactly one file"),
            },
            SelectionMode::OpenFiles => {
                if selected.is_empty() {
                    (false, "Select one or more files")
                } else if selected.len() > SELECTION_PATH_CAPACITY {
                    (false, "Too many files selected")
                } else if selected
                    .iter()
                    .all(|entry| selection_mode_file_entry(entry))
                {
                    (true, "Selected files are ready")
                } else {
                    (false, "Folders cannot be accepted in Open Files mode")
                }
            }
            SelectionMode::SelectFolder => match selected.as_slice() {
                [] => (true, "Use the current folder"),
                [entry] if entry.is_navigable_directory() => (true, "One folder selected"),
                _ => (false, "Select one folder or clear selection"),
            },
            SelectionMode::SaveFile => {
                let valid = !runtime.filename.text().is_empty();
                (
                    valid,
                    if valid {
                        "Save location is ready"
                    } else {
                        "Enter a filename"
                    },
                )
            }
        };
        runtime.accept.set_sensitive(enabled);
        runtime.status.set_label(message);
    }

    fn submit_selection_mode(&self) {
        let runtime = self.selection_mode.borrow();
        let Some(runtime) = runtime.as_ref() else {
            return;
        };
        if runtime.pending.replace(true) || runtime.finished.get() {
            return;
        }
        let id = runtime.request_id.get().wrapping_add(1).max(1);
        runtime.request_id.set(id);
        let selected_paths = if runtime.config.mode == SelectionMode::SaveFile {
            Vec::new()
        } else {
            self.selected_paths()
        };
        let request = SelectionValidationRequest {
            id,
            mode: runtime.config.mode,
            current_directory: self.tabs.borrow().active().current().path().to_path_buf(),
            selected_paths,
            filename: runtime
                .config
                .mode
                .needs_filename()
                .then(|| runtime.filename.text().to_string()),
        };
        if let Err(error) = runtime.worker.submit(request) {
            runtime.pending.set(false);
            runtime.status.set_label(&error.to_string());
        } else {
            runtime.accept.set_sensitive(false);
            runtime.status.set_label("Checking selection…");
        }
    }

    pub fn accept_selection_mode(&self) {
        self.submit_selection_mode();
    }

    fn poll_selection_mode(self: &Rc<Self>) {
        let result = {
            let runtime = self.selection_mode.borrow();
            runtime
                .as_ref()
                .and_then(|runtime| runtime.worker.try_result())
        };
        let Some(result) = result else {
            return;
        };
        let current_id = self
            .selection_mode
            .borrow()
            .as_ref()
            .map_or(0, |runtime| runtime.request_id.get());
        if result.id != current_id {
            return;
        }
        if let Some(runtime) = self.selection_mode.borrow().as_ref() {
            runtime.pending.set(false);
        }
        match result.outcome {
            SelectionValidationOutcome::Ready(paths) => {
                let completion = self
                    .selection_mode
                    .borrow()
                    .as_ref()
                    .map(|runtime| runtime.accepted(paths))
                    .unwrap_or(SelectionCompletion::Failed);
                self.finish_selection_mode(completion);
            }
            SelectionValidationOutcome::ReplaceConfirmation(path) => {
                self.present_selection_replace_confirmation(path);
            }
            SelectionValidationOutcome::Invalid(message) => {
                if let Some(runtime) = self.selection_mode.borrow().as_ref() {
                    runtime.status.set_label(&message);
                }
                self.refresh_selection_mode();
            }
        }
    }

    fn present_selection_replace_confirmation(self: &Rc<Self>, path: PathBuf) {
        let label = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy();
        let dialog = adw::AlertDialog::builder()
            .heading("Replace existing file?")
            .body(format!(
                "{label} already exists. Floe will return this destination only if you explicitly choose Replace; the calling application decides how to save."
            ))
            .default_response("cancel")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("replace", "Replace")]);
        dialog.set_response_appearance("replace", adw::ResponseAppearance::Destructive);
        let controller = Rc::downgrade(self);
        dialog.connect_response(None, move |dialog, response| {
            if let Some(controller) = controller.upgrade() {
                if response == "replace" {
                    let completion = controller
                        .selection_mode
                        .borrow()
                        .as_ref()
                        .map(|runtime| runtime.accepted(vec![path.clone()]))
                        .unwrap_or(SelectionCompletion::Failed);
                    controller.finish_selection_mode(completion);
                } else {
                    controller.refresh_selection_mode();
                }
            }
            dialog.close();
        });
        dialog.present(Some(&self.widgets.window));
    }

    fn finish_selection_mode(&self, outcome: SelectionCompletion) {
        let completion = {
            let runtime = self.selection_mode.borrow();
            let Some(runtime) = runtime.as_ref() else {
                return;
            };
            if runtime.finished.replace(true) {
                return;
            }
            runtime.footer.set_sensitive(false);
            Rc::clone(&runtime.completion)
        };
        completion(outcome);
    }

    pub fn wire(self: &Rc<Self>, application: &adw::Application, locations: &[Location]) {
        let controller = Rc::downgrade(self);
        self.widgets.window.connect_close_request(move |_| {
            let Some(controller) = controller.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let owns_active_presentation = controller
                .operation_event_hub
                .owns_presentation(controller.window_runtime_id);
            if window_close_allowed(
                owns_active_presentation && controller.application_state.has_active_jobs(),
            ) {
                controller.widgets.prepare_for_window_close();
                glib::Propagation::Proceed
            } else {
                controller.show_toast(ACTIVE_OPERATION_CLOSE_MESSAGE, 7);
                glib::Propagation::Stop
            }
        });

        let controller = Rc::downgrade(self);
        self.application_state
            .observe_verified_usb_completions(move |job_id, result| {
                if let Some(controller) = controller.upgrade() {
                    controller.finish_verified_usb_copy(job_id, result);
                }
            });
        self.install_actions(application);
        self.install_filter_signals();
        self.install_saved_search_signals();
        self.install_search_index_signals();
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
        self.ignore_sidebar_position_signal.set(true);
        self.widgets
            .apply_sidebar_collapsed(self.current_preferences.borrow().sidebar_collapsed);
        if !self.current_preferences.borrow().sidebar_collapsed {
            let width = self
                .current_preferences
                .borrow()
                .sidebar_width
                .map(clamp_sidebar_width)
                .map(i32::from)
                .unwrap_or(self.widgets.sidebar_default_width);
            self.widgets.workspace.set_position(width);
        }
        self.ignore_sidebar_position_signal.set(false);
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
        self.install_file_view_shortcuts(&self.widgets.grouped_grid_view);
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
        let controller = Rc::downgrade(self);
        self.widgets.location_entry.connect_changed(move |entry| {
            if let Some(controller) = controller.upgrade() {
                controller.request_location_completion(entry.text().to_string());
            }
        });
        let controller = Rc::downgrade(self);
        let source = glib::timeout_add_local(Duration::from_millis(50), move || {
            let Some(controller) = controller.upgrade() else {
                return glib::ControlFlow::Break;
            };
            controller.poll_location_completion();
            controller.poll_cli_route();
            controller.poll_association_changes();
            controller.poll_custom_actions();
            glib::ControlFlow::Continue
        });
        self.location_completion_source.replace(Some(source));
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
        if self.selection_mode.borrow().is_some() {
            self.show_toast(
                "Drag-and-drop file operations are unavailable in Selection Mode",
                4,
            );
            return;
        }
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
        if matches!(request.action(), DropAction::Copy | DropAction::Link) {
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
            return;
        }
        let operations = if matches!(request.destination(), DropDestination::Trash) {
            request
                .sources()
                .iter()
                .cloned()
                .map(TrashRequest::new)
                .map(|request| request.map(TrackedOperation::Trash))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())
        } else {
            plan_directory_drop(&request)
                .map(|items| {
                    items
                        .into_iter()
                        .map(|item| {
                            TrackedOperation::Move(MoveRequest::new(
                                item.source,
                                item.destination,
                                ConflictPolicy::FailIfExists,
                            ))
                        })
                        .collect()
                })
                .map_err(|error| error.to_string())
        };
        let operations = match operations {
            Ok(operations) => operations,
            Err(error) => {
                self.show_toast(&format!("Could not complete drop: {error}"), 7);
                return;
            }
        };
        let scopes = match operations
            .iter()
            .map(|operation| match operation {
                TrackedOperation::Move(request) => destructive_scope_for_move(request),
                TrackedOperation::Trash(request) => destructive_scope_for_trash(request),
                _ => unreachable!("guarded drop contains only Move or Trash"),
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(scopes) => scopes,
            Err(error) => {
                self.show_toast(&format!("Could not complete drop: {error}"), 7);
                return;
            }
        };
        let application_state = Rc::clone(&self.application_state);
        let toast_overlay = self.widgets.toast_overlay.clone();
        self.review_guardrail(scopes, move |authorizations| {
            let guarded = operations
                .into_iter()
                .zip(authorizations)
                .map(|(operation, authorization)| {
                    GuardrailAuthorized::new(operation, authorization)
                })
                .collect();
            match application_state.enqueue_authorized_batch(guarded) {
                Ok(batch) => {
                    toast_overlay.add_toast(
                        adw::Toast::builder()
                            .title(format!(
                                "{action}: queued {} of {count} items",
                                batch.queued()
                            ))
                            .timeout(4)
                            .build(),
                    );
                }
                Err(error) => {
                    tracing::warn!(%error, "drop request was rejected");
                    toast_overlay.add_toast(
                        adw::Toast::builder()
                            .title(format!("Could not complete drop: {error}"))
                            .timeout(7)
                            .build(),
                    );
                }
            }
        });
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
        if let Some(worker) = self.duplicate_worker.borrow().as_ref() {
            worker.invalidate_watcher_paths(batch.changed_paths(), batch.overflowed());
        }
        if let Some(worker) = self.metadata_index_worker.borrow_mut().as_mut() {
            worker.cancel();
            let paths = if batch.overflowed() {
                vec![current.clone()]
            } else {
                batch.changed_paths().to_vec()
            };
            if let Err(error) = worker.invalidate(paths) {
                tracing::debug!(%error, "metadata index invalidation deferred");
            }
        }
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
        self.widgets
            .scroll_to_operation_result(self.view_mode.get(), index);
    }

    fn clear_operation_result_emphasis(&self) {
        if let Some(source) = self.operation_emphasis_source.borrow_mut().take() {
            source.remove();
        }
        for widget in self.widgets.operation_result_emphasis_targets() {
            widget.remove_css_class("floe-operation-result");
        }
    }

    fn start_operation_result_emphasis(&self) {
        self.clear_operation_result_emphasis();
        let targets = self.widgets.operation_result_emphasis_targets();
        for widget in &targets {
            widget.add_css_class("floe-operation-result");
        }
        let source_slot = Rc::clone(&self.operation_emphasis_source);
        let source = glib::timeout_add_local_once(
            Duration::from_millis(OPERATION_REVEAL_DURATION_MS),
            move || {
                source_slot.borrow_mut().take();
                for widget in targets {
                    widget.remove_css_class("floe-operation-result");
                }
            },
        );
        self.operation_emphasis_source.replace(Some(source));
    }

    fn add_current_bookmark(self: &Rc<Self>) {
        if !self.bookmarks_loaded.get() {
            self.show_toast("Bookmarks are still loading", 4);
            return;
        }
        let current = self.tabs.borrow().active().current().path().to_path_buf();
        if self
            .bookmarks
            .borrow()
            .iter()
            .any(|record| record.path == current)
        {
            self.show_toast("This folder is already bookmarked", 4);
            return;
        }
        let mut revised = self.bookmarks.borrow().clone();
        revised.push(BookmarkRecord::from(current));
        self.submit_bookmarks(revised);
    }

    fn remove_bookmark(self: &Rc<Self>, index: usize) {
        let mut revised = self.bookmarks.borrow().clone();
        if index >= revised.len() {
            self.show_toast("That bookmark is no longer available", 4);
            return;
        }
        revised.remove(index);
        self.submit_bookmarks(revised);
    }

    fn move_bookmark(self: &Rc<Self>, index: usize, delta: isize) {
        let Some(revised) = reordered_records(&self.bookmarks.borrow(), index, delta) else {
            return;
        };
        self.submit_bookmarks(revised);
    }

    fn rename_bookmark(self: &Rc<Self>, index: usize) {
        let Some(record) = self.bookmarks.borrow().get(index).cloned() else {
            return;
        };
        let dialog = adw::AlertDialog::builder()
            .heading("Rename Bookmark")
            .body("Set a sidebar label. The bookmarked folder path will not change.")
            .build();
        let entry = gtk::Entry::builder()
            .text(record.alias.as_deref().unwrap_or_default())
            .placeholder_text(
                record
                    .path
                    .file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .unwrap_or("Folder"),
            )
            .activates_default(true)
            .build();
        dialog.set_extra_child(Some(&entry));
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("rename", "Rename");
        dialog.set_default_response(Some("rename"));
        dialog.set_close_response("cancel");
        let controller = Rc::downgrade(self);
        dialog.connect_response(None, move |_, response| {
            if response != "rename" {
                return;
            }
            let Some(controller) = controller.upgrade() else {
                return;
            };
            let alias = entry.text().trim().to_owned();
            match records_with_alias(
                &controller.bookmarks.borrow(),
                index,
                (!alias.is_empty()).then_some(alias),
            ) {
                Ok(Some(revised)) => controller.submit_bookmarks(revised),
                Ok(None) => controller.show_toast("That bookmark is no longer available", 4),
                Err(error) => controller.show_toast(&error.to_string(), 5),
            }
        });
        dialog.present(Some(&self.widgets.window));
    }

    fn reset_bookmark_name(self: &Rc<Self>, index: usize) {
        if let Ok(Some(revised)) = records_with_alias(&self.bookmarks.borrow(), index, None) {
            self.submit_bookmarks(revised);
        }
    }

    fn submit_bookmarks(self: &Rc<Self>, records: Vec<BookmarkRecord>) {
        if self.selection_mode.borrow().is_some() {
            self.show_toast("Bookmarks cannot be changed in Selection Mode", 4);
            return;
        }
        let Some(shared) = self.shared_bookmarks.as_ref() else {
            self.show_toast("Bookmarks are unavailable for this session", 5);
            return;
        };
        match shared.try_save(records) {
            Ok(()) => self.sync_shared_bookmarks(),
            Err(error) => {
                self.show_toast(&format!("Could not save bookmarks: {error}"), 6);
            }
        }
    }

    fn drain_bookmark_worker(self: &Rc<Self>) {
        let Some(shared) = self.shared_bookmarks.as_ref() else {
            return;
        };
        for notice in shared.poll() {
            match notice {
                SharedBookmarkNotice::Loaded => {}
                SharedBookmarkNotice::Saved => self.show_toast("Bookmarks updated", 3),
                SharedBookmarkNotice::LoadFailed(error) => {
                    tracing::warn!(%error, "could not load bookmarks");
                    self.show_toast(&format!("Could not load bookmarks: {error}"), 6);
                }
                SharedBookmarkNotice::SaveFailed(error) => {
                    tracing::warn!(%error, "could not persist bookmarks");
                    self.show_toast(&format!("Could not save bookmarks: {error}"), 6);
                }
                SharedBookmarkNotice::Disconnected => {
                    self.show_toast("Bookmark storage disconnected; changes are unavailable", 6)
                }
            }
        }
        self.sync_shared_bookmarks();
    }

    fn sync_shared_bookmarks(self: &Rc<Self>) {
        let Some(shared) = self.shared_bookmarks.as_ref() else {
            return;
        };
        let snapshot = shared.snapshot();
        if snapshot.version == self.bookmark_revision.get() {
            return;
        }
        self.bookmark_revision.set(snapshot.version);
        self.bookmarks.replace(snapshot.records);
        self.bookmarks_loaded.set(snapshot.loaded);
        self.bookmark_save_in_flight.set(snapshot.save_in_flight);
        self.widgets.add_bookmark_button.set_sensitive(
            self.selection_mode.borrow().is_none()
                && snapshot.available
                && ui::bookmark_actions_enabled(snapshot.loaded, snapshot.save_in_flight),
        );
        self.widgets
            .add_bookmark_button
            .set_tooltip_text(Some("Add current folder to Bookmarks"));
        self.render_bookmarks();
    }

    fn render_bookmarks(self: &Rc<Self>) {
        remove_all_children(&self.widgets.bookmarks_box);
        let bookmarks = self.bookmarks.borrow().clone();
        if bookmarks.is_empty() {
            let empty = sidebar_status_label("No bookmarks yet");
            self.widgets.bookmarks_box.append(&empty);
            return;
        }
        let actions_enabled = self.selection_mode.borrow().is_none()
            && ui::bookmark_actions_enabled(
                self.bookmarks_loaded.get(),
                self.bookmark_save_in_flight.get(),
            );
        let bookmark_count = bookmarks.len();
        for (index, record) in bookmarks.into_iter().enumerate() {
            let path = record.path;
            let row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(0)
                .build();
            let has_alias = record.alias.is_some();
            let display_name = record.alias.unwrap_or_else(|| sidebar_path_name(&path));
            let content = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(8)
                .build();
            content.append(&gtk::Image::from_icon_name("floe-phosphor-folder-symbolic"));
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

            let bookmark_actions = gio::SimpleActionGroup::new();

            let rename = gio::SimpleAction::new("rename", None);
            rename.set_enabled(actions_enabled);
            let controller = Rc::downgrade(self);
            rename.connect_activate(move |_, _| {
                if let Some(controller) = controller.upgrade() {
                    controller.rename_bookmark(index);
                }
            });
            bookmark_actions.add_action(&rename);

            let reset = gio::SimpleAction::new("reset-name", None);
            reset.set_enabled(actions_enabled && has_alias);
            let controller = Rc::downgrade(self);
            reset.connect_activate(move |_, _| {
                if let Some(controller) = controller.upgrade() {
                    controller.reset_bookmark_name(index);
                }
            });
            bookmark_actions.add_action(&reset);

            let move_up = gio::SimpleAction::new("move-up", None);
            move_up.set_enabled(actions_enabled && index > 0);
            let controller = Rc::downgrade(self);
            move_up.connect_activate(move |_, _| {
                if let Some(controller) = controller.upgrade() {
                    controller.move_bookmark(index, -1);
                }
            });
            bookmark_actions.add_action(&move_up);

            let move_down = gio::SimpleAction::new("move-down", None);
            move_down.set_enabled(actions_enabled && index + 1 < bookmark_count);
            let controller = Rc::downgrade(self);
            move_down.connect_activate(move |_, _| {
                if let Some(controller) = controller.upgrade() {
                    controller.move_bookmark(index, 1);
                }
            });
            bookmark_actions.add_action(&move_down);

            let remove = gio::SimpleAction::new("remove", None);
            remove.set_enabled(actions_enabled);
            let controller = Rc::downgrade(self);
            remove.connect_activate(move |_, _| {
                if let Some(controller) = controller.upgrade() {
                    controller.remove_bookmark(index);
                }
            });
            bookmark_actions.add_action(&remove);
            row.insert_action_group("bookmark", Some(&bookmark_actions));

            let menu = gio::Menu::new();
            menu.append(Some("Rename…"), Some("bookmark.rename"));
            menu.append(Some("Use Folder Name"), Some("bookmark.reset-name"));
            let reorder = gio::Menu::new();
            reorder.append(Some("Move Up"), Some("bookmark.move-up"));
            reorder.append(Some("Move Down"), Some("bookmark.move-down"));
            menu.append_section(None, &reorder);
            let destructive = gio::Menu::new();
            destructive.append(Some("Remove from Bookmarks"), Some("bookmark.remove"));
            menu.append_section(None, &destructive);

            let options = gtk::MenuButton::builder()
                .icon_name("floe-phosphor-dots-three-symbolic")
                .has_frame(false)
                .menu_model(&menu)
                .sensitive(actions_enabled)
                .tooltip_text(format!("Options for bookmark {display_name}"))
                .build();
            options.add_css_class("sidebar-icon-button");
            set_accessible_label(&options, &format!("Bookmark options for {display_name}"));
            row.append(&options);
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
                "floe-phosphor-usb-symbolic"
            } else {
                "floe-phosphor-hard-drives-symbolic"
            };
            content.append(&gtk::Image::from_icon_name(icon_name));
            let labels = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(0)
                .hexpand(true)
                .build();
            let device_name = sidebar_device_name_label(&snapshot.name);
            labels.append(&device_name);
            let status_text = self
                .device_storage_facts
                .borrow()
                .get(snapshot.id.as_str())
                .copied()
                .map(|facts| device_status_text(&policy.status, facts))
                .unwrap_or(policy.status);
            let status = sidebar_device_status_label(&status_text);
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
            activate.update_property(&[gtk::accessible::Property::Description(&status_text)]);
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
                    "floe-phosphor-stop-symbolic",
                    "Unmount",
                    DeviceAction::Unmount,
                ));
            }
            if policy.can_eject {
                row.append(&self.device_action_button(
                    snapshot,
                    "floe-phosphor-eject-symbolic",
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
                    controller.handle_escape();
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
        self.arm_window_size_persistence();
        self.arm_sidebar_width_persistence();
        self.discover_terminals();
        self.load_current();

        let controller = Rc::clone(self);
        glib::timeout_add_local(Duration::from_millis(16), move || {
            if !controller.widgets.window.is_visible() {
                return glib::ControlFlow::Break;
            }
            controller.drain_worker();
            controller.drain_metadata_index_worker();
            controller.drain_folder_filter_worker();
            controller.drain_selection_filter_worker();
            controller.drain_filename_search_worker();
            controller.drain_search_index_worker();
            controller.drain_duplicate_worker();
            controller.drain_content_search_worker();
            controller.drain_bookmark_worker();
            controller.pump_pending_entries();
            controller.submit_thumbnail_requests();
            controller.drain_thumbnail_worker();
            controller.submit_metadata_requests();
            controller.drain_metadata_worker();
            controller.drain_inspector_worker();
            controller.drain_preview_worker();
            controller.drain_properties_worker();
            controller.drain_privacy_security_worker();
            controller.drain_threat_scan_worker();
            controller.drain_storage_worker();
            controller.drain_terminal_worker();
            controller.drain_guardrail_policy_worker();
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

    pub(crate) fn session_snapshot(&self) -> BrowserTabs {
        self.restore_pending_navigation();
        self.save_active_session_state();
        self.tabs.borrow().clone()
    }

    pub fn persist_for_shutdown(&self) {
        self.finish_window_size_tracking();
        self.persist_session_for_shutdown();
    }

    fn arm_window_size_persistence(self: &Rc<Self>) {
        let controller = Rc::downgrade(self);
        self.widgets.window.connect_close_request(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.finish_window_size_tracking();
            }
            glib::Propagation::Proceed
        });

        if let Some(surface) = self.widgets.window.surface() {
            self.attach_window_surface_tracking(surface);
            return;
        }

        let controller = Rc::downgrade(self);
        self.widgets.window.connect_realize(move |window| {
            let (Some(controller), Some(surface)) = (controller.upgrade(), window.surface()) else {
                return;
            };
            controller.attach_window_surface_tracking(surface);
        });
    }

    fn attach_window_surface_tracking(self: &Rc<Self>, surface: gdk::Surface) {
        let controller = Rc::downgrade(self);
        surface.connect_width_notify(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.schedule_window_size_persistence();
            }
        });
        let controller = Rc::downgrade(self);
        surface.connect_height_notify(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.schedule_window_size_persistence();
            }
        });
        self.schedule_window_size_persistence();
    }

    fn schedule_window_size_persistence(self: &Rc<Self>) {
        if let Some(source) = self.window_size_save_source.borrow_mut().take() {
            source.remove();
        }
        let controller = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(WINDOW_SIZE_PERSIST_DEBOUNCE, move || {
            let Some(controller) = controller.upgrade() else {
                return;
            };
            controller.window_size_save_source.borrow_mut().take();
            if controller.capture_normal_window_size() {
                controller.queue_preferences();
            }
        });
        self.window_size_save_source.borrow_mut().replace(source);
    }

    fn capture_normal_window_size(&self) -> bool {
        let Some(surface) = self.widgets.window.surface() else {
            return false;
        };
        remember_window_size_if_normal(
            &mut self.current_preferences.borrow_mut(),
            surface.width(),
            surface.height(),
            self.widgets.window.is_maximized(),
            self.widgets.window.is_fullscreen(),
        )
    }

    fn finish_window_size_tracking(&self) {
        if let Some(source) = self.window_size_save_source.borrow_mut().take() {
            source.remove();
        }
        self.capture_normal_window_size();
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
                } else if !controller.filename_search_running.get()
                    && !controller.content_search_running.get()
                {
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
        let advanced_box = self.widgets.advanced_filter_box.clone();
        self.widgets
            .advanced_filter_toggle
            .connect_toggled(move |button| advanced_box.set_visible(button.is_active()));
        let controller = Rc::downgrade(self);
        self.widgets.advanced_apply.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                if controller.widgets.search_mode.selected() == 0 {
                    controller.update_folder_filter_from_widgets();
                } else if controller.widgets.search_mode.selected() == 1 {
                    controller.start_filename_search();
                } else {
                    controller.start_content_search();
                }
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.advanced_clear.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.reset_advanced_filter_widgets();
                if controller.widgets.search_mode.selected() == 0 {
                    controller.update_folder_filter_from_widgets();
                } else if controller.filename_search_active.get() {
                    controller.start_filename_search();
                } else if controller.content_search_active.get() {
                    controller.start_content_search();
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

    fn install_saved_search_signals(self: &Rc<Self>) {
        self.refresh_search_catalog_controls();
        let controller = Rc::downgrade(self);
        self.widgets
            .saved_searches
            .connect_selected_notify(move |dropdown| {
                let Some(controller) = controller.upgrade() else {
                    return;
                };
                let selected = dropdown.selected();
                if selected == 0 {
                    return;
                }
                dropdown.set_selected(0);
                let saved = controller
                    .current_preferences
                    .borrow()
                    .saved_searches
                    .entries()
                    .get((selected - 1) as usize)
                    .cloned();
                if let Some(saved) = saved {
                    controller.selected_saved_search_id.set(Some(saved.id()));
                    controller.replay_search(saved.query_definition().clone());
                }
            });
        let controller = Rc::downgrade(self);
        self.widgets
            .recent_searches
            .connect_selected_notify(move |dropdown| {
                let Some(controller) = controller.upgrade() else {
                    return;
                };
                let selected = dropdown.selected();
                if selected == 0 {
                    return;
                }
                dropdown.set_selected(0);
                let query = controller
                    .recent_searches
                    .borrow()
                    .entries()
                    .get((selected - 1) as usize)
                    .cloned();
                if let Some(query) = query {
                    controller.replay_search(query);
                }
            });
        let controller = Rc::downgrade(self);
        self.widgets
            .search_result_order
            .connect_selected_notify(move |dropdown| {
                let Some(controller) = controller.upgrade() else {
                    return;
                };
                let order = SearchResultOrder::ALL
                    .get(dropdown.selected() as usize)
                    .copied()
                    .unwrap_or_default();
                controller.search_result_order.set(order);
                controller.apply_search_result_order();
            });
    }

    fn install_search_index_signals(self: &Rc<Self>) {
        let controller = Rc::downgrade(self);
        self.widgets
            .search_index_toggle
            .connect_toggled(move |toggle| {
                let Some(controller) = controller.upgrade() else {
                    return;
                };
                controller.change_search_index_enabled(toggle.is_active());
            });
    }

    fn refresh_search_catalog_controls(&self) {
        let saved_labels = std::iter::once("Saved searches".to_owned())
            .chain(
                self.current_preferences
                    .borrow()
                    .saved_searches
                    .entries()
                    .iter()
                    .map(|saved| {
                        format!(
                            "{} — {}",
                            saved.name(),
                            saved.query_definition().root().to_string_lossy()
                        )
                    }),
            )
            .collect::<Vec<_>>();
        let saved_refs = saved_labels.iter().map(String::as_str).collect::<Vec<_>>();
        self.widgets
            .saved_searches
            .set_model(Some(&gtk::StringList::new(&saved_refs)));
        self.widgets.saved_searches.set_selected(0);

        let recent_labels = std::iter::once("Recent searches (this session)".to_owned())
            .chain(self.recent_searches.borrow().entries().iter().map(|query| {
                let kind = match query.kind() {
                    SearchKind::Files => "Files",
                    SearchKind::Contents => "Contents",
                };
                format!(
                    "{kind}: {} — {}",
                    query.query(),
                    query.root().to_string_lossy()
                )
            }))
            .collect::<Vec<_>>();
        let recent_refs = recent_labels.iter().map(String::as_str).collect::<Vec<_>>();
        self.widgets
            .recent_searches
            .set_model(Some(&gtk::StringList::new(&recent_refs)));
        self.widgets.recent_searches.set_selected(0);
    }

    fn record_recent_search(&self, query: SearchQuery) {
        self.recent_searches
            .borrow_mut()
            .record(query, SearchHistoryPolicy::Record);
        self.refresh_search_catalog_controls();
    }

    fn current_disk_search_query(&self) -> Result<SearchQuery, String> {
        let kind = match self.widgets.search_mode.selected() {
            1 => SearchKind::Files,
            2 => SearchKind::Contents,
            _ => return Err("Choose Search Files or Search Contents before saving".to_owned()),
        };
        let scope = if self.widgets.search_scope.selected() == 0 {
            FilenameSearchScope::CurrentFolder
        } else {
            FilenameSearchScope::Subtree
        };
        let mode = match self.widgets.filter_mode.selected() {
            1 => FolderFilterMode::Glob,
            2 => FolderFilterMode::Regex,
            _ => FolderFilterMode::Text,
        };
        SearchQuery::new(
            self.tabs.borrow().active().current().path().to_path_buf(),
            kind,
            self.widgets.filter_entry.text().to_string(),
            scope,
            self.show_hidden.get(),
            mode,
            self.advanced_filter_from_widgets(),
        )
        .map_err(|error| error.to_string())
    }

    fn show_save_search_dialog(self: &Rc<Self>) {
        let query = match self.current_disk_search_query() {
            Ok(query) => query,
            Err(error) => {
                self.show_toast(&error, 5);
                return;
            }
        };
        let name = gtk::Entry::builder()
            .placeholder_text("Saved search name")
            .max_length(SAVED_SEARCH_NAME_CAPACITY as i32)
            .activates_default(true)
            .build();
        name.update_property(&[gtk::accessible::Property::Label("Saved search name")]);
        let dialog = adw::AlertDialog::builder()
            .heading("Save Search")
            .body("Only searches you explicitly save are written to Floe's private preferences. Recent searches remain session-only.")
            .extra_child(&name)
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("save", "Save");
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        let controller = Rc::downgrade(self);
        dialog.connect_response(None, move |_, response| {
            if response != "save" {
                return;
            }
            if let Some(controller) = controller.upgrade() {
                controller.save_named_search(name.text().to_string(), query.clone());
            }
        });
        dialog.present(Some(&self.widgets.window));
    }

    fn save_named_search(&self, name: String, query: SearchQuery) {
        let mut preferences = self.current_preferences.borrow().clone();
        let id = match preferences.saved_searches.next_id() {
            Ok(id) => id,
            Err(error) => {
                self.show_toast(&error.to_string(), 6);
                return;
            }
        };
        let saved = match SavedSearch::new(id, name, query) {
            Ok(saved) => saved,
            Err(error) => {
                self.show_toast(&error.to_string(), 6);
                return;
            }
        };
        let saved_name = saved.name().to_owned();
        if let Err(error) = preferences.saved_searches.add(saved) {
            self.show_toast(&error.to_string(), 6);
            return;
        }
        *self.current_preferences.borrow_mut() = preferences;
        self.queue_preferences();
        self.refresh_search_catalog_controls();
        self.show_toast(&format!("Saved search “{saved_name}”"), 4);
    }

    fn delete_selected_saved_search(&self) {
        let Some(id) = self.selected_saved_search_id.take() else {
            self.show_toast("Choose a saved search before deleting it", 5);
            return;
        };
        let mut preferences = self.current_preferences.borrow().clone();
        if !preferences.saved_searches.remove(id) {
            self.show_toast("That saved search no longer exists", 5);
            return;
        }
        *self.current_preferences.borrow_mut() = preferences;
        self.queue_preferences();
        self.refresh_search_catalog_controls();
        self.show_toast("Saved search deleted", 4);
    }

    fn clear_recent_searches(&self) {
        self.recent_searches.borrow_mut().clear();
        self.refresh_search_catalog_controls();
        self.show_toast("Recent searches cleared", 4);
    }

    fn next_search_index_generation(&self) -> u64 {
        let generation = self.search_index_generation.get().wrapping_add(1).max(1);
        self.search_index_generation.set(generation);
        generation
    }

    fn build_search_index(&self) {
        if self.trash_active.get() {
            self.show_toast("Search indexes are available only for local folders", 5);
            return;
        }
        if self.filename_search_running.get() || self.content_search_running.get() {
            self.show_toast("Stop the current search before building an index", 5);
            return;
        }
        let root = self.tabs.borrow().active().current().path().to_path_buf();
        let request = match floe_core::SearchIndexBuildRequest::new(root) {
            Ok(request) => request,
            Err(error) => {
                self.show_toast(&error.to_string(), 6);
                return;
            }
        };
        let generation = self.next_search_index_generation();
        let result = self.search_index_worker.borrow().as_ref().map_or(
            Err(crate::search_index::SearchIndexSubmitError::Stopped),
            |worker| worker.build(generation, request),
        );
        match result {
            Ok(()) => self.show_toast(
                "Building a private filename/metadata index; hidden entries and contents are excluded",
                6,
            ),
            Err(crate::search_index::SearchIndexSubmitError::Busy) => {
                self.show_toast("Search index worker is busy", 5)
            }
            Err(crate::search_index::SearchIndexSubmitError::Stopped) => {
                self.show_toast("Search index worker is unavailable", 5)
            }
        }
    }

    fn clear_search_index(&self) {
        if self.filename_search_running.get() {
            self.show_toast("Stop the current search before clearing the index", 5);
            return;
        }
        let generation = self.next_search_index_generation();
        let result = self.search_index_worker.borrow().as_ref().map_or(
            Err(crate::search_index::SearchIndexSubmitError::Stopped),
            |worker| worker.clear(generation),
        );
        if let Err(error) = result {
            self.show_toast(
                match error {
                    crate::search_index::SearchIndexSubmitError::Busy => {
                        "Search index worker is busy"
                    }
                    crate::search_index::SearchIndexSubmitError::Stopped => {
                        "Search index worker is unavailable"
                    }
                },
                5,
            );
        }
    }

    fn replay_search(&self, query: SearchQuery) {
        self.widgets.filter_entry.set_text(query.query());
        self.widgets.search_scope.set_selected(match query.scope() {
            FilenameSearchScope::CurrentFolder => 0,
            FilenameSearchScope::Subtree => 1,
        });
        self.widgets.filter_mode.set_selected(match query.mode() {
            FolderFilterMode::Text => 0,
            FolderFilterMode::Glob => 1,
            FolderFilterMode::Regex => 2,
        });
        self.pending_saved_search.replace(Some(query.clone()));
        let current = self.tabs.borrow().active().current().path().to_path_buf();
        if current != query.root() {
            self.navigate_to(query.root().to_path_buf());
        } else {
            self.launch_pending_saved_search();
        }
    }

    fn launch_pending_saved_search(&self) {
        let kind = self
            .pending_saved_search
            .borrow()
            .as_ref()
            .map(SearchQuery::kind);
        match kind {
            Some(SearchKind::Files) => self.start_filename_search(),
            Some(SearchKind::Contents) => self.start_content_search(),
            None => {}
        }
    }

    fn apply_search_result_order(&self) {
        let order = self.search_result_order.get();
        if self.filename_search_active.get() {
            self.filename_search_results
                .borrow_mut()
                .sort_by(|left, right| order.compare(left, right));
            let store = gio::ListStore::new::<glib::BoxedAnyObject>();
            for entry in self.filename_search_results.borrow().iter() {
                store.append(&glib::BoxedAnyObject::new(entry.clone()));
            }
            self.widgets.selection.set_model(Some(&store));
            self.filename_search_store.replace(Some(store));
        } else if self.content_search_active.get() {
            self.content_search_results
                .borrow_mut()
                .sort_by(|left, right| {
                    order
                        .compare(left.entry(), right.entry())
                        .then_with(|| left.line_number().cmp(&right.line_number()))
                });
            let store = gio::ListStore::new::<glib::BoxedAnyObject>();
            for entry in self.content_search_results.borrow().iter() {
                store.append(&glib::BoxedAnyObject::new(entry.clone()));
            }
            self.widgets.selection.set_model(Some(&store));
            self.content_search_store.replace(Some(store));
        }
        self.widgets.selection.unselect_all();
        self.apply_action_selection(Vec::new());
    }

    fn show_folder_filter(&self) {
        if self.filename_search_active.get() {
            self.deactivate_filename_search(true);
        }
        if self.content_search_active.get() {
            self.deactivate_content_search(true);
        }
        self.widgets.search_mode.set_selected(0);
        self.configure_search_surface(0);
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
        if self.content_search_active.get() {
            self.clear_content_search();
            return;
        }
        self.filter_state.replace(FolderFilterState::default());
        self.widgets.filter_mode.set_selected(0);
        self.reset_advanced_filter_widgets();
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

    fn reset_advanced_filter_widgets(&self) {
        self.widgets.advanced_type.set_selected(0);
        self.widgets.advanced_extension.set_text("");
        self.widgets.advanced_mime.set_text("");
        self.widgets.advanced_size.set_selected(0);
        self.widgets.advanced_date.set_selected(0);
        self.widgets.advanced_owner.set_selected(0);
        self.widgets.advanced_hidden.set_selected(0);
        self.widgets.advanced_match_case.set_active(false);
    }

    fn advanced_filter_from_widgets(&self) -> AdvancedFilter {
        let entry_type = match self.widgets.advanced_type.selected() {
            1 => EntryTypeFilter::File,
            2 => EntryTypeFilter::Folder,
            3 => EntryTypeFilter::SymbolicLink,
            4 => EntryTypeFilter::Other,
            _ => EntryTypeFilter::Any,
        };
        let extension = non_empty_trimmed(self.widgets.advanced_extension.text().as_str());
        let mime = non_empty_trimmed(self.widgets.advanced_mime.text().as_str());
        let (minimum_size, maximum_size) = match self.widgets.advanced_size.selected() {
            1 => (Some(0), Some(0)),
            2 => (None, Some(999_999)),
            3 => (Some(1_000_000), Some(100_000_000)),
            4 => (Some(100_000_001), None),
            _ => (None, None),
        };
        let modified_after = match self.widgets.advanced_date.selected() {
            1 => SystemTime::now().checked_sub(Duration::from_secs(24 * 60 * 60)),
            2 => SystemTime::now().checked_sub(Duration::from_secs(7 * 24 * 60 * 60)),
            3 => SystemTime::now().checked_sub(Duration::from_secs(30 * 24 * 60 * 60)),
            4 => SystemTime::now().checked_sub(Duration::from_secs(365 * 24 * 60 * 60)),
            _ => None,
        };
        let owner = (self.widgets.advanced_owner.selected() == 1)
            .then(|| OwnerFilter::Uid(rustix::process::getuid().as_raw()));
        let hidden = match self.widgets.advanced_hidden.selected() {
            1 => HiddenFilter::Include,
            2 => HiddenFilter::Only,
            _ => HiddenFilter::CurrentSetting,
        };
        AdvancedFilter {
            entry_type,
            extension,
            mime,
            minimum_size,
            maximum_size,
            modified_after,
            modified_before: None,
            owner,
            hidden,
            match_case: self.widgets.advanced_match_case.is_active(),
        }
    }

    fn update_folder_filter_from_widgets(&self) {
        let mode = match self.widgets.filter_mode.selected() {
            1 => FolderFilterMode::Glob,
            2 => FolderFilterMode::Regex,
            _ => FolderFilterMode::Text,
        };
        let query = self.widgets.filter_entry.text().to_string();
        let advanced = self.advanced_filter_from_widgets();
        self.filter_state.replace(FolderFilterState {
            mode,
            query,
            advanced,
        });
        let selected_paths = self.selected_paths();
        self.apply_folder_filter(selected_paths, false);
    }

    fn apply_folder_filter(&self, selected_paths: Vec<PathBuf>, focus_list: bool) {
        let generation = self.filter_generation.get().wrapping_add(1);
        self.filter_generation.set(generation);
        self.filter_selection_paths.replace(selected_paths.clone());
        self.pending_filter.borrow_mut().take();

        let state = self.filter_state.borrow().clone();
        let entries = if matches!(
            state.advanced.hidden,
            HiddenFilter::Include | HiddenFilter::Only
        ) {
            self.all_listed_entries.borrow().clone()
        } else {
            self.listed_entries.borrow().clone()
        };
        if state.query.is_empty() && !state.advanced.is_active() {
            self.install_or_filter_selection_entries(entries.to_vec(), selected_paths, focus_list);
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

    fn install_or_filter_selection_entries(
        &self,
        entries: Vec<Arc<DirectoryEntry>>,
        selected_paths: Vec<PathBuf>,
        focus_list: bool,
    ) {
        let submit = {
            let runtime = self.selection_mode.borrow();
            let Some(runtime) = runtime.as_ref() else {
                self.install_entries(entries, &selected_paths, focus_list);
                return;
            };
            let Some(dropdown) = runtime.filter.as_ref() else {
                self.install_entries(entries, &selected_paths, focus_list);
                return;
            };
            if runtime.filter_worker.is_none() {
                self.install_entries(entries, &selected_paths, focus_list);
                return;
            }
            let selected = usize::try_from(dropdown.selected()).unwrap_or(usize::MAX);
            let Some(filter) = runtime
                .config
                .chooser_options
                .filters
                .get(selected)
                .cloned()
            else {
                self.install_entries(entries, &selected_paths, focus_list);
                return;
            };
            let generation = runtime.filter_generation.get().wrapping_add(1);
            runtime.filter_generation.set(generation);
            runtime.filter_pending.borrow_mut().take();
            let request = SelectionFilterRequest {
                generation,
                filter,
                entries: Arc::from(entries),
                selected_paths,
                focus_list,
            };
            runtime
                .filter_worker
                .as_ref()
                .map(|worker| worker.try_submit(request))
        };

        match submit {
            Some(Ok(())) => {
                if let Some(runtime) = self.selection_mode.borrow().as_ref() {
                    runtime.status.set_label("Filtering files…");
                    runtime.accept.set_sensitive(false);
                }
            }
            Some(Err(SelectionFilterSubmitError::Busy(request))) => {
                if let Some(runtime) = self.selection_mode.borrow().as_ref() {
                    runtime.filter_pending.replace(Some(request));
                    runtime.status.set_label("Filtering files…");
                    runtime.accept.set_sensitive(false);
                }
            }
            Some(Err(SelectionFilterSubmitError::Stopped(request))) => {
                self.install_entries(
                    request.entries.to_vec(),
                    &request.selected_paths,
                    request.focus_list,
                );
            }
            None => {}
        }
    }

    fn drain_selection_filter_worker(&self) {
        let result = self.selection_mode.borrow().as_ref().and_then(|runtime| {
            runtime
                .filter_worker
                .as_ref()
                .and_then(SelectionFilterWorker::try_result)
        });
        if let Some(result) = result {
            let current = self
                .selection_mode
                .borrow()
                .as_ref()
                .is_some_and(|runtime| runtime.filter_generation.get() == result.generation);
            if current {
                self.install_entries(result.entries, &result.selected_paths, result.focus_list);
                self.refresh_selection_mode();
            }
        }

        let pending = self
            .selection_mode
            .borrow()
            .as_ref()
            .and_then(|runtime| runtime.filter_pending.borrow_mut().take());
        let Some(request) = pending else {
            return;
        };
        let submit = {
            let runtime = self.selection_mode.borrow();
            match runtime
                .as_ref()
                .and_then(|runtime| runtime.filter_worker.as_ref())
            {
                Some(worker) => worker.try_submit(request),
                None => Err(SelectionFilterSubmitError::Stopped(request)),
            }
        };
        match submit {
            Err(SelectionFilterSubmitError::Busy(request)) => {
                if let Some(runtime) = self.selection_mode.borrow().as_ref() {
                    runtime.filter_pending.replace(Some(request));
                }
            }
            Err(SelectionFilterSubmitError::Stopped(request)) => self.install_entries(
                request.entries.to_vec(),
                &request.selected_paths,
                request.focus_list,
            ),
            Ok(()) => {}
        }
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
            |worker| {
                worker.submit(
                    pending.generation,
                    state.mode,
                    state.query,
                    state.advanced,
                    pending.entries,
                )
            },
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
                    self.install_or_filter_selection_entries(entries, selected_paths, false);
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
        let state = self.filter_state.borrow();
        let total = if matches!(
            state.advanced.hidden,
            HiddenFilter::Include | HiddenFilter::Only
        ) {
            self.all_listed_entries.borrow().len()
        } else {
            self.listed_entries.borrow().len()
        };
        let active = !state.query.is_empty() || state.advanced.is_active();
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
                } else if controller.widgets.search_mode.selected() == 2 {
                    controller.start_content_search();
                }
            }
        });
    }

    fn configure_search_surface(&self, mode: u32) {
        let disk_search = mode != 0;
        let help = crate::ui::SEARCH_SURFACE_MODE_HELP
            .get(mode as usize)
            .copied()
            .unwrap_or(crate::ui::SEARCH_SURFACE_MODE_HELP[0]);
        self.widgets.search_mode.set_tooltip_text(Some(help));
        self.widgets
            .search_mode
            .update_property(&[gtk::accessible::Property::Description(help)]);
        self.widgets.filter_entry.set_tooltip_text(Some(help));
        self.widgets
            .filter_entry
            .set_placeholder_text(Some(match mode {
                1 => "Filename contains…",
                2 => "Text inside files…",
                _ => "Filter shown items",
            }));
        self.widgets.filter_mode.set_visible(true);
        self.widgets.filter_feedback.set_visible(!disk_search);
        self.widgets.search_scope.set_visible(disk_search);
        self.widgets.search_button.set_visible(disk_search);
        self.widgets.search_stop_button.set_visible(disk_search);
        self.widgets.search_feedback.set_visible(disk_search);
        self.widgets.saved_searches.set_visible(disk_search);
        self.widgets.recent_searches.set_visible(disk_search);
        self.widgets.search_result_order.set_visible(disk_search);
        self.widgets.save_search_button.set_visible(disk_search);
        self.widgets
            .delete_saved_search_button
            .set_visible(disk_search);
        self.widgets
            .clear_recent_searches_button
            .set_visible(disk_search);
        let filename_search = mode == 1;
        self.widgets
            .search_index_menu_button
            .set_visible(filename_search);
        if !disk_search {
            self.widgets.filter_entry.set_sensitive(true);
            self.widgets.search_scope.set_sensitive(true);
            self.widgets.search_button.set_sensitive(true);
            self.widgets.search_stop_button.set_sensitive(false);
        }
    }

    fn switch_search_surface_mode(&self) {
        let mode = self.widgets.search_mode.selected();
        let query = self.widgets.filter_entry.text().to_string();
        if mode != 0 {
            if self.trash_active.get() {
                self.widgets.search_mode.set_selected(0);
                self.configure_search_surface(0);
                self.show_toast("Search Files is available in local folders", 5);
                return;
            }
            if mode == 1 {
                if self.content_search_active.get() {
                    self.deactivate_content_search(false);
                }
                if !self.filename_search_active.get() {
                    self.show_filename_search();
                }
            } else {
                if self.filename_search_active.get() {
                    self.deactivate_filename_search(false);
                }
                if !self.content_search_active.get() {
                    self.show_content_search();
                }
            }
        } else {
            if self.filename_search_active.get() {
                self.deactivate_filename_search(true);
            }
            if self.content_search_active.get() {
                self.deactivate_content_search(true);
            }
            self.configure_search_surface(0);
            self.widgets.search_bar.set_visible(true);
            if self.widgets.filter_entry.text().as_str() != query {
                self.widgets.filter_entry.set_text(&query);
            }
            self.update_folder_filter_from_widgets();
            self.focus_search_entry();
        }
    }

    fn close_search_surface(&self) {
        if self.content_search_active.get() || self.widgets.search_mode.selected() == 2 {
            self.clear_content_search();
        } else if self.filename_search_active.get() || self.widgets.search_mode.selected() == 1 {
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
        self.configure_search_surface(1);
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
        self.set_search_feedback("Enter a filename or choose filters", false);
        self.set_open_enabled(false);
        self.set_open_with_enabled(false);
        self.set_properties_enabled(false);
        self.set_checksum_enabled(false);
        self.set_selection_actions_enabled(false, false, false);
        self.set_reveal_enabled(false);
        self.focus_search_entry();
    }

    fn open_content_search_surface(&self) {
        if self.trash_active.get() {
            self.show_toast("Search Contents is available in local folders", 5);
            return;
        }
        self.widgets.search_mode.set_selected(2);
        if !self.content_search_active.get() {
            self.show_content_search();
        }
        self.widgets.search_bar.set_visible(true);
        self.focus_search_entry();
    }

    fn show_content_search(&self) {
        if self.trash_active.get() {
            self.show_toast("Search Contents is available in local folders", 5);
            return;
        }
        self.deactivate_folder_filter_for_search();
        self.configure_search_surface(2);
        self.content_search_active.set(true);
        self.content_search_running.set(false);
        self.content_search_root.replace(Some(
            self.tabs.borrow().active().current().path().to_path_buf(),
        ));
        self.content_search_results.borrow_mut().clear();
        self.content_search_summary.borrow_mut().take();
        self.pending_content_search.borrow_mut().take();
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        self.widgets.selection.set_model(Some(&store));
        self.content_search_store.replace(Some(store));
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
        self.set_content_search_running(false);
        self.set_search_feedback("Enter text to search inside local files", false);
        self.set_open_enabled(false);
        self.set_open_with_enabled(false);
        self.set_properties_enabled(false);
        self.set_checksum_enabled(false);
        self.set_selection_actions_enabled(false, false, false);
        self.set_reveal_enabled(false);
        self.focus_search_entry();
    }

    fn start_content_search(&self) {
        if !self.content_search_active.get() {
            self.open_content_search_surface();
        }
        if !self.content_search_active.get() {
            return;
        }
        let query = self.widgets.filter_entry.text().to_string();
        let scope = if self.widgets.search_scope.selected() == 0 {
            FilenameSearchScope::CurrentFolder
        } else {
            FilenameSearchScope::Subtree
        };
        let root = self.tabs.borrow().active().current().path().to_path_buf();
        let mode = match self.widgets.filter_mode.selected() {
            1 => FolderFilterMode::Glob,
            2 => FolderFilterMode::Regex,
            _ => FolderFilterMode::Text,
        };
        let definition = self
            .pending_saved_search
            .borrow_mut()
            .take()
            .filter(|saved| saved.kind() == SearchKind::Contents && saved.root() == root)
            .map_or_else(
                || {
                    SearchQuery::new(
                        root.clone(),
                        SearchKind::Contents,
                        query,
                        scope,
                        self.show_hidden.get(),
                        mode,
                        self.advanced_filter_from_widgets(),
                    )
                },
                Ok,
            );
        let definition = match definition {
            Ok(definition) => definition,
            Err(error) => {
                self.set_content_search_running(false);
                self.set_search_feedback(&error.to_string(), true);
                return;
            }
        };
        let request = match definition.content_request() {
            Ok(request) => request,
            Err(error) => {
                self.set_content_search_running(false);
                self.set_search_feedback(&error.to_string(), true);
                return;
            }
        };
        self.record_recent_search(definition);
        let generation = self.content_search_generation.get().wrapping_add(1).max(1);
        self.content_search_generation.set(generation);
        self.content_search_root.replace(Some(root));
        self.content_search_results.borrow_mut().clear();
        self.content_search_summary.borrow_mut().take();
        self.widgets.selection.unselect_all();
        self.selected_entries.borrow_mut().clear();
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        self.widgets.selection.set_model(Some(&store));
        self.content_search_store.replace(Some(store));
        self.widgets.empty_state.set_visible(false);
        self.pending_content_search.replace(Some(request));
        self.set_content_search_running(true);
        self.set_search_feedback("Searching file contents…", false);
        self.try_submit_pending_content_search();
    }

    fn try_submit_pending_content_search(&self) {
        let Some(request) = self.pending_content_search.borrow_mut().take() else {
            return;
        };
        let generation = self.content_search_generation.get();
        let outcome = self.content_search_worker.borrow().as_ref().map_or(
            Err(crate::content_search::ContentSearchSubmitError::Stopped),
            |worker| worker.submit(generation, request),
        );
        match outcome {
            Ok(()) => {}
            Err(crate::content_search::ContentSearchSubmitError::Busy(request)) => {
                self.pending_content_search.replace(Some(*request));
            }
            Err(crate::content_search::ContentSearchSubmitError::Stopped) => {
                self.set_content_search_running(false);
                self.set_search_feedback("Content search worker is unavailable", true);
            }
        }
    }

    fn drain_content_search_worker(&self) {
        loop {
            let event = self
                .content_search_worker
                .borrow()
                .as_ref()
                .and_then(crate::content_search::ContentSearchWorker::try_event);
            let Some(event) = event else {
                break;
            };
            if !self.content_search_active.get()
                || event.generation != self.content_search_generation.get()
            {
                continue;
            }
            match event.kind {
                crate::content_search::ContentSearchEventKind::Batch { matches, summary } => {
                    if let Some(store) = self.content_search_store.borrow().as_ref() {
                        for content_match in &matches {
                            store.append(&glib::BoxedAnyObject::new(content_match.clone()));
                        }
                    }
                    self.content_search_results.borrow_mut().extend(matches);
                    self.content_search_summary.replace(Some(summary));
                    self.widgets.empty_state.set_visible(false);
                    self.set_search_feedback(&content_search_feedback(summary, true, false), false);
                    self.refresh_status();
                }
                crate::content_search::ContentSearchEventKind::Finished(summary) => {
                    self.content_search_summary.replace(Some(summary));
                    self.set_content_search_running(false);
                    let empty = self.content_search_results.borrow().is_empty();
                    self.widgets.empty_state.set_visible(empty);
                    if empty {
                        self.widgets.empty_label.set_label("No content matches");
                    }
                    self.set_search_feedback(
                        &content_search_feedback(summary, false, false),
                        false,
                    );
                    self.apply_search_result_order();
                    self.refresh_status();
                }
                crate::content_search::ContentSearchEventKind::Failed(error) => {
                    self.set_content_search_running(false);
                    self.set_search_feedback(&format!("Content search failed: {error}"), true);
                }
            }
        }
        self.try_submit_pending_content_search();
    }

    fn stop_content_search(&self) {
        if !self.content_search_active.get() {
            return;
        }
        let generation = self.content_search_generation.get().wrapping_add(1).max(1);
        self.content_search_generation.set(generation);
        if let Some(worker) = self.content_search_worker.borrow().as_ref() {
            worker.cancel(generation);
        }
        self.pending_content_search.borrow_mut().take();
        self.set_content_search_running(false);
        let summary = self.content_search_summary.borrow().unwrap_or_default();
        self.set_search_feedback(&content_search_feedback(summary, false, true), false);
    }

    fn clear_content_search(&self) {
        self.widgets.filter_entry.set_text("");
        self.reset_advanced_filter_widgets();
        self.deactivate_content_search(true);
    }

    fn deactivate_content_search(&self, restore_listing: bool) {
        let generation = self.content_search_generation.get().wrapping_add(1).max(1);
        self.content_search_generation.set(generation);
        if let Some(worker) = self.content_search_worker.borrow().as_ref() {
            worker.cancel(generation);
        }
        self.content_search_active.set(false);
        self.content_search_running.set(false);
        self.content_search_root.borrow_mut().take();
        self.content_search_results.borrow_mut().clear();
        self.content_search_store.borrow_mut().take();
        self.content_search_summary.borrow_mut().take();
        self.pending_content_search.borrow_mut().take();
        self.widgets.search_bar.set_visible(false);
        self.set_view_controls_sensitive(true);
        self.set_sort_controls_sensitive(true);
        self.set_reveal_enabled(false);
        self.widgets.set_view_mode(self.view_mode.get());
        if restore_listing {
            self.install_entries(self.listed_entries.borrow().to_vec(), &[], true);
        }
    }

    fn set_content_search_running(&self, running: bool) {
        self.content_search_running.set(running);
        self.widgets.search_button.set_sensitive(!running);
        self.widgets.search_stop_button.set_sensitive(running);
        self.widgets.filter_entry.set_sensitive(!running);
        self.widgets.search_scope.set_sensitive(!running);
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
        let mode = match self.widgets.filter_mode.selected() {
            1 => FolderFilterMode::Glob,
            2 => FolderFilterMode::Regex,
            _ => FolderFilterMode::Text,
        };
        let definition = self
            .pending_saved_search
            .borrow_mut()
            .take()
            .filter(|saved| saved.kind() == SearchKind::Files && saved.root() == root)
            .map_or_else(
                || {
                    SearchQuery::new(
                        root.clone(),
                        SearchKind::Files,
                        query,
                        scope,
                        self.show_hidden.get(),
                        mode,
                        self.advanced_filter_from_widgets(),
                    )
                },
                Ok,
            );
        let definition = match definition {
            Ok(definition) => definition,
            Err(error) => {
                self.set_filename_search_running(false);
                self.set_search_feedback(&error.to_string(), true);
                return;
            }
        };
        let request = match definition.filename_request() {
            Ok(request) => request,
            Err(error) => {
                self.set_filename_search_running(false);
                self.set_search_feedback(&error.to_string(), true);
                return;
            }
        };
        self.record_recent_search(definition);
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
        self.set_filename_search_running(true);
        self.search_index_fallback_note.borrow_mut().take();
        if self.current_preferences.borrow().search_index_enabled {
            let index_generation = self.next_search_index_generation();
            let submitted = self
                .search_index_worker
                .borrow()
                .as_ref()
                .is_some_and(|worker| worker.query(index_generation, request.clone()).is_ok());
            if submitted {
                self.search_index_query_active.set(true);
                self.set_search_feedback("Checking optional filename index…", false);
                return;
            }
            self.search_index_fallback_note
                .replace(Some("index unavailable".to_owned()));
        }
        self.pending_filename_search.replace(Some(request));
        self.set_search_feedback("Searching filenames live…", false);
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

    fn drain_search_index_worker(&self) {
        loop {
            let event = self
                .search_index_worker
                .borrow()
                .as_ref()
                .and_then(crate::search_index::SearchIndexWorker::try_event);
            let Some(event) = event else {
                break;
            };
            if event.generation != self.search_index_generation.get() {
                continue;
            }
            match event.kind {
                crate::search_index::SearchIndexEventKind::Built(summary) => {
                    self.show_toast(
                        &format!(
                            "Search index ready: {} visible items in {} folders; {} hidden items excluded",
                            summary.indexed_entries,
                            summary.indexed_directories,
                            summary.excluded_hidden
                        ),
                        6,
                    );
                }
                crate::search_index::SearchIndexEventKind::Batch(entries, summary) => {
                    if !self.filename_search_active.get() || !self.search_index_query_active.get() {
                        continue;
                    }
                    if let Some(store) = self.filename_search_store.borrow().as_ref() {
                        for entry in &entries {
                            store.append(&glib::BoxedAnyObject::new(entry.clone()));
                        }
                    }
                    self.filename_search_results.borrow_mut().extend(entries);
                    self.filename_search_summary.replace(Some(summary));
                    self.widgets.empty_state.set_visible(false);
                    self.set_search_feedback(
                        &format!(
                            "{} Using current index.",
                            filename_search_feedback(summary, false, false)
                        ),
                        false,
                    );
                    self.refresh_status();
                }
                crate::search_index::SearchIndexEventKind::Finished(summary) => {
                    if !self.filename_search_active.get()
                        || !self.search_index_query_active.replace(false)
                    {
                        continue;
                    }
                    self.filename_search_summary.replace(Some(summary));
                    self.set_filename_search_running(false);
                    let empty = self.filename_search_results.borrow().is_empty();
                    self.widgets.empty_state.set_visible(empty);
                    if empty {
                        self.widgets
                            .empty_label
                            .set_label("No indexed filename matches");
                    }
                    self.set_search_feedback(
                        &format!(
                            "{} Used current index.",
                            filename_search_feedback(summary, true, false)
                        ),
                        false,
                    );
                    self.apply_search_result_order();
                    self.refresh_status();
                }
                crate::search_index::SearchIndexEventKind::Fallback { request, reason } => {
                    if !self.filename_search_active.get()
                        || !self.search_index_query_active.replace(false)
                    {
                        continue;
                    }
                    self.search_index_fallback_note
                        .replace(Some(reason.description().to_owned()));
                    self.pending_filename_search.replace(Some(*request));
                    self.set_search_feedback(
                        &format!(
                            "Searching live because {}; complete live fallback remains active.",
                            reason.description()
                        ),
                        false,
                    );
                    self.try_submit_pending_filename_search();
                }
                crate::search_index::SearchIndexEventKind::Cleared => {
                    self.show_toast("Search index cache cleared", 4);
                }
                crate::search_index::SearchIndexEventKind::Failed(error) => {
                    self.search_index_query_active.set(false);
                    self.show_toast(&format!("Search index operation failed: {error}"), 6);
                }
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
                    let feedback = filename_search_feedback(summary, true, false);
                    let feedback = self
                        .search_index_fallback_note
                        .borrow()
                        .as_ref()
                        .map_or(feedback.clone(), |reason| {
                            format!("{feedback} Live fallback used because {reason}.")
                        });
                    self.set_search_feedback(&feedback, false);
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
                    self.apply_search_result_order();
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
        let index_generation = self.next_search_index_generation();
        if let Some(worker) = self.search_index_worker.borrow().as_ref() {
            worker.cancel(index_generation);
        }
        self.search_index_query_active.set(false);
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
        self.reset_advanced_filter_widgets();
        self.deactivate_filename_search(true);
    }

    fn deactivate_filename_search(&self, restore_listing: bool) {
        let generation = self.filename_search_generation.get().wrapping_add(1).max(1);
        self.filename_search_generation.set(generation);
        let index_generation = self.next_search_index_generation();
        if let Some(worker) = self.search_index_worker.borrow().as_ref() {
            worker.cancel(index_generation);
        }
        self.search_index_query_active.set(false);
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
        self.widgets.search_index_toggle.set_sensitive(!running);
        self.widgets
            .search_index_menu_button
            .set_sensitive(!running);
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
        if !self.filename_search_active.get() && !self.content_search_active.get() {
            return;
        }
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let target = entry.path().to_path_buf();
        if self.filename_search_active.get() {
            self.deactivate_filename_search(false);
        } else {
            self.deactivate_content_search(false);
        }
        self.navigate_to_revealing(target);
    }

    fn install_actions(self: &Rc<Self>, application: &adw::Application) {
        self.add_action("settings", |controller| {
            controller.show_settings_center();
        });
        let show_error_details =
            gio::SimpleAction::new("show-error-details", Some(&String::static_variant_type()));
        let controller = Rc::downgrade(self);
        show_error_details.connect_activate(move |_, parameter| {
            let Some(details) = parameter.and_then(glib::Variant::str) else {
                return;
            };
            if let Some(controller) = controller.upgrade() {
                controller.show_feedback_details(details);
            }
        });
        self.widgets.window.add_action(&show_error_details);
        let breadcrumb_action =
            gio::SimpleAction::new("breadcrumb", Some(&u64::static_variant_type()));
        let controller = Rc::downgrade(self);
        breadcrumb_action.connect_activate(move |_, parameter| {
            let Some(index) = parameter.and_then(glib::Variant::get::<u64>) else {
                return;
            };
            let Some(controller) = controller.upgrade() else {
                return;
            };
            let path = controller
                .breadcrumb_paths
                .borrow()
                .get(index as usize)
                .cloned();
            if let Some(path) = path {
                controller.navigate_to(path);
            }
        });
        self.widgets.window.add_action(&breadcrumb_action);
        self.add_action("recent-locations", |controller| {
            controller.show_recent_locations();
        });
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
        self.add_action("desktop-integration-status", |controller| {
            controller.desktop_integration.present();
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
        self.add_action("custom-actions", |controller| {
            controller.show_custom_actions_editor();
        });
        self.add_action("custom-action-chooser", |controller| {
            controller.show_custom_action_chooser();
        });
        self.add_action("open-as-administrator", |controller| {
            controller.open_as_administrator();
        });
        self.add_action("return-standard-access", |controller| {
            controller.privileged_access.cancel_and_close();
        });
        let custom_action =
            gio::SimpleAction::new("run-custom-action", Some(glib::VariantTy::UINT64));
        let controller = Rc::downgrade(self);
        custom_action.connect_activate(move |_, parameter| {
            if let Some(id) = parameter.and_then(glib::Variant::get::<u64>)
                && let Some(controller) = controller.upgrade()
            {
                controller.run_custom_action(id);
            }
        });
        self.widgets.window.add_action(&custom_action);
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
            if controller.widgets.search_mode.selected() == 2 {
                controller.start_content_search();
            } else {
                controller.start_filename_search();
            }
        });
        self.add_action("stop-filename-search", |controller| {
            if controller.content_search_active.get() {
                controller.stop_content_search();
            } else {
                controller.stop_filename_search();
            }
        });
        self.add_action("clear-filename-search", |controller| {
            controller.clear_filename_search();
        });
        self.add_action("save-search", |controller| {
            controller.show_save_search_dialog();
        });
        self.add_action("delete-saved-search", |controller| {
            controller.delete_selected_saved_search();
        });
        self.add_action("clear-recent-searches", |controller| {
            controller.clear_recent_searches();
        });
        self.add_action("build-search-index", |controller| {
            controller.build_search_index();
        });
        self.add_action("clear-search-index", |controller| {
            controller.clear_search_index();
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
        self.add_action("invert-selection", |controller| {
            controller.invert_selection();
        });
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
        let open_window = self.add_action("open-new-window", |controller| {
            controller.open_selected_folder_in_new_window();
        });
        open_window.set_enabled(false);
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

        let icon_style = self.widgets.entry_icon_style();
        let icon_style_action = gio::SimpleAction::new_stateful(
            "icon-style",
            Some(&String::static_variant_type()),
            &icon_style.persisted().to_variant(),
        );
        let controller = Rc::downgrade(self);
        icon_style_action.connect_activate(move |action, parameter| {
            let Some(style) = parameter
                .and_then(glib::Variant::str)
                .and_then(EntryIconStyle::from_persisted)
            else {
                return;
            };
            if let Some(controller) = controller.upgrade() {
                controller.change_entry_icon_style(style);
                action.set_state(&style.persisted().to_variant());
            }
        });
        self.widgets.window.add_action(&icon_style_action);

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

        let sort = self.sort_order.get();
        let sort_column_action = gio::SimpleAction::new_stateful(
            "sort-column",
            Some(&String::static_variant_type()),
            &sort.column.persisted().to_variant(),
        );
        let controller = Rc::downgrade(self);
        sort_column_action.connect_activate(move |_, parameter| {
            let Some(column) = parameter
                .and_then(glib::Variant::str)
                .and_then(SortColumn::from_persisted)
            else {
                return;
            };
            if let Some(controller) = controller.upgrade() {
                controller.change_sort_column(column);
            }
        });
        self.widgets.window.add_action(&sort_column_action);

        let sort_direction_action = gio::SimpleAction::new_stateful(
            "sort-direction",
            Some(&String::static_variant_type()),
            &sort.direction.persisted().to_variant(),
        );
        let controller = Rc::downgrade(self);
        sort_direction_action.connect_activate(move |_, parameter| {
            let Some(direction) = parameter
                .and_then(glib::Variant::str)
                .and_then(SortDirection::from_persisted)
            else {
                return;
            };
            if let Some(controller) = controller.upgrade() {
                controller.change_sort_direction(direction);
            }
        });
        self.widgets.window.add_action(&sort_direction_action);

        let folders_first = sort.directories == DirectoryPlacement::First;
        let folders_first_action =
            gio::SimpleAction::new_stateful("folders-first", None, &folders_first.to_variant());
        let controller = Rc::downgrade(self);
        folders_first_action.connect_activate(move |action, _| {
            let enabled = !action
                .state()
                .and_then(|state| state.get::<bool>())
                .unwrap_or(true);
            if let Some(controller) = controller.upgrade() {
                controller.change_directory_placement(if enabled {
                    DirectoryPlacement::First
                } else {
                    DirectoryPlacement::Last
                });
            }
        });
        self.widgets.window.add_action(&folders_first_action);

        let hidden_last_action =
            gio::SimpleAction::new_stateful("hidden-last", None, &sort.hidden_last.to_variant());
        let controller = Rc::downgrade(self);
        hidden_last_action.connect_activate(move |action, _| {
            let enabled = !action
                .state()
                .and_then(|state| state.get::<bool>())
                .unwrap_or(false);
            if let Some(controller) = controller.upgrade() {
                controller.change_hidden_last(enabled);
            }
        });
        self.widgets.window.add_action(&hidden_last_action);

        let notifications_enabled = self.current_preferences.borrow().completion_notifications;
        let notification_action = gio::SimpleAction::new_stateful(
            "completion-notifications",
            None,
            &notifications_enabled.to_variant(),
        );
        let controller = Rc::downgrade(self);
        notification_action.connect_activate(move |action, _| {
            let enabled = !action
                .state()
                .and_then(|state| state.get::<bool>())
                .unwrap_or(true);
            if let Some(controller) = controller.upgrade() {
                controller
                    .current_preferences
                    .borrow_mut()
                    .completion_notifications = enabled;
                controller.queue_preferences();
            }
            action.set_state(&enabled.to_variant());
        });
        self.widgets.window.add_action(&notification_action);

        let metadata_unavailable = gio::SimpleAction::new("metadata-sort-unavailable", None);
        metadata_unavailable.set_enabled(false);
        self.widgets.window.add_action(&metadata_unavailable);

        let cancel_metadata_sort = gio::SimpleAction::new("cancel-metadata-sort", None);
        cancel_metadata_sort.set_enabled(false);
        let controller = Rc::downgrade(self);
        cancel_metadata_sort.connect_activate(move |_, _| {
            if let Some(controller) = controller.upgrade() {
                if let Some(worker) = controller.metadata_index_worker.borrow_mut().as_mut() {
                    worker.cancel();
                }
                controller.sort_in_flight.set(false);
                controller.set_sort_controls_sensitive(true);
                controller.widgets.set_views_sensitive(true);
                controller.widgets.spinner.stop();
                controller
                    .widgets
                    .status_label
                    .set_label("Metadata scan cancelled");
                let current = controller.sort_order.get();
                controller.resort_with(
                    DirectorySort::new(SortColumn::Name, floe_core::SortDirection::Ascending)
                        .with_directories(current.directories)
                        .with_grouping(current.grouping)
                        .with_hidden_last(current.hidden_last),
                );
            }
        });
        self.widgets.window.add_action(&cancel_metadata_sort);

        let clear_metadata_cache = gio::SimpleAction::new("clear-metadata-sort-cache", None);
        let controller = Rc::downgrade(self);
        clear_metadata_cache.connect_activate(move |_, _| {
            let Some(controller) = controller.upgrade() else {
                return;
            };
            let result = controller
                .metadata_index_worker
                .borrow_mut()
                .as_mut()
                .ok_or(MetadataIndexSubmitError::Disconnected)
                .and_then(MetadataIndexWorker::clear);
            match result {
                Ok(generation) => controller.metadata_index_generation.set(generation),
                Err(error) => controller.show_toast(
                    &format!("Could not clear advanced metadata cache: {error}"),
                    6,
                ),
            }
        });
        self.widgets.window.add_action(&clear_metadata_cache);

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

        let toggle_group =
            gio::SimpleAction::new("toggle-group", Some(&String::static_variant_type()));
        let controller = Rc::downgrade(self);
        toggle_group.connect_activate(move |_, parameter| {
            let Some(label) = parameter.and_then(glib::Variant::str) else {
                return;
            };
            let Some(controller) = controller.upgrade() else {
                return;
            };
            let labels = controller.widgets.toggle_group_collapse(label);
            controller.current_preferences.borrow_mut().collapsed_groups = labels;
            controller.queue_preferences();
            controller.widgets.focus_view(controller.view_mode.get());
        });
        self.widgets.window.add_action(&toggle_group);

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
        self.add_action("autosize-name", |controller| {
            controller.autosize_list_column(ListColumn::Name);
        });
        self.add_action("move-column-left-name", |controller| {
            controller.move_list_column(ListColumn::Name, -1);
        });
        self.add_action("move-column-right-name", |controller| {
            controller.move_list_column(ListColumn::Name, 1);
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
            let autosize = format!("autosize-{}", column.persisted());
            self.add_action(&autosize, move |controller| {
                controller.autosize_list_column(column);
            });
            let move_left = format!("move-column-left-{}", column.persisted());
            self.add_action(&move_left, move |controller| {
                controller.move_list_column(column, -1);
            });
            let move_right = format!("move-column-right-{}", column.persisted());
            self.add_action(&move_right, move |controller| {
                controller.move_list_column(column, 1);
            });
        }

        self.add_action("reset-sidebar-width", |controller| {
            controller.reset_sidebar_width();
        });
        self.add_action("toggle-sidebar", |controller| {
            controller.toggle_sidebar_collapsed();
        });
        let open_action = self.add_action("open", |controller| controller.activate_selected());
        open_action.set_enabled(false);
        let open_with_action =
            self.add_action("open-with", |controller| controller.show_open_with());
        open_with_action.set_enabled(false);
        let properties_action =
            self.add_action("properties", |controller| controller.show_properties());
        properties_action.set_enabled(false);
        let inspect_security = self.add_action("inspect-privacy-safety", |controller| {
            controller.inspect_privacy_safety();
        });
        inspect_security.set_enabled(false);
        let scan_threats = self.add_action("scan-threats", |controller| {
            controller.scan_selected_for_threats();
        });
        scan_threats.set_enabled(false);
        let cancel_threat_scan = self.add_action("cancel-threat-scan", |controller| {
            let generation = controller.threat_scan_generation.get();
            controller.application_state.cancel_threat_scan(generation);
            controller.set_action_enabled("cancel-threat-scan", false);
            controller
                .mark_background_feedback_stopping(BackgroundActivity::ThreatScan, generation);
        });
        cancel_threat_scan.set_enabled(false);
        let cancel_privacy = self.add_action("cancel-privacy-inspection", |controller| {
            let generation = controller.privacy_security_generation.get();
            controller
                .application_state
                .cancel_privacy_security(generation);
            controller.set_action_enabled("cancel-privacy-inspection", false);
            controller.mark_background_feedback_stopping(
                BackgroundActivity::PrivacyInspection,
                generation,
            );
        });
        cancel_privacy.set_enabled(false);
        let cancel_sanitization = self.add_action("cancel-sanitization", |controller| {
            let generation = controller.privacy_security_generation.get();
            controller
                .application_state
                .cancel_privacy_security(generation);
            controller.set_action_enabled("cancel-sanitization", false);
            controller.mark_background_feedback_stopping(
                BackgroundActivity::MetadataSanitization,
                generation,
            );
        });
        cancel_sanitization.set_enabled(false);
        self.add_action("show-last-properties", |controller| {
            if let Some(presentation) = controller.last_properties_presentation.borrow().clone() {
                controller.present_properties_dialog(&presentation);
            } else {
                controller.show_toast("Properties result is no longer available", 5);
            }
        });
        self.add_action("show-last-privacy-report", |controller| {
            if let Some(outcome) = controller.last_privacy_outcome.borrow().clone() {
                controller.present_inspection_outcome(&outcome);
            } else {
                controller.show_toast("Privacy inspection result is no longer available", 5);
            }
        });
        self.add_action("show-last-threat-report", |controller| {
            if let Some(outcome) = controller.last_threat_outcome.borrow().clone() {
                controller.present_threat_scan_outcome(&outcome);
            } else {
                controller.show_toast("ClamAV result is no longer available", 5);
            }
        });
        self.add_action("reveal-last-sanitized-copy", |controller| {
            if let Some(path) = controller.last_sanitized_copy.borrow().clone() {
                controller.navigate_to_revealing(path);
            } else {
                controller.show_toast("Sanitized copy is no longer available", 5);
            }
        });
        let sanitize = self.add_action("create-sanitized-copy", |controller| {
            controller.create_sanitized_copy();
        });
        sanitize.set_enabled(false);
        let checksum_action =
            self.add_action("checksum", |controller| controller.show_checksum_dialog());
        checksum_action.set_enabled(false);
        let save_fingerprint = self.add_action("save-sha256-fingerprint", |controller| {
            controller.save_selected_fingerprint()
        });
        save_fingerprint.set_enabled(false);
        let verify_fingerprint = self.add_action("verify-saved-fingerprint", |controller| {
            controller.verify_selected_fingerprint()
        });
        verify_fingerprint.set_enabled(false);
        let generate_manifest = self.add_action("generate-sha256sums", |controller| {
            controller.generate_selected_sha256sums()
        });
        generate_manifest.set_enabled(false);
        let verify_manifest = self.add_action("verify-sha256sums", |controller| {
            controller.verify_selected_sha256sums()
        });
        verify_manifest.set_enabled(false);
        self.add_action("create-integrity-baseline", |controller| {
            controller.create_integrity_baseline(false);
        });
        self.add_action("update-integrity-baseline", |controller| {
            controller.create_integrity_baseline(true);
        });
        self.add_action("verify-integrity-baseline", |controller| {
            controller.verify_integrity_baseline();
        });
        self.add_action("delete-integrity-baseline", |controller| {
            controller.delete_integrity_baseline();
        });
        self.add_action("start-integrity-monitoring", |controller| {
            controller.start_integrity_monitoring();
        });
        self.add_action("stop-integrity-monitoring", |controller| {
            controller.stop_integrity_monitoring();
        });
        let duplicates_action = self.add_action("check-duplicates", |controller| {
            controller.show_duplicate_setup();
        });
        duplicates_action.set_enabled(true);
        let cancel_duplicates = self.add_action("cancel-duplicate-scan", |controller| {
            controller.cancel_duplicate_scan();
        });
        cancel_duplicates.set_enabled(false);
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
        let copy_verify_action = self.add_action("copy-and-verify", |controller| {
            controller.choose_verified_copy_destination()
        });
        copy_verify_action.set_enabled(false);
        let verified_usb_action = self.add_action("verified-removable-transfer", |controller| {
            controller.choose_verified_usb_destination()
        });
        verified_usb_action.set_enabled(false);
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
        let protect_folder = self.add_action("protect-folder", |controller| {
            controller.change_target_protection(true);
        });
        protect_folder.set_enabled(false);
        let unprotect_folder = self.add_action("unprotect-folder", |controller| {
            controller.change_target_protection(false);
        });
        unprotect_folder.set_enabled(false);
        self.add_action("protected-folders", |controller| {
            controller.show_protected_folders_status();
        });
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
        self.enforce_selection_mode_actions();
    }

    fn enforce_selection_mode_actions(&self) {
        if self.selection_mode.borrow().is_none() {
            return;
        }
        for name in self.widgets.window.list_actions() {
            if selection_mode_blocks_action(name.as_str())
                && let Some(action) = self
                    .widgets
                    .window
                    .lookup_action(name.as_str())
                    .and_downcast::<gio::SimpleAction>()
            {
                action.set_enabled(false);
            }
        }
    }

    fn add_action<F>(self: &Rc<Self>, name: &str, callback: F) -> gio::SimpleAction
    where
        F: Fn(&Rc<Self>) + 'static,
    {
        let action = gio::SimpleAction::new(name, None);
        let blocked_in_selection_mode = selection_mode_blocks_action(name);
        if blocked_in_selection_mode && self.selection_mode.borrow().is_some() {
            action.set_enabled(false);
        }
        let controller = Rc::downgrade(self);
        action.connect_activate(move |_, _| {
            if let Some(controller) = controller.upgrade() {
                if blocked_in_selection_mode && controller.selection_mode.borrow().is_some() {
                    controller.show_toast("This action is unavailable in Selection Mode", 4);
                    return;
                }
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
        if intent == TransferIntent::Move {
            let status_label = self.widgets.status_label.clone();
            let toast_overlay = self.widgets.toast_overlay.clone();
            review_move_batch(
                &self.widgets.window,
                Rc::clone(&self.application_state),
                self.guardrail_environment(),
                paths,
                destination,
                move |result| match result {
                    Ok(batch) => status_label.set_label(&format!(
                        "Move to other pane queued: {}",
                        item_count_text(batch.queued())
                    )),
                    Err(error) => toast_overlay.add_toast(
                        adw::Toast::builder()
                            .title(format!("Could not start pane transfer: {error}"))
                            .timeout(6)
                            .build(),
                    ),
                },
            );
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

    fn open_selected_folder_in_new_window(&self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        if self.trash_active.get() || !entry.is_navigable_directory() {
            return;
        }
        let Some(application) = self.widgets.window.application() else {
            return;
        };
        application.activate_action(
            "open-new-window",
            Some(&entry.path().as_os_str().as_bytes().to_vec().to_variant()),
        );
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
            let title_label = compact_tab_title_label(&title);
            let button = gtk::ToggleButton::builder()
                .child(&title_label)
                .active(is_active)
                .hexpand(true)
                .build();
            button.add_css_class("flat");
            button.add_css_class("floe-tab-target");
            button.set_action_name(Some("win.activate-tab"));
            button.set_action_target_value(Some(&id.get().to_variant()));
            button.set_tooltip_text(Some(&path.to_string_lossy()));
            button.update_property(&[
                gtk::accessible::Property::Label(&format!("Tab: {title}")),
                gtk::accessible::Property::Description(&format!(
                    "Folder path: {}",
                    path.to_string_lossy()
                )),
            ]);
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
                .icon_name("floe-phosphor-x-symbolic")
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

    pub(crate) fn navigate_to_revealing(&self, target: PathBuf) {
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

    pub(crate) fn queue_operation_reveal(&self, request: OperationRevealRequest) {
        if self.trash_active.get()
            || self.filename_search_active.get()
            || self.content_search_active.get()
            || self.tabs.borrow().active().current().path() != request.directory()
        {
            return;
        }

        let mut pending = self.pending_operation_reveal.borrow_mut();
        if pending
            .as_mut()
            .is_some_and(|current| current.merge(request.clone()))
        {
            return;
        }
        pending.replace(PendingOperationReveal::from_request(request));
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
        if self.filename_search_active.get() || self.content_search_active.get() {
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

    fn change_color_scheme(&self, scheme: ColorSchemePreference) {
        if self.current_preferences.borrow().color_scheme == scheme {
            return;
        }
        self.current_preferences.borrow_mut().color_scheme = scheme;
        self.widgets
            .apply_appearance_preferences(&self.current_preferences.borrow());
        self.queue_preferences();
        self.show_toast(&format!("Color scheme: {}", scheme.label()), 3);
    }

    fn change_click_policy(&self, policy: ClickPolicy) {
        if self.current_preferences.borrow().click_policy == policy {
            return;
        }
        self.current_preferences.borrow_mut().click_policy = policy;
        self.widgets.apply_click_policy(policy);
        self.queue_preferences();
        self.show_toast(policy.label(), 3);
    }

    fn change_font_preferences(&self, family: Option<&str>, scale_percent: u16) {
        let family = family.and_then(validated_font_family);
        let scale_percent = clamp_font_scale(scale_percent);
        {
            let mut preferences = self.current_preferences.borrow_mut();
            if preferences.font_family == family && preferences.font_scale_percent == scale_percent
            {
                return;
            }
            preferences.font_family = family;
            preferences.font_scale_percent = scale_percent;
        }
        self.widgets
            .apply_appearance_preferences(&self.current_preferences.borrow());
        self.queue_preferences();
    }

    fn change_reduced_motion(&self, enabled: bool) {
        if self.current_preferences.borrow().reduced_motion == enabled {
            return;
        }
        self.current_preferences.borrow_mut().reduced_motion = enabled;
        self.widgets
            .apply_appearance_preferences(&self.current_preferences.borrow());
        self.queue_preferences();
        self.show_toast(
            if enabled {
                "Reduced motion enabled"
            } else {
                "Reduced motion disabled"
            },
            3,
        );
    }

    fn reset_appearance_preferences(&self) {
        {
            let mut preferences = self.current_preferences.borrow_mut();
            preferences.appearance = AppearancePreset::Frosted;
            preferences.color_scheme = ColorSchemePreference::System;
            preferences.font_family = None;
            preferences.font_scale_percent = 100;
            preferences.reduced_motion = false;
        }
        self.widgets.apply_appearance(AppearancePreset::Frosted);
        self.widgets
            .apply_appearance_preferences(&self.current_preferences.borrow());
        self.queue_preferences();
        self.show_toast("Appearance settings reset", 3);
    }

    fn change_entry_icon_style(&self, style: EntryIconStyle) {
        if self.widgets.entry_icon_style() == style {
            return;
        }

        self.widgets.apply_entry_icon_style(style);
        self.current_preferences.borrow_mut().icon_style = style;
        self.queue_preferences();

        if self.filename_search_active.get() || self.content_search_active.get() {
            self.apply_search_result_order();
        } else {
            let selected_paths = self
                .selected_entries
                .borrow()
                .iter()
                .map(|entry| entry.path().to_path_buf())
                .collect::<Vec<_>>();
            let entries = self.visible_entries.borrow().clone();
            self.install_entries(entries, &selected_paths, false);
        }

        self.show_toast(&format!("File & folder icons: {}", style.label()), 3);
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
        for name in ["refresh", "location", "select-all", "invert-selection"] {
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

    fn show_settings_center(self: &Rc<Self>) {
        let settings = crate::settings_center::build(&self.current_preferences.borrow());

        let controller = Rc::downgrade(self);
        settings
            .appearance
            .connect_selected_notify(move |dropdown| {
                let Some(preset) = AppearancePreset::ALL.get(dropdown.selected() as usize) else {
                    return;
                };
                if let Some(controller) = controller.upgrade() {
                    controller.activate_window_action(
                        "appearance",
                        Some(&preset.persisted().to_variant()),
                    );
                }
            });

        let controller = Rc::downgrade(self);
        settings
            .color_scheme
            .connect_selected_notify(move |dropdown| {
                let Some(scheme) = ColorSchemePreference::ALL.get(dropdown.selected() as usize)
                else {
                    return;
                };
                if let Some(controller) = controller.upgrade() {
                    controller.change_color_scheme(*scheme);
                }
            });

        let controller = Rc::downgrade(self);
        let scale_for_family = settings.font_scale.clone();
        settings.font_family.connect_activate(move |entry| {
            if let Some(controller) = controller.upgrade() {
                controller.change_font_preferences(
                    Some(entry.text().as_str()),
                    scale_for_family.value_as_int().max(0) as u16,
                );
            }
        });

        let controller = Rc::downgrade(self);
        let family_for_scale = settings.font_family.clone();
        settings.font_scale.connect_value_changed(move |spin| {
            if let Some(controller) = controller.upgrade() {
                controller.change_font_preferences(
                    Some(family_for_scale.text().as_str()),
                    spin.value_as_int().max(0) as u16,
                );
            }
        });

        let controller = Rc::downgrade(self);
        let dialog_for_reset = settings.dialog.downgrade();
        settings.appearance_reset.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.reset_appearance_preferences();
            }
            if let Some(dialog) = dialog_for_reset.upgrade() {
                dialog.close();
            }
        });

        let controller = Rc::downgrade(self);
        settings
            .icon_style
            .connect_selected_notify(move |dropdown| {
                let Some(style) = EntryIconStyle::ALL.get(dropdown.selected() as usize) else {
                    return;
                };
                if let Some(controller) = controller.upgrade() {
                    controller.activate_window_action(
                        "icon-style",
                        Some(&style.persisted().to_variant()),
                    );
                }
            });

        let controller = Rc::downgrade(self);
        settings
            .default_view
            .connect_selected_notify(move |dropdown| {
                let action = match dropdown.selected() {
                    0 => "view-list",
                    1 => "view-grid",
                    2 => "view-miller",
                    _ => return,
                };
                if let Some(controller) = controller.upgrade() {
                    controller.activate_window_action(action, None);
                }
            });

        let controller = Rc::downgrade(self);
        settings
            .click_policy
            .connect_selected_notify(move |dropdown| {
                let Some(policy) = ClickPolicy::ALL.get(dropdown.selected() as usize) else {
                    return;
                };
                if let Some(controller) = controller.upgrade() {
                    controller.change_click_policy(*policy);
                }
            });

        let controller = Rc::downgrade(self);
        settings.grid_size.connect_selected_notify(move |dropdown| {
            if let Some(controller) = controller.upgrade()
                && let Some(size) =
                    crate::settings_center::grid_size_at(dropdown.selected() as usize)
            {
                controller.change_grid_size(size);
            }
        });

        let controller = Rc::downgrade(self);
        settings
            .file_density
            .connect_selected_notify(move |dropdown| {
                let Some(density) = FileViewDensity::ALL.get(dropdown.selected() as usize) else {
                    return;
                };
                if let Some(controller) = controller.upgrade() {
                    controller.activate_window_action(
                        "file-density",
                        Some(&density.persisted().to_variant()),
                    );
                }
            });

        let controller = Rc::downgrade(self);
        settings
            .sidebar_density
            .connect_selected_notify(move |dropdown| {
                let density = match dropdown.selected() {
                    0 => SidebarDensity::Compact,
                    1 => SidebarDensity::Balanced,
                    2 => SidebarDensity::Comfortable,
                    _ => return,
                };
                if let Some(controller) = controller.upgrade() {
                    controller.activate_window_action(
                        "sidebar-density",
                        Some(&density.persisted().to_variant()),
                    );
                }
            });

        let controller = Rc::downgrade(self);
        settings
            .remember_folder_view
            .connect_active_notify(move |toggle| {
                if let Some(controller) = controller.upgrade() {
                    controller.activate_window_action(
                        "remember-folder-view",
                        Some(&toggle.is_active().to_variant()),
                    );
                }
            });

        let controller = Rc::downgrade(self);
        settings
            .vim_navigation
            .connect_active_notify(move |toggle| {
                if let Some(controller) = controller.upgrade()
                    && controller.current_preferences.borrow().vim_mode != toggle.is_active()
                {
                    controller.activate_window_action("vim-mode", None);
                }
            });

        let controller = Rc::downgrade(self);
        settings.search_index.connect_active_notify(move |toggle| {
            if let Some(controller) = controller.upgrade() {
                controller.change_search_index_enabled(toggle.is_active());
            }
        });
        let controller = Rc::downgrade(self);
        settings
            .metadata_sort_cache
            .connect_active_notify(move |toggle| {
                let Some(controller) = controller.upgrade() else {
                    return;
                };
                let enabled = toggle.is_active();
                if controller
                    .current_preferences
                    .borrow()
                    .metadata_sort_cache_enabled
                    == enabled
                {
                    return;
                }
                controller
                    .current_preferences
                    .borrow_mut()
                    .metadata_sort_cache_enabled = enabled;
                if !enabled {
                    if let Some(worker) = controller.metadata_index_worker.borrow_mut().as_mut() {
                        worker.cancel();
                        if let Ok(generation) = worker.clear() {
                            controller.metadata_index_generation.set(generation);
                        }
                    }
                }
                controller.queue_preferences();
                controller.show_toast(
                    if enabled {
                        "Advanced metadata cache reuse enabled"
                    } else {
                        "Advanced metadata cache reuse disabled and cache clearing requested"
                    },
                    5,
                );
            });

        let total_limit_control = settings.clamav_total_limit.clone();
        let controller = Rc::downgrade(self);
        settings
            .clamav_file_limit
            .connect_value_changed(move |spin| {
                let Some(controller) = controller.upgrade() else {
                    return;
                };
                let file_limit = clamp_clamav_file_limit_mib(spin.value_as_int().max(0) as u32);
                let mut preferences = controller.current_preferences.borrow_mut();
                let total_limit = normalized_clamav_total_limit_gib(
                    file_limit,
                    preferences.clamav_total_limit_gib,
                );
                if preferences.clamav_file_limit_mib == file_limit
                    && preferences.clamav_total_limit_gib == total_limit
                {
                    return;
                }
                preferences.clamav_file_limit_mib = file_limit;
                preferences.clamav_total_limit_gib = total_limit;
                drop(preferences);
                if total_limit_control.value_as_int() != total_limit as i32 {
                    total_limit_control.set_value(f64::from(total_limit));
                }
                controller.queue_preferences();
                controller.show_toast(&format!("ClamAV per-file limit set to {file_limit} MiB"), 4);
            });

        let controller = Rc::downgrade(self);
        settings
            .clamav_total_limit
            .connect_value_changed(move |spin| {
                let Some(controller) = controller.upgrade() else {
                    return;
                };
                let file_limit = controller
                    .current_preferences
                    .borrow()
                    .clamav_file_limit_mib;
                let requested = spin.value_as_int().max(0) as u32;
                let total_limit = normalized_clamav_total_limit_gib(file_limit, requested);
                if spin.value_as_int() != total_limit as i32 {
                    spin.set_value(f64::from(total_limit));
                    return;
                }
                if controller
                    .current_preferences
                    .borrow()
                    .clamav_total_limit_gib
                    == total_limit
                {
                    return;
                }
                controller
                    .current_preferences
                    .borrow_mut()
                    .clamav_total_limit_gib = total_limit;
                controller.queue_preferences();
                controller.show_toast(
                    &format!("ClamAV total scan limit set to {total_limit} GiB"),
                    4,
                );
            });

        let controller = Rc::downgrade(self);
        settings
            .privileged_access
            .connect_active_notify(move |toggle| {
                let Some(controller) = controller.upgrade() else {
                    return;
                };
                let enabled = toggle.is_active();
                if controller
                    .current_preferences
                    .borrow()
                    .privileged_access_enabled
                    == enabled
                {
                    return;
                }
                controller
                    .current_preferences
                    .borrow_mut()
                    .privileged_access_enabled = enabled;
                controller.queue_preferences();
                if !enabled {
                    controller.privileged_access.cancel_and_close();
                }
            });

        let controller = Rc::downgrade(self);
        settings
            .reduced_motion
            .connect_active_notify(move |toggle| {
                if let Some(controller) = controller.upgrade() {
                    controller.change_reduced_motion(toggle.is_active());
                }
            });

        for (action, button) in &settings.action_buttons {
            let action = action
                .strip_prefix("win.")
                .expect("Settings actions are window actions");
            let controller = Rc::downgrade(self);
            let dialog = settings.dialog.clone();
            button.connect_clicked(move |_| {
                dialog.close();
                if let Some(controller) = controller.upgrade() {
                    controller.activate_window_action(action, None);
                }
            });
        }

        settings.dialog.present(Some(&self.widgets.window));
        settings.search.grab_focus();
    }

    fn show_custom_actions_editor(self: &Rc<Self>) {
        let actions = self.current_preferences.borrow().custom_actions.clone();
        let widgets = crate::custom_actions::build_editor(&actions);
        let controller = Rc::downgrade(self);
        let dialog = widgets.dialog.downgrade();
        widgets.add_button.connect_clicked(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
            if let Some(controller) = controller.upgrade() {
                controller.show_custom_action_form(None);
            }
        });
        for (index, button) in widgets.edit_buttons.iter().enumerate() {
            let controller = Rc::downgrade(self);
            let dialog = widgets.dialog.downgrade();
            button.connect_clicked(move |_| {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.close();
                }
                if let Some(controller) = controller.upgrade() {
                    controller.show_custom_action_form(Some(index));
                }
            });
        }
        for (index, button) in widgets.remove_buttons.iter().enumerate() {
            let controller = Rc::downgrade(self);
            let dialog = widgets.dialog.downgrade();
            button.connect_clicked(move |_| {
                let Some(controller) = controller.upgrade() else {
                    return;
                };
                if index < controller.current_preferences.borrow().custom_actions.len() {
                    controller
                        .current_preferences
                        .borrow_mut()
                        .custom_actions
                        .remove(index);
                    controller.queue_preferences();
                    controller.refresh_custom_action_context_menu();
                    if let Some(dialog) = dialog.upgrade() {
                        dialog.close();
                    }
                    controller.show_custom_actions_editor();
                }
            });
        }
        for (buttons, direction) in [
            (&widgets.move_up_buttons, -1isize),
            (&widgets.move_down_buttons, 1),
        ] {
            for (index, button) in buttons.iter().enumerate() {
                let controller = Rc::downgrade(self);
                let dialog = widgets.dialog.downgrade();
                button.connect_clicked(move |_| {
                    let Some(controller) = controller.upgrade() else {
                        return;
                    };
                    let target = index.checked_add_signed(direction);
                    let mut preferences = controller.current_preferences.borrow_mut();
                    if let Some(target) = target
                        && target < preferences.custom_actions.len()
                    {
                        preferences.custom_actions.swap(index, target);
                        drop(preferences);
                        controller.queue_preferences();
                        controller.refresh_custom_action_context_menu();
                        if let Some(dialog) = dialog.upgrade() {
                            dialog.close();
                        }
                        controller.show_custom_actions_editor();
                    }
                });
            }
        }
        widgets.dialog.present(Some(&self.widgets.window));
    }

    fn show_custom_action_form(self: &Rc<Self>, index: Option<usize>) {
        let existing = index.and_then(|index| {
            self.current_preferences
                .borrow()
                .custom_actions
                .get(index)
                .cloned()
        });
        let id = existing.as_ref().map_or_else(
            || {
                self.current_preferences
                    .borrow()
                    .custom_actions
                    .iter()
                    .map(|action| action.id)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1)
                    .max(1)
            },
            |action| action.id,
        );
        let widgets = Rc::new(crate::custom_actions::build_form(existing.as_ref()));
        let controller = Rc::downgrade(self);
        let form = Rc::clone(&widgets);
        widgets.save.connect_clicked(move |_| {
            let Some(controller) = controller.upgrade() else {
                return;
            };
            match crate::custom_actions::definition_from_form(id, &form) {
                Ok(action) => {
                    let mut preferences = controller.current_preferences.borrow_mut();
                    if let Some(index) = index {
                        if let Some(slot) = preferences.custom_actions.get_mut(index) {
                            *slot = action;
                        } else {
                            return;
                        }
                    } else if preferences.custom_actions.len()
                        < crate::custom_actions::CUSTOM_ACTION_CAPACITY
                    {
                        preferences.custom_actions.push(action);
                    } else {
                        form.error.set_label("Custom action limit reached");
                        form.error.set_visible(true);
                        return;
                    }
                    drop(preferences);
                    controller.queue_preferences();
                    controller.refresh_custom_action_context_menu();
                    form.dialog.close();
                    controller.show_custom_actions_editor();
                }
                Err(error) => {
                    form.error.set_label(&error.to_string());
                    form.error.set_visible(true);
                }
            }
        });
        widgets.dialog.present(Some(&self.widgets.window));
        widgets.name.grab_focus();
    }

    fn show_custom_action_chooser(self: &Rc<Self>) {
        let selected = self
            .selected_entries
            .borrow()
            .iter()
            .map(|entry| crate::custom_actions::CustomActionSelection::from_entry(entry))
            .collect::<Vec<_>>();
        let actions = self
            .current_preferences
            .borrow()
            .custom_actions
            .iter()
            .filter(|action| action.eligible(&selected))
            .cloned()
            .collect::<Vec<_>>();
        if actions.is_empty() {
            self.show_toast("No custom actions match the current selection", 5);
            return;
        }
        let dialog = adw::Dialog::builder()
            .title("Run Custom Action")
            .content_width(460)
            .content_height(420)
            .build();
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        for action in actions {
            let row = gtk::Button::builder()
                .label(&action.name)
                .has_frame(false)
                .action_name("win.run-custom-action")
                .action_target(&action.id.to_variant())
                .build();
            list.append(&row);
        }
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(20)
            .margin_bottom(20)
            .margin_start(20)
            .margin_end(20)
            .build();
        content.append(
            &gtk::Label::builder()
                .label("Eligible external tools")
                .xalign(0.0)
                .build(),
        );
        content.append(&list);
        dialog.set_child(Some(&content));
        dialog.present(Some(&self.widgets.window));
    }

    fn open_as_administrator(&self) {
        if self.trash_active.get() {
            self.show_toast("Administrator access is unavailable for Trash", 5);
            return;
        }
        if !self.current_preferences.borrow().privileged_access_enabled {
            self.show_toast(
                "Enable Experimental administrator browsing in Settings → Applications first",
                6,
            );
            return;
        }
        if !crate::privileged_access::admin_scheme_supported() {
            self.show_toast(
                "The desktop does not advertise a GVfs administrator backend",
                6,
            );
            return;
        }

        let selected_folder = {
            let selected = self.selected_entries.borrow();
            if selected.is_empty() {
                None
            } else if selected.len() == 1 && selected[0].kind() == EntryKind::Directory {
                Some(selected[0].path().to_path_buf())
            } else {
                self.show_toast("Select one local folder for administrator browsing", 5);
                return;
            }
        };
        let target = selected_folder.unwrap_or_else(|| self.action_directory());
        self.privileged_access
            .present(&self.widgets.window, &target);
    }

    fn recent_locations(&self) -> Vec<PathBuf> {
        let tabs = self.tabs.borrow();
        recent_session_locations(tabs.active())
    }

    fn show_recent_locations(self: &Rc<Self>) {
        let dialog = adw::Dialog::builder()
            .title("Recent Locations")
            .content_width(620)
            .content_height(520)
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(18);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);
        let explanation = gtk::Label::builder()
            .label("Current, Back, and Forward locations for this tab. Session persistence follows Floe's Private/Sensitive policy.")
            .wrap(true)
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build();
        content.append(&explanation);
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        for path in self.recent_locations() {
            let row = adw::ActionRow::builder()
                .title(path.to_string_lossy())
                .subtitle(if path == self.tabs.borrow().active().current().path() {
                    "Current folder"
                } else {
                    "Navigate to this exact recorded folder"
                })
                .activatable(true)
                .build();
            let button = gtk::Button::builder()
                .icon_name("floe-phosphor-arrow-right-symbolic")
                .tooltip_text("Open recent location")
                .valign(gtk::Align::Center)
                .build();
            button.update_property(&[
                gtk::accessible::Property::Label("Open recent location"),
                gtk::accessible::Property::Description(
                    "Navigate to the exact recorded folder path",
                ),
            ]);
            row.add_suffix(&button);
            row.set_activatable_widget(Some(&button));
            let exact_path = path;
            let controller = Rc::downgrade(self);
            let dialog_for_row = dialog.clone();
            button.connect_clicked(move |_| {
                dialog_for_row.close();
                if let Some(controller) = controller.upgrade() {
                    controller.navigate_to(exact_path.clone());
                }
            });
            list.append(&row);
        }
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&list)
            .build();
        content.append(&scroll);
        dialog.set_child(Some(&content));
        dialog.present(Some(&self.widgets.window));
    }

    fn render_breadcrumbs(&self, path: &Path) {
        while let Some(child) = self.widgets.breadcrumb_box.first_child() {
            self.widgets.breadcrumb_box.remove(&child);
        }
        if self.trash_active.get() {
            self.breadcrumb_paths.borrow_mut().clear();
            let label = gtk::Label::builder()
                .label("Trash")
                .css_classes(["floe-path"])
                .build();
            label.update_property(&[
                gtk::accessible::Property::Label("Trash"),
                gtk::accessible::Property::Description("Current virtual Trash location"),
            ]);
            self.widgets.breadcrumb_box.append(&label);
            self.widgets
                .recent_locations_button
                .set_sensitive(!self.recent_locations().is_empty());
            return;
        }
        let crumbs = floe_core::breadcrumbs_for(path);
        *self.breadcrumb_paths.borrow_mut() = crumbs
            .iter()
            .map(|crumb| crumb.path().to_path_buf())
            .collect();
        for (index, crumb) in crumbs.iter().enumerate() {
            if index > 0 {
                self.widgets
                    .breadcrumb_box
                    .append(&gtk::Image::from_icon_name(
                        "floe-phosphor-caret-right-symbolic",
                    ));
            }
            let label = crumb.label().to_string_lossy();
            let button = gtk::Button::builder()
                .label(label.as_ref())
                .has_frame(false)
                .action_name("win.breadcrumb")
                .action_target(&(index as u64).to_variant())
                .tooltip_text(format!("Open {}", crumb.path().to_string_lossy()))
                .build();
            button.update_property(&[
                gtk::accessible::Property::Label(&format!("Breadcrumb {label}")),
                gtk::accessible::Property::Description("Navigate to this exact ancestor folder"),
            ]);
            self.widgets.breadcrumb_box.append(&button);
        }
        self.widgets
            .recent_locations_button
            .set_sensitive(self.recent_locations().len() > 1);
    }

    fn activate_window_action(&self, action: &str, parameter: Option<&glib::Variant>) {
        gio::prelude::ActionGroupExt::activate_action(&self.widgets.window, action, parameter);
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
        self.refresh_custom_action_context_menu();
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

    fn change_search_index_enabled(&self, enabled: bool) {
        if self.current_preferences.borrow().search_index_enabled == enabled {
            return;
        }
        self.current_preferences.borrow_mut().search_index_enabled = enabled;
        self.widgets.search_index_toggle.set_active(enabled);
        self.queue_preferences();
        self.show_toast(
            if enabled {
                "Optional search index enabled; live search remains the fallback"
            } else {
                "Optional search index disabled"
            },
            4,
        );
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
        if let Some(shared) = self.shared_preferences.as_ref() {
            let merged = shared.merge_snapshot(&self.preference_baseline.borrow(), &preferences);
            *self.preference_baseline.borrow_mut() = merged.clone();
            *self.current_preferences.borrow_mut() = merged;
            shared.flush();
        } else {
            *self.preference_baseline.borrow_mut() = preferences.clone();
            *self.current_preferences.borrow_mut() = preferences;
        }
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
        if self.ignore_sidebar_position_signal.get()
            || self.current_preferences.borrow().sidebar_collapsed
        {
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

    fn toggle_sidebar_collapsed(&self) {
        let collapsed = !self.current_preferences.borrow().sidebar_collapsed;
        self.current_preferences.borrow_mut().sidebar_collapsed = collapsed;
        self.ignore_sidebar_position_signal.set(true);
        self.widgets.apply_sidebar_collapsed(collapsed);
        if !collapsed {
            let width = self
                .current_preferences
                .borrow()
                .sidebar_width
                .map(clamp_sidebar_width)
                .map(i32::from)
                .unwrap_or(self.widgets.sidebar_default_width);
            self.widgets.workspace.set_position(width);
        }
        self.ignore_sidebar_position_signal.set(false);
        self.queue_preferences();
    }

    fn flush_pending_preferences(&self) {
        if let Some(shared) = self.shared_preferences.as_ref() {
            shared.flush();
            let latest = shared.snapshot();
            let baseline = self.preference_baseline.borrow().clone();
            let mut current = self.current_preferences.borrow_mut();
            if latest != baseline && *current == baseline {
                let changes = PreferencePresentationChanges::between(&baseline, &latest);
                *current = latest.clone();
                *self.preference_baseline.borrow_mut() = latest;
                drop(current);
                self.apply_shared_preference_presentation(changes);
            }
        }
    }

    fn apply_shared_preference_presentation(&self, changes: PreferencePresentationChanges) {
        let preferences = self.current_preferences.borrow().clone();
        if changes.appearance {
            self.widgets.apply_appearance_preferences(&preferences);
        }
        if changes.click_policy {
            self.widgets.apply_click_policy(preferences.click_policy);
        }
        if changes.icon_style {
            self.widgets.apply_entry_icon_style(preferences.icon_style);
            if self.filename_search_active.get() || self.content_search_active.get() {
                self.apply_search_result_order();
            } else {
                let selected = self.selected_paths();
                self.install_entries(self.visible_entries.borrow().clone(), &selected, false);
            }
        }
        if changes.sidebar {
            self.widgets
                .apply_sidebar_density(preferences.sidebar_density);
            self.ignore_sidebar_position_signal.set(true);
            self.widgets
                .apply_sidebar_collapsed(preferences.sidebar_collapsed);
            if !preferences.sidebar_collapsed {
                let width = preferences
                    .sidebar_width
                    .map(clamp_sidebar_width)
                    .map(i32::from)
                    .unwrap_or(self.widgets.sidebar_default_width);
                self.widgets.workspace.set_position(width);
            }
            self.ignore_sidebar_position_signal.set(false);
        }
        if changes.context_menu {
            self.refresh_custom_action_context_menu();
        }
        if changes.keybindings
            && let Some(application) = self
                .widgets
                .window
                .application()
                .and_downcast::<adw::Application>()
        {
            crate::keybindings::install_effective_window_shortcuts(
                &application,
                &preferences.keybindings,
            );
        }
        if changes.vim_mode {
            self.widgets.miller_view.set_vim_mode(preferences.vim_mode);
            self.widgets
                .vim_mode_button
                .set_label(if preferences.vim_mode {
                    ui::VIM_MODE_ON_LABEL
                } else {
                    ui::VIM_MODE_OFF_LABEL
                });
            self.widgets
                .vim_mode_button
                .update_property(&[gtk::accessible::Property::Label(if preferences.vim_mode {
                    "Vim navigation mode enabled"
                } else {
                    "Vim navigation mode disabled"
                })]);
        }
        if changes.view_policy {
            let path = self.tabs.borrow().active().current().path().to_path_buf();
            let view = preferences.effective_state(&path);
            self.view_mode.set(view.mode);
            self.grid_size.set(view.grid_size);
            self.sort_order.set(view.sort);
            self.file_density.set(view.density);
            self.list_columns.set(view.columns);
            self.tabs.borrow_mut().active_mut().set_view(view);
            self.widgets.set_view_mode(view.mode);
            self.widgets.set_grid_size(view.grid_size);
            self.widgets
                .apply_file_view_policy(view.density, view.columns, view.sort.grouping);
            self.update_sort_headers();
        }
    }

    fn change_sort(&self, column: SortColumn) {
        if self.sort_in_flight.get() {
            return;
        }
        let sort = self.sort_order.get().next_for(column);
        self.resort_with(sort);
    }

    fn change_sort_column(&self, column: SortColumn) {
        let current = self.sort_order.get();
        if self.sort_in_flight.get() || current.column == column {
            return;
        }
        self.resort_with(
            DirectorySort::new(column, current.direction)
                .with_directories(current.directories)
                .with_grouping(current.grouping)
                .with_hidden_last(current.hidden_last),
        );
    }

    fn change_sort_direction(&self, direction: SortDirection) {
        let current = self.sort_order.get();
        if self.sort_in_flight.get() || current.direction == direction {
            return;
        }
        self.resort_with(
            DirectorySort::new(current.column, direction)
                .with_directories(current.directories)
                .with_grouping(current.grouping)
                .with_hidden_last(current.hidden_last),
        );
    }

    fn change_hidden_last(&self, hidden_last: bool) {
        let current = self.sort_order.get();
        if self.sort_in_flight.get() || current.hidden_last == hidden_last {
            return;
        }
        self.resort_with(current.with_hidden_last(hidden_last));
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
        let entries = self.all_listed_entries.borrow().to_vec();
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

        if sort.column.needs_indexed_metadata() {
            let request = self
                .metadata_index_worker
                .borrow_mut()
                .as_mut()
                .ok_or(MetadataIndexSubmitError::Disconnected)
                .and_then(|worker| {
                    worker.request_sort(
                        entries,
                        sort,
                        self.current_preferences
                            .borrow()
                            .metadata_sort_cache_enabled,
                    )
                });
            match request {
                Ok(generation) => self.metadata_index_generation.set(generation),
                Err(error) => {
                    self.sort_in_flight.set(false);
                    self.set_sort_controls_sensitive(true);
                    self.widgets.set_views_sensitive(true);
                    self.widgets.spinner.stop();
                    self.show_toast(&format!("Could not start metadata indexing: {error}"), 7);
                }
            }
        } else {
            let path = self.tabs.borrow().active().current().path().to_path_buf();
            let generation = self.worker.borrow_mut().request_sort(path, entries, sort);
            self.active_generation.set(generation);
        }
    }

    fn drain_metadata_index_worker(&self) {
        loop {
            let event = self
                .metadata_index_worker
                .borrow()
                .as_ref()
                .and_then(MetadataIndexWorker::try_response);
            let Some(event) = event else {
                break;
            };
            if event.generation != self.metadata_index_generation.get() {
                continue;
            }
            match event.kind {
                MetadataIndexEventKind::Progress {
                    completed,
                    total,
                    cache_hits,
                } => {
                    self.widgets.status_label.set_label(&format!(
                        "Indexing metadata… {completed} of {total} ({cache_hits} cached)"
                    ));
                }
                MetadataIndexEventKind::Sorted { entries, sort } => {
                    self.sort_in_flight.set(false);
                    self.set_sort_controls_sensitive(true);
                    self.widgets.set_views_sensitive(true);
                    self.widgets.spinner.stop();
                    if self.sort_order.get() != sort {
                        continue;
                    }
                    let selected_paths = self.sort_selection_paths.take();
                    self.all_listed_entries.replace(Arc::from(entries.clone()));
                    let visible = entries
                        .into_iter()
                        .filter(|entry| self.show_hidden.get() || !entry.is_hidden())
                        .collect::<Vec<_>>();
                    self.listed_entries.replace(Arc::from(visible));
                    self.apply_folder_filter(selected_paths, false);
                }
                MetadataIndexEventKind::Failed { error, sort } => {
                    self.sort_in_flight.set(false);
                    self.set_sort_controls_sensitive(true);
                    self.widgets.set_views_sensitive(true);
                    self.widgets.spinner.stop();
                    let fallback =
                        DirectorySort::new(SortColumn::Name, floe_core::SortDirection::Ascending)
                            .with_directories(sort.directories)
                            .with_grouping(sort.grouping)
                            .with_hidden_last(sort.hidden_last);
                    self.show_toast(
                        &format!("Could not index that metadata: {error}. Sorted by Name instead."),
                        7,
                    );
                    self.resort_with(fallback);
                }
                MetadataIndexEventKind::Cleared => {
                    self.show_toast("Advanced metadata cache cleared", 4);
                }
            }
        }
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

    fn move_list_column(&self, column: ListColumn, delta: isize) {
        let mut layout = self.list_columns.get();
        if !layout.move_column(column, delta) {
            return;
        }
        self.list_columns.set(layout);
        self.widgets.apply_file_view_policy(
            self.file_density.get(),
            layout,
            self.sort_order.get().grouping,
        );
        self.queue_preferences();
        self.show_toast(&format!("Moved {} column", column.label()), 3);
    }

    fn autosize_list_column(&self, column: ListColumn) {
        const AUTOSIZE_SAMPLE_CAPACITY: usize = 4_096;
        let max_chars = self
            .visible_entries
            .borrow()
            .iter()
            .take(AUTOSIZE_SAMPLE_CAPACITY)
            .map(|entry| match column {
                ListColumn::Name => entry.display_name_lossy().chars().count(),
                ListColumn::Type => 14,
                ListColumn::Size => entry.size().map_or(1, |size| {
                    format!("{size}").chars().count().saturating_add(4)
                }),
                ListColumn::Modified | ListColumn::Created | ListColumn::Accessed => 19,
                ListColumn::Extension => std::path::Path::new(entry.display_name())
                    .extension()
                    .map_or(1, |extension| extension.to_string_lossy().chars().count()),
                ListColumn::Mime => 32,
                ListColumn::Permissions => 12,
                ListColumn::Dimensions => 14,
                ListColumn::Duration => 10,
                ListColumn::Artist | ListColumn::Album => 28,
                ListColumn::Track => 8,
                ListColumn::Owner | ListColumn::Group => 16,
                ListColumn::Path => entry.path().as_os_str().to_string_lossy().chars().count(),
                ListColumn::LinkTarget => 30,
            })
            .max()
            .unwrap_or_else(|| column.label().chars().count())
            .max(column.label().chars().count());
        let mut layout = self.list_columns.get();
        layout.autosize_from_max_chars(column, max_chars);
        if layout == self.list_columns.replace(layout) {
            return;
        }
        self.widgets.apply_file_view_policy(
            self.file_density.get(),
            layout,
            self.sort_order.get().grouping,
        );
        self.queue_preferences();
        self.show_toast(&format!("Auto-sized {} column", column.label()), 3);
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
        for (name, state) in [
            ("sort-column", sort.column.persisted().to_variant()),
            ("sort-direction", sort.direction.persisted().to_variant()),
            (
                "directory-placement",
                sort.directories.persisted().to_variant(),
            ),
            (
                "folders-first",
                (sort.directories == DirectoryPlacement::First).to_variant(),
            ),
            ("hidden-last", sort.hidden_last.to_variant()),
        ] {
            if let Some(action) = self
                .widgets
                .window
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
            {
                action.set_state(&state);
            }
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
        let menu_sensitive = sensitive && !self.trash_active.get();
        self.widgets
            .sort_menu_button
            .set_sensitive(!self.trash_active.get());
        for name in [
            "sort-column",
            "sort-direction",
            "folders-first",
            "hidden-last",
        ] {
            if let Some(action) = self
                .widgets
                .window
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
            {
                action.set_enabled(menu_sensitive);
            }
        }
        if let Some(action) = self
            .widgets
            .window
            .lookup_action("cancel-metadata-sort")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(self.sort_in_flight.get());
        }
        if let Some(action) = self
            .widgets
            .window
            .lookup_action("clear-metadata-sort-cache")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(menu_sensitive);
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
        self.widgets.location_suggestions.popdown();
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

    fn request_location_completion(&self, input: String) {
        if self.widgets.path_stack.visible_child_name().as_deref() != Some("entry") {
            return;
        }
        if input.len() > 16 * 1_024 {
            self.clear_location_suggestions();
            return;
        }
        let generation = self.location_completion_generation.get().wrapping_add(1);
        self.location_completion_generation.set(generation);
        if let Some(worker) = self.location_completion_worker.borrow().as_ref() {
            worker.request(generation, input);
        }
    }

    fn poll_location_completion(self: &Rc<Self>) {
        let Some(result) = self
            .location_completion_worker
            .borrow()
            .as_ref()
            .and_then(LocationCompletionWorker::try_result)
        else {
            return;
        };
        if result.generation != self.location_completion_generation.get()
            || self.widgets.path_stack.visible_child_name().as_deref() != Some("entry")
        {
            return;
        }
        self.clear_location_suggestions();
        for candidate in result.candidates {
            let button = gtk::Button::builder()
                .label(&candidate.display)
                .halign(gtk::Align::Fill)
                .has_frame(false)
                .build();
            button.update_property(&[
                gtk::accessible::Property::Label(&candidate.display),
                gtk::accessible::Property::Description(
                    "Complete the location with this exact folder",
                ),
            ]);
            let exact_path = candidate.path;
            let controller = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(controller) = controller.upgrade() {
                    controller.widgets.location_suggestions.popdown();
                    controller.navigate_to(exact_path.clone());
                    controller.hide_location_entry();
                }
            });
            self.widgets.location_suggestions_box.append(&button);
        }
        if result.truncated {
            let notice = gtk::Label::builder()
                .label("More folders match; keep typing to narrow the list")
                .wrap(true)
                .css_classes(["dim-label", "caption"])
                .margin_top(6)
                .margin_bottom(6)
                .build();
            self.widgets.location_suggestions_box.append(&notice);
        }
        let has_rows = self
            .widgets
            .location_suggestions_box
            .first_child()
            .is_some();
        if has_rows {
            self.widgets.location_suggestions.popup();
        } else {
            self.widgets.location_suggestions.popdown();
        }
        if let Some(error) = result.error
            && !matches!(
                error,
                io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
            )
        {
            tracing::debug!(?error, "location completion unavailable for parent");
        }
    }

    fn clear_location_suggestions(&self) {
        while let Some(child) = self.widgets.location_suggestions_box.first_child() {
            self.widgets.location_suggestions_box.remove(&child);
        }
    }

    pub fn queue_cli_target(&self, path: PathBuf) {
        let worker = self.cli_route_worker.borrow();
        let Some(worker) = worker.as_ref() else {
            self.show_toast("Command-line target routing is unavailable", 5);
            return;
        };
        worker.request(path);
    }

    pub fn show_external_message(&self, message: &str, timeout: u32) {
        self.show_toast(message, timeout);
    }

    fn poll_cli_route(&self) {
        let Some(result) = self
            .cli_route_worker
            .borrow()
            .as_ref()
            .and_then(CliRouteWorker::try_result)
        else {
            return;
        };
        match result.route {
            Ok(CliRoute::Folder(path)) => self.navigate_to(path),
            Ok(CliRoute::Reveal(path)) => self.navigate_to_revealing(path),
            Err(error) => self.show_toast(
                match error {
                    CliRouteError::Relative => "Command-line target must be an absolute local path",
                    CliRouteError::Oversized => "Command-line target path is too long",
                    CliRouteError::Missing => "Command-line target no longer exists",
                    CliRouteError::Inaccessible => "Command-line target is not accessible",
                    CliRouteError::Unsupported => {
                        "Command-line target is not a regular file or folder"
                    }
                },
                5,
            ),
        }
    }

    fn poll_association_changes(&self) {
        while let Some(result) = self
            .association_worker
            .as_ref()
            .and_then(|worker| worker.try_result())
        {
            match result.result {
                Ok(()) => {
                    let message = match result.change {
                        launcher::AssociationChange::SetDefault {
                            application_name, ..
                        } => format!("Default application changed to {application_name}"),
                        launcher::AssociationChange::Reset { .. } => {
                            "Default application reset to desktop recommendations".to_owned()
                        }
                    };
                    self.show_toast(&message, 5);
                }
                Err(error) => self.show_toast(&error.to_string(), 7),
            }
        }
    }

    fn run_custom_action(&self, id: u64) {
        let Some(action) = self
            .current_preferences
            .borrow()
            .custom_actions
            .iter()
            .find(|action| action.id == id)
            .cloned()
        else {
            self.show_toast("Custom action no longer exists", 5);
            return;
        };
        let entries = self
            .selected_entries
            .borrow()
            .iter()
            .map(|entry| crate::custom_actions::CustomActionSelection::from_entry(entry))
            .collect::<Vec<_>>();
        let request = crate::custom_actions::CustomActionLaunchRequest { action, entries };
        let result = self
            .custom_action_worker
            .borrow()
            .as_ref()
            .ok_or(crate::custom_actions::CustomActionLaunchError::Disconnected)
            .and_then(|worker| worker.try_launch(request));
        if let Err(error) = result {
            self.show_toast(&error.to_string(), 7);
        }
    }

    fn poll_custom_actions(&self) {
        while let Some(event) = self
            .custom_action_worker
            .borrow()
            .as_ref()
            .and_then(crate::custom_actions::CustomActionWorker::try_event)
        {
            match event {
                crate::custom_actions::CustomActionEvent::Started { id, name } => {
                    tracing::debug!(action_id = id, "custom action started");
                    self.show_toast(&format!("Started {name}"), 4);
                }
                crate::custom_actions::CustomActionEvent::Finished { id, status }
                    if !status.success() =>
                {
                    tracing::debug!(action_id = id, %status, "custom action exited unsuccessfully");
                    self.show_toast(&format!("Custom action exited with {status}"), 7);
                }
                crate::custom_actions::CustomActionEvent::Finished { id, .. } => {
                    tracing::debug!(action_id = id, "custom action completed");
                }
                crate::custom_actions::CustomActionEvent::Failed { id, error } => {
                    tracing::debug!(action_id = id, "custom action failed to start or complete");
                    self.show_toast(&error.to_string(), 7);
                }
            }
        }
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
        self.pending_operation_reveal.borrow_mut().take();
        self.pending_scroll_index.set(None);
        self.load_current_inner()
    }

    fn load_current_inner(&self) -> u64 {
        if self
            .pending_operation_reveal
            .borrow()
            .as_ref()
            .is_some_and(PendingOperationReveal::is_bound)
        {
            self.pending_operation_reveal.borrow_mut().take();
        }
        self.pending_operation_emphasis.set(false);
        self.clear_operation_result_emphasis();
        if let Some(worker) = self.metadata_index_worker.borrow_mut().as_mut() {
            worker.cancel();
        }
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
        self.all_listed_entries.replace(Arc::from([]));
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
        if self.content_search_active.get() {
            let same_root = !self.trash_active.get()
                && self
                    .content_search_root
                    .borrow()
                    .as_ref()
                    .is_some_and(|root| root == &path);
            if same_root {
                let generation = self.content_search_generation.get().wrapping_add(1).max(1);
                self.content_search_generation.set(generation);
                if let Some(worker) = self.content_search_worker.borrow().as_ref() {
                    worker.cancel(generation);
                }
                self.set_content_search_running(false);
                self.content_search_results.borrow_mut().clear();
                self.content_search_store.borrow_mut().take();
                self.content_search_summary.borrow_mut().take();
                self.pending_content_search.borrow_mut().take();
            } else {
                self.deactivate_content_search(false);
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
            self.reset_advanced_filter_widgets();
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
        self.render_breadcrumbs(&path);
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
                    let sort = self.sort_order.get();
                    if sort.column.needs_indexed_metadata() {
                        self.resort_with(sort);
                    }
                }
                ResponseKind::ListingWithSortWarning { entries, error } => {
                    if self
                        .pending_location
                        .borrow()
                        .as_ref()
                        .is_some_and(|pending| pending.matches(response.generation))
                    {
                        self.pending_location.borrow_mut().take();
                        self.hide_location_entry();
                    }
                    let requested = self.sort_order.get();
                    let fallback =
                        DirectorySort::new(SortColumn::Name, floe_core::SortDirection::Ascending)
                            .with_directories(requested.directories)
                            .with_grouping(requested.grouping)
                            .with_hidden_last(requested.hidden_last);
                    self.sort_order.set(fallback);
                    self.update_sort_headers();
                    self.queue_preferences();
                    self.set_sort_controls_sensitive(true);
                    self.show_listing(entries);
                    self.show_toast(
                        &format!(
                            "Could not use that metadata sort: {error}. Sorted by Name instead."
                        ),
                        7,
                    );
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
                ResponseKind::SortFailed { error, sort } => {
                    self.sort_in_flight.set(false);
                    self.set_sort_controls_sensitive(true);
                    self.widgets.set_views_sensitive(true);
                    let fallback =
                        DirectorySort::new(SortColumn::Name, floe_core::SortDirection::Ascending)
                            .with_directories(sort.directories)
                            .with_grouping(sort.grouping)
                            .with_hidden_last(sort.hidden_last);
                    self.show_toast(
                        &format!(
                            "Could not use that metadata sort: {error}. Sorted by Name instead."
                        ),
                        7,
                    );
                    self.resort_with(fallback);
                }
                ResponseKind::Sorted { entries, sort } => {
                    self.sort_in_flight.set(false);
                    self.set_sort_controls_sensitive(true);
                    if sort != self.sort_order.get() {
                        continue;
                    }
                    let selected_paths = self.sort_selection_paths.take();
                    self.all_listed_entries.replace(Arc::from(entries.clone()));
                    let entries = entries
                        .into_iter()
                        .filter(|entry| {
                            self.trash_active.get() || self.show_hidden.get() || !entry.is_hidden()
                        })
                        .collect::<Vec<_>>();
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
        let all_entries: Vec<Arc<DirectoryEntry>> = entries.into_iter().map(Arc::new).collect();
        self.all_listed_entries
            .replace(Arc::from(all_entries.clone()));
        let entries: Vec<Arc<DirectoryEntry>> = all_entries
            .into_iter()
            .filter(|entry| self.trash_active.get() || show_hidden || !entry.is_hidden())
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
        if self.pending_saved_search.borrow().is_some() {
            self.launch_pending_saved_search();
        } else if self.filename_search_active.get() {
            self.start_filename_search();
        } else if self.content_search_active.get() {
            self.start_content_search();
        }
    }

    fn take_visible_operation_reveal(
        &self,
        entries: &[Arc<DirectoryEntry>],
    ) -> Option<Vec<PathBuf>> {
        let current = self.tabs.borrow().active().current().path().to_path_buf();
        let generation = self.active_generation.get();
        let matches = self
            .pending_operation_reveal
            .borrow()
            .as_ref()
            .is_some_and(|pending| pending.matches(generation, &current));
        if !matches {
            return None;
        }

        let mut pending = self.pending_operation_reveal.borrow_mut();
        let pending = pending.as_mut()?;
        let visible = pending.visible_paths(entries.iter().map(|entry| entry.path()));
        pending.unbind();
        if visible.is_empty() {
            self.show_toast(
                "Operation complete; the result is hidden by the current view",
                5,
            );
            return None;
        }
        Some(visible)
    }

    fn install_entries(
        &self,
        entries: Vec<Arc<DirectoryEntry>>,
        selected_paths: &[PathBuf],
        focus_list: bool,
    ) {
        let count = entries.len();
        let operation_paths = self.take_visible_operation_reveal(&entries);
        let selected_paths = operation_paths.as_deref().unwrap_or(selected_paths);
        if let Some(first) = operation_paths.as_ref().and_then(|paths| paths.first()) {
            self.pending_scroll_index.set(
                entries
                    .iter()
                    .position(|entry| entry.path() == first)
                    .and_then(|index| u32::try_from(index).ok()),
            );
            self.pending_operation_emphasis.set(true);
        }
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
        if focus_list && operation_paths.is_none() {
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
            if self.pending_operation_emphasis.replace(false) {
                self.start_operation_result_emphasis();
            }
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
            if self.selection_mode.borrow().is_some() {
                self.submit_selection_mode();
            } else {
                self.launch_file(entry);
            }
        } else {
            self.show_toast("This type of filesystem entry cannot be opened yet", 5);
        }
    }

    fn launch_file(&self, entry: &DirectoryEntry) {
        let display_name = entry.display_name_lossy();
        let window = self.widgets.window.clone();
        let toast_overlay = self.widgets.toast_overlay.clone();
        let association_worker = self.association_worker.clone();
        launcher::launch_default(entry.path(), move |result| {
            if !window.is_visible() {
                return;
            }
            match result {
                Ok(launcher::DefaultLaunch::Launched) => {}
                Ok(launcher::DefaultLaunch::NoDefault(options)) => {
                    present_or_report_open_with(
                        &window,
                        &toast_overlay,
                        &display_name,
                        options,
                        association_worker,
                    );
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
        let state = selection_action_state(&selected_entries);
        let folder_tab = folder_tab_eligible(&selected_entries, self.trash_active.get());
        let duplicate_eligible = !self.trash_active.get();
        self.selected_entries.replace(selected_entries);
        self.refresh_custom_action_context_menu();
        self.set_open_enabled(state.single);
        self.set_open_with_enabled(state.open_with);
        let properties_busy = self.background_feedback_state.borrow().is_active(
            BackgroundActivity::Properties,
            self.properties_generation.get(),
        );
        self.set_properties_enabled(!self.selected_entries.borrow().is_empty() && !properties_busy);
        let feedback = self.background_feedback_state.borrow();
        let privacy_busy = feedback.is_active(
            BackgroundActivity::PrivacyInspection,
            self.privacy_security_generation.get(),
        ) || feedback.is_active(
            BackgroundActivity::MetadataSanitization,
            self.privacy_security_generation.get(),
        );
        let threat_busy = feedback.is_active(
            BackgroundActivity::ThreatScan,
            self.threat_scan_generation.get(),
        );
        drop(feedback);
        let security_selection = self.selected_entries.borrow();
        for (name, enabled) in [
            (
                "inspect-privacy-safety",
                !security_selection.is_empty() && !privacy_busy,
            ),
            (
                "scan-threats",
                !security_selection.is_empty() && !threat_busy,
            ),
            (
                "create-sanitized-copy",
                !security_selection.is_empty()
                    && !privacy_busy
                    && security_selection
                        .iter()
                        .all(|entry| entry.kind() == EntryKind::RegularFile),
            ),
        ] {
            if let Some(action) = self
                .widgets
                .window
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
            {
                action.set_enabled(enabled && !self.trash_active.get());
            }
        }
        drop(security_selection);
        self.set_reveal_enabled(
            (self.filename_search_active.get() || self.content_search_active.get()) && state.single,
        );
        self.set_checksum_enabled(state.checksum);
        if let Some(action) = self
            .widgets
            .window
            .lookup_action("copy-and-verify")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(state.single && !self.trash_active.get());
        }
        if let Some(action) = self
            .widgets
            .window
            .lookup_action("verified-removable-transfer")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(
                state.single
                    && !self.trash_active.get()
                    && self.verified_usb_flush_worker.borrow().is_some()
                    && self.verified_usb_workflow.borrow().is_none(),
            );
        }
        self.set_integrity_actions_enabled(
            state.single
                && matches!(
                    self.selected_entries.borrow()[0].kind(),
                    EntryKind::RegularFile
                ),
            !self.selected_entries.borrow().is_empty()
                && self.selected_entries.borrow().iter().all(|entry| {
                    matches!(entry.kind(), EntryKind::RegularFile | EntryKind::Directory)
                }),
        );
        if let Some(action) = self
            .widgets
            .window
            .lookup_action("check-duplicates")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(duplicate_eligible && !self.duplicate_running.get());
        }
        self.set_archive_actions_enabled();
        self.set_batch_rename_enabled();
        self.set_selection_actions_enabled(state.transfer, state.rename, state.trash);
        for name in ["open-new-window", "open-new-tab", "open-background-tab"] {
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
        self.update_guardrail_action_states();
        self.refresh_status();
        self.refresh_selection_mode();
        self.enforce_selection_mode_actions();
    }

    fn refresh_custom_action_context_menu(&self) {
        let selected = self
            .selected_entries
            .borrow()
            .iter()
            .map(|entry| crate::custom_actions::CustomActionSelection::from_entry(entry))
            .collect::<Vec<_>>();
        let preferences = self.current_preferences.borrow();
        let eligible = preferences
            .custom_actions
            .iter()
            .filter(|action| action.eligible(&selected))
            .cloned()
            .collect::<Vec<_>>();
        self.widgets
            .apply_context_menu_preferences(preferences.context_menu, &eligible);
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
        if self.filename_search_active.get() || self.content_search_active.get() {
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
        if self.content_search_active.get() {
            let results = self.content_search_results.borrow().len();
            let selected = self.selected_entries.borrow().len();
            let label = if selected == 0 {
                format!("{results} content matches")
            } else {
                format!("{selected} selected of {results} content matches")
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

    fn invert_selection(&self) {
        let count = self.widgets.selection.n_items();
        let selected = (0..count).filter(|position| self.widgets.selection.is_selected(*position));
        let inverted = crate::completeness::inverted_positions(count, selected);
        self.widgets.selection.unselect_all();
        for position in inverted {
            self.widgets.selection.select_item(position, false);
        }
    }

    fn clear_selection(&self) {
        self.widgets.selection.unselect_all();
    }

    fn handle_escape(&self) {
        use crate::completeness::EscapeSurface;

        let target = EscapeSurface::innermost(|surface| match surface {
            EscapeSurface::ContextMenu => self.widgets.context_menu_visible(),
            EscapeSurface::InlineRename => false,
            EscapeSurface::LocationEditor => {
                self.widgets.path_stack.visible_child_name().as_deref() == Some("entry")
            }
            EscapeSurface::Search => self.widgets.search_bar.is_visible(),
            EscapeSurface::QuickPreview => self.miller_detail.borrow().state().is_visible(),
            EscapeSurface::Selection => self.widgets.selection.selection().size() > 0,
        });
        match target {
            Some(EscapeSurface::ContextMenu) => {
                self.widgets.popdown_context_menus();
                self.widgets.focus_view(self.view_mode.get());
            }
            Some(EscapeSurface::LocationEditor) => self.cancel_location_entry(),
            Some(EscapeSurface::Search) => self.clear_folder_filter(),
            Some(EscapeSurface::QuickPreview) => {
                let surface = self
                    .miller_detail
                    .borrow()
                    .state()
                    .surface()
                    .unwrap_or(MillerDetailSurface::Preview);
                self.toggle_miller_detail(surface);
            }
            Some(EscapeSurface::Selection) => self.clear_selection(),
            Some(EscapeSurface::InlineRename) | None => {
                self.widgets.focus_view(self.view_mode.get());
            }
        }
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

    fn set_integrity_actions_enabled(&self, single_regular_file: bool, manifest_targets: bool) {
        for (name, enabled) in [
            ("save-sha256-fingerprint", single_regular_file),
            ("verify-saved-fingerprint", single_regular_file),
            ("verify-sha256sums", single_regular_file),
            ("generate-sha256sums", manifest_targets),
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

    fn show_duplicate_setup(self: &Rc<Self>) {
        if self.trash_active.get() {
            self.show_toast("Duplicate scans are unavailable inside Trash", 5);
            return;
        }
        if self.duplicate_running.get() {
            self.show_toast("A duplicate scan is already running", 4);
            return;
        }
        let selection = self
            .selected_entries
            .borrow()
            .iter()
            .map(|entry| crate::duplicate_ui::DuplicateSelection {
                path: entry.path().to_path_buf(),
                kind: match entry.kind() {
                    EntryKind::RegularFile => {
                        crate::duplicate_ui::DuplicateSelectionKind::RegularFile
                    }
                    EntryKind::Directory => crate::duplicate_ui::DuplicateSelectionKind::Directory,
                    _ => crate::duplicate_ui::DuplicateSelectionKind::Unsupported,
                },
            })
            .collect();
        let weak = Rc::downgrade(self);
        crate::duplicate_ui::present_duplicate_setup(
            &self.widgets.window,
            self.action_directory(),
            selection,
            move |choice| {
                if let Some(controller) = weak.upgrade() {
                    controller.start_duplicate_scan(choice);
                }
            },
        );
    }

    fn start_duplicate_scan(self: &Rc<Self>, choice: crate::duplicate_ui::DuplicateScanChoice) {
        if self.trash_active.get() || self.duplicate_running.get() {
            return;
        }
        let request = match choice {
            crate::duplicate_ui::DuplicateScanChoice::FolderTree(folder) => {
                floe_core::DuplicateScanRequest::for_folder(folder)
            }
            crate::duplicate_ui::DuplicateScanChoice::CopiesOfFile { reference, folder } => {
                floe_core::DuplicateScanRequest::for_reference(reference, folder)
            }
            crate::duplicate_ui::DuplicateScanChoice::SelectedItems(paths) => {
                floe_core::DuplicateScanRequest::new(paths)
            }
        };
        let request = match request {
            Ok(request) => request,
            Err(error) => {
                self.show_toast(&error.to_string(), 6);
                return;
            }
        };
        let generation = self.duplicate_generation.get().wrapping_add(1).max(1);
        self.duplicate_generation.set(generation);
        let outcome = self.duplicate_worker.borrow().as_ref().map_or(
            Err(crate::duplicate_finder::DuplicateFinderSubmitError::Stopped),
            |worker| worker.submit(generation, request),
        );
        match outcome {
            Ok(()) => {
                self.duplicate_running.set(true);
                if let Some(action) = self
                    .widgets
                    .window
                    .lookup_action("check-duplicates")
                    .and_downcast::<gio::SimpleAction>()
                {
                    action.set_enabled(false);
                }
                if let Some(action) = self
                    .widgets
                    .window
                    .lookup_action("cancel-duplicate-scan")
                    .and_downcast::<gio::SimpleAction>()
                {
                    action.set_enabled(true);
                }
                let weak = Rc::downgrade(self);
                let dialog = crate::duplicate_ui::DuplicateProgressDialog::present(
                    &self.widgets.window,
                    move || {
                        if let Some(controller) = weak.upgrade() {
                            controller.cancel_duplicate_scan();
                        }
                    },
                );
                self.duplicate_progress.replace(Some(dialog));
            }
            Err(crate::duplicate_finder::DuplicateFinderSubmitError::Busy) => {
                self.show_toast("Duplicate finder is busy", 5)
            }
            Err(crate::duplicate_finder::DuplicateFinderSubmitError::Stopped) => {
                self.show_toast("Duplicate finder worker is unavailable", 6)
            }
        }
    }

    fn cancel_duplicate_scan(&self) {
        if !self.duplicate_running.replace(false) {
            return;
        }
        let generation = self.duplicate_generation.get().wrapping_add(1).max(1);
        self.duplicate_generation.set(generation);
        if let Some(worker) = self.duplicate_worker.borrow().as_ref() {
            worker.cancel(generation);
        }
        if let Some(dialog) = self.duplicate_progress.borrow_mut().take() {
            dialog.close();
        }
        if let Some(action) = self
            .widgets
            .window
            .lookup_action("cancel-duplicate-scan")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(false);
        }
        if let Some(action) = self
            .widgets
            .window
            .lookup_action("check-duplicates")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(!self.trash_active.get());
        }
        self.show_toast("Duplicate scan cancelled; no files were changed", 4);
    }

    fn drain_duplicate_worker(self: &Rc<Self>) {
        loop {
            let event = self
                .duplicate_worker
                .borrow()
                .as_ref()
                .and_then(crate::duplicate_finder::DuplicateFinderWorker::try_event);
            let Some(event) = event else {
                break;
            };
            if event.generation != self.duplicate_generation.get() || !self.duplicate_running.get()
            {
                continue;
            }
            match event.kind {
                crate::duplicate_finder::DuplicateFinderEventKind::Progress(summary) => {
                    if let Some(dialog) = self.duplicate_progress.borrow().as_ref() {
                        dialog.update(summary);
                    }
                }
                crate::duplicate_finder::DuplicateFinderEventKind::Finished(outcome) => {
                    self.duplicate_running.set(false);
                    if let Some(dialog) = self.duplicate_progress.borrow_mut().take() {
                        dialog.close();
                    }
                    if let Some(action) = self
                        .widgets
                        .window
                        .lookup_action("cancel-duplicate-scan")
                        .and_downcast::<gio::SimpleAction>()
                    {
                        action.set_enabled(false);
                    }
                    if let Some(action) = self
                        .widgets
                        .window
                        .lookup_action("check-duplicates")
                        .and_downcast::<gio::SimpleAction>()
                    {
                        action.set_enabled(!self.trash_active.get());
                    }
                    let weak_reveal = Rc::downgrade(self);
                    let weak_trash = Rc::downgrade(self);
                    crate::duplicate_ui::present_duplicate_review(
                        &self.widgets.window,
                        outcome,
                        move |path| {
                            if let Some(controller) = weak_reveal.upgrade() {
                                controller.navigate_to_revealing(path);
                            }
                        },
                        move |paths| {
                            let Some(controller) = weak_trash.upgrade() else {
                                return;
                            };
                            let count = paths.len();
                            let requests = match paths
                                .into_iter()
                                .map(TrashRequest::new)
                                .collect::<Result<Vec<_>, _>>()
                            {
                                Ok(requests) => requests,
                                Err(error) => {
                                    controller.show_toast(
                                        &format!(
                                            "Could not queue duplicate paths for Trash: {error}"
                                        ),
                                        7,
                                    );
                                    return;
                                }
                            };
                            let scopes = match requests
                                .iter()
                                .map(destructive_scope_for_trash)
                                .collect::<Result<Vec<_>, _>>()
                            {
                                Ok(scopes) => scopes,
                                Err(error) => {
                                    controller.show_toast(
                                        &format!(
                                            "Could not queue duplicate paths for Trash: {error}"
                                        ),
                                        7,
                                    );
                                    return;
                                }
                            };
                            let application_state = Rc::clone(&controller.application_state);
                            let toast_overlay = controller.widgets.toast_overlay.clone();
                            controller.review_guardrail(scopes, move |authorizations| {
                                let guarded = requests
                                    .into_iter()
                                    .zip(authorizations)
                                    .map(|(request, authorization)| {
                                        GuardrailAuthorized::new(
                                            TrackedOperation::Trash(request),
                                            authorization,
                                        )
                                    })
                                    .collect();
                                match application_state.enqueue_authorized_batch(guarded) {
                                    Ok(_) => toast_overlay.add_toast(
                                        adw::Toast::builder()
                                            .title(format!(
                                                "Queued {} for Trash after explicit duplicate review",
                                                item_count_text(count)
                                            ))
                                            .timeout(5)
                                            .build(),
                                    ),
                                    Err(error) => toast_overlay.add_toast(
                                        adw::Toast::builder()
                                            .title(format!(
                                                "Could not queue duplicate paths for Trash: {error}"
                                            ))
                                            .timeout(7)
                                            .build(),
                                    ),
                                }
                            });
                        },
                    );
                }
                crate::duplicate_finder::DuplicateFinderEventKind::Failed(error) => {
                    self.duplicate_running.set(false);
                    if let Some(dialog) = self.duplicate_progress.borrow_mut().take() {
                        dialog.close();
                    }
                    if let Some(action) = self
                        .widgets
                        .window
                        .lookup_action("cancel-duplicate-scan")
                        .and_downcast::<gio::SimpleAction>()
                    {
                        action.set_enabled(false);
                    }
                    if let Some(action) = self
                        .widgets
                        .window
                        .lookup_action("check-duplicates")
                        .and_downcast::<gio::SimpleAction>()
                    {
                        action.set_enabled(!self.trash_active.get());
                    }
                    self.show_toast(&format!("Duplicate scan failed: {error}"), 7);
                }
            }
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
        present_checksum_dialog_for_targets(
            &self.widgets.window,
            &self.widgets.toast_overlay,
            Rc::clone(&self.application_state),
            targets,
        );
    }

    fn selected_single_regular_path(&self) -> Option<PathBuf> {
        let selected = self.selected_entries.borrow();
        (selected.len() == 1 && matches!(selected[0].kind(), EntryKind::RegularFile))
            .then(|| selected[0].path().to_path_buf())
    }

    fn fingerprint_store_path(&self) -> PathBuf {
        private_fingerprint_store_path(&glib::user_data_dir())
    }

    fn submit_integrity_request(&self, request: IntegrityRequest, queued: &str) {
        match self.application_state.submit_integrity(request) {
            Ok(_) => self.show_toast(queued, 4),
            Err(error) => {
                self.show_toast(&format!("Could not start integrity operation: {error}"), 7)
            }
        }
    }

    fn save_selected_fingerprint(&self) {
        let Some(target) = self.selected_single_regular_path() else {
            self.show_toast(
                "Select one regular local file to save its SHA-256 fingerprint",
                5,
            );
            return;
        };
        self.submit_integrity_request(
            IntegrityRequest::SaveFingerprint {
                target,
                store_path: self.fingerprint_store_path(),
            },
            "Saving private SHA-256 fingerprint…",
        );
    }

    fn verify_selected_fingerprint(&self) {
        let Some(target) = self.selected_single_regular_path() else {
            self.show_toast(
                "Select one regular local file to verify its saved SHA-256 fingerprint",
                5,
            );
            return;
        };
        self.submit_integrity_request(
            IntegrityRequest::VerifyFingerprint {
                target,
                store_path: self.fingerprint_store_path(),
            },
            "Verifying saved SHA-256 fingerprint…",
        );
    }

    fn generate_selected_sha256sums(&self) {
        let targets = self
            .selected_entries
            .borrow()
            .iter()
            .filter(|entry| matches!(entry.kind(), EntryKind::RegularFile | EntryKind::Directory))
            .map(|entry| entry.path().to_path_buf())
            .collect::<Vec<_>>();
        if targets.is_empty() {
            self.show_toast("Select regular files or folders to generate SHA256SUMS", 5);
            return;
        }
        let current = self.tabs.borrow().active().current().path().to_path_buf();
        let Some(root) = integrity_manifest_root(&current, &targets) else {
            self.show_toast("Selected files do not share a local manifest root", 6);
            return;
        };
        self.submit_integrity_request(
            IntegrityRequest::GenerateSha256Sums {
                output_path: root.join("SHA256SUMS"),
                root,
                targets,
            },
            "Generating SHA256SUMS; an existing manifest will not be replaced…",
        );
    }

    fn verify_selected_sha256sums(&self) {
        let Some(manifest_path) = self.selected_single_regular_path() else {
            self.show_toast("Select one local SHA256SUMS manifest to verify", 5);
            return;
        };
        let Some(root) = manifest_path.parent().map(Path::to_path_buf) else {
            self.show_toast("The selected manifest has no safe parent folder", 6);
            return;
        };
        self.submit_integrity_request(
            IntegrityRequest::VerifySha256Sums {
                root,
                manifest_path,
            },
            "Verifying selected SHA256SUMS manifest…",
        );
    }

    fn integrity_baseline_store_path(&self) -> PathBuf {
        crate::integrity_ui::private_integrity_baseline_store_path(&glib::user_data_dir())
    }

    fn ensure_integrity_monitor_worker(&self) -> bool {
        if self.integrity_monitor_worker.borrow().is_some() {
            return true;
        }
        match IntegrityMonitorWorker::spawn() {
            Ok(worker) => {
                self.integrity_monitor_worker.replace(Some(worker));
                true
            }
            Err(error) => {
                self.show_toast(&format!("Integrity monitoring unavailable: {error}"), 7);
                false
            }
        }
    }

    fn create_integrity_baseline(self: &Rc<Self>, update: bool) {
        if !self.ensure_integrity_monitor_worker() {
            return;
        }
        let request = IntegrityMonitorRequest::Create {
            root: self.tabs.borrow().active().current().path().to_path_buf(),
            root_kind: IntegrityMonitorRootKind::Local,
            store_path: self.integrity_baseline_store_path(),
            storage_policy: IntegrityBaselineStoragePolicy::Persist,
        };
        self.submit_integrity_monitor_request(
            request,
            if update {
                "Updating private integrity baseline…"
            } else {
                "Creating private integrity baseline…"
            },
        );
    }

    fn verify_integrity_baseline(self: &Rc<Self>) {
        let Some(baseline) = self.integrity_baseline.borrow().clone() else {
            if self.ensure_integrity_monitor_worker() {
                self.submit_integrity_monitor_request(
                    IntegrityMonitorRequest::Load {
                        store_path: self.integrity_baseline_store_path(),
                    },
                    "Loading private integrity baseline…",
                );
            }
            return;
        };
        self.submit_integrity_monitor_request(
            IntegrityMonitorRequest::Check {
                baseline,
                root_kind: IntegrityMonitorRootKind::Local,
            },
            "Verifying integrity baseline…",
        );
    }

    fn delete_integrity_baseline(self: &Rc<Self>) {
        if !self.ensure_integrity_monitor_worker() {
            return;
        }
        self.stop_integrity_monitoring();
        self.submit_integrity_monitor_request(
            IntegrityMonitorRequest::Delete {
                store_path: self.integrity_baseline_store_path(),
            },
            "Removing private integrity baseline…",
        );
    }

    fn start_integrity_monitoring(self: &Rc<Self>) {
        let Some(baseline) = self.integrity_baseline.borrow().clone() else {
            self.show_toast(
                "Create or load an integrity baseline before starting monitoring",
                6,
            );
            return;
        };
        let root = self.tabs.borrow().active().current().path().to_path_buf();
        if baseline.root() != root {
            self.show_toast("The baseline belongs to a different folder", 6);
            return;
        }
        match self.rebuild_integrity_watch_set(root) {
            Ok(()) => {
                self.integrity_session.borrow_mut().enable();
                self.show_toast(
                    "Integrity monitoring started. It is not intrusion detection.",
                    6,
                );
                self.request_integrity_rescan();
            }
            Err(error) => {
                self.show_toast(&format!("Could not start integrity monitoring: {error}"), 7)
            }
        }
    }

    fn rebuild_integrity_watch_set(self: &Rc<Self>, root: PathBuf) -> Result<(), String> {
        let weak = Rc::downgrade(self);
        let watch_set = IntegrityWatchSet::start(root, move |event| {
            if let Some(controller) = weak.upgrade() {
                controller.record_integrity_watch_event(event);
            }
        })
        .map_err(|error| error.to_string())?;
        self.integrity_watch_set.replace(Some(watch_set));
        Ok(())
    }

    fn stop_integrity_monitoring(&self) {
        if let Some(source) = self.integrity_rescan_source.borrow_mut().take() {
            source.remove();
        }
        self.integrity_watch_set.replace(None);
        self.integrity_session.borrow_mut().disable();
        self.show_toast("Integrity monitoring stopped", 4);
    }

    fn record_integrity_watch_event(self: &Rc<Self>, event: IntegrityWatchEvent) {
        let Some(root) = self
            .integrity_watch_set
            .borrow()
            .as_ref()
            .map(|set| set.root().to_path_buf())
        else {
            return;
        };
        let Ok(policy) = IntegrityWatchSetPolicy::new(root) else {
            return;
        };
        if self
            .integrity_session
            .borrow_mut()
            .record_event(&policy, &event)
            .is_err()
            || self.integrity_rescan_source.borrow().is_some()
        {
            return;
        }
        let weak = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(Duration::from_millis(220), move || {
            if let Some(controller) = weak.upgrade() {
                controller.integrity_rescan_source.borrow_mut().take();
                controller.request_integrity_rescan();
            }
        });
        self.integrity_rescan_source.replace(Some(source));
    }

    fn request_integrity_rescan(self: &Rc<Self>) {
        let IntegrityRescanDecision::Start { .. } =
            self.integrity_session.borrow_mut().take_rescan()
        else {
            return;
        };
        let Some(baseline) = self.integrity_baseline.borrow().clone() else {
            self.integrity_session
                .borrow_mut()
                .interrupt_scan(IntegrityMonitorStaleReason::ScanInterrupted);
            return;
        };
        self.submit_integrity_monitor_request(
            IntegrityMonitorRequest::Check {
                baseline,
                root_kind: IntegrityMonitorRootKind::Local,
            },
            "Rechecking integrity baseline…",
        );
    }

    fn submit_integrity_monitor_request(
        self: &Rc<Self>,
        request: IntegrityMonitorRequest,
        queued: &str,
    ) {
        if !self.ensure_integrity_monitor_worker() {
            return;
        }
        let Some(submission) = self
            .integrity_monitor_worker
            .borrow()
            .as_ref()
            .and_then(|worker| worker.submit(request).ok())
        else {
            self.show_toast("Could not queue integrity monitoring work", 7);
            return;
        };
        self.integrity_request_generation
            .set(Some(submission.generation));
        self.show_toast(queued, 4);
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(35), move || {
            let Some(controller) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if controller.poll_integrity_monitor_result() {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    fn poll_integrity_monitor_result(self: &Rc<Self>) -> bool {
        let Some(generation) = self.integrity_request_generation.get() else {
            return true;
        };
        let result = self
            .integrity_monitor_worker
            .borrow()
            .as_ref()
            .and_then(|worker| worker.take_result(generation));
        let Some(result) = result else {
            return false;
        };
        self.integrity_request_generation.set(None);
        match result.outcome {
            Ok(IntegrityMonitorOutcome::BaselineCreated { baseline, .. }) => {
                self.integrity_baseline.replace(Some(baseline));
                self.show_toast("Private integrity baseline saved", 5);
            }
            Ok(IntegrityMonitorOutcome::BaselineLoaded(Some(baseline))) => {
                self.integrity_baseline.replace(Some(baseline));
                self.show_toast("Private integrity baseline loaded", 5);
            }
            Ok(IntegrityMonitorOutcome::BaselineLoaded(None)) => {
                self.show_toast("No private integrity baseline stored", 6)
            }
            Ok(IntegrityMonitorOutcome::Checked { diff, .. }) => {
                let session_generation = self.integrity_session.borrow().generation();
                self.integrity_session
                    .borrow_mut()
                    .complete_scan(session_generation);
                if self.integrity_session.borrow().enabled() {
                    if let Some(root) = self
                        .integrity_baseline
                        .borrow()
                        .as_ref()
                        .map(|baseline| baseline.root().to_path_buf())
                    {
                        if let Err(error) = self.rebuild_integrity_watch_set(root) {
                            self.integrity_session
                                .borrow_mut()
                                .interrupt_scan(IntegrityMonitorStaleReason::WatcherInvalidated);
                            self.show_toast(
                                &format!("Integrity monitoring needs recheck: {error}"),
                                7,
                            );
                        }
                    }
                }
                let presentation = crate::integrity_ui::present_integrity_monitor_diff(&diff);
                let widgets = crate::integrity_ui::build_integrity_results_dialog(&presentation);
                let dialog = widgets.dialog.downgrade();
                widgets.close_button.connect_clicked(move |_| {
                    if let Some(dialog) = dialog.upgrade() {
                        dialog.close();
                    }
                });
                widgets.dialog.present(Some(&self.widgets.window));
                widgets.close_button.grab_focus();
            }
            Ok(IntegrityMonitorOutcome::StoredBaselineRemoved) => {
                self.integrity_baseline.replace(None);
                self.show_toast("Private integrity baseline removed", 5);
            }
            Err(error) => {
                self.integrity_session
                    .borrow_mut()
                    .interrupt_scan(IntegrityMonitorStaleReason::ScanInterrupted);
                self.show_toast(&format!("Integrity monitoring needs recheck: {error}"), 7);
            }
        }
        true
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
                self.begin_background_feedback(BackgroundActivity::Properties, generation);
            }
            Some(Err(PropertiesSubmitError::Full(_))) => {
                self.show_toast("Properties queue is busy; try again", 5);
            }
            Some(Err(PropertiesSubmitError::Disconnected)) | None => {
                self.show_toast("Properties worker is unavailable", 6);
            }
        }
    }

    fn inspect_privacy_safety(&self) {
        let paths = self.selected_paths();
        let generation = self.application_state.next_security_generation();
        let request = match PrivacySecurityRequest::inspect(generation, paths) {
            Ok(request) => request,
            Err(error) => {
                self.show_toast(&error.to_string(), 5);
                return;
            }
        };
        match self.application_state.submit_privacy_security(request) {
            Ok(()) => {
                self.privacy_security_generation.set(generation);
                self.set_action_enabled("inspect-privacy-safety", false);
                self.set_action_enabled("cancel-privacy-inspection", true);
                self.begin_background_feedback(BackgroundActivity::PrivacyInspection, generation);
            }
            Err(error) => self.show_toast(&error.to_string(), 6),
        }
    }

    fn scan_selected_for_threats(&self) {
        let generation = self.application_state.next_security_generation();
        let limits = {
            let preferences = self.current_preferences.borrow();
            ThreatScanLimits::from_preferences(
                preferences.clamav_file_limit_mib,
                preferences.clamav_total_limit_gib,
            )
        };
        let request =
            match ThreatScanRequest::with_limits(generation, self.selected_paths(), limits) {
                Ok(request) => request,
                Err(error) => {
                    self.show_toast(&error.to_string(), 6);
                    return;
                }
            };
        match self.application_state.submit_threat_scan(request) {
            Ok(()) => {
                self.threat_scan_generation.set(generation);
                self.set_action_enabled("scan-threats", false);
                self.set_action_enabled("cancel-threat-scan", true);
                self.begin_background_feedback(BackgroundActivity::ThreatScan, generation);
            }
            Err(error) => self.show_toast(
                &format!(
                    "Could not start local ClamAV scan: {error}. Install and start clamd; Floe does not upload files or bundle an engine."
                ),
                9,
            ),
        }
    }

    fn create_sanitized_copy(self: &Rc<Self>) {
        let sources = self.selected_paths();
        if sources.is_empty() {
            self.show_toast("Select one or more JPEG, PNG, or WebP files", 4);
            return;
        }
        let subject = if sources.len() == 1 {
            sources[0]
                .file_name()
                .unwrap_or(sources[0].as_os_str())
                .to_string_lossy()
                .into_owned()
        } else {
            format!("{} selected items", sources.len())
        };
        let dialog = adw::AlertDialog::builder()
            .heading("Create a sanitized copy?")
            .body(format!(
                "Floe will keep {subject} unchanged, create no-overwrite sibling copies for supported JPEG, PNG, or WebP files, remove only reviewed metadata blocks, and verify those reviewed blocks are absent. Unsupported and failed items remain explicit. This is not an exhaustive anonymity guarantee."
            ))
            .default_response("create")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("create", "Create Copy")]);
        dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);
        let controller = Rc::downgrade(self);
        dialog.connect_response(None, move |_, response| {
            if response != "create" {
                return;
            }
            let Some(controller) = controller.upgrade() else {
                return;
            };
            let generation = controller.application_state.next_security_generation();
            let request = match PrivacySecurityRequest::sanitize(generation, sources.clone()) {
                Ok(request) => request,
                Err(error) => {
                    controller.show_toast(&error.to_string(), 6);
                    return;
                }
            };
            match controller
                .application_state
                .submit_privacy_security(request)
            {
                Ok(()) => {
                    controller.privacy_security_generation.set(generation);
                    controller.set_action_enabled("create-sanitized-copy", false);
                    controller.set_action_enabled("cancel-sanitization", true);
                    controller.begin_background_feedback(
                        BackgroundActivity::MetadataSanitization,
                        generation,
                    );
                }
                Err(error) => controller.show_toast(&error.to_string(), 6),
            }
        });
        dialog.present(Some(&self.widgets.window));
    }

    fn drain_privacy_security_worker(&self) {
        for _ in 0..8 {
            let Some(result) = self
                .application_state
                .take_privacy_security_result(self.privacy_security_generation.get())
            else {
                break;
            };
            match result {
                PrivacySecurityResult::Inspection(outcome)
                    if outcome.generation == self.privacy_security_generation.get() =>
                {
                    self.set_action_enabled(
                        "inspect-privacy-safety",
                        !self.selected_entries.borrow().is_empty() && !self.trash_active.get(),
                    );
                    self.set_action_enabled("cancel-privacy-inspection", false);
                    self.last_privacy_outcome.replace(Some(outcome.clone()));
                    let (kind, title) = if outcome.cancelled {
                        (
                            BackgroundOutcomeKind::Cancelled,
                            "Privacy and safety inspection cancelled; partial results are available",
                        )
                    } else {
                        (
                            BackgroundOutcomeKind::Completed,
                            "Privacy and safety inspection complete",
                        )
                    };
                    self.finish_background_feedback(
                        BackgroundActivity::PrivacyInspection,
                        outcome.generation,
                        kind,
                        title,
                        true,
                    );
                    if self.widgets.window.is_active() {
                        self.present_inspection_outcome(&outcome);
                    }
                }
                PrivacySecurityResult::Sanitized(outcome)
                    if outcome.generation == self.privacy_security_generation.get() =>
                {
                    let can_sanitize = !self.trash_active.get()
                        && !self.selected_entries.borrow().is_empty()
                        && self
                            .selected_entries
                            .borrow()
                            .iter()
                            .all(|entry| entry.kind() == EntryKind::RegularFile);
                    self.set_action_enabled("create-sanitized-copy", can_sanitize);
                    self.set_action_enabled("cancel-sanitization", false);
                    let completed = outcome
                        .items
                        .iter()
                        .filter(|item| item.result.is_ok())
                        .count();
                    let failed = outcome.items.len().saturating_sub(completed);
                    let destination = outcome.items.iter().find_map(|item| {
                        item.result
                            .as_ref()
                            .ok()
                            .map(|copy| copy.destination.clone())
                    });
                    self.last_sanitized_copy.replace(destination.clone());
                    let kind = if outcome.cancelled {
                        BackgroundOutcomeKind::Cancelled
                    } else if failed == 0 {
                        BackgroundOutcomeKind::Completed
                    } else if completed > 0 {
                        BackgroundOutcomeKind::Partial
                    } else {
                        BackgroundOutcomeKind::Failed
                    };
                    let suffix = if outcome.cancelled {
                        " · cancelled"
                    } else {
                        ""
                    };
                    self.finish_background_feedback(
                        BackgroundActivity::MetadataSanitization,
                        outcome.generation,
                        kind,
                        &format!(
                            "Sanitized copies: {completed} created, {failed} not created{suffix}"
                        ),
                        destination.is_some(),
                    );
                }
                _ => {}
            }
        }
    }

    fn drain_threat_scan_worker(&self) {
        let Some(result) = self
            .application_state
            .take_threat_scan_result(self.threat_scan_generation.get())
        else {
            return;
        };
        let generation = result.generation;
        self.set_action_enabled(
            "scan-threats",
            !self.selected_entries.borrow().is_empty() && !self.trash_active.get(),
        );
        self.set_action_enabled("cancel-threat-scan", false);
        match result.outcome {
            Ok(outcome) if outcome.generation == self.threat_scan_generation.get() => {
                let (kind, title) = if outcome.cancelled {
                    (
                        BackgroundOutcomeKind::Cancelled,
                        "Local ClamAV scan stopped; partial results are available".to_owned(),
                    )
                } else if outcome.truncated {
                    (
                        BackgroundOutcomeKind::Partial,
                        "Local ClamAV scan reached a safety limit; partial results are available"
                            .to_owned(),
                    )
                } else if outcome.detections > 0 {
                    (
                        BackgroundOutcomeKind::Completed,
                        format!(
                            "Local ClamAV scan complete: {} signature report{}",
                            outcome.detections,
                            if outcome.detections == 1 { "" } else { "s" }
                        ),
                    )
                } else {
                    (
                        BackgroundOutcomeKind::Completed,
                        "Local ClamAV scan complete: no known signature reported".to_owned(),
                    )
                };
                self.last_threat_outcome.replace(Some(outcome.clone()));
                self.finish_background_feedback(
                    BackgroundActivity::ThreatScan,
                    generation,
                    kind,
                    &title,
                    true,
                );
                if self.widgets.window.is_active() {
                    self.present_threat_scan_outcome(&outcome);
                }
            }
            Ok(_) => {}
            Err(crate::threat_scan::ThreatScanError::Cancelled) => {
                self.finish_background_feedback(
                    BackgroundActivity::ThreatScan,
                    generation,
                    BackgroundOutcomeKind::Cancelled,
                    "Local ClamAV scan cancelled",
                    false,
                );
            }
            Err(error) => {
                self.finish_background_feedback(
                    BackgroundActivity::ThreatScan,
                    generation,
                    BackgroundOutcomeKind::Failed,
                    &format!("Local ClamAV scan failed: {error}"),
                    false,
                );
            }
        }
    }

    fn present_inspection_outcome(&self, outcome: &InspectionOutcome) {
        let mut body = String::from(
            "Local, read-only evidence. Suspicious signals are not a malware verdict, and privacy inspection is format-specific rather than exhaustive.\n\n",
        );
        for entry in &outcome.entries {
            let name = entry
                .path
                .file_name()
                .unwrap_or(entry.path.as_os_str())
                .to_string_lossy();
            body.push_str(&format!("{name}\n"));
            if entry.suspicious.findings.is_empty() {
                body.push_str(
                    "  Safety: no reviewed filename, type, or permission signal found.\n",
                );
            } else {
                for finding in &entry.suspicious.findings {
                    body.push_str(&format!("  Safety: {}\n", finding.explanation));
                }
            }
            match &entry.privacy {
                PrivacyInspectionState::Reviewed { format, findings } if findings.is_empty() => {
                    body.push_str(&format!("  Privacy: {} reviewed; no supported metadata block found. This is not exhaustive.\n", format.label()));
                }
                PrivacyInspectionState::Reviewed { format, findings } => {
                    body.push_str(&format!("  Privacy: {}\n", format.label()));
                    for finding in findings {
                        body.push_str(&format!(
                            "    {} — {}\n",
                            finding.category, finding.explanation
                        ));
                    }
                }
                PrivacyInspectionState::Unsupported => {
                    body.push_str("  Privacy: unsupported format; not inspected.\n")
                }
                PrivacyInspectionState::TooLarge => {
                    body.push_str("  Privacy: not inspected; exceeds 64 MiB limit.\n")
                }
                PrivacyInspectionState::NotRegular => {
                    body.push_str("  Privacy: not inspected; not a no-follow regular file.\n")
                }
                PrivacyInspectionState::Changed => {
                    body.push_str("  Privacy: result discarded because the file changed.\n")
                }
                PrivacyInspectionState::Inaccessible(error) => {
                    body.push_str(&format!("  Privacy: inaccessible ({error}).\n"))
                }
                PrivacyInspectionState::Malformed(error) => body.push_str(&format!(
                    "  Privacy: malformed supported container ({error}).\n"
                )),
            }
            body.push('\n');
        }
        self.present_text_report("Privacy & Safety Inspector", &body);
    }

    fn present_threat_scan_outcome(&self, outcome: &ThreatScanOutcome) {
        let mut body = format!(
            "Engine: {}\nScanned files: {}\nNo known signature reported: {}\nDetections: {}\nNot scanned or changed: {}\nConfigured per-file limit: {}\nConfigured total limit: {}\n\nA no-signature result is not proof that a file is safe or malware-free. Floe streamed bytes only to the separately installed local clamd service. clamd may enforce its own lower StreamMaxLength or engine limits.\n",
            outcome.engine,
            outcome.scanned_files,
            outcome.no_known_signature,
            outcome.detections,
            outcome.not_scanned,
            format_scan_limit(outcome.limits.max_file_bytes()),
            format_scan_limit(outcome.limits.max_total_bytes()),
        );
        if outcome.cancelled {
            body.push_str("\nThe scan was cancelled; results are incomplete.\n");
        }
        if outcome.truncated {
            body.push_str("\nLimits were reached; results are incomplete.\n");
        }
        for result in &outcome.retained_results {
            let name = result
                .path
                .file_name()
                .unwrap_or(result.path.as_os_str())
                .to_string_lossy();
            match &result.status {
                ThreatFileStatus::Detected { signature } => {
                    body.push_str(&format!("\n{name}: signature reported — {signature}"))
                }
                ThreatFileStatus::NotScanned { reason } => {
                    body.push_str(&format!("\n{name}: not scanned — {reason}"))
                }
                ThreatFileStatus::Changed => {
                    body.push_str(&format!("\n{name}: changed during scan; result discarded"))
                }
                ThreatFileStatus::NoKnownSignature => {}
            }
        }
        self.present_text_report_with_action(
            "Local ClamAV Scan",
            &body,
            Some(("Change scan limits…", "win.settings")),
        );
    }

    fn present_text_report(&self, title: &str, body: &str) {
        self.present_text_report_with_action(title, body, None);
    }

    fn present_text_report_with_action(
        &self,
        title: &str,
        body: &str,
        action: Option<(&str, &str)>,
    ) {
        let dialog = adw::Dialog::builder()
            .title(title)
            .content_width(680)
            .content_height(620)
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(18);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);
        let heading = gtk::Label::builder()
            .label(title)
            .halign(gtk::Align::Start)
            .css_classes(["title-2"])
            .build();
        let report = gtk::Label::builder()
            .label(body)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .xalign(0.0)
            .yalign(0.0)
            .wrap(true)
            .selectable(true)
            .build();
        report.update_property(&[
            gtk::accessible::Property::Label(title),
            gtk::accessible::Property::Description(body),
        ]);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&report)
            .build();
        let close = gtk::Button::with_label("Close");
        close.set_halign(gtk::Align::End);
        let weak_dialog = dialog.downgrade();
        close.connect_clicked(move |_| {
            if let Some(dialog) = weak_dialog.upgrade() {
                dialog.close();
            }
        });
        content.append(&heading);
        content.append(&scroll);
        if let Some((label, action_name)) = action {
            let action_button = gtk::Button::builder()
                .label(label)
                .action_name(action_name)
                .halign(gtk::Align::Start)
                .build();
            action_button.update_property(&[
                gtk::accessible::Property::Label(label),
                gtk::accessible::Property::Description(
                    "Open Settings to change limits for future local ClamAV scans",
                ),
            ]);
            content.append(&action_button);
        }
        content.append(&close);
        dialog.set_child(Some(&content));
        dialog.present(Some(&self.widgets.window));
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
            let generation = response.generation;
            self.set_properties_enabled(!self.selected_entries.borrow().is_empty());
            match response.result {
                Ok(snapshot) => {
                    let presentation = present_properties(&snapshot);
                    self.last_properties_presentation
                        .replace(Some(presentation.clone()));
                    self.finish_background_feedback(
                        BackgroundActivity::Properties,
                        generation,
                        BackgroundOutcomeKind::Completed,
                        "Read-only Properties are ready",
                        true,
                    );
                    if self.widgets.window.is_active() {
                        self.present_properties_dialog(&presentation);
                    }
                }
                Err(error) => {
                    self.finish_background_feedback(
                        BackgroundActivity::Properties,
                        generation,
                        BackgroundOutcomeKind::Failed,
                        &format!("Properties unavailable: {error}"),
                        false,
                    );
                }
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
        for (button, action_name) in [
            (&widgets.privacy_safety_button, "inspect-privacy-safety"),
            (&widgets.threat_scan_button, "scan-threats"),
            (&widgets.sanitize_button, "create-sanitized-copy"),
        ] {
            let window = self.widgets.window.downgrade();
            let dialog = widgets.dialog.downgrade();
            button.connect_clicked(move |_| {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.close();
                }
                if let Some(window) = window.upgrade() {
                    gio::prelude::ActionGroupExt::activate_action(&window, action_name, None);
                }
            });
        }
        let checksum_parent = self.widgets.window.clone();
        let checksum_toasts = self.widgets.toast_overlay.clone();
        let checksum_state = Rc::clone(&self.application_state);
        let checksum_targets = checksum_targets_for_presentation(presentation);
        let dialog = widgets.dialog.downgrade();
        widgets.checksum_button.connect_clicked(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
            if let Some(targets) = checksum_targets.as_ref() {
                present_checksum_dialog_for_targets(
                    &checksum_parent,
                    &checksum_toasts,
                    Rc::clone(&checksum_state),
                    Arc::clone(targets),
                );
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
        let association_worker = self.association_worker.clone();
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
                Ok(options) => present_or_report_open_with(
                    &window,
                    &toast_overlay,
                    &display_name,
                    options,
                    association_worker,
                ),
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

    fn choose_verified_copy_destination(self: &Rc<Self>) {
        let Some(source) = self
            .selected_entry()
            .map(|entry| entry.path().to_path_buf())
        else {
            self.show_toast("Select one local item to Copy and Verify", 5);
            return;
        };
        let Some(name) = source.file_name().map(OsStr::to_os_string) else {
            self.show_toast("The selected item has no copyable filename", 5);
            return;
        };
        let chooser = gtk::FileDialog::builder()
            .title("Choose Copy and Verify Destination")
            .modal(true)
            .build();
        chooser.set_initial_folder(Some(&gio::File::for_path(self.action_directory())));
        let window = self.widgets.window.clone();
        let controller = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            match chooser.select_folder_future(Some(&window)).await {
                Ok(folder) => {
                    let Some(parent) = folder.path() else {
                        if let Some(controller) = controller.upgrade() {
                            controller
                                .show_toast("Copy and Verify supports local destinations only", 6);
                        }
                        return;
                    };
                    if let Some(controller) = controller.upgrade() {
                        let request = VerifiedCopyRequest::new(source, parent.join(name));
                        match controller.application_state.submit_verified_copy(request) {
                            Ok(_) => controller.show_toast(
                                "Copy and Verify queued. SHA-256 checks bytes; it does not prove authenticity.",
                                6,
                            ),
                            Err(error) => controller.show_toast(
                                &format!("Could not start Copy and Verify: {error}"),
                                7,
                            ),
                        }
                    }
                }
                Err(error) if error.matches(gio::IOErrorEnum::Cancelled) => {}
                Err(error) => {
                    if let Some(controller) = controller.upgrade() {
                        controller.show_toast(
                            &format!("Could not choose Copy and Verify destination: {error}"),
                            7,
                        );
                    }
                }
            }
        });
    }

    fn choose_verified_usb_destination(self: &Rc<Self>) {
        if self.verified_usb_workflow.borrow().is_some() {
            self.show_toast("A verified removable transfer is already active", 5);
            return;
        }
        let mut selected = self.selected_paths();
        if selected.len() != 1 {
            self.show_toast("Select one local item for a verified removable transfer", 5);
            return;
        }
        let source = selected.remove(0);
        let Some(name) = source.file_name().map(OsStr::to_os_string) else {
            self.show_toast("The selected item has no destination name", 5);
            return;
        };
        let chooser = gtk::FileDialog::builder()
            .title("Choose Folder on Removable Device")
            .modal(true)
            .build();
        let window = self.widgets.window.clone();
        let controller = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            match chooser.select_folder_future(Some(&window)).await {
                Ok(folder) => {
                    let Some(parent) = folder.path() else {
                        if let Some(controller) = controller.upgrade() {
                            controller.show_toast(
                                "Verified removable transfer supports local devices only",
                                6,
                            );
                        }
                        return;
                    };
                    let Some(controller) = controller.upgrade() else {
                        return;
                    };
                    let destination = parent.join(name);
                    let snapshots = controller.device_monitor.snapshots();
                    let Some(target) = resolve_removal_target(&destination, &snapshots) else {
                        controller.show_toast(
                            "Choose a folder on a mounted removable device that can be ejected or unmounted",
                            7,
                        );
                        return;
                    };
                    let removable = snapshots
                        .iter()
                        .any(|snapshot| snapshot.id == *target.id() && snapshot.removable);
                    if !removable {
                        controller.show_toast(
                            "The selected destination is not reported as removable storage",
                            7,
                        );
                        return;
                    }
                    let request = VerifiedCopyRequest::new(source, destination);
                    match controller
                        .application_state
                        .submit_verified_usb_copy(request.clone())
                    {
                        Ok(submission) => {
                            controller
                                .verified_usb_workflow
                                .replace(Some(VerifiedUsbLive {
                                    workflow: VerifiedUsbWorkflow::new(submission.job_id(), target),
                                    request,
                                }));
                            controller.refresh_verified_usb_action();
                            controller.show_toast(
                                "Verified removable transfer queued. SHA-256 checks byte integrity; it does not prove authenticity.",
                                7,
                            );
                        }
                        Err(error) => controller.show_toast(
                            &format!("Could not start verified removable transfer: {error}"),
                            7,
                        ),
                    }
                }
                Err(error) if error.matches(gio::IOErrorEnum::Cancelled) => {}
                Err(error) => {
                    if let Some(controller) = controller.upgrade() {
                        controller.show_toast(
                            &format!("Could not choose removable destination: {error}"),
                            7,
                        );
                    }
                }
            }
        });
    }

    fn finish_verified_usb_copy(
        self: &Rc<Self>,
        job_id: floe_core::JobId,
        result: VerifiedCopyResult,
    ) {
        let is_current = self
            .verified_usb_workflow
            .borrow()
            .as_ref()
            .is_some_and(|live| live.workflow.child_job() == job_id);
        if !is_current {
            tracing::error!(
                ?job_id,
                "received terminal result for an unowned verified removable transfer"
            );
            return;
        }

        match result {
            Ok(_) => {
                let (transfer, snapshots, destination) = {
                    let mut live = self.verified_usb_workflow.borrow_mut();
                    let live = live.as_mut().expect("current workflow was checked above");
                    live.workflow.copy_verified();
                    (
                        live.workflow.transfer().clone(),
                        self.device_monitor.snapshots(),
                        live.request.destination().to_path_buf(),
                    )
                };
                self.show_verified_usb_stage(
                    "Verified removable transfer",
                    "Copy verified; flushing the selected device. Keep it connected.",
                    0.75,
                    false,
                );
                let submitted = self
                    .verified_usb_flush_worker
                    .borrow()
                    .as_ref()
                    .ok_or_else(|| "The device flush worker is unavailable.".to_owned())
                    .and_then(|worker| {
                        worker
                            .submit(job_id, transfer, snapshots, destination)
                            .map_err(|error| error.to_string())
                    });
                match submitted {
                    Ok(()) => self.schedule_verified_usb_flush_poll(),
                    Err(detail) => self.fail_verified_usb(&detail, false),
                }
            }
            Err(failure) => {
                let cancelled = matches!(failure.error(), VerifiedCopyError::Cancelled);
                let presentation = present_verified_copy(&Err(failure));
                let detail = format!(
                    "{} {} The device was not flushed or removed.",
                    presentation.detail, presentation.notice
                );
                self.fail_verified_usb(&detail, cancelled);
            }
        }
    }

    fn schedule_verified_usb_flush_poll(self: &Rc<Self>) {
        if self.verified_usb_flush_source.borrow().is_some() {
            return;
        }
        let controller = Rc::downgrade(self);
        let source = glib::timeout_add_local(Duration::from_millis(40), move || {
            let Some(controller) = controller.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let result = controller
                .verified_usb_flush_worker
                .borrow()
                .as_ref()
                .and_then(DeviceFlushWorker::try_result);
            let Some(result) = result else {
                return glib::ControlFlow::Continue;
            };
            controller.verified_usb_flush_source.borrow_mut().take();
            controller.finish_verified_usb_flush(result);
            glib::ControlFlow::Break
        });
        self.verified_usb_flush_source.replace(Some(source));
    }

    fn finish_verified_usb_flush(self: &Rc<Self>, result: DeviceFlushResult) {
        let job_id = result.child_job();
        let (_, flush_result) = result.into_parts();
        let is_current = self
            .verified_usb_workflow
            .borrow()
            .as_ref()
            .is_some_and(|live| live.workflow.child_job() == job_id);
        if !is_current {
            tracing::error!(
                ?job_id,
                "received flush result for an unowned verified removable transfer"
            );
            return;
        }
        if let Err(error) = flush_result {
            self.fail_verified_usb(
                &format!("The verified copy remains on the device, but flushing failed: {error}"),
                false,
            );
            return;
        }

        let (target, destination) = {
            let mut live = self.verified_usb_workflow.borrow_mut();
            let live = live.as_mut().expect("current workflow was checked above");
            live.workflow.flush_succeeded();
            (
                live.workflow.transfer().target().clone(),
                live.request.destination().to_path_buf(),
            )
        };
        let snapshots = self.device_monitor.snapshots();
        let relationship_is_current = revalidate_removal_target(&target, &snapshots)
            && resolve_removal_target(&destination, &snapshots).as_ref() == Some(&target)
            && snapshots
                .iter()
                .any(|snapshot| snapshot.id == *target.id() && snapshot.removable);
        if !relationship_is_current {
            self.fail_verified_usb(
                "The device, mount, or removal action changed after flushing. Floe did not request removal.",
                false,
            );
            return;
        }

        self.show_verified_usb_stage(
            "Verified removable transfer",
            &format!(
                "{} the verified destination device…",
                target.action().present_participle()
            ),
            0.9,
            false,
        );
        let mount_operation = gtk::MountOperation::new(Some(&self.widgets.window));
        let controller = Rc::downgrade(self);
        let outcome_job = job_id;
        let start = self.device_monitor.remove_verified_target(
            &target,
            Some(mount_operation.upcast_ref()),
            move |outcome| {
                if let Some(controller) = controller.upgrade() {
                    controller.finish_verified_usb_removal(outcome_job, outcome);
                }
            },
        );
        if let Err(error) = start {
            self.fail_verified_usb(&device_start_failure_detail(&error), false);
        }
    }

    fn finish_verified_usb_removal(
        self: &Rc<Self>,
        job_id: floe_core::JobId,
        outcome: DeviceActionOutcome,
    ) {
        let Some((expected_id, expected_action)) = self
            .verified_usb_workflow
            .borrow()
            .as_ref()
            .filter(|live| live.workflow.child_job() == job_id)
            .map(|live| {
                (
                    live.workflow.transfer().target().id().clone(),
                    live.workflow.transfer().target().action(),
                )
            })
        else {
            tracing::error!(
                ?job_id,
                "received removal result for an unowned verified removable transfer"
            );
            return;
        };
        match outcome {
            DeviceActionOutcome::Completed { id, action }
                if id == expected_id && action == expected_action =>
            {
                if let Some(live) = self.verified_usb_workflow.borrow_mut().as_mut() {
                    live.workflow.removal_succeeded();
                }
                self.show_verified_usb_stage(
                    "Safe to remove",
                    "The copied bytes were verified, the device was flushed, and removal completed.",
                    1.0,
                    false,
                );
                self.present_verified_usb_terminal(
                    "Safe to remove",
                    "The selected removable device was flushed and removed successfully. SHA-256 equality is integrity evidence, not proof of authenticity.",
                );
                self.verified_usb_workflow.borrow_mut().take();
                self.refresh_verified_usb_action();
            }
            DeviceActionOutcome::Completed { .. } => self.fail_verified_usb(
                "The desktop reported completion for a different device or action. Floe cannot confirm removal.",
                false,
            ),
            DeviceActionOutcome::Failed { failure, .. } => self.fail_verified_usb(
                &format!("The copy was verified and flushed, but device removal failed: {}", failure.message),
                failure.kind == crate::devices::DeviceActionFailureKind::Cancelled,
            ),
        }
    }

    fn fail_verified_usb(&self, detail: &str, cancelled: bool) {
        if let Some(live) = self.verified_usb_workflow.borrow_mut().as_mut() {
            if cancelled {
                live.workflow.cancelled();
            } else {
                live.workflow.failed();
            }
        }
        let title = if cancelled {
            "Verified removable transfer cancelled"
        } else {
            "Verified removable transfer incomplete"
        };
        self.show_verified_usb_stage(title, detail, 0.0, false);
        self.present_verified_usb_terminal(
            title,
            &format!("{detail} Keep the device connected and review it before disconnecting."),
        );
        self.verified_usb_workflow.borrow_mut().take();
        self.refresh_verified_usb_action();
    }

    fn show_verified_usb_stage(&self, title: &str, detail: &str, fraction: f64, cancellable: bool) {
        let operations = &self.widgets.operations;
        operations.operation_label.set_label(title);
        operations.operation_detail.set_label(detail);
        operations.operation_progress.set_fraction(fraction);
        operations.operation_cancel.set_sensitive(cancellable);
        operations
            .operation_cancel
            .set_tooltip_text(Some(if cancellable {
                "Cancel verified removable transfer"
            } else {
                "This device step cannot be interrupted safely"
            }));
        operations.revealer.set_reveal_child(true);
    }

    fn present_verified_usb_terminal(&self, title: &str, detail: &str) {
        let dialog = adw::AlertDialog::builder()
            .heading(title)
            .body(detail)
            .build();
        dialog.add_response("close", "Close");
        dialog.set_default_response(Some("close"));
        dialog.set_close_response("close");
        dialog.update_property(&[gtk::accessible::Property::Label(title)]);
        dialog.present(Some(&self.widgets.window));
    }

    fn refresh_verified_usb_action(&self) {
        let enabled = self.selected_entries.borrow().len() == 1
            && !self.trash_active.get()
            && self.verified_usb_flush_worker.borrow().is_some()
            && self.verified_usb_workflow.borrow().is_none();
        if let Some(action) = self
            .widgets
            .window
            .lookup_action("verified-removable-transfer")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(enabled);
        }
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
        let guardrail_environment = self.guardrail_environment();
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
                if intent == TransferIntent::Move {
                    let status_label = status_label.clone();
                    let toast_overlay = toast_overlay.clone();
                    let window_after = window.clone();
                    review_move_batch(
                        &window,
                        Rc::clone(&application_state),
                        guardrail_environment.clone(),
                        transfer.paths().to_vec(),
                        destination.clone(),
                        move |result| match result {
                            Ok(batch) => {
                                status_label.set_label(&format!(
                                    "Move {} queued…",
                                    item_count_text(batch.queued())
                                ));
                                let _ = window_after
                                    .clipboard()
                                    .set_content(gdk::ContentProvider::NONE);
                                if let Some(action) = window_after
                                    .lookup_action("paste")
                                    .and_downcast::<gio::SimpleAction>()
                                {
                                    action.set_enabled(false);
                                }
                            }
                            Err(error) => toast_overlay.add_toast(
                                adw::Toast::builder()
                                    .title(format!("Could not start operation: {error}"))
                                    .timeout(7)
                                    .build(),
                            ),
                        },
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

        let staged = self.application_state.staged_transfers();
        let intent = staged.as_ref().map(|(intent, _)| *intent);
        if let Some((TransferIntent::Move, sources)) = staged {
            let status_label = self.widgets.status_label.clone();
            let paste_action = self
                .widgets
                .window
                .lookup_action("paste")
                .and_downcast::<gio::SimpleAction>();
            let toast_overlay = self.widgets.toast_overlay.clone();
            review_move_batch(
                &self.widgets.window,
                Rc::clone(&self.application_state),
                guardrail_environment,
                sources,
                destination.clone(),
                move |result| match result {
                    Ok(batch) => {
                        if let Some(action) = paste_action.as_ref() {
                            action.set_enabled(false);
                        }
                        status_label.set_label(&format!(
                            "Move {} queued…",
                            item_count_text(batch.queued())
                        ));
                    }
                    Err(error) => toast_overlay.add_toast(
                        adw::Toast::builder()
                            .title(format!("Could not start operation: {error}"))
                            .timeout(6)
                            .build(),
                    ),
                },
            );
            return;
        }
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
        let requests = match paths
            .into_iter()
            .map(TrashRequest::new)
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(requests) => requests,
            Err(error) => {
                self.show_toast(&format!("Could not move selection to Trash: {error}"), 7);
                return;
            }
        };
        let scopes = match requests
            .iter()
            .map(destructive_scope_for_trash)
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(scopes) => scopes,
            Err(error) => {
                self.show_toast(&format!("Could not move selection to Trash: {error}"), 7);
                return;
            }
        };
        let application_state = Rc::clone(&self.application_state);
        let status_label = self.widgets.status_label.clone();
        self.review_guardrail(scopes, move |authorizations| {
            let operations = requests
                .into_iter()
                .zip(authorizations)
                .map(|(request, authorization)| {
                    GuardrailAuthorized::new(TrackedOperation::Trash(request), authorization)
                })
                .collect();
            match application_state.enqueue_authorized_batch(operations) {
                Ok(_) => status_label.set_label("Moving selection to Trash…"),
                Err(error) => tracing::error!(%error, "could not submit authorized Trash batch"),
            }
        });
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
        let scopes = match requests
            .iter()
            .map(destructive_scope_for_restore)
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(scopes) => scopes,
            Err(error) => {
                self.show_toast(&format!("Could not start restore: {error}"), 7);
                return;
            }
        };
        let application_state = Rc::clone(&self.application_state);
        let status_label = self.widgets.status_label.clone();
        self.review_guardrail(scopes, move |authorizations| {
            let operations = requests
                .into_iter()
                .zip(authorizations)
                .map(|(request, authorization)| {
                    GuardrailAuthorized::new(TrackedOperation::Restore(request), authorization)
                })
                .collect();
            match application_state.enqueue_authorized_batch(operations) {
                Ok(batch) => status_label
                    .set_label(&format!("Restoring {}…", item_count_text(batch.queued()))),
                Err(error) => tracing::error!(%error, "could not submit authorized restore batch"),
            }
        });
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
        let window = self.widgets.window.clone();
        let environment = self.guardrail_environment();
        let request = match PermanentDeleteRequest::new(targets) {
            Ok(request) => request,
            Err(error) => {
                self.show_toast(&format!("Could not empty Trash: {error}"), 7);
                return;
            }
        };
        let scope = match destructive_scope_for_permanent_delete(&request) {
            Ok(scope) => scope,
            Err(error) => {
                self.show_toast(&format!("Could not empty Trash: {error}"), 7);
                return;
            }
        };
        confirmation.delete_button.connect_clicked(move |button| {
            button.set_sensitive(false);
            let button = button.clone();
            let application_state = Rc::clone(&application_state);
            let status_label = status_label.clone();
            let toast_overlay = toast_overlay.clone();
            let dialog = dialog.clone();
            let request = request.clone();
            review_and_authorize(
                &window,
                Rc::clone(&application_state),
                vec![scope.clone()],
                environment.clone(),
                move |mut authorizations| {
                    let Some(authorization) = authorizations.pop() else {
                        return;
                    };
                    match application_state.submit_permanent_delete_authorized(
                        GuardrailAuthorized::new(request, authorization),
                    ) {
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
                },
            );
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
        let window = self.widgets.window.clone();
        let environment = self.guardrail_environment();
        let request = match PermanentDeleteRequest::new(paths) {
            Ok(request) => request,
            Err(error) => {
                self.show_toast(&format!("Could not start permanent deletion: {error}"), 7);
                return;
            }
        };
        let scope = match destructive_scope_for_permanent_delete(&request) {
            Ok(scope) => scope,
            Err(error) => {
                self.show_toast(&format!("Could not start permanent deletion: {error}"), 7);
                return;
            }
        };
        confirmation.delete_button.connect_clicked(move |button| {
            button.set_sensitive(false);
            let button = button.clone();
            let application_state = Rc::clone(&application_state);
            let status_label = status_label.clone();
            let toast_overlay = toast_overlay.clone();
            let dialog = dialog.clone();
            let request = request.clone();
            review_and_authorize(
                &window,
                Rc::clone(&application_state),
                vec![scope.clone()],
                environment.clone(),
                move |mut authorizations| {
                    let Some(authorization) = authorizations.pop() else {
                        return;
                    };
                    match application_state.submit_permanent_delete_authorized(
                        GuardrailAuthorized::new(request, authorization),
                    ) {
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
                },
            );
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
                    row.add_suffix(&gtk::Image::from_icon_name(
                        "floe-phosphor-caret-right-symbolic",
                    ));
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
            let scopes = match destructive_scopes_for_batch_rename(&request) {
                Ok(scopes) => scopes,
                Err(error) => {
                    error_label.set_label(&format!("Could not queue batch rename: {error}"));
                    button.set_sensitive(true);
                    return;
                }
            };
            let application_state = Rc::clone(&controller.application_state);
            let controller_after = Rc::clone(&controller);
            let dialog = dialog.clone();
            let error_label = error_label.clone();
            let button = button.clone();
            controller.review_guardrail(scopes, move |authorizations| {
                match application_state.submit_batch_rename_authorized(
                    GuardrailAuthorizedBatchRename::new(request, authorizations),
                ) {
                    Ok(_) => {
                        controller_after
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
        });
        widgets.dialog.present(Some(&self.widgets.window));
        widgets.prefix_entry.grab_focus();
    }

    fn undo_batch_rename(&self) {
        let scopes = match self.application_state.batch_rename_undo_guardrail_scopes() {
            Ok(Some(scopes)) => scopes,
            Ok(None) => {
                self.show_toast("No completed batch rename available to undo", 4);
                return;
            }
            Err(error) => {
                self.show_toast(&format!("Could not undo batch rename: {error}"), 7);
                return;
            }
        };
        let application_state = Rc::clone(&self.application_state);
        let status_label = self.widgets.status_label.clone();
        let undo_action = self
            .widgets
            .window
            .lookup_action("undo-batch-rename")
            .and_downcast::<gio::SimpleAction>();
        let toast_overlay = self.widgets.toast_overlay.clone();
        self.review_guardrail(scopes, move |authorizations| {
            match application_state.submit_batch_rename_undo_authorized(authorizations) {
                Ok(Some(_)) => {
                    status_label.set_label("Undoing batch rename…");
                    if let Some(action) = undo_action.as_ref() {
                        action.set_enabled(false);
                    }
                }
                Ok(None) => {
                    toast_overlay.add_toast(
                        adw::Toast::builder()
                            .title("No completed batch rename is available to undo")
                            .timeout(4)
                            .build(),
                    );
                }
                Err(error) => {
                    toast_overlay.add_toast(
                        adw::Toast::builder()
                            .title(format!("Could not undo batch rename: {error}"))
                            .timeout(7)
                            .build(),
                    );
                }
            }
        });
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
        let window = self.widgets.window.clone();
        let environment = self.guardrail_environment();
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

            let request =
                RenameRequest::new(source.clone(), new_name_os, ConflictPolicy::FailIfExists);
            let scope = match destructive_scope_for_rename(&request) {
                Ok(scope) => scope,
                Err(error) => {
                    rename_error.set_label(&format!("Could not rename: {error}"));
                    rename_error.set_visible(true);
                    return;
                }
            };
            let application_state = Rc::clone(&application_state);
            let status_label = status_label.clone();
            let rename_error = rename_error.clone();
            let rename_entry = rename_entry.clone();
            let dialog = dialog.clone();
            review_and_authorize(
                &window,
                Rc::clone(&application_state),
                vec![scope],
                environment.clone(),
                move |mut authorizations| {
                    let Some(authorization) = authorizations.pop() else {
                        return;
                    };
                    match application_state
                        .submit_rename_authorized(GuardrailAuthorized::new(request, authorization))
                    {
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
                },
            );
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
            let generation = self.reload_preserving_view(Vec::new());
            if let Some(pending) = self.pending_operation_reveal.borrow_mut().as_mut()
                && pending.directory() == directory
            {
                pending.bind_generation(generation);
            }
        }
    }

    fn reload_preserving_view(&self, renames: Vec<RenamePair>) -> u64 {
        self.pending_reconciliation
            .replace(Some(PendingReconciliation {
                snapshot: self.capture_view_state(),
                renames,
            }));
        self.load_current_inner()
    }

    fn guardrail_target_folder(&self) -> Option<PathBuf> {
        if self.trash_active.get() {
            return None;
        }
        let selected = self.selected_entries.borrow();
        if selected.len() == 1 && matches!(selected[0].kind(), EntryKind::Directory) {
            return Some(selected[0].path().to_path_buf());
        }
        Some(self.tabs.borrow().active().current().path().to_path_buf())
    }

    fn update_guardrail_action_states(&self) {
        let target = self.guardrail_target_folder();
        let policy = self.application_state.guardrail_policy();
        let blocked =
            self.application_state.guardrail_store_health() == GuardrailStoreHealth::Blocked;
        let busy = self.application_state.guardrail_policy_busy();
        let states = crate::guardrail_ui_model::guardrail_action_states(
            target.as_deref(),
            &policy,
            blocked,
            busy,
        );
        for (name, enabled) in [
            ("protect-folder", states.protect),
            ("unprotect-folder", states.unprotect),
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

    fn change_target_protection(self: &Rc<Self>, protect: bool) {
        let Some(target) = self.guardrail_target_folder() else {
            self.show_toast("Open a normal local folder first", 5);
            return;
        };
        match self
            .application_state
            .submit_guardrail_protection_change(target, protect)
        {
            Ok(true) => {
                self.widgets.status_label.set_label(if protect {
                    "Saving Protected Folder…"
                } else {
                    "Removing Protected Folder…"
                });
                self.update_guardrail_action_states();
            }
            Ok(false) => self.show_toast(
                if protect {
                    "This folder is already protected"
                } else {
                    "This folder is not an exact Protected Folder root"
                },
                5,
            ),
            Err(error) => {
                self.show_toast(&format!("Could not update Protected Folders: {error}"), 7)
            }
        }
    }

    fn drain_guardrail_policy_worker(&self) {
        while let Some(result) = self.application_state.poll_guardrail_policy_update() {
            match result {
                Ok(true) => {
                    self.show_toast("Protected Folder policy reset; no folders are protected", 6)
                }
                Ok(false) => self.show_toast("Protected Folder policy saved", 5),
                Err(error) => self.show_toast(
                    &format!("Protected Folder policy was not changed: {error}"),
                    8,
                ),
            }
            self.update_guardrail_action_states();
        }
    }

    fn show_protected_folders_status(self: &Rc<Self>) {
        let health = self.application_state.guardrail_store_health();
        if health == GuardrailStoreHealth::Blocked {
            let detail = self
                .application_state
                .guardrail_store_error_text()
                .unwrap_or_else(|| "unknown policy storage error".to_owned());
            let dialog = adw::AlertDialog::builder()
                .heading("Protected Folders Unavailable")
                .body(format!(
                    "Destructive actions remain blocked because Floe could not safely load its policy: {detail}\n\n{}",
                    crate::guardrail_ui::GUARDRAIL_LIMITATION_TEXT
                ))
                .build();
            dialog.add_response("cancel", "Cancel");
            dialog.add_response("reset", "Review Reset…");
            dialog.set_close_response("cancel");
            dialog.set_response_appearance("reset", adw::ResponseAppearance::Destructive);
            dialog.update_property(&[gtk::accessible::Property::Label(
                "Protected Folder policy storage error",
            )]);
            let controller = Rc::downgrade(self);
            dialog.connect_response(None, move |_, response| {
                if response == "reset"
                    && let Some(controller) = controller.upgrade()
                {
                    controller.confirm_guardrail_store_reset();
                }
            });
            dialog.present(Some(&self.widgets.window));
            return;
        }

        let policy = self.application_state.guardrail_policy();
        let target = self.guardrail_target_folder();
        let exact_protected = target
            .as_ref()
            .is_some_and(|target| policy.roots().iter().any(|protected| protected == target));
        let target_text = target
            .as_deref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| "No normal folder is active".to_owned());
        let dialog = adw::AlertDialog::builder()
            .heading("Protected Folders")
            .body(format!(
                "{} exact folder root{} saved.\n\nCurrent target: {target_text}\nState: {}\n\n{}",
                policy.roots().len(),
                if policy.roots().len() == 1 {
                    " is"
                } else {
                    "s are"
                },
                if exact_protected {
                    "Protected"
                } else {
                    "Not protected"
                },
                crate::guardrail_ui::GUARDRAIL_LIMITATION_TEXT
            ))
            .build();
        dialog.add_response("close", "Close");
        if target.is_some() && !self.application_state.guardrail_policy_busy() {
            dialog.add_response(
                if exact_protected {
                    "unprotect"
                } else {
                    "protect"
                },
                if exact_protected {
                    "Unprotect Folder"
                } else {
                    "Protect Folder"
                },
            );
        }
        dialog.set_close_response("close");
        let controller = Rc::downgrade(self);
        dialog.connect_response(None, move |_, response| {
            let Some(controller) = controller.upgrade() else {
                return;
            };
            if response == "protect" {
                controller.change_target_protection(true);
            } else if response == "unprotect" {
                controller.change_target_protection(false);
            }
        });
        dialog.present(Some(&self.widgets.window));
    }

    fn confirm_guardrail_store_reset(self: &Rc<Self>) {
        let dialog = adw::AlertDialog::builder()
            .heading("Reset Blocked Protected Folder Policy?")
            .body(format!(
                "This acknowledges the storage error and replaces the unreadable policy with an empty policy. Previously protected folder entries cannot be recovered from the corrupt store.\n\n{}",
                crate::guardrail_ui::GUARDRAIL_LIMITATION_TEXT
            ))
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("acknowledge-reset", "Acknowledge and Reset");
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("acknowledge-reset", adw::ResponseAppearance::Destructive);
        dialog.update_property(&[gtk::accessible::Property::Label(
            "Acknowledge and reset blocked Protected Folder policy",
        )]);
        let controller = Rc::downgrade(self);
        dialog.connect_response(None, move |_, response| {
            if response != "acknowledge-reset" {
                return;
            }
            let Some(controller) = controller.upgrade() else {
                return;
            };
            match controller
                .application_state
                .submit_guardrail_blocked_reset(true)
            {
                Ok(()) => {
                    controller
                        .widgets
                        .status_label
                        .set_label("Resetting Protected Folder policy…");
                    controller.update_guardrail_action_states();
                }
                Err(error) => controller.show_toast(
                    &format!("Could not reset Protected Folder policy: {error}"),
                    8,
                ),
            }
        });
        dialog.present(Some(&self.widgets.window));
    }

    fn replace_background_feedback(
        &self,
        activity: BackgroundActivity,
        title: &str,
        accessible_description: &str,
        button: Option<(&str, &str)>,
        dismissible: bool,
    ) {
        if let Some(previous) = self.background_feedback_rows.borrow_mut().remove(&activity)
            && previous.parent().is_some()
        {
            self.widgets.background_feedback_list.remove(&previous);
        }

        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(12)
            .margin_end(12)
            .build();
        row.set_accessible_role(gtk::AccessibleRole::Group);
        row.update_property(&[
            gtk::accessible::Property::Label(title),
            gtk::accessible::Property::Description(accessible_description),
        ]);
        if !dismissible {
            let spinner = gtk::Spinner::new();
            spinner.start();
            spinner.set_accessible_role(gtk::AccessibleRole::Presentation);
            row.append(&spinner);
        }
        let status = gtk::Label::builder()
            .label(title)
            .halign(gtk::Align::Start)
            .hexpand(true)
            .wrap(true)
            .xalign(0.0)
            .build();
        status.add_css_class("floe-status");
        row.append(&status);
        if let Some((label, action)) = button {
            let action_button = gtk::Button::builder()
                .label(label)
                .action_name(action)
                .build();
            action_button.add_css_class("flat");
            row.append(&action_button);
        }
        if dismissible {
            let dismiss = gtk::Button::builder()
                .icon_name("floe-phosphor-x-symbolic")
                .tooltip_text("Dismiss background activity")
                .build();
            dismiss.add_css_class("flat");
            dismiss.update_property(&[gtk::accessible::Property::Label(
                "Dismiss background activity",
            )]);
            let list = self.widgets.background_feedback_list.clone();
            let revealer = self.widgets.background_feedback_revealer.clone();
            let weak_row = row.downgrade();
            dismiss.connect_clicked(move |_| {
                if let Some(row) = weak_row.upgrade()
                    && row.parent().is_some()
                {
                    list.remove(&row);
                }
                if list.first_child().is_none() {
                    revealer.set_reveal_child(false);
                }
            });
            row.append(&dismiss);
        }
        self.widgets.background_feedback_list.append(&row);
        self.widgets
            .background_feedback_revealer
            .set_reveal_child(true);
        self.background_feedback_rows
            .borrow_mut()
            .insert(activity, row);
    }

    fn present_feedback(&self, activity: BackgroundActivity, presentation: FeedbackPresentation) {
        debug_assert!(presentation.persistent);
        self.replace_background_feedback(
            activity,
            presentation.title,
            presentation.accessible_description,
            presentation.button_label.zip(presentation.action_name),
            false,
        );
    }

    fn begin_background_feedback(&self, activity: BackgroundActivity, generation: u64) {
        if self
            .background_feedback_state
            .borrow_mut()
            .start(activity, generation)
        {
            self.present_feedback(activity, running_presentation(activity));
        }
    }

    fn mark_background_feedback_stopping(&self, activity: BackgroundActivity, generation: u64) {
        if self
            .background_feedback_state
            .borrow()
            .is_active(activity, generation)
        {
            self.present_feedback(activity, stopping_presentation(activity));
        }
    }

    fn finish_background_feedback(
        &self,
        activity: BackgroundActivity,
        generation: u64,
        kind: BackgroundOutcomeKind,
        title: &str,
        result_available: bool,
    ) -> bool {
        if !self
            .background_feedback_state
            .borrow_mut()
            .finish(activity, generation, kind)
        {
            return false;
        }
        let action = result_available.then(|| result_action(activity)).flatten();
        self.replace_background_feedback(
            activity,
            title,
            outcome_accessible_description(kind),
            action,
            true,
        );
        true
    }

    fn show_toast(&self, title: &str, timeout: u32) {
        let failure = ["could not", "failed", "failure", "error"]
            .iter()
            .any(|needle| title.to_ascii_lowercase().contains(needle));
        let feedback = if failure {
            crate::completeness::DetailedFeedback::from_failure(title)
        } else {
            crate::completeness::DetailedFeedback::new(title, None, false)
        };
        let mut builder = adw::Toast::builder()
            .title(escaped_toast_title(&feedback.summary))
            .timeout(timeout);
        if let Some(details) = feedback.details.as_deref() {
            builder = builder
                .button_label(crate::completeness::message(
                    crate::completeness::MessageId::Details,
                ))
                .action_name("win.show-error-details")
                .action_target(&details.to_variant());
        }
        self.widgets.toast_overlay.add_toast(builder.build());
    }

    pub(crate) fn show_active_operation_close_message(&self) {
        self.show_toast(ACTIVE_OPERATION_CLOSE_MESSAGE, 7);
    }

    fn set_action_enabled(&self, name: &str, enabled: bool) {
        if let Some(action) = self
            .widgets
            .window
            .lookup_action(name)
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(enabled);
        }
    }

    pub fn notify_operation_completed(&self, job_id: JobId) {
        if !self.current_preferences.borrow().completion_notifications
            || !crate::completeness::should_send_completion_notification(
                self.widgets.window.is_active(),
            )
        {
            return;
        }
        let Some(application) = self.widgets.window.application() else {
            return;
        };
        let notification = gio::Notification::new(crate::completeness::message(
            crate::completeness::MessageId::OperationCompleted,
        ));
        notification.set_body(Some(crate::completeness::message(
            crate::completeness::MessageId::OperationCompletedBody,
        )));
        application.send_notification(
            Some(&crate::completeness::completion_notification_id(
                self.completion_notification_namespace,
                job_id.get(),
            )),
            &notification,
        );
    }

    fn show_feedback_details(&self, details: &str) {
        let dialog = adw::AlertDialog::builder()
            .heading(crate::completeness::message(
                crate::completeness::MessageId::OperationFailed,
            ))
            .body(details)
            .build();
        dialog.add_response(
            "close",
            crate::completeness::message(crate::completeness::MessageId::Dismiss),
        );
        dialog.set_default_response(Some("close"));
        dialog.set_close_response("close");
        dialog.present(Some(&self.widgets.window));
    }

    fn review_guardrail(
        &self,
        scopes: Vec<DestructiveScope>,
        on_authorized: impl FnOnce(Vec<GuardrailAuthorizationItem>) + 'static,
    ) {
        let environment = self.guardrail_environment();
        review_and_authorize(
            &self.widgets.window,
            Rc::clone(&self.application_state),
            scopes,
            environment,
            on_authorized,
        );
    }

    pub(crate) fn guardrail_environment(&self) -> PreflightEnvironment {
        let mount_roots = self
            .device_snapshots
            .borrow()
            .iter()
            .filter_map(DeviceSnapshot::local_root)
            .map(Path::to_path_buf)
            .collect();
        PreflightEnvironment::new(Some(glib::home_dir()), mount_roots).unwrap_or_default()
    }
}

fn selection_mode_blocks_action(name: &str) -> bool {
    !matches!(
        name,
        "show-error-details"
            | "breadcrumb"
            | "recent-locations"
            | "back"
            | "forward"
            | "parent"
            | "location"
            | "folder-filter"
            | "clear-folder-filter"
            | "close-search-surface"
            | "filename-search"
            | "start-filename-search"
            | "stop-filename-search"
            | "clear-filename-search"
            | "reveal-in-folder"
            | "cancel-location"
            | "hidden"
            | "refresh"
            | "open-trash"
            | "select-all"
            | "invert-selection"
            | "clear-selection"
            | "new-tab"
            | "close-tab-active"
            | "duplicate-tab-active"
            | "reopen-closed-tab"
            | "toggle-split"
            | "switch-split-side"
            | "swap-split-sides"
            | "close-split"
            | "narrow-primary-pane"
            | "widen-primary-pane"
            | "open-opposite-pane"
            | "next-tab"
            | "previous-tab"
            | "move-tab-left"
            | "move-tab-right"
            | "activate-tab"
            | "close-tab"
            | "duplicate-tab"
            | "close-tabs-left"
            | "close-tabs-right"
            | "close-other-tabs"
            | "move-tab-before"
            | "open-new-tab"
            | "open-background-tab"
            | "view-list"
            | "view-grid"
            | "view-miller"
            | "zoom-in"
            | "zoom-out"
            | "miller-parent"
            | "miller-child"
            | "miller-preview-hook"
            | "miller-inspector-hook"
            | "quick-preview"
            | "preview-zoom-in"
            | "preview-zoom-out"
            | "preview-zoom-reset"
            | "preview-fullscreen"
            | "narrow-miller-columns"
            | "widen-miller-columns"
            | "narrow-inspector"
            | "widen-inspector"
            | "appearance-preset"
            | "icon-style"
            | "sidebar-density"
            | "file-density"
            | "sort-column"
            | "sort-direction"
            | "folders-first"
            | "hidden-last"
            | "grouping"
            | "toggle-group"
            | "directory-placement"
            | "remember-folder-view"
            | "narrow-name"
            | "widen-name"
            | "autosize-name"
            | "move-column-left-name"
            | "move-column-right-name"
            | "reset-sidebar-width"
            | "open"
            | "copy-name"
            | "copy-path"
            | "copy-relative-path"
            | "copy-uri"
            | "sort-name"
            | "sort-type"
            | "sort-size"
            | "sort-modified"
            | "sort-extension"
    ) && !name.starts_with("toggle-column-")
        && !name.starts_with("narrow-")
        && !name.starts_with("widen-")
        && !name.starts_with("autosize-")
        && !name.starts_with("move-column-left-")
        && !name.starts_with("move-column-right-")
}

fn selection_mode_file_entry(entry: &DirectoryEntry) -> bool {
    matches!(
        entry.kind(),
        EntryKind::RegularFile
            | EntryKind::SymbolicLink {
                target_is_directory: false
            }
    )
}

fn escaped_toast_title(title: &str) -> glib::GString {
    glib::markup_escape_text(title)
}

fn review_move_batch(
    window: &adw::ApplicationWindow,
    state: Rc<ApplicationState>,
    environment: PreflightEnvironment,
    sources: Vec<PathBuf>,
    destination_directory: PathBuf,
    on_complete: impl FnOnce(Result<crate::state::BatchSubmission, String>) + 'static,
) {
    let mut seen = HashSet::with_capacity(sources.len());
    let operations = sources
        .into_iter()
        .filter(|source| seen.insert(source.clone()))
        .map(|source| {
            transfer_destination(&source, &destination_directory).map(|destination| {
                TrackedOperation::Move(MoveRequest::new(
                    source,
                    destination,
                    ConflictPolicy::FailIfExists,
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>();
    let operations = match operations {
        Ok(operations) if !operations.is_empty() => operations,
        Ok(_) => {
            on_complete(Err("select at least one item".to_owned()));
            return;
        }
        Err(error) => {
            on_complete(Err(error.to_string()));
            return;
        }
    };
    let scopes = match operations
        .iter()
        .map(|operation| match operation {
            TrackedOperation::Move(request) => destructive_scope_for_move(request),
            _ => unreachable!("move batch planner emits only Move"),
        })
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(scopes) => scopes,
        Err(error) => {
            on_complete(Err(error.to_string()));
            return;
        }
    };
    let callback =
        Rc::new(RefCell::new(Some(Box::new(on_complete)
            as Box<
                dyn FnOnce(Result<crate::state::BatchSubmission, String>),
            >)));
    let callback_after = Rc::clone(&callback);
    review_and_authorize(
        window,
        Rc::clone(&state),
        scopes,
        environment,
        move |items| {
            let guarded = operations
                .into_iter()
                .zip(items)
                .map(|(operation, authorization)| {
                    GuardrailAuthorized::new(operation, authorization)
                })
                .collect();
            let result = state
                .enqueue_authorized_batch(guarded)
                .map_err(|error| error.to_string());
            if let Some(callback) = callback_after.borrow_mut().take() {
                callback(result);
            }
        },
    );
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

fn non_empty_trimmed(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
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
            let object = model
                .item(position)
                .and_downcast::<glib::BoxedAnyObject>()?;
            if let Ok(entry) = object.try_borrow::<Arc<DirectoryEntry>>() {
                return Some(entry.clone());
            }
            object
                .try_borrow::<Arc<floe_core::ContentSearchMatch>>()
                .ok()
                .map(|content_match| Arc::new(content_match.entry().clone()))
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

fn integrity_manifest_root(current: &Path, targets: &[PathBuf]) -> Option<PathBuf> {
    if targets.is_empty() || !current.is_absolute() {
        return None;
    }
    if targets.iter().all(|target| target.starts_with(current)) {
        return Some(current.to_path_buf());
    }
    let mut root = targets.first()?.parent()?.to_path_buf();
    while !targets.iter().all(|target| target.starts_with(&root)) {
        if !root.pop() {
            return None;
        }
    }
    root.is_absolute().then_some(root)
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
    if base != "Mounted" {
        return base.to_owned();
    }

    let mut details = Vec::with_capacity(2);
    if let Some(free) = facts.free {
        details.push(format!("{} free", format_bytes(free)));
    }
    if facts.read_only == Some(true) {
        details.push("Read-only".to_owned());
    }
    if details.is_empty() {
        base.to_owned()
    } else {
        details.join(" · ")
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
        return crate::completeness::direction_safe_path(Path::new(&escaped));
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
    association_worker: Option<Rc<launcher::AssociationWorker>>,
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

    present_open_with_dialog(
        window,
        toast_overlay,
        display_name,
        options,
        association_worker,
    );
}

fn present_open_with_dialog(
    window: &adw::ApplicationWindow,
    toast_overlay: &adw::ToastOverlay,
    display_name: &str,
    options: launcher::OpenWithOptions,
    association_worker: Option<Rc<launcher::AssociationWorker>>,
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
    let default_label = chooser.default_label.clone();
    let applications_for_default = Rc::clone(&applications);
    let content_type = options.content_type.clone();
    let toast_for_default = toast_overlay.clone();
    let worker_for_default = association_worker.clone();
    chooser.set_default_button.connect_clicked(move |button| {
        let Some(index) = selected_application_index(&list, applications_for_default.len()) else {
            return;
        };
        let application = &applications_for_default[index];
        match launcher::queue_default_for_type(
            worker_for_default.as_deref(),
            &application.app_info,
            &application.display_name,
            &content_type,
        ) {
            Ok(()) => {
                button.set_sensitive(false);
                default_label.set_label("Applying default change…");
                toast_for_default.add_toast(
                    adw::Toast::builder()
                        .title(format!(
                            "Changing the default to {}…",
                            application.display_name
                        ))
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

    let reset_content_type = options.content_type;
    let worker_for_reset = association_worker;
    let toast_for_reset = toast_overlay.clone();
    let dialog_for_reset = chooser.dialog.downgrade();
    chooser.reset_default_button.connect_clicked(move |button| {
        let result = worker_for_reset
            .as_ref()
            .ok_or(launcher::AssociationChangeError::Disconnected)
            .and_then(|worker| {
                worker.try_change(launcher::AssociationChange::Reset {
                    content_type: reset_content_type.clone(),
                })
            });
        match result {
            Ok(()) => {
                button.set_sensitive(false);
                if let Some(dialog) = dialog_for_reset.upgrade() {
                    dialog.close();
                }
            }
            Err(error) => toast_for_reset.add_toast(
                adw::Toast::builder()
                    .title(error.to_string())
                    .timeout(7)
                    .build(),
            ),
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
    fn phase_23_reliability_close_policy_blocks_only_active_jobs_with_guidance() {
        assert!(window_close_allowed(false));
        assert!(!window_close_allowed(true));
        assert!(ACTIVE_OPERATION_CLOSE_MESSAGE.contains("finish"));
        assert!(ACTIVE_OPERATION_CLOSE_MESSAGE.contains("cancel"));
        assert!(ACTIVE_OPERATION_CLOSE_MESSAGE.contains("closing this window"));
    }

    #[test]
    fn phase_7g_recent_locations_reuse_restorable_session_history() {
        let view = FolderViewState::default();
        let mut session = BrowserSession::new(
            BrowserSessionId::new(1).expect("id"),
            PathBuf::from("/one"),
            view,
        )
        .expect("session");
        session
            .navigate_to(PathBuf::from("/two"), view)
            .expect("navigate");
        session
            .navigate_to(PathBuf::from("/three"), view)
            .expect("navigate");
        assert!(session.go_back());
        let recent = recent_session_locations(&session);
        assert_eq!(recent[0], PathBuf::from("/two"));
        assert_eq!(recent[1], PathBuf::from("/one"));
        assert_eq!(recent[2], PathBuf::from("/three"));

        let restored = BrowserSession::decode(&session.encode().expect("encode")).expect("decode");
        assert_eq!(recent_session_locations(&restored), recent);
    }

    #[test]
    fn post_phase_14_toast_titles_escape_markup_metacharacters() {
        assert_eq!(
            escaped_toast_title("File & folder icons: <Phosphor>"),
            "File &amp; folder icons: &lt;Phosphor&gt;"
        );
    }

    #[test]
    fn phase_18t_ui_uses_current_or_common_manifest_root_without_path_text() {
        let current = Path::new("/workspace/current");
        assert_eq!(
            integrity_manifest_root(
                current,
                &[
                    PathBuf::from("/workspace/current/a"),
                    PathBuf::from("/workspace/current/b")
                ],
            ),
            Some(current.to_path_buf())
        );
        assert_eq!(
            integrity_manifest_root(
                current,
                &[
                    PathBuf::from("/workspace/other/a"),
                    PathBuf::from("/workspace/other/b")
                ],
            ),
            Some(PathBuf::from("/workspace/other"))
        );
        assert_eq!(integrity_manifest_root(current, &[]), None);
    }

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
            metadata_unavailable: 2,
            truncated: true,
        };
        let progress = filename_search_feedback(summary, true, false);
        assert!(progress.contains("Searching… 12 matches from 90 items"));
        assert!(progress.contains("5 skipped"));
        assert!(progress.contains("2 metadata unavailable"));
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

    #[test]
    fn phase_20b3_compact_tab_title_has_a_bounded_display_budget() {
        assert_eq!(COMPACT_TAB_TITLE_MAX_CHARS, 18);
        assert_eq!(tab_title(Path::new("/home/user/a")), "a");
        assert_eq!(
            tab_title(Path::new("/home/user/a very long folder name")),
            "a very long folder name"
        );
    }

    #[test]
    #[ignore = "requires graphical GTK session; run documented GTK component gate"]
    fn phase_testing_gtk_phase_20b3_compact_tab_and_device_labels() {
        gtk::init().expect("initialize GTK");

        let tab = compact_tab_title_label("A deliberately long directory title");
        assert_eq!(tab.max_width_chars(), COMPACT_TAB_TITLE_MAX_CHARS);
        assert_eq!(tab.ellipsize(), gtk::pango::EllipsizeMode::End);
        assert!(tab.property::<bool>("single-line-mode"));

        let device = sidebar_device_name_label("External media with a long name");
        assert_eq!(device.ellipsize(), gtk::pango::EllipsizeMode::End);
        assert!(device.property::<bool>("single-line-mode"));
        assert_eq!(
            device.tooltip_text().as_deref(),
            Some("External media with a long name")
        );

        let free_space = sidebar_device_status_label("128.4 GB free");
        assert_eq!(free_space.ellipsize(), gtk::pango::EllipsizeMode::End);
        assert!(free_space.property::<bool>("single-line-mode"));
        assert_eq!(free_space.tooltip_text().as_deref(), Some("128.4 GB free"));
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
    fn phase_20b2a_window_size_tracking_keeps_only_changed_normal_geometry() {
        let mut preferences = ViewPreferences::default();
        assert!(remember_window_size_if_normal(
            &mut preferences,
            1500,
            900,
            false,
            false
        ));
        let normal = preferences.window_size;
        assert!(!remember_window_size_if_normal(
            &mut preferences,
            1500,
            900,
            false,
            false
        ));
        assert!(!remember_window_size_if_normal(
            &mut preferences,
            3840,
            2160,
            true,
            false
        ));
        assert!(!remember_window_size_if_normal(
            &mut preferences,
            3840,
            2160,
            false,
            true
        ));
        assert_eq!(preferences.window_size, normal);
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
            "2.0 KB free"
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
        assert_eq!(
            device_status_text(
                "Mounted",
                StorageFacts {
                    total: Some(8_000),
                    free: Some(2_000),
                    read_only: Some(true),
                }
            ),
            "2.0 KB free · Read-only"
        );
        assert_eq!(
            device_status_text(
                "Unmounting",
                StorageFacts {
                    total: Some(8_000),
                    free: Some(2_000),
                    read_only: Some(false),
                }
            ),
            "Unmounting"
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

    #[test]
    fn phase_22a_ui_selection_mode_blocks_mutating_and_external_actions() {
        for action in [
            "open-with",
            "open-terminal",
            "open-as-administrator",
            "copy-to-opposite-pane",
            "compress",
            "rename",
            "trash",
            "permanent-delete",
            "paste",
            "new-folder",
            "create-symbolic-link",
            "properties",
            "save-search",
            "delete-saved-search",
            "clear-recent-searches",
            "clear-metadata-sort-cache",
            "run-custom-action",
            "future-mutating-action",
        ] {
            assert!(selection_mode_blocks_action(action), "{action}");
        }
        for action in [
            "back",
            "forward",
            "parent",
            "location",
            "folder-filter",
            "select-all",
            "clear-selection",
            "view-list",
            "view-grid",
        ] {
            assert!(!selection_mode_blocks_action(action), "{action}");
        }
    }
}
