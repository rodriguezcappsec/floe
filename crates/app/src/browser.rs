use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    ffi::{OsStr, OsString},
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
    launcher,
    locations::Location,
    state::{ApplicationState, TransferIntent, validate_rename_name},
    thumbnail::{ThumbnailSubmitError, ThumbnailWorker},
    ui::{self, BrowserWidgets},
    worker::{BrowserWorker, ResponseKind},
};

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
    pending_selection_index: Cell<Option<u32>>,
    selected_entry: RefCell<Option<Arc<DirectoryEntry>>>,
    sort_order: Cell<DirectorySort>,
    sort_in_flight: Cell<bool>,
    sort_selection_path: RefCell<Option<PathBuf>>,
    application_state: Rc<ApplicationState>,
}

impl BrowserController {
    pub fn new(
        widgets: BrowserWidgets,
        initial_path: PathBuf,
        worker: BrowserWorker,
        thumbnail_worker: Option<ThumbnailWorker>,
        application_state: Rc<ApplicationState>,
    ) -> Rc<Self> {
        Rc::new(Self {
            widgets,
            navigation: RefCell::new(NavigationState::new(initial_path)),
            worker: RefCell::new(worker),
            thumbnail_worker: RefCell::new(thumbnail_worker),
            thumbnail_generation: Cell::new(0),
            active_generation: Cell::new(0),
            show_hidden: Cell::new(false),
            visible_entries: RefCell::new(Vec::new()),
            pending_entries: RefCell::new(VecDeque::new()),
            pending_store: RefCell::new(None),
            pending_total: Cell::new(0),
            pending_selection_index: Cell::new(None),
            selected_entry: RefCell::new(None),
            sort_order: Cell::new(DirectorySort::default()),
            sort_in_flight: Cell::new(false),
            sort_selection_path: RefCell::new(None),
            application_state,
        })
    }

    pub fn wire(self: &Rc<Self>, application: &adw::Application, locations: &[Location]) {
        self.install_actions(application);
        self.update_sort_headers();

        for (button, location) in self.widgets.location_buttons.iter().zip(locations) {
            let controller = Rc::downgrade(self);
            let path = location.path.clone();
            button.connect_clicked(move |_| {
                if let Some(controller) = controller.upgrade() {
                    controller.navigate_to(path.clone());
                }
            });
        }

        let controller = Rc::downgrade(self);
        self.widgets.list_view.connect_activate(move |_, position| {
            if let Some(controller) = controller.upgrade() {
                controller.activate(position);
            }
        });

        let controller = Rc::downgrade(self);
        self.widgets.selection.connect_selected_notify(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.selection_changed();
            }
        });

        let file_list_shortcuts = gtk::EventControllerKey::new();
        let controller = Rc::downgrade(self);
        file_list_shortcuts.connect_key_pressed(move |_, key, _, modifiers| {
            if key == gtk::gdk::Key::Delete && modifiers.is_empty() {
                if let Some(controller) = controller.upgrade() {
                    controller.trash_selected();
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
        self.widgets.list_view.add_controller(file_list_shortcuts);

        let controller = Rc::downgrade(self);
        self.widgets.location_entry.connect_activate(move |entry| {
            if let Some(controller) = controller.upgrade() {
                let text = entry.text();
                if !text.is_empty() {
                    controller.navigate_to(PathBuf::from(text.as_str()));
                }
                controller.hide_location_entry();
            }
        });
    }

    pub fn present_and_start(self: &Rc<Self>) {
        self.widgets.window.present();
        self.load_current();

        let controller = Rc::clone(self);
        glib::timeout_add_local(Duration::from_millis(16), move || {
            if !controller.widgets.window.is_visible() {
                return glib::ControlFlow::Break;
            }
            controller.drain_worker();
            controller.pump_pending_entries();
            controller.submit_thumbnail_requests();
            controller.drain_thumbnail_worker();
            glib::ControlFlow::Continue
        });
    }

    fn install_actions(self: &Rc<Self>, application: &adw::Application) {
        self.add_action("back", |controller| controller.go_back());
        self.add_action("forward", |controller| controller.go_forward());
        self.add_action("parent", |controller| controller.go_parent());
        self.add_action("location", |controller| controller.show_location_entry());
        self.add_action("cancel-location", |controller| {
            controller.hide_location_entry();
        });
        self.add_action("hidden", |controller| controller.toggle_hidden());
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
        application.set_accels_for_action("win.cancel-location", &["Escape"]);
        application.set_accels_for_action("win.copy", &["<Control>c"]);
        application.set_accels_for_action("win.cut", &["<Control>x"]);
        application.set_accels_for_action("win.paste", &["<Control>v"]);
        application.set_accels_for_action("win.rename", &["F2"]);
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
        if self.navigation.borrow_mut().navigate_to(destination) {
            self.load_current();
        }
    }

    fn go_back(&self) {
        if self.navigation.borrow_mut().go_back() {
            self.load_current();
        }
    }

    fn go_forward(&self) {
        if self.navigation.borrow_mut().go_forward() {
            self.load_current();
        }
    }

    fn go_parent(&self) {
        if self.navigation.borrow_mut().go_parent() {
            self.load_current();
        }
    }

    fn toggle_hidden(&self) {
        let show_hidden = !self.show_hidden.get();
        self.show_hidden.set(show_hidden);
        self.widgets.hidden_button.set_active(show_hidden);
        self.load_current();
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

        let selected_path = self
            .selected_entry
            .borrow()
            .as_ref()
            .map(|entry| entry.path().to_path_buf());
        self.sort_selection_path.replace(selected_path);
        self.pending_entries.borrow_mut().clear();
        self.pending_store.borrow_mut().take();
        self.pending_selection_index.set(None);
        self.widgets.context_menu.popdown();
        self.widgets.list_view.set_sensitive(false);
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
        self.widgets.location_entry.set_text("");
        self.widgets.path_stack.set_visible_child_name("entry");
        self.widgets.location_entry.grab_focus();
    }

    fn hide_location_entry(&self) {
        self.widgets.path_stack.set_visible_child_name("path");
        self.widgets.list_view.grab_focus();
    }

    fn load_current(&self) {
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
        self.pending_selection_index.set(None);
        self.sort_selection_path.borrow_mut().take();
        self.sort_in_flight.set(false);
        self.set_sort_controls_sensitive(false);
        self.widgets.context_menu.popdown();
        self.selected_entry.borrow_mut().take();
        self.widgets.selection.unselect_all();
        self.widgets
            .selection
            .set_model(Some(&gio::ListStore::new::<glib::BoxedAnyObject>()));
        self.widgets.list_view.set_sensitive(false);
        self.widgets.empty_state.set_visible(false);
        self.set_open_enabled(false);
        self.set_open_with_enabled(false);
        self.set_selection_actions_enabled(false);
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
                    self.set_sort_controls_sensitive(true);
                    self.show_listing(entries);
                }
                ResponseKind::Listing(Err(DirectoryError::Cancelled)) => {}
                ResponseKind::Listing(Err(error)) => {
                    tracing::warn!(path = ?response.path, %error, "directory enumeration failed");
                    self.set_sort_controls_sensitive(true);
                    self.widgets.list_view.set_sensitive(true);
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
                    let selected_path = self.sort_selection_path.borrow_mut().take();
                    self.install_entries(entries, selected_path.as_deref(), false);
                }
            }
        }
    }

    fn show_listing(&self, entries: Vec<DirectoryEntry>) {
        let show_hidden = self.show_hidden.get();
        let entries: Vec<Arc<DirectoryEntry>> = entries
            .into_iter()
            .filter(|entry| show_hidden || !entry.is_hidden())
            .map(Arc::new)
            .collect();
        self.install_entries(entries, None, true);
    }

    fn install_entries(
        &self,
        entries: Vec<Arc<DirectoryEntry>>,
        selected_path: Option<&Path>,
        focus_list: bool,
    ) {
        let count = entries.len();
        let selection_index =
            selected_path.and_then(|path| selection_index_for_path(&entries, path));
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        self.widgets.selection.set_model(Some(&store));
        self.widgets.list_view.set_sensitive(true);
        self.widgets.empty_state.set_visible(count == 0);
        self.pending_total.set(count);
        self.pending_selection_index.set(selection_index);
        self.pending_entries
            .replace(entries.iter().cloned().collect());
        self.visible_entries.replace(entries);
        self.pending_store.replace(Some(store));
        self.update_loading_status(0, count);
        if focus_list {
            self.widgets.list_view.grab_focus();
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
        if self
            .pending_selection_index
            .get()
            .is_some_and(|index| usize::try_from(index).is_ok_and(|index| index < loaded))
        {
            if let Some(index) = self.pending_selection_index.take() {
                self.widgets.selection.set_selected(index);
            }
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
        let toast_overlay = self.widgets.toast_overlay.clone();
        launcher::launch_default(entry.path(), move |result| {
            if let Err(error) = result {
                tracing::warn!(%error, "default application launch failed");
                toast_overlay.add_toast(
                    adw::Toast::builder()
                        .title(format!("Could not open {display_name}: {error}"))
                        .timeout(6)
                        .build(),
                );
            }
        });
    }

    fn selection_changed(&self) {
        let selected_entry = self.selected_model_entry();
        let has_selection = selected_entry.is_some();
        self.selected_entry.replace(selected_entry);
        self.set_open_enabled(has_selection);
        self.set_open_with_enabled(
            self.selected_entry
                .borrow()
                .as_ref()
                .is_some_and(|entry| open_with_eligible(entry)),
        );
        self.set_selection_actions_enabled(has_selection);
        self.refresh_status();
    }

    fn selected_entry(&self) -> Option<Arc<DirectoryEntry>> {
        self.selected_entry.borrow().clone()
    }

    fn show_context_menu(&self) {
        if self.selected_entry().is_none() {
            return;
        }
        self.widgets.context_menu.set_pointing_to(None);
        self.widgets.context_menu.popup();
    }

    fn selected_model_entry(&self) -> Option<Arc<DirectoryEntry>> {
        let object = self
            .widgets
            .selection
            .selected_item()?
            .downcast::<glib::BoxedAnyObject>()
            .ok()?;
        let entry = object.borrow::<Arc<DirectoryEntry>>().clone();
        Some(entry)
    }

    fn refresh_status(&self) {
        let label = if let Some(entry) = self.selected_entry() {
            format!("{} selected", entry.display_name_lossy())
        } else {
            match self.pending_total.get() {
                1 => "1 item".to_owned(),
                count => format!("{count} items"),
            }
        };
        self.widgets.status_label.set_label(&label);
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
                let eligible = selection
                    .selected_item()
                    .and_downcast::<glib::BoxedAnyObject>()
                    .is_some_and(|object| {
                        open_with_eligible(&object.borrow::<Arc<DirectoryEntry>>())
                    });
                action.set_enabled(eligible);
            }
            status_label.set_label(&format!("{display_name} selected"));
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
                    present_open_with_dialog(&window, &toast_overlay, &display_name, options)
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

    fn set_selection_actions_enabled(&self, enabled: bool) {
        for action_name in ["copy", "cut", "rename", "trash"] {
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
        let Some(entry) = self.selected_entry() else {
            self.show_toast("Select an item to copy", 4);
            return;
        };
        if matches!(entry.kind(), floe_core::EntryKind::Other) {
            self.show_toast("This special file type cannot be copied yet", 5);
            return;
        }

        match self
            .application_state
            .stage_copy(entry.path().to_path_buf())
        {
            Ok(()) => {
                self.set_paste_enabled(true);
                self.show_toast(
                    &format!(
                        "Ready to copy {}. Open a destination and press Ctrl+V.",
                        entry.display_name_lossy()
                    ),
                    5,
                );
            }
            Err(error) => self.show_toast(&format!("Could not stage copy: {error}"), 6),
        }
    }

    fn stage_selected_move(&self) {
        let Some(entry) = self.selected_entry() else {
            self.show_toast("Select an item to move", 4);
            return;
        };
        match self
            .application_state
            .stage_move(entry.path().to_path_buf())
        {
            Ok(()) => {
                self.set_paste_enabled(true);
                self.show_toast(
                    &format!(
                        "Ready to move {}. Open a destination and press Ctrl+V.",
                        entry.display_name_lossy()
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
            .staged_transfer()
            .map(|(intent, _)| intent);
        match self.application_state.submit_paste(&destination) {
            Ok(_) => self.widgets.status_label.set_label(match intent {
                Some(TransferIntent::Move) => "Move queued…",
                _ => "Copy queued…",
            }),
            Err(error) => self.show_toast(&format!("Could not start operation: {error}"), 6),
        }
    }

    fn trash_selected(&self) {
        let Some(entry) = self.selected_entry() else {
            self.show_toast("Select an item to move to Trash", 4);
            return;
        };
        let display_name = entry.display_name_lossy();
        match self
            .application_state
            .submit_trash(entry.path().to_path_buf())
        {
            Ok(_) => self
                .widgets
                .status_label
                .set_label(&format!("Moving {display_name} to Trash…")),
            Err(error) => self.show_toast(
                &format!("Could not move {display_name} to Trash: {error}"),
                7,
            ),
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

fn selection_index_for_path(entries: &[Arc<DirectoryEntry>], path: &Path) -> Option<u32> {
    entries
        .iter()
        .position(|entry| entry.path() == path)
        .and_then(|index| u32::try_from(index).ok())
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

    #[cfg(unix)]
    #[test]
    fn phase_6b_selection_restoration_uses_exact_non_utf8_path() {
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
        let target_path = directory.path().join(target_name);
        let index = selection_index_for_path(&entries, &target_path)
            .expect("exact target path should remain selectable");

        assert_eq!(entries[index as usize].path(), target_path);
        assert_eq!(
            entries[0].display_name_lossy(),
            entries[1].display_name_lossy(),
            "the test must exercise colliding lossy display names"
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
}
