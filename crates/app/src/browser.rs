use std::{
    cell::{Cell, RefCell},
    collections::{HashSet, VecDeque},
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use adw::prelude::*;
use floe_core::{
    DirectoryEntry, DirectoryError, DirectorySort, EntryKind, NavigationState, SortColumn,
};
use gtk::{gio, glib};

use crate::{
    bookmarks::{BookmarkWorker, BookmarkWorkerEvent},
    devices::{
        DeviceAction, DeviceActionOutcome, DeviceId, DeviceMonitor, DeviceSnapshot,
        DeviceSubscriptionId,
    },
    launcher,
    location_input::{
        PendingLocation, location_failure_message, location_text, resolve_location_input,
    },
    locations::Location,
    preferences::{
        PreferenceSubmitError, PreferenceWorker, SIDEBAR_WIDTH_MAX, SIDEBAR_WIDTH_MIN,
        SidebarDensity, ViewPreferences, clamp_sidebar_width,
    },
    state::{ApplicationState, TransferIntent, validate_rename_name},
    thumbnail::{ThumbnailSubmitError, ThumbnailWorker},
    ui::{self, BrowserWidgets},
    view::{GridSize, VIEW_ACTIONS, ViewCommand, ViewMode},
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

const SIDEBAR_PERSIST_DEBOUNCE: Duration = Duration::from_millis(320);

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
    bookmarks: Option<BookmarkWorker>,
    devices: DeviceMonitor,
    preferences: Option<PreferenceWorker>,
}

impl BrowserServices {
    pub fn new(
        browser: BrowserWorker,
        thumbnails: Option<ThumbnailWorker>,
        bookmarks: Option<BookmarkWorker>,
        devices: DeviceMonitor,
        preferences: Option<PreferenceWorker>,
    ) -> Self {
        Self {
            browser,
            thumbnails,
            bookmarks,
            devices,
            preferences,
        }
    }
}

pub struct BrowserController {
    widgets: BrowserWidgets,
    navigation: RefCell<NavigationState>,
    worker: RefCell<BrowserWorker>,
    thumbnail_worker: RefCell<Option<ThumbnailWorker>>,
    thumbnail_generation: Cell<u64>,
    active_generation: Cell<u64>,
    show_hidden: Cell<bool>,
    visible_entries: RefCell<Vec<Arc<DirectoryEntry>>>,
    pending_entries: RefCell<VecDeque<Arc<DirectoryEntry>>>,
    pending_store: RefCell<Option<gio::ListStore>>,
    pending_total: Cell<usize>,
    pending_selection_indices: RefCell<Vec<u32>>,
    selected_entries: RefCell<Vec<Arc<DirectoryEntry>>>,
    sort_order: Cell<DirectorySort>,
    sort_in_flight: Cell<bool>,
    sort_selection_paths: RefCell<Vec<PathBuf>>,
    view_mode: Cell<ViewMode>,
    grid_size: Cell<GridSize>,
    preference_worker: RefCell<Option<PreferenceWorker>>,
    pending_preferences: Cell<Option<ViewPreferences>>,
    current_preferences: RefCell<ViewPreferences>,
    sidebar_save_source: RefCell<Option<glib::SourceId>>,
    ignore_sidebar_position_signal: Cell<bool>,
    pending_location: RefCell<Option<PendingLocation>>,
    application_state: Rc<ApplicationState>,
    bookmark_worker: RefCell<Option<BookmarkWorker>>,
    bookmarks: RefCell<Vec<PathBuf>>,
    bookmarks_loaded: Cell<bool>,
    bookmark_revision: Cell<u64>,
    bookmark_save_in_flight: Cell<bool>,
    device_monitor: DeviceMonitor,
    device_subscription: Cell<Option<DeviceSubscriptionId>>,
}

impl Drop for BrowserController {
    fn drop(&mut self) {
        if let Some(source) = self.sidebar_save_source.get_mut().take() {
            source.remove();
        }
        if let Some(subscription) = self.device_subscription.take() {
            self.device_monitor.disconnect_changed(subscription);
        }
        let Some(worker) = self.preference_worker.get_mut().as_ref() else {
            return;
        };
        if let Err(error) = worker.save_before_shutdown(*self.current_preferences.get_mut()) {
            tracing::warn!(%error, "could not submit final view preferences");
        }
    }
}

impl BrowserController {
    pub fn new(
        widgets: BrowserWidgets,
        initial_path: PathBuf,
        services: BrowserServices,
        view_preferences: ViewPreferences,
        application_state: Rc<ApplicationState>,
    ) -> Rc<Self> {
        let BrowserServices {
            browser,
            thumbnails,
            bookmarks,
            devices,
            preferences,
        } = services;
        Rc::new(Self {
            widgets,
            navigation: RefCell::new(NavigationState::new(initial_path)),
            worker: RefCell::new(browser),
            thumbnail_worker: RefCell::new(thumbnails),
            thumbnail_generation: Cell::new(0),
            active_generation: Cell::new(0),
            show_hidden: Cell::new(false),
            visible_entries: RefCell::new(Vec::new()),
            pending_entries: RefCell::new(VecDeque::new()),
            pending_store: RefCell::new(None),
            pending_total: Cell::new(0),
            pending_selection_indices: RefCell::new(Vec::new()),
            selected_entries: RefCell::new(Vec::new()),
            sort_order: Cell::new(DirectorySort::default()),
            sort_in_flight: Cell::new(false),
            sort_selection_paths: RefCell::new(Vec::new()),
            view_mode: Cell::new(view_preferences.mode),
            grid_size: Cell::new(view_preferences.grid_size),
            preference_worker: RefCell::new(preferences),
            pending_preferences: Cell::new(None),
            current_preferences: RefCell::new(view_preferences),
            sidebar_save_source: RefCell::new(None),
            ignore_sidebar_position_signal: Cell::new(false),
            pending_location: RefCell::new(None),
            application_state,
            bookmark_worker: RefCell::new(bookmarks),
            bookmarks: RefCell::new(Vec::new()),
            bookmarks_loaded: Cell::new(false),
            bookmark_revision: Cell::new(0),
            bookmark_save_in_flight: Cell::new(false),
            device_monitor: devices,
            device_subscription: Cell::new(None),
        })
    }

    pub fn wire(self: &Rc<Self>, application: &adw::Application, locations: &[Location]) {
        self.install_actions(application);
        self.update_sort_headers();
        self.widgets
            .apply_sidebar_density(self.current_preferences.borrow().sidebar_density);

        for (button, location) in self.widgets.location_buttons.iter().zip(locations) {
            let controller = Rc::downgrade(self);
            let path = exact_sidebar_target(&location.path);
            button.connect_clicked(move |_| {
                if let Some(controller) = controller.upgrade() {
                    controller.navigate_to(exact_sidebar_target(&path));
                }
            });
        }
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
            .selection
            .connect_selection_changed(move |_, _, _| {
                if let Some(controller) = controller.upgrade() {
                    controller.selection_changed();
                }
            });

        self.install_file_view_shortcuts(&self.widgets.list_view);
        self.install_file_view_shortcuts(&self.widgets.grid_view);

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

    fn add_current_bookmark(self: &Rc<Self>) {
        if !self.bookmarks_loaded.get() {
            self.show_toast("Bookmarks are still loading", 4);
            return;
        }
        let current = self.navigation.borrow().current().to_path_buf();
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
            let status = sidebar_status_label(&policy.status);
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
            } else {
                glib::Propagation::Proceed
            }
        });
        view.add_controller(shortcuts);
    }

    pub fn present_and_start(self: &Rc<Self>) {
        self.widgets.window.present();
        self.arm_sidebar_width_persistence();
        self.load_current();

        let controller = Rc::clone(self);
        glib::timeout_add_local(Duration::from_millis(16), move || {
            if !controller.widgets.window.is_visible() {
                return glib::ControlFlow::Break;
            }
            controller.drain_worker();
            controller.drain_bookmark_worker();
            controller.pump_pending_entries();
            controller.submit_thumbnail_requests();
            controller.drain_thumbnail_worker();
            controller.flush_pending_preferences();
            glib::ControlFlow::Continue
        });
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

    fn install_actions(self: &Rc<Self>, application: &adw::Application) {
        self.add_action("back", |controller| controller.go_back());
        self.add_action("forward", |controller| controller.go_forward());
        self.add_action("parent", |controller| controller.go_parent());
        self.add_action("location", |controller| controller.show_location_entry());
        self.add_action("cancel-location", |controller| {
            controller.cancel_location_entry();
        });
        self.add_action("hidden", |controller| controller.toggle_hidden());
        self.add_action("refresh", |controller| {
            controller.load_current();
        });
        self.add_action("select-all", |controller| controller.select_all());
        self.add_action("clear-selection", |controller| controller.clear_selection());
        for (name, command) in VIEW_ACTIONS {
            self.add_action(name, move |controller| {
                controller.apply_view_command(command);
            });
        }

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

        self.add_action("reset-sidebar-width", |controller| {
            controller.reset_sidebar_width();
        });
        let open_action = self.add_action("open", |controller| controller.activate_selected());
        open_action.set_enabled(false);
        let open_with_action =
            self.add_action("open-with", |controller| controller.show_open_with());
        open_with_action.set_enabled(false);
        let copy_action = self.add_action("copy", |controller| controller.stage_selected_copy());
        copy_action.set_enabled(false);
        let cut_action = self.add_action("cut", |controller| controller.stage_selected_move());
        cut_action.set_enabled(false);
        let rename_action = self.add_action("rename", |controller| controller.show_rename());
        rename_action.set_enabled(false);
        let trash_action = self.add_action("trash", |controller| controller.trash_selected());
        trash_action.set_enabled(false);
        let permanent_delete_action = self.add_action("permanent-delete", |controller| {
            controller.confirm_permanent_delete();
        });
        permanent_delete_action.set_enabled(false);
        let paste_action = self.add_action("paste", |controller| controller.paste_transfer());
        paste_action.set_enabled(false);
        for (name, column) in ui::SORT_ACTIONS {
            self.add_action(name, move |controller| controller.change_sort(column));
        }

        application.set_accels_for_action("win.back", &["<Alt>Left"]);
        application.set_accels_for_action("win.forward", &["<Alt>Right"]);
        application.set_accels_for_action("win.parent", &["<Alt>Up"]);
        application.set_accels_for_action("win.location", &["<Control>l"]);
        application.set_accels_for_action("win.hidden", &["<Control>h"]);
        application.set_accels_for_action("win.refresh", &["F5", "<Control>r"]);
        application.set_accels_for_action("win.select-all", &["<Control>a"]);
        application.set_accels_for_action("win.clear-selection", &["<Control><Shift>a"]);
        application.set_accels_for_action("win.cancel-location", &["Escape"]);
        application.set_accels_for_action("win.copy", &["<Control>c"]);
        application.set_accels_for_action("win.cut", &["<Control>x"]);
        application.set_accels_for_action("win.paste", &["<Control>v"]);
        application.set_accels_for_action("win.rename", &["F2"]);
        application.set_accels_for_action("win.permanent-delete", &["<Shift>Delete"]);
        application.set_accels_for_action("win.view-list", &["<Control>1"]);
        application.set_accels_for_action("win.view-grid", &["<Control>2"]);
        application.set_accels_for_action("win.zoom-out", &["<Control>minus"]);
        application.set_accels_for_action("win.zoom-in", &["<Control>plus", "<Control>equal"]);
    }

    fn add_action<F>(self: &Rc<Self>, name: &str, callback: F) -> gio::SimpleAction
    where
        F: Fn(&Self) + 'static,
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

    fn navigate_to(&self, destination: PathBuf) {
        self.restore_pending_navigation();
        if self.navigation.borrow_mut().navigate_to(destination) {
            self.load_current();
        }
    }

    fn go_back(&self) {
        self.restore_pending_navigation();
        if self.navigation.borrow_mut().go_back() {
            self.load_current();
        }
    }

    fn go_forward(&self) {
        self.restore_pending_navigation();
        if self.navigation.borrow_mut().go_forward() {
            self.load_current();
        }
    }

    fn go_parent(&self) {
        self.restore_pending_navigation();
        if self.navigation.borrow_mut().go_parent() {
            self.load_current();
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
        self.widgets.popdown_context_menus();
        self.widgets.set_view_mode(mode);
        self.widgets.focus_view(mode);
        if self.view_mode.replace(mode) != mode {
            self.queue_preferences();
        }
    }

    fn apply_view_command(&self, command: ViewCommand) {
        match command {
            ViewCommand::List => self.change_view_mode(ViewMode::List),
            ViewCommand::Grid => self.change_view_mode(ViewMode::Grid),
            ViewCommand::ZoomIn => self.change_grid_size(self.grid_size.get().zoom_in()),
            ViewCommand::ZoomOut => self.change_grid_size(self.grid_size.get().zoom_out()),
        }
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

    fn queue_preferences(&self) {
        let preferences = with_current_view_preferences(
            *self.current_preferences.borrow(),
            self.view_mode.get(),
            self.grid_size.get(),
        );
        *self.current_preferences.borrow_mut() = preferences;
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
        let reset = preferences_after_sidebar_reset(*self.current_preferences.borrow());
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
        self.sort_order.set(sort);
        self.update_sort_headers();
        let entries = self.visible_entries.borrow().clone();
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

        let path = self.navigation.borrow().current().to_path_buf();
        let generation = self.worker.borrow_mut().request_sort(path, entries, sort);
        self.active_generation.set(generation);
    }

    fn update_sort_headers(&self) {
        let sort = self.sort_order.get();
        for header in &self.widgets.sort_headers {
            ui::update_sort_header(header, sort);
        }
    }

    fn set_sort_controls_sensitive(&self, sensitive: bool) {
        for header in &self.widgets.sort_headers {
            header.button.set_sensitive(sensitive);
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
        let current = self.navigation.borrow().current().to_path_buf();
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

    fn restore_pending_navigation(&self) {
        if let Some(pending) = self.pending_location.borrow_mut().take() {
            self.navigation.replace(pending.previous_navigation);
        }
    }

    fn submit_location_entry(&self, input: &str) {
        let current = self.navigation.borrow().current().to_path_buf();
        let destination = match resolve_location_input(input, &current) {
            Ok(path) => path,
            Err(error) => {
                self.show_location_error(&error.to_string());
                return;
            }
        };

        if destination == self.navigation.borrow().current() {
            self.hide_location_entry();
            return;
        }

        let previous_navigation = self.navigation.borrow().clone();
        if !self.navigation.borrow_mut().navigate_to(destination) {
            self.hide_location_entry();
            return;
        }

        self.clear_location_error();
        self.widgets.location_entry.set_sensitive(false);
        let generation = self.load_current();
        self.pending_location.replace(Some(PendingLocation {
            generation,
            previous_navigation,
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
        self.widgets.thumbnails.begin_generation();
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
        self.set_selection_actions_enabled(false, false, false);
        let path = self.navigation.borrow().current().to_path_buf();
        let generation = self
            .worker
            .borrow_mut()
            .request(path.clone(), self.sort_order.get());
        self.active_generation.set(generation);

        let display_path = path.to_string_lossy();
        self.widgets.path_label.set_label(&display_path);
        self.widgets
            .path_label
            .set_tooltip_text(Some(&display_path));
        let title = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy();
        self.widgets
            .window
            .set_title(Some(&format!("{title} — Floe")));
        self.widgets.spinner.start();
        self.widgets.status_label.set_label("Loading directory…");
        self.update_navigation_controls();
        generation
    }

    fn update_navigation_controls(&self) {
        let navigation = self.navigation.borrow();
        self.widgets
            .back_button
            .set_sensitive(navigation.can_go_back());
        self.widgets
            .forward_button
            .set_sensitive(navigation.can_go_forward());
        self.widgets
            .parent_button
            .set_sensitive(navigation.can_go_parent());
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
                ResponseKind::Sorted { entries, sort } => {
                    self.sort_in_flight.set(false);
                    self.set_sort_controls_sensitive(true);
                    if sort != self.sort_order.get() {
                        continue;
                    }
                    let selected_paths = self.sort_selection_paths.take();
                    self.install_entries(entries, &selected_paths, false);
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
        let submitted_text = pending.restore(&mut self.navigation.borrow_mut());
        self.load_current();
        self.widgets.location_entry.set_text(&submitted_text);
        self.show_location_error(&location_failure_message(error));
        true
    }

    fn show_listing(&self, entries: Vec<DirectoryEntry>) {
        let show_hidden = self.show_hidden.get();
        let entries: Vec<Arc<DirectoryEntry>> = entries
            .into_iter()
            .filter(|entry| show_hidden || !entry.is_hidden())
            .map(Arc::new)
            .collect();
        self.install_entries(entries, &[], true);
    }

    fn install_entries(
        &self,
        entries: Vec<Arc<DirectoryEntry>>,
        selected_paths: &[PathBuf],
        focus_list: bool,
    ) {
        let count = entries.len();
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
        self.pending_store.replace(Some(store));
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
        let state = selection_action_state(&selected_entries);
        self.selected_entries.replace(selected_entries);
        self.set_open_enabled(state.single);
        self.set_open_with_enabled(state.open_with);
        self.set_selection_actions_enabled(state.transfer, state.rename, state.trash);
        self.refresh_status();
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
        let label = selection_status(self.pending_total.get(), &self.selected_entries.borrow());
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
        let total = self.pending_total.get();
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
                status_label.set_label(&selection_status(total, &selected));
            } else {
                let selected = selected_entries_for_selection(&selection);
                status_label.set_label(&selection_status(total, &selected));
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
        for (action_name, enabled) in [
            ("copy", transfer),
            ("cut", transfer),
            ("rename", rename),
            ("trash", trash),
            ("permanent-delete", trash),
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

        match self.application_state.stage_copy_many(paths) {
            Ok(()) => {
                self.set_paste_enabled(true);
                self.show_toast(
                    &format!(
                        "Ready to copy {}. Open a destination and press Ctrl+V.",
                        item_count_text(count)
                    ),
                    5,
                );
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
        match self.application_state.stage_move_many(paths) {
            Ok(()) => {
                self.set_paste_enabled(true);
                self.show_toast(
                    &format!(
                        "Ready to move {}. Open a destination and press Ctrl+V.",
                        item_count_text(count)
                    ),
                    5,
                );
            }
            Err(error) => self.show_toast(&format!("Could not stage move: {error}"), 6),
        }
    }

    fn paste_transfer(&self) {
        let destination = self.navigation.borrow().current().to_path_buf();
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

    fn confirm_permanent_delete(&self) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            self.show_toast("Select one or more items to delete permanently", 4);
            return;
        }

        let labels = paths
            .iter()
            .map(|path| permanent_delete_target_label(path))
            .collect::<Vec<_>>();
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
        if self.navigation.borrow().current() == directory {
            self.load_current();
        }
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
    transfer: bool,
    rename: bool,
    trash: bool,
}

fn selection_action_state(entries: &[Arc<DirectoryEntry>]) -> SelectionActionState {
    let single = entries.len() == 1;
    SelectionActionState {
        single,
        open_with: single && open_with_eligible(&entries[0]),
        transfer: !entries.is_empty()
            && entries
                .iter()
                .all(|entry| !matches!(entry.kind(), EntryKind::Other)),
        rename: single,
        trash: !entries.is_empty(),
    }
}

fn selection_status(total: usize, selected: &[Arc<DirectoryEntry>]) -> String {
    match selected {
        [] => item_count_text(total),
        [entry] => format!("{} selected", entry.display_name_lossy()),
        entries => format!("{} selected", item_count_text(entries.len())),
    }
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
        let current = ViewPreferences {
            mode: ViewMode::List,
            grid_size: GridSize::default(),
            sidebar_density: SidebarDensity::Comfortable,
            sidebar_width: Some(312),
        };
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

        let current = ViewPreferences {
            mode: ViewMode::Grid,
            grid_size: GridSize::default(),
            sidebar_density: SidebarDensity::Balanced,
            sidebar_width: Some(312),
        };
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
                transfer: false,
                rename: false,
                trash: false,
            }
        );
        assert_eq!(
            selection_action_state(&entries[..1]),
            SelectionActionState {
                single: true,
                open_with: true,
                transfer: true,
                rename: true,
                trash: true,
            }
        );
        assert_eq!(
            selection_action_state(&entries),
            SelectionActionState {
                single: false,
                open_with: false,
                transfer: true,
                rename: false,
                trash: true,
            }
        );
        assert_eq!(
            selection_status(entries.len(), &entries),
            "2 items selected"
        );
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
}
