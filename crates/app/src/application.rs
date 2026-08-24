use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};
use tracing_subscriber::EnvFilter;

use crate::{
    appearance::Appearance, browser::BrowserController, locations, operations::OperationController,
    state::ApplicationState, thumbnail::ThumbnailWorker, ui, worker::BrowserWorker,
};

const APPLICATION_ID: &str = "io.github.floe.FileManager";

pub fn run() -> glib::ExitCode {
    init_logging();

    let application = adw::Application::builder()
        .application_id(APPLICATION_ID)
        .flags(gio::ApplicationFlags::FLAGS_NONE)
        .build();

    application.connect_activate(build_window);

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

fn build_window(application: &adw::Application) {
    if let Some(window) = application.active_window() {
        window.present();
        return;
    }

    let appearance = Appearance::from_environment();
    appearance.install();

    let places = locations::standard_locations();
    let initial_path = places
        .first()
        .map(|place| place.path.clone())
        .unwrap_or_else(glib::home_dir);
    let widgets = ui::build(application, &places, appearance);
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
    let operation_toasts = widgets.toast_overlay.clone();
    let controller = BrowserController::new(
        widgets,
        initial_path,
        worker,
        thumbnail_worker,
        Rc::clone(&application_state),
    );
    let browser = Rc::downgrade(&controller);
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
