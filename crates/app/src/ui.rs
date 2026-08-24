use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

use adw::prelude::*;
use floe_core::{DirectoryEntry, DirectorySort, EntryKind, SortColumn, SortDirection};
use gtk::{gio, glib};

use crate::{
    appearance::Appearance,
    iconography::{EntryIcon, LIST_ICON_EDGE, grid_icon_edge, icon_for_entry},
    launcher::OpenWithOptions,
    locations::Location,
    preferences::ViewPreferences,
    thumbnail::{LIST_THUMBNAIL_EDGE, ThumbnailError, ThumbnailKey, ThumbnailPixels},
    view::{GRID_SIZES, GridSize, ViewMode},
};

const FILE_CONTEXT_ACTIONS: [(&str, &str); 6] = [
    ("Open", "win.open"),
    ("Open With…", "win.open-with"),
    ("Copy", "win.copy"),
    ("Cut", "win.cut"),
    ("Rename…", "win.rename"),
    ("Move to Trash", "win.trash"),
];

const CONFLICT_DECISION_LABELS: [&str; 2] = ["Keep Existing", "Retry with New Name"];
const LIST_COLUMN_LABELS: [&str; 4] = ["Name", "Type", "Size", "Modified"];
const TYPE_COLUMN_WIDTH: i32 = 11;
const SIZE_COLUMN_WIDTH: i32 = 10;
const MODIFIED_COLUMN_WIDTH: i32 = 18;
const THUMBNAIL_CACHE_CAPACITY: usize = 256;
pub const SORT_ACTIONS: [(&str, SortColumn); 4] = [
    ("sort-name", SortColumn::Name),
    ("sort-type", SortColumn::Type),
    ("sort-size", SortColumn::Size),
    ("sort-modified", SortColumn::Modified),
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
}

impl ThumbnailPresentation {
    fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(ThumbnailPresentationState {
                disabled: false,
                completed: HashMap::new(),
                cache_order: VecDeque::new(),
                pending: HashSet::new(),
                requests: VecDeque::new(),
                bindings: Vec::new(),
            })),
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
        image.remove_css_class("floe-thumbnail");
        apply_entry_icon(image, entry, icon_edge);

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
pub struct SortHeaderWidgets {
    pub column: SortColumn,
    pub button: gtk::Button,
    label: gtk::Label,
}

pub struct OpenWithDialogWidgets {
    pub dialog: adw::Dialog,
    pub default_label: gtk::Label,
    pub list: gtk::ListBox,
    pub rows: Vec<gtk::ListBoxRow>,
    pub cancel_button: gtk::Button,
    pub set_default_button: gtk::Button,
    pub open_button: gtk::Button,
}

pub struct ConflictDialogWidgets {
    pub dialog: adw::Dialog,
    pub name_entry: gtk::Entry,
    pub name_error: gtk::Label,
    pub cancel_button: gtk::Button,
    pub keep_existing_button: gtk::Button,
    pub retry_button: gtk::Button,
}

#[derive(Clone)]
pub struct OperationWidgets {
    pub revealer: gtk::Revealer,
    pub operation_label: gtk::Label,
    pub operation_detail: gtk::Label,
    pub operation_progress: gtk::ProgressBar,
    pub operation_retry: gtk::Button,
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

pub struct BrowserWidgets {
    pub window: adw::ApplicationWindow,
    pub toast_overlay: adw::ToastOverlay,
    pub back_button: gtk::Button,
    pub forward_button: gtk::Button,
    pub parent_button: gtk::Button,
    pub hidden_button: gtk::ToggleButton,
    pub path_label: gtk::Label,
    pub path_stack: gtk::Stack,
    pub location_entry: gtk::Entry,
    pub selection: gtk::SingleSelection,
    pub list_view: gtk::ListView,
    pub grid_view: gtk::GridView,
    pub view_stack: gtk::Stack,
    pub list_header: gtk::Box,
    pub list_context_menu: gtk::PopoverMenu,
    pub grid_context_menu: gtk::PopoverMenu,
    pub list_view_button: gtk::ToggleButton,
    pub grid_view_button: gtk::ToggleButton,
    pub grid_size_controls: gtk::Box,
    pub grid_size_scale: gtk::Scale,
    pub empty_state: gtk::Box,
    pub spinner: gtk::Spinner,
    pub status_label: gtk::Label,
    pub sort_headers: Vec<SortHeaderWidgets>,
    pub thumbnails: ThumbnailPresentation,
    pub location_buttons: Vec<gtk::Button>,
    pub operations: OperationWidgets,
}

struct DirectoryPanelWidgets {
    content: gtk::Box,
    selection: gtk::SingleSelection,
    list_view: gtk::ListView,
    grid_view: gtk::GridView,
    view_stack: gtk::Stack,
    list_header: gtk::Box,
    list_context_menu: gtk::PopoverMenu,
    grid_context_menu: gtk::PopoverMenu,
    empty_state: gtk::Box,
    spinner: gtk::Spinner,
    status_label: gtk::Label,
    sort_headers: Vec<SortHeaderWidgets>,
    thumbnails: ThumbnailPresentation,
}

impl BrowserWidgets {
    pub fn set_view_mode(&self, mode: ViewMode) {
        self.view_stack.set_visible_child_name(mode.stack_name());
        self.list_header.set_visible(mode == ViewMode::List);
        self.grid_size_controls.set_visible(mode == ViewMode::Grid);
        self.list_view_button.set_active(mode == ViewMode::List);
        self.grid_view_button.set_active(mode == ViewMode::Grid);
    }

    pub fn set_grid_size(&self, size: GridSize) {
        self.grid_size_scale.set_value(size.index() as f64);
        let label = format!("Grid icon size: {} pixels", size.edge());
        self.grid_size_scale.set_tooltip_text(Some(&label));
        set_accessible_label(&self.grid_size_scale, &label);
        let factory = build_grid_factory(
            &self.selection,
            &self.grid_context_menu,
            &self.thumbnails,
            size,
        );
        self.grid_view.set_factory(Some(&factory));
    }

    pub fn focus_view(&self, mode: ViewMode) {
        match mode {
            ViewMode::List => {
                self.list_view.grab_focus();
            }
            ViewMode::Grid => {
                self.grid_view.grab_focus();
            }
        }
    }

    pub fn set_views_sensitive(&self, sensitive: bool) {
        self.list_view.set_sensitive(sensitive);
        self.grid_view.set_sensitive(sensitive);
    }

    pub fn popdown_context_menus(&self) {
        self.list_context_menu.popdown();
        self.grid_context_menu.popdown();
    }

    pub fn context_menu(&self, mode: ViewMode) -> &gtk::PopoverMenu {
        match mode {
            ViewMode::List => &self.list_context_menu,
            ViewMode::Grid => &self.grid_context_menu,
        }
    }
}

pub fn build(
    application: &adw::Application,
    locations: &[Location],
    appearance: Appearance,
    preferences: ViewPreferences,
) -> BrowserWidgets {
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title("Floe")
        .default_width(1060)
        .default_height(720)
        .width_request(720)
        .height_request(480)
        .build();
    window.add_css_class("floe-window");
    window.add_css_class(appearance.class_name());

    let back_button = icon_button("go-previous-symbolic", "Back (Alt+Left)", "win.back");
    let forward_button = icon_button("go-next-symbolic", "Forward (Alt+Right)", "win.forward");
    let parent_button = icon_button("go-up-symbolic", "Parent folder (Alt+Up)", "win.parent");
    let hidden_button = gtk::ToggleButton::builder()
        .icon_name("view-reveal-symbolic")
        .tooltip_text("Show hidden files (Ctrl+H)")
        .action_name("win.hidden")
        .build();
    set_accessible_label(&hidden_button, "Show hidden files");
    let open_button = icon_button(
        "document-open-symbolic",
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
    let path_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .build();
    path_box.append(&gtk::Image::from_icon_name("folder-symbolic"));
    path_box.append(&path_label);

    let location_entry = gtk::Entry::builder()
        .placeholder_text("Enter a local path")
        .hexpand(true)
        .width_chars(42)
        .build();
    location_entry.set_tooltip_text(Some(
        "Type a local filesystem path. Floe retains original paths during normal browsing.",
    ));

    let path_stack = gtk::Stack::new();
    path_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    path_stack.add_named(&path_box, Some("path"));
    path_stack.add_named(&location_entry, Some("entry"));
    path_stack.set_visible_child_name("path");

    let header = adw::HeaderBar::new();
    header.pack_start(&back_button);
    header.pack_start(&forward_button);
    header.pack_start(&parent_button);
    header.set_title_widget(Some(&path_stack));
    let file_actions_model = gio::Menu::new();
    file_actions_model.append(Some("Open With…"), Some("win.open-with"));
    file_actions_model.append(Some("Copy"), Some("win.copy"));
    file_actions_model.append(Some("Move"), Some("win.cut"));
    file_actions_model.append(Some("Rename…"), Some("win.rename"));
    file_actions_model.append(Some("Move to Trash"), Some("win.trash"));
    let file_actions = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text("File actions")
        .menu_model(&file_actions_model)
        .build();
    set_accessible_label(&file_actions, "File actions");

    let list_view_button = gtk::ToggleButton::builder()
        .icon_name("view-list-symbolic")
        .tooltip_text("List view (Ctrl+1)")
        .action_name("win.view-list")
        .build();
    set_accessible_label(&list_view_button, "List view");
    let grid_view_button = gtk::ToggleButton::builder()
        .icon_name("view-grid-symbolic")
        .tooltip_text("Grid view (Ctrl+2)")
        .action_name("win.view-grid")
        .group(&list_view_button)
        .build();
    set_accessible_label(&grid_view_button, "Grid view");
    list_view_button.set_active(preferences.mode == ViewMode::List);
    grid_view_button.set_active(preferences.mode == ViewMode::Grid);
    let view_controls = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    view_controls.add_css_class("linked");
    view_controls.append(&list_view_button);
    view_controls.append(&grid_view_button);

    let zoom_out_button = icon_button(
        "zoom-out-symbolic",
        "Decrease grid icon size (Ctrl+-)",
        "win.zoom-out",
    );
    let zoom_in_button = icon_button(
        "zoom-in-symbolic",
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

    header.pack_end(&hidden_button);
    header.pack_end(&open_button);
    header.pack_end(&file_actions);
    header.pack_end(&grid_size_controls);
    header.pack_end(&view_controls);

    let (sidebar, location_buttons) = build_sidebar(locations, appearance.sidebar_min_width());
    let DirectoryPanelWidgets {
        content,
        selection,
        list_view,
        grid_view,
        view_stack,
        list_header,
        list_context_menu,
        grid_context_menu,
        empty_state,
        spinner,
        status_label,
        sort_headers,
        thumbnails,
    } = build_directory_panel(preferences);

    content.set_width_request(420);
    let workspace = gtk::Paned::builder()
        .orientation(gtk::Orientation::Horizontal)
        .position(appearance.sidebar_width())
        .wide_handle(true)
        .resize_start_child(false)
        .resize_end_child(true)
        .shrink_start_child(false)
        .shrink_end_child(false)
        .hexpand(true)
        .vexpand(true)
        .build();
    workspace.add_css_class("floe-workspace");
    workspace.set_start_child(Some(&sidebar));
    workspace.set_end_child(Some(&content));

    if !appearance.floating_panels() {
        sidebar.remove_css_class("floe-panel");
        content.remove_css_class("floe-panel");
    }

    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    root.append(&header);
    root.append(&workspace);

    let operations = build_operations_island();
    let content_overlay = gtk::Overlay::new();
    content_overlay.set_child(Some(&root));
    content_overlay.add_overlay(&operations.revealer);

    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&content_overlay));
    window.set_content(Some(&toast_overlay));

    BrowserWidgets {
        window,
        toast_overlay,
        back_button,
        forward_button,
        parent_button,
        hidden_button,
        path_label,
        path_stack,
        location_entry,
        selection,
        list_view,
        grid_view,
        view_stack,
        list_header,
        list_context_menu,
        grid_context_menu,
        list_view_button,
        grid_view_button,
        grid_size_controls,
        grid_size_scale,
        empty_state,
        spinner,
        status_label,
        sort_headers,
        thumbnails,
        location_buttons,
        operations,
    }
}

pub fn build_rename_dialog(current_name: &str) -> RenameDialogWidgets {
    let rename_entry = gtk::Entry::builder()
        .text(current_name)
        .activates_default(true)
        .hexpand(true)
        .build();
    set_accessible_label(&rename_entry, "New filename");

    let rename_error = gtk::Label::builder()
        .label("Invalid name")
        .halign(gtk::Align::Start)
        .wrap(true)
        .visible(false)
        .build();
    rename_error.add_css_class("error");
    set_accessible_label(&rename_error, "Rename error");

    let cancel_button = gtk::Button::with_label("Cancel");
    let rename_button = gtk::Button::with_label("Rename");
    rename_button.add_css_class("suggested-action");

    let actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .halign(gtk::Align::End)
        .spacing(8)
        .build();
    actions.append(&cancel_button);
    actions.append(&rename_button);

    let heading = gtk::Label::builder()
        .label("Rename item")
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
        .title("Rename item")
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

pub fn build_conflict_dialog(source_name: &str, destination: &str) -> ConflictDialogWidgets {
    let heading = gtk::Label::builder()
        .label("An item already exists")
        .halign(gtk::Align::Start)
        .build();
    heading.add_css_class("title-2");

    let explanation = gtk::Label::builder()
        .label("Keep the existing item, or retry with a different filename.")
        .halign(gtk::Align::Start)
        .wrap(true)
        .build();
    explanation.add_css_class("floe-status");

    let source_row = adw::ActionRow::builder()
        .title("Incoming item")
        .subtitle(source_name)
        .build();
    source_row.add_prefix(&gtk::Image::from_icon_name("document-open-symbolic"));
    let destination_row = adw::ActionRow::builder()
        .title("Existing destination")
        .subtitle(destination)
        .build();
    destination_row.add_prefix(&gtk::Image::from_icon_name("folder-symbolic"));
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
    let retry_button = gtk::Button::with_label(CONFLICT_DECISION_LABELS[1]);
    retry_button.add_css_class("suggested-action");
    retry_button.set_sensitive(false);

    let actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .halign(gtk::Align::End)
        .spacing(8)
        .build();
    actions.append(&cancel_button);
    actions.append(&keep_existing_button);
    actions.append(&retry_button);

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
    content.append(&actions);

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
        retry_button,
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
    let mut rows = Vec::with_capacity(options.applications.len());
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
        rows.push(list_row);
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
    let set_default_button = gtk::Button::with_label("Set as Default");
    let open_button = gtk::Button::with_label("Open");
    open_button.add_css_class("suggested-action");
    let actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::End)
        .build();
    actions.append(&cancel_button);
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
        rows,
        cancel_button,
        set_default_button,
        open_button,
    }
}

fn build_operations_island() -> OperationWidgets {
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

    let operation_progress = gtk::ProgressBar::builder()
        .hexpand(true)
        .width_request(220)
        .build();
    set_accessible_label(&operation_progress, "File operation progress");

    let operation_cancel = gtk::Button::builder()
        .icon_name("process-stop-symbolic")
        .tooltip_text("Cancel file operation")
        .has_frame(false)
        .build();
    set_accessible_label(&operation_cancel, "Cancel file operation");

    let operation_retry = gtk::Button::builder()
        .label("Retry")
        .tooltip_text("Retry file operation")
        .visible(false)
        .build();

    let progress_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .build();
    progress_row.append(&operation_progress);
    progress_row.append(&operation_retry);
    progress_row.append(&operation_cancel);

    let island = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .width_request(300)
        .build();
    island.add_css_class("operations-island");
    island.append(&operation_label);
    island.append(&operation_detail);
    island.append(&progress_row);

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
        operation_cancel,
    }
}

fn build_sidebar(locations: &[Location], minimum_width: i32) -> (gtk::Box, Vec<gtk::Button>) {
    let sidebar = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .width_request(minimum_width)
        .vexpand(true)
        .build();
    sidebar.add_css_class("floe-panel");
    sidebar.add_css_class("floe-sidebar");

    let heading = gtk::Label::builder()
        .label("Places")
        .halign(gtk::Align::Start)
        .margin_start(10)
        .margin_bottom(6)
        .build();
    heading.add_css_class("heading");
    sidebar.append(&heading);

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
            .build();
        content.append(&label);

        let button = gtk::Button::builder()
            .child(&content)
            .has_frame(false)
            .build();
        set_accessible_label(&button, location.label);
        sidebar.append(&button);
        buttons.push(button);
    }

    let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    spacer.set_vexpand(true);
    sidebar.append(&spacer);

    let mode = gtk::Label::builder()
        .label("Local files · Generic Wayland")
        .halign(gtk::Align::Start)
        .margin_start(10)
        .wrap(true)
        .build();
    mode.add_css_class("floe-status");
    sidebar.append(&mode);

    (sidebar, buttons)
}

fn build_directory_panel(preferences: ViewPreferences) -> DirectoryPanelWidgets {
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    let selection = gtk::SingleSelection::new(Some(store));
    selection.set_autoselect(false);
    selection.set_can_unselect(true);

    let list_context_menu = gtk::PopoverMenu::from_model(Some(&build_file_context_menu_model()));
    list_context_menu.set_has_arrow(false);
    let grid_context_menu = gtk::PopoverMenu::from_model(Some(&build_file_context_menu_model()));
    grid_context_menu.set_has_arrow(false);

    let thumbnails = ThumbnailPresentation::new();
    let factory = gtk::SignalListItemFactory::new();
    let row_selection = selection.clone();
    let row_context_menu = list_context_menu.clone();
    factory.connect_setup(move |_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .build();
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
        row.append(&icon);
        row.append(&name);
        row.append(&entry_type);
        row.append(&size);
        row.append(&modified);

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

            selection.set_selected(position);
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
        list_item.set_child(Some(&row));
    });
    let thumbnails_for_bind = thumbnails.clone();
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
        let Some(object) = list_item.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let entry = object.borrow::<std::sync::Arc<DirectoryEntry>>();
        let display_name = entry.display_name_lossy();
        name.set_label(&display_name);
        name.set_tooltip_text(Some(&display_name));
        thumbnails_for_bind.request_thumbnail(&icon, &entry);
        entry_type.set_label(entry_type_label(entry.kind()));
        size.set_label(&entry.size().map(format_size).unwrap_or_default());
        let modified_text = entry
            .modified()
            .and_then(format_modified)
            .unwrap_or_default();
        modified.set_label(&modified_text);
        modified.set_tooltip_text((!modified_text.is_empty()).then_some(modified_text.as_str()));
    });
    let thumbnails_for_unbind = thumbnails.clone();
    factory.connect_unbind(move |_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(row) = list_item.child().and_downcast::<gtk::Box>() else {
            return;
        };
        let Some(icon) = row.first_child().and_downcast::<gtk::Image>() else {
            return;
        };
        thumbnails_for_unbind.unbind(&icon);
    });

    let list_view = gtk::ListView::new(Some(selection.clone()), Some(factory));
    list_view.add_css_class("floe-directory-list");
    list_view.set_single_click_activate(false);
    list_view.set_vexpand(true);
    list_context_menu.set_parent(&list_view);

    let list_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&list_view)
        .vexpand(true)
        .build();
    let grid_factory = build_grid_factory(
        &selection,
        &grid_context_menu,
        &thumbnails,
        preferences.grid_size,
    );
    let grid_view = gtk::GridView::new(Some(selection.clone()), Some(grid_factory));
    grid_view.add_css_class("floe-directory-grid");
    grid_view.set_single_click_activate(false);
    grid_view.set_enable_rubberband(true);
    grid_view.set_min_columns(1);
    grid_view.set_max_columns(24);
    grid_view.set_vexpand(true);
    grid_context_menu.set_parent(&grid_view);
    let grid_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&grid_view)
        .vexpand(true)
        .build();
    let view_stack = gtk::Stack::new();
    view_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    view_stack.add_named(&list_scroller, Some("list"));
    view_stack.add_named(&grid_scroller, Some("grid"));
    view_stack.set_visible_child_name(preferences.mode.stack_name());
    view_stack.set_vexpand(true);

    let empty_icon = gtk::Image::builder()
        .icon_name("folder-symbolic")
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
    empty_state.set_visible(false);

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&view_stack));
    overlay.add_overlay(&empty_state);
    overlay.set_vexpand(true);

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
    let (list_header, sort_headers) = build_list_header();
    list_header.set_visible(preferences.mode == ViewMode::List);
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
        view_stack,
        list_header,
        list_context_menu,
        grid_context_menu,
        empty_state,
        spinner,
        status_label,
        sort_headers,
        thumbnails,
    }
}

fn build_grid_factory(
    selection: &gtk::SingleSelection,
    context_menu: &gtk::PopoverMenu,
    thumbnails: &ThumbnailPresentation,
    grid_size: GridSize,
) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    let row_selection = selection.clone();
    let row_context_menu = context_menu.clone();
    let edge = grid_size.edge();
    let tile_width = grid_size.tile_width();
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
            let position = list_item.position();
            if !is_bound_list_position(position) {
                return;
            }
            selection.set_selected(position);
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
        list_item.set_child(Some(&cell));
    });

    let thumbnails_for_bind = thumbnails.clone();
    factory.connect_bind(move |_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(cell) = list_item.child().and_downcast::<gtk::Box>() else {
            return;
        };
        let Some(icon) = cell.first_child().and_downcast::<gtk::Image>() else {
            return;
        };
        let Some(name) = icon.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(object) = list_item.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let entry = object.borrow::<std::sync::Arc<DirectoryEntry>>();
        let display_name = entry.display_name_lossy();
        name.set_label(&display_name);
        name.set_tooltip_text(Some(&display_name));
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
        let Some(icon) = cell.first_child().and_downcast::<gtk::Image>() else {
            return;
        };
        thumbnails_for_unbind.unbind(&icon);
    });
    factory
}

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
    menu.append_section(None, &primary);

    let editing = gio::Menu::new();
    for (label, action) in &FILE_CONTEXT_ACTIONS[2..5] {
        editing.append(Some(label), Some(action));
    }
    menu.append_section(None, &editing);

    let destructive = gio::Menu::new();
    destructive.append(
        Some(FILE_CONTEXT_ACTIONS[5].0),
        Some(FILE_CONTEXT_ACTIONS[5].1),
    );
    menu.append_section(None, &destructive);

    menu
}

fn is_bound_list_position(position: u32) -> bool {
    position != gtk::INVALID_LIST_POSITION
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

fn apply_entry_icon(image: &gtk::Image, entry: &DirectoryEntry, pixel_size: i32) {
    for icon in EntryIcon::ALL {
        image.remove_css_class(icon.css_class());
    }
    let icon = icon_for_entry(entry);
    image.add_css_class("floe-entry-icon");
    image.add_css_class(icon.css_class());
    image.set_pixel_size(pixel_size);
    image.set_icon_name(Some(icon.icon_name()));
}

fn apply_thumbnail(image: &gtk::Image, texture: &gtk::gdk::Texture, edge: u16) {
    for icon in EntryIcon::ALL {
        image.remove_css_class(icon.css_class());
    }
    image.set_pixel_size(i32::from(edge));
    image.set_paintable(Some(texture));
    image.add_css_class("floe-thumbnail");
}

fn build_list_header() -> (gtk::Box, Vec<SortHeaderWidgets>) {
    let header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .build();
    header.add_css_class("floe-list-header");

    header.append(
        &gtk::Box::builder()
            .width_request(i32::from(LIST_THUMBNAIL_EDGE))
            .build(),
    );
    let mut widgets = Vec::with_capacity(SortColumn::ALL.len());
    for (index, ((column, label), width)) in SortColumn::ALL
        .into_iter()
        .zip(LIST_COLUMN_LABELS)
        .zip([
            None,
            Some(TYPE_COLUMN_WIDTH),
            Some(SIZE_COLUMN_WIDTH),
            Some(MODIFIED_COLUMN_WIDTH),
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
        button.add_css_class("flat");
        button.add_css_class("floe-sort-heading");
        header.append(&button);
        widgets.push(SortHeaderWidgets {
            column,
            button,
            label: heading,
        });
    }

    (header, widgets)
}

fn sort_action_name(column: SortColumn) -> &'static str {
    match column {
        SortColumn::Name => "win.sort-name",
        SortColumn::Type => "win.sort-type",
        SortColumn::Size => "win.sort-size",
        SortColumn::Modified => "win.sort-modified",
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
    use std::{path::PathBuf, time::Duration};

    use super::*;

    #[test]
    fn phase_6a_columns_have_stable_scannable_semantics() {
        assert_eq!(LIST_COLUMN_LABELS, ["Name", "Type", "Size", "Modified"]);
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
        let presentation = ThumbnailPresentation::new();
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
    fn phase_5c_context_menu_reuses_complete_existing_action_mapping() {
        assert_eq!(
            FILE_CONTEXT_ACTIONS,
            [
                ("Open", "win.open"),
                ("Open With…", "win.open-with"),
                ("Copy", "win.copy"),
                ("Cut", "win.cut"),
                ("Rename…", "win.rename"),
                ("Move to Trash", "win.trash"),
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
}
