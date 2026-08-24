use adw::prelude::*;
use floe_core::{DirectoryEntry, EntryKind};
use gtk::{gio, glib};

use crate::{appearance::Appearance, launcher::OpenWithOptions, locations::Location};

const FILE_CONTEXT_ACTIONS: [(&str, &str); 6] = [
    ("Open", "win.open"),
    ("Open With…", "win.open-with"),
    ("Copy", "win.copy"),
    ("Cut", "win.cut"),
    ("Rename…", "win.rename"),
    ("Move to Trash", "win.trash"),
];

const CONFLICT_DECISION_LABELS: [&str; 2] = ["Keep Existing", "Retry with New Name"];

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
    pub context_menu: gtk::PopoverMenu,
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
    header.pack_end(&hidden_button);
    header.pack_end(&open_button);
    header.pack_end(&file_actions);

    let (sidebar, location_buttons) = build_sidebar(locations, appearance.sidebar_min_width());
    let (content, selection, list_view, context_menu, empty_state, spinner, status_label) =
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
        context_menu,
        empty_state,
        spinner,
        status_label,
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

fn build_directory_panel() -> (
    gtk::Box,
    gtk::SingleSelection,
    gtk::ListView,
    gtk::PopoverMenu,
    gtk::Box,
    gtk::Spinner,
    gtk::Label,
) {
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    let selection = gtk::SingleSelection::new(Some(store));
    selection.set_autoselect(false);
    selection.set_can_unselect(true);

    let context_menu = gtk::PopoverMenu::from_model(Some(&build_file_context_menu_model()));
    context_menu.set_has_arrow(false);

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
    context_menu.set_parent(&list_view);

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
        context_menu,
        empty_state,
        spinner,
        status_label,
    )
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

#[cfg(test)]
mod tests {
    use super::*;

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
