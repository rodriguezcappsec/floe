use adw::prelude::*;
use floe_core::{DirectoryEntry, EntryKind};
use gtk::{gio, glib};

use crate::{appearance::Appearance, locations::Location};

#[derive(Clone)]
pub struct OperationWidgets {
    pub revealer: gtk::Revealer,
    pub operation_label: gtk::Label,
    pub operation_detail: gtk::Label,
    pub operation_progress: gtk::ProgressBar,
    pub operation_cancel: gtk::Button,
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
    pub empty_state: gtk::Box,
    pub spinner: gtk::Spinner,
    pub status_label: gtk::Label,
    pub location_buttons: Vec<gtk::Button>,
    pub operations: OperationWidgets,
}

pub fn build(
    application: &adw::Application,
    locations: &[Location],
    appearance: Appearance,
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
    header.pack_end(&hidden_button);
    header.pack_end(&open_button);

    let (sidebar, location_buttons) = build_sidebar(locations, appearance.sidebar_min_width());
    let (content, selection, list_view, empty_state, spinner, status_label) =
        build_directory_panel();

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
        empty_state,
        spinner,
        status_label,
        location_buttons,
        operations,
    }
}

fn build_operations_island() -> OperationWidgets {
    let operation_label = gtk::Label::builder()
        .label("Copying item")
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
    set_accessible_label(&operation_progress, "Copy progress");

    let operation_cancel = gtk::Button::builder()
        .icon_name("process-stop-symbolic")
        .tooltip_text("Cancel copy")
        .has_frame(false)
        .build();
    set_accessible_label(&operation_cancel, "Cancel copy");

    let progress_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .build();
    progress_row.append(&operation_progress);
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

fn build_directory_panel() -> (
    gtk::Box,
    gtk::SingleSelection,
    gtk::ListView,
    gtk::Box,
    gtk::Spinner,
    gtk::Label,
) {
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    let selection = gtk::SingleSelection::new(Some(store));
    selection.set_autoselect(false);
    selection.set_can_unselect(true);

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .build();
        let icon = gtk::Image::builder().pixel_size(24).build();
        let name = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .single_line_mode(true)
            .build();
        name.add_css_class("floe-entry-name");
        let detail = gtk::Label::builder()
            .halign(gtk::Align::End)
            .width_chars(13)
            .xalign(1.0)
            .build();
        detail.add_css_class("floe-entry-detail");
        row.append(&icon);
        row.append(&name);
        row.append(&detail);
        list_item.set_child(Some(&row));
    });
    factory.connect_bind(|_, object| {
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
        let Some(detail) = name.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(object) = list_item.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let entry = object.borrow::<DirectoryEntry>();
        let display_name = entry.display_name_lossy();
        name.set_label(&display_name);
        name.set_tooltip_text(Some(&display_name));
        icon.set_icon_name(Some(icon_name(entry.kind())));
        detail.set_label(&entry_detail(&entry));
    });

    let list_view = gtk::ListView::new(Some(selection.clone()), Some(factory));
    list_view.add_css_class("floe-directory-list");
    list_view.set_single_click_activate(false);
    list_view.set_vexpand(true);

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&list_view)
        .vexpand(true)
        .build();

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
    overlay.set_child(Some(&scroller));
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
    panel.append(&overlay);
    panel.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    panel.append(&status);

    (
        panel,
        selection,
        list_view,
        empty_state,
        spinner,
        status_label,
    )
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

fn icon_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Directory => "folder-symbolic",
        EntryKind::RegularFile => "text-x-generic-symbolic",
        EntryKind::SymbolicLink { .. } => "emblem-symbolic-link-symbolic",
        EntryKind::Other => "application-x-generic-symbolic",
    }
}

fn entry_detail(entry: &DirectoryEntry) -> String {
    match entry.kind() {
        EntryKind::Directory => "Folder".into(),
        EntryKind::SymbolicLink { .. } => "Link".into(),
        EntryKind::Other => "Special".into(),
        EntryKind::RegularFile => entry.size().map(format_size).unwrap_or_default(),
    }
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
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
