use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    path::PathBuf,
    rc::Rc,
    time::Duration,
};

use adw::prelude::*;
use floe_core::{DirectoryEntry, DirectoryError, DirectoryListing, NavigationState};
use gtk::{gio, glib};

use crate::{
    launcher, locations::Location, state::ApplicationState, ui::BrowserWidgets,
    worker::BrowserWorker,
};

pub struct BrowserController {
    widgets: BrowserWidgets,
    navigation: RefCell<NavigationState>,
    worker: RefCell<BrowserWorker>,
    active_generation: Cell<u64>,
    show_hidden: Cell<bool>,
    pending_entries: RefCell<VecDeque<DirectoryEntry>>,
    pending_store: RefCell<Option<gio::ListStore>>,
    pending_total: Cell<usize>,
    selected_entry: RefCell<Option<DirectoryEntry>>,
    _application_state: Rc<ApplicationState>,
}

impl BrowserController {
    pub fn new(
        widgets: BrowserWidgets,
        initial_path: PathBuf,
        worker: BrowserWorker,
        application_state: Rc<ApplicationState>,
    ) -> Rc<Self> {
        Rc::new(Self {
            widgets,
            navigation: RefCell::new(NavigationState::new(initial_path)),
            worker: RefCell::new(worker),
            active_generation: Cell::new(0),
            show_hidden: Cell::new(false),
            pending_entries: RefCell::new(VecDeque::new()),
            pending_store: RefCell::new(None),
            pending_total: Cell::new(0),
            selected_entry: RefCell::new(None),
            _application_state: application_state,
        })
    }

    pub fn wire(self: &Rc<Self>, application: &adw::Application, locations: &[Location]) {
        self.install_actions(application);

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

        application.set_accels_for_action("win.back", &["<Alt>Left"]);
        application.set_accels_for_action("win.forward", &["<Alt>Right"]);
        application.set_accels_for_action("win.parent", &["<Alt>Up"]);
        application.set_accels_for_action("win.location", &["<Control>l"]);
        application.set_accels_for_action("win.hidden", &["<Control>h"]);
        application.set_accels_for_action("win.cancel-location", &["Escape"]);
    }

    fn add_action(self: &Rc<Self>, name: &str, callback: fn(&Self)) -> gio::SimpleAction {
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
        self.pending_entries.borrow_mut().clear();
        self.pending_store.borrow_mut().take();
        self.pending_total.set(0);
        self.selected_entry.borrow_mut().take();
        self.widgets.selection.unselect_all();
        self.widgets
            .selection
            .set_model(Some(&gio::ListStore::new::<glib::BoxedAnyObject>()));
        self.widgets.list_view.set_sensitive(false);
        self.widgets.empty_state.set_visible(false);
        self.set_open_enabled(false);
        let path = self.navigation.borrow().current().to_path_buf();
        let generation = self.worker.borrow_mut().request(path.clone());
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
            match response.result {
                Ok(listing) => self.show_listing(listing),
                Err(DirectoryError::Cancelled) => {}
                Err(error) => {
                    tracing::warn!(path = ?response.path, %error, "directory enumeration failed");
                    self.widgets.list_view.set_sensitive(true);
                    self.widgets.status_label.set_label("Could not load folder");
                    let toast = adw::Toast::builder()
                        .title(format!("Could not open folder: {error}"))
                        .timeout(6)
                        .build();
                    self.widgets.toast_overlay.add_toast(toast);
                }
            }
        }
    }

    fn show_listing(&self, listing: DirectoryListing) {
        let show_hidden = self.show_hidden.get();
        let entries: VecDeque<DirectoryEntry> = listing
            .into_entries()
            .into_iter()
            .filter(|entry| show_hidden || !entry.is_hidden())
            .collect();
        let count = entries.len();
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        self.widgets.selection.set_model(Some(&store));
        self.widgets.list_view.set_sensitive(true);
        self.widgets.empty_state.set_visible(count == 0);
        self.pending_total.set(count);
        self.pending_entries.replace(entries);
        self.pending_store.replace(Some(store));
        self.update_loading_status(0, count);
        self.widgets.list_view.grab_focus();
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
        let entry = object.borrow::<DirectoryEntry>().clone();
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
        self.refresh_status();
    }

    fn selected_entry(&self) -> Option<DirectoryEntry> {
        self.selected_entry.borrow().clone()
    }

    fn selected_model_entry(&self) -> Option<DirectoryEntry> {
        let object = self
            .widgets
            .selection
            .selected_item()?
            .downcast::<glib::BoxedAnyObject>()
            .ok()?;
        let entry = object.borrow::<DirectoryEntry>().clone();
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

    fn show_toast(&self, title: &str, timeout: u32) {
        self.widgets
            .toast_overlay
            .add_toast(adw::Toast::builder().title(title).timeout(timeout).build());
    }
}
