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
    operations::{OperationCallbacks, OperationController},
    preferences::{PreferenceWorker, ViewPreferences},
    preview::{PreviewProviderRegistry, PreviewWorker},
    properties::PropertiesWorker,
    selection_mode::{
        SelectionCompletion, SelectionConfig, SelectionProcessOutput, parse_selection_invocation,
        process_output, selection_application_id,
    },
    session_store::{SessionStoreWorker, SessionTracePolicy},
    state::ApplicationState,
    storage::StorageWorker,
    thumbnail::ThumbnailWorker,
    ui,
    worker::BrowserWorker,
};

pub(crate) const APPLICATION_ID: &str = "io.github.rodriguezcappsec.Floe";

#[derive(Clone)]
struct SelectionLaunch {
    config: SelectionConfig,
    completion: Rc<RefCell<Option<SelectionCompletion>>>,
}

const MULTIPLE_OPEN_TARGETS_MESSAGE: &str = "Open one command-line file or folder at a time";
const NON_LOCAL_OPEN_TARGET_MESSAGE: &str =
    "Only local command-line file and folder targets are supported";

fn local_open_target(files: &[gio::File]) -> Result<std::path::PathBuf, &'static str> {
    if files.len() != 1 {
        return Err(MULTIPLE_OPEN_TARGETS_MESSAGE);
    }
    files[0].path().ok_or(NON_LOCAL_OPEN_TARGET_MESSAGE)
}

pub fn run() -> glib::ExitCode {
    init_logging();

    match parse_selection_invocation(std::env::args_os()) {
        Ok(Some(config)) => return run_selection(config),
        Ok(None) => {}
        Err(error) => {
            eprintln!("Floe Selection Mode: {error}");
            return glib::ExitCode::FAILURE;
        }
    }

    run_normal()
}

fn run_normal() -> glib::ExitCode {
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
    let browser_controller = Rc::new(RefCell::new(None::<std::rc::Weak<BrowserController>>));

    let application = adw::Application::builder()
        .application_id(APPLICATION_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    let preference_worker_for_activate = Rc::clone(&preference_worker);
    let restored_tabs_for_activate = Rc::clone(&restored_tabs);
    let session_worker_for_activate = Rc::clone(&session_worker);
    let browser_for_activate = Rc::clone(&browser_controller);
    let preferences_for_activate = view_preferences.clone();
    application.connect_activate(move |application| {
        build_window(
            application,
            preferences_for_activate.clone(),
            &preference_worker_for_activate,
            &restored_tabs_for_activate,
            &session_worker_for_activate,
            &browser_for_activate,
            None,
        );
    });

    let preference_worker_for_open = Rc::clone(&preference_worker);
    let restored_tabs_for_open = Rc::clone(&restored_tabs);
    let session_worker_for_open = Rc::clone(&session_worker);
    let browser_for_open = Rc::clone(&browser_controller);
    let preferences_for_open = view_preferences.clone();
    application.connect_open(move |application, files, _hint| {
        build_window(
            application,
            preferences_for_open.clone(),
            &preference_worker_for_open,
            &restored_tabs_for_open,
            &session_worker_for_open,
            &browser_for_open,
            None,
        );
        let Some(controller) = browser_for_open
            .borrow()
            .as_ref()
            .and_then(std::rc::Weak::upgrade)
        else {
            return;
        };
        match local_open_target(files) {
            Ok(path) => controller.queue_cli_target(path),
            Err(message) => controller.show_external_message(message, 5),
        }
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

fn run_selection(config: SelectionConfig) -> glib::ExitCode {
    let view_preferences = match PreferenceWorker::load_read_only() {
        Ok(preferences) => preferences,
        Err(error) => {
            tracing::warn!(%error, "could not read view preferences; using defaults");
            ViewPreferences::default()
        }
    };
    // Selection Mode may consume ordinary preferences for a familiar view, but
    // every chooser-local adjustment is transient and must not overwrite them.
    let preference_worker = Rc::new(RefCell::new(None));
    let restored_tabs = Rc::new(RefCell::new(None));
    let session_worker = Rc::new(RefCell::new(None));
    let browser_controller = Rc::new(RefCell::new(None::<std::rc::Weak<BrowserController>>));
    let completion = Rc::new(RefCell::new(None));
    let launch = SelectionLaunch {
        config,
        completion: Rc::clone(&completion),
    };
    let application_id = selection_application_id(std::process::id());
    let application = adw::Application::builder()
        .application_id(&application_id)
        .build();
    let preferences_for_activate = view_preferences.clone();
    let preference_worker_for_activate = Rc::clone(&preference_worker);
    let restored_tabs_for_activate = Rc::clone(&restored_tabs);
    let session_worker_for_activate = Rc::clone(&session_worker);
    let browser_for_activate = Rc::clone(&browser_controller);
    application.connect_activate(move |application| {
        build_window(
            application,
            preferences_for_activate.clone(),
            &preference_worker_for_activate,
            &restored_tabs_for_activate,
            &session_worker_for_activate,
            &browser_for_activate,
            Some(launch.clone()),
        );
    });
    let accept = gio::SimpleAction::new("accept-selection", None);
    let browser_for_accept = Rc::clone(&browser_controller);
    accept.connect_activate(move |_, _| {
        if let Some(controller) = browser_for_accept
            .borrow()
            .as_ref()
            .and_then(std::rc::Weak::upgrade)
        {
            controller.accept_selection_mode();
        }
    });
    application.add_action(&accept);
    let cancel = gio::SimpleAction::new("cancel-selection", None);
    let application_weak = application.downgrade();
    let completion_for_cancel = Rc::clone(&completion);
    cancel.connect_activate(move |_, _| {
        if completion_for_cancel.borrow().is_none() {
            *completion_for_cancel.borrow_mut() = Some(SelectionCompletion::Cancelled);
        }
        if let Some(application) = application_weak.upgrade() {
            application.quit();
        }
    });
    application.add_action(&cancel);
    application.set_accels_for_action("app.cancel-selection", &["<Control>q"]);

    let process_exit = application.run_with_args(&["floe-selection"]);
    let completion = completion.borrow();
    match process_output(completion.as_ref(), process_exit == glib::ExitCode::SUCCESS) {
        SelectionProcessOutput::Accepted(uris) => {
            for uri in uris {
                println!("{uri}");
            }
            glib::ExitCode::SUCCESS
        }
        SelectionProcessOutput::Cancelled => glib::ExitCode::FAILURE,
        SelectionProcessOutput::Failed => process_exit,
    }
}

fn build_window(
    application: &adw::Application,
    view_preferences: ViewPreferences,
    preference_worker: &Rc<RefCell<Option<PreferenceWorker>>>,
    restored_tabs: &Rc<RefCell<Option<floe_core::BrowserTabs>>>,
    session_worker: &Rc<RefCell<Option<SessionStoreWorker>>>,
    browser_controller: &Rc<RefCell<Option<std::rc::Weak<BrowserController>>>>,
    selection_launch: Option<SelectionLaunch>,
) {
    if let Some(window) = application.active_window() {
        window.present();
        return;
    }

    let appearance = Appearance::from_environment_or(view_preferences.appearance);
    if let Some(display) = gtk::gdk::Display::default() {
        iconography::register(&display);
    }

    let places = locations::standard_locations();
    let restored_tabs = restored_tabs.borrow_mut().take();
    let initial_path = selection_launch
        .as_ref()
        .map(|launch| launch.config.initial_directory_or_else(glib::home_dir))
        .or_else(|| {
            restored_tabs
                .as_ref()
                .map(|tabs| tabs.active().current().path().to_path_buf())
        })
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
            if fail_selection_launch(&selection_launch) {
                application.quit();
                return;
            }
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
    let metadata_index_worker = match crate::sort_metadata_index::MetadataIndexWorker::spawn() {
        Ok(worker) => Some(worker),
        Err(error) => {
            tracing::warn!(%error, "could not start advanced metadata index worker");
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
    let application_state = match if selection_launch.is_some() {
        ApplicationState::new_selection_mode()
    } else {
        ApplicationState::new()
    } {
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
            if fail_selection_launch(&selection_launch) {
                application.quit();
                return;
            }
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
            metadata_index_worker,
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
    if let Some(launch) = selection_launch {
        let application = application.downgrade();
        let completion = Rc::clone(&launch.completion);
        controller.configure_selection_mode(launch.config, move |result| {
            *completion.borrow_mut() = Some(result);
            if let Some(application) = application.upgrade() {
                application.quit();
            }
        });
    }
    *browser_controller.borrow_mut() = Some(Rc::downgrade(&controller));
    let browser = Rc::downgrade(&controller);
    let browser_for_guardrails = browser.clone();
    let browser_for_shutdown = Rc::downgrade(&controller);
    let application_state_for_shutdown = Rc::clone(&application_state);
    application.connect_shutdown(move |_| {
        if let Some(browser) = browser_for_shutdown.upgrade() {
            browser.persist_for_shutdown();
        }
        application_state_for_shutdown.cleanup_selection_transient_state();
    });
    let operation_controller = OperationController::new(
        operation_window,
        operation_toasts,
        operation_widgets,
        application_state,
        move || {
            browser_for_guardrails
                .upgrade()
                .map(|browser| browser.guardrail_environment())
                .unwrap_or_else(|| {
                    crate::guardrail_preflight::PreflightEnvironment::new(
                        Some(glib::home_dir()),
                        Vec::new(),
                    )
                    .unwrap_or_default()
                })
        },
        OperationCallbacks::new(
            move |destination| {
                if let Some(browser) = browser.upgrade() {
                    browser.refresh_if_current(destination);
                }
            },
            {
                let browser = Rc::downgrade(&controller);
                move |request| {
                    if let Some(browser) = browser.upgrade() {
                        browser.queue_operation_reveal(request);
                    }
                }
            },
            {
                let browser = Rc::downgrade(&controller);
                move |path| {
                    if let Some(browser) = browser.upgrade() {
                        browser.navigate_to_revealing(path);
                    }
                }
            },
        ),
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

fn fail_selection_launch(selection_launch: &Option<SelectionLaunch>) -> bool {
    let Some(launch) = selection_launch else {
        return false;
    };
    *launch.completion.borrow_mut() = Some(SelectionCompletion::Failed);
    true
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_7g_application_accepts_exactly_one_local_open_target() {
        let target = std::path::PathBuf::from("/tmp/floe phase 7g");
        let file = gio::File::for_path(&target);

        assert_eq!(local_open_target(&[file]), Ok(target));
    }

    #[test]
    fn phase_7g_application_rejects_zero_or_multiple_open_targets() {
        assert_eq!(local_open_target(&[]), Err(MULTIPLE_OPEN_TARGETS_MESSAGE));

        let first = gio::File::for_path("/tmp/first");
        let second = gio::File::for_path("/tmp/second");
        assert_eq!(
            local_open_target(&[first, second]),
            Err(MULTIPLE_OPEN_TARGETS_MESSAGE)
        );
    }

    #[test]
    fn phase_7g_application_rejects_non_local_open_target() {
        let remote = gio::File::for_uri("sftp://example.invalid/folder");

        assert_eq!(
            local_open_target(&[remote]),
            Err(NON_LOCAL_OPEN_TARGET_MESSAGE)
        );
    }
}
