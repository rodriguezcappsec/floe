use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::{gio, glib};
use tracing_subscriber::EnvFilter;

use crate::{
    appearance::Appearance,
    bookmarks::BookmarkWorker,
    browser::{BrowserController, BrowserServices},
    devices::DeviceMonitor,
    iconography,
    inspector::InspectorWorker,
    locations,
    metadata::MetadataWorker,
    operations::OperationController,
    preferences::{PreferenceWorker, ViewPreferences},
    preview::{PreviewProviderRegistry, PreviewWorker},
    properties::PropertiesWorker,
    session_store::{SessionStoreWorker, SessionTracePolicy},
    state::ApplicationState,
    storage::StorageWorker,
    thumbnail::ThumbnailWorker,
    ui,
    worker::BrowserWorker,
};

const APPLICATION_ID: &str = "io.github.floe.FileManager";

pub fn run() -> glib::ExitCode {
    init_logging();

    let (view_preferences, preference_worker) = match PreferenceWorker::spawn() {
        Ok(preferences) => (preferences.0, Some(preferences.1)),
        Err(error) => {
            tracing::warn!(%error, "could not start view preference worker; using defaults");
            (ViewPreferences::default(), None)
        }
    };
    let preference_worker = Rc::new(RefCell::new(preference_worker));
    let session_policy = SessionTracePolicy::from_environment();
    let (restored_tabs, session_worker) = match SessionStoreWorker::spawn(session_policy) {
        Ok(result) => (result.0, Some(result.1)),
        Err(error) => {
            tracing::warn!(%error, "could not start session store; using one new tab");
            (None, None)
        }
    };
    let restored_tabs = Rc::new(RefCell::new(restored_tabs));
    let session_worker = Rc::new(RefCell::new(session_worker));

    let application = adw::Application::builder()
        .application_id(APPLICATION_ID)
        .flags(gio::ApplicationFlags::FLAGS_NONE)
        .build();

    let preference_worker_for_activate = Rc::clone(&preference_worker);
    let restored_tabs_for_activate = Rc::clone(&restored_tabs);
    let session_worker_for_activate = Rc::clone(&session_worker);
    application.connect_activate(move |application| {
        build_window(
            application,
            view_preferences.clone(),
            &preference_worker_for_activate,
            &restored_tabs_for_activate,
            &session_worker_for_activate,
        );
    });

    let quit = gio::SimpleAction::new("quit", None);
    let application_weak = application.downgrade();
    quit.connect_activate(move |_, _| {
        if let Some(application) = application_weak.upgrade() {
            application.quit();
        }
    });
    application.add_action(&quit);
    application.set_accels_for_action("app.quit", &["<Control>q"]);

    application.run()
}

fn build_window(
    application: &adw::Application,
    view_preferences: ViewPreferences,
    preference_worker: &Rc<RefCell<Option<PreferenceWorker>>>,
    restored_tabs: &Rc<RefCell<Option<floe_core::BrowserTabs>>>,
    session_worker: &Rc<RefCell<Option<SessionStoreWorker>>>,
) {
    if let Some(window) = application.active_window() {
        window.present();
        return;
    }

    let appearance = Appearance::from_environment();
    appearance.install();
    if let Some(display) = gtk::gdk::Display::default() {
        iconography::register(&display);
    }

    let places = locations::standard_locations();
    let restored_tabs = restored_tabs.borrow_mut().take();
    let initial_path = restored_tabs
        .as_ref()
        .map(|tabs| tabs.active().current().path().to_path_buf())
        .or_else(|| places.first().map(|place| place.path.clone()))
        .unwrap_or_else(glib::home_dir);
    let widgets = ui::build(application, &places, appearance, view_preferences.clone());
    let worker = match BrowserWorker::spawn() {
        Ok(worker) => worker,
        Err(error) => {
            tracing::error!(%error, "could not start directory worker");
            widgets.spinner.stop();
            widgets
                .status_label
                .set_label("Directory browsing is unavailable");
            widgets.toast_overlay.add_toast(
                adw::Toast::builder()
                    .title(format!("Could not start directory browser: {error}"))
                    .timeout(0)
                    .build(),
            );
            widgets.window.present();
            return;
        }
    };
    let thumbnail_worker = match ThumbnailWorker::spawn() {
        Ok(worker) => Some(worker),
        Err(error) => {
            tracing::warn!(%error, "could not start thumbnail worker; using generic icons");
            None
        }
    };
    let metadata_worker = match MetadataWorker::spawn() {
        Ok(worker) => Some(worker),
        Err(error) => {
            tracing::warn!(%error, "could not start metadata worker; using basic details");
            None
        }
    };
    let inspector_worker = match InspectorWorker::spawn() {
        Ok(worker) => Some(worker),
        Err(error) => {
            tracing::warn!(%error, "could not start Inspector worker; Inspector unavailable");
            None
        }
    };
    let preview_worker = match PreviewWorker::spawn(PreviewProviderRegistry::first_party()) {
        Ok(worker) => Some(worker),
        Err(error) => {
            tracing::warn!(%error, "could not start preview worker; Preview unavailable");
            None
        }
    };
    let properties_worker = match PropertiesWorker::spawn() {
        Ok(worker) => Some(worker),
        Err(error) => {
            tracing::warn!(%error, "could not start Properties worker; Properties unavailable");
            None
        }
    };
    let storage_worker = match StorageWorker::spawn() {
        Ok(worker) => Some(worker),
        Err(error) => {
            tracing::warn!(%error, "could not start storage facts worker");
            None
        }
    };
    let bookmark_worker = match BookmarkWorker::spawn() {
        Ok(worker) => Some(worker),
        Err(error) => {
            tracing::warn!(%error, "could not start bookmark worker");
            widgets.toast_overlay.add_toast(
                adw::Toast::builder()
                    .title("Bookmarks are unavailable for this session")
                    .timeout(6)
                    .build(),
            );
            None
        }
    };
    let device_monitor = DeviceMonitor::new();
    let application_state = match ApplicationState::new() {
        Ok(state) => Rc::new(state),
        Err(error) => {
            tracing::error!(%error, "could not start copy executor");
            widgets.spinner.stop();
            widgets
                .status_label
                .set_label("Filesystem operations are unavailable");
            widgets.toast_overlay.add_toast(
                adw::Toast::builder()
                    .title(format!("Could not start filesystem operations: {error}"))
                    .timeout(0)
                    .build(),
            );
            widgets.window.present();
            return;
        }
    };
    let operation_widgets = widgets.operations.clone();
    let operation_window = widgets.window.clone();
    let command_window = widgets.window.clone();
    let operation_toasts = widgets.toast_overlay.clone();
    let controller = BrowserController::new(
        widgets,
        initial_path,
        restored_tabs,
        BrowserServices::new(
            worker,
            thumbnail_worker,
            metadata_worker,
            inspector_worker,
            preview_worker,
            properties_worker,
            storage_worker,
            bookmark_worker,
            device_monitor,
            preference_worker.borrow_mut().take(),
            session_worker.borrow_mut().take(),
        ),
        view_preferences,
        Rc::clone(&application_state),
    );
    let browser = Rc::downgrade(&controller);
    let browser_for_shutdown = Rc::downgrade(&controller);
    application.connect_shutdown(move |_| {
        if let Some(browser) = browser_for_shutdown.upgrade() {
            browser.persist_session_for_shutdown();
        }
    });
    let operation_controller = OperationController::new(
        operation_window,
        operation_toasts,
        operation_widgets,
        application_state,
        move |destination| {
            if let Some(browser) = browser.upgrade() {
                browser.refresh_if_current(destination);
            }
        },
    );
    operation_controller.wire();
    controller.wire(application, &places);
    if let Err(error) = crate::command_registry::validate_contract() {
        tracing::error!(error, "command registry contract is invalid");
    }
    let resolved = crate::command_registry::resolve_all(&command_window);
    let missing = crate::command_registry::missing_registered_actions(&command_window);
    let disabled = resolved
        .iter()
        .filter(|command| !command.can_activate())
        .count();
    if missing.is_empty() {
        tracing::debug!(
            commands = resolved.len(),
            disabled,
            "command registry action parity verified"
        );
    } else {
        tracing::warn!(
            missing = missing.len(),
            "command registry references unavailable actions"
        );
    }
    controller.present_and_start();
    tracing::info!("Floe application started");
}

fn init_logging() {
    let filter = match std::env::var("RUST_LOG") {
        Ok(value) if !value.trim().is_empty() => EnvFilter::new(value),
        _ => EnvFilter::new("floe_app=info,floe_core=info"),
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .try_init();
}
