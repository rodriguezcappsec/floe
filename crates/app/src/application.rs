use std::{
    cell::RefCell,
    collections::VecDeque,
    os::unix::ffi::OsStringExt,
    rc::{Rc, Weak},
};

use adw::prelude::*;
use gtk::{gio, glib};
use tracing_subscriber::EnvFilter;

use crate::{
    appearance::Appearance,
    bookmarks::SharedBookmarks,
    browser::{BrowserController, BrowserServices},
    devices::DeviceMonitor,
    iconography,
    inspector::InspectorWorker,
    locations,
    metadata::MetadataWorker,
    operation_hub::OperationEventHub,
    operations::{OperationCallbacks, OperationController},
    preferences::{PreferenceWorker, SharedPreferences, ViewPreferences},
    preview::{PreviewProviderRegistry, PreviewWorker},
    properties::PropertiesWorker,
    selection_mode::{
        SelectionCompletion, SelectionConfig, SelectionProcessOutput, encode_option_result,
        parse_selection_invocation, process_output, selection_application_id,
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
const NEW_WINDOW_ACTION: &str = "new-window";
const OPEN_NEW_WINDOW_ACTION: &str = "open-new-window";
const NEW_WINDOW_ACCELERATOR: &str = "<Control>n";

type BrowserRegistry = Rc<RefCell<Vec<Weak<BrowserController>>>>;

fn most_recent_weak<T>(registry: &Rc<RefCell<Vec<Weak<T>>>>) -> Option<Rc<T>> {
    let mut registry = registry.borrow_mut();
    registry.retain(|browser| browser.strong_count() > 0);
    registry.iter().rev().find_map(Weak::upgrade)
}

fn most_recent_browser(registry: &BrowserRegistry) -> Option<Rc<BrowserController>> {
    most_recent_weak(registry)
}

fn local_open_target(files: &[gio::File]) -> Result<std::path::PathBuf, &'static str> {
    if files.len() != 1 {
        return Err(MULTIPLE_OPEN_TARGETS_MESSAGE);
    }
    files[0].path().ok_or(NON_LOCAL_OPEN_TARGET_MESSAGE)
}

fn route_new_window_target<T>(
    controller: Option<Rc<T>>,
    target: std::path::PathBuf,
    route: impl FnOnce(&T, std::path::PathBuf),
) -> bool {
    let Some(controller) = controller else {
        return false;
    };
    route(&controller, target);
    true
}

fn application_quit_allowed(has_application_jobs: bool) -> bool {
    !has_application_jobs
}

pub fn run() -> glib::ExitCode {
    init_logging();

    let arguments = std::env::args_os().collect::<Vec<_>>();
    if crate::portal_filechooser::requested(arguments.clone()) {
        return crate::portal_filechooser::run();
    }

    match parse_selection_invocation(arguments) {
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
    let shared_preferences = SharedPreferences::new(
        view_preferences.clone(),
        preference_worker
            .borrow()
            .as_ref()
            .and_then(PreferenceWorker::handle),
    );
    let shared_bookmarks = match SharedBookmarks::spawn() {
        Ok(bookmarks) => Some(bookmarks),
        Err(error) => {
            tracing::warn!(%error, "could not start bookmark worker; bookmarks unavailable");
            None
        }
    };
    let session_policy = SessionTracePolicy::from_environment();
    let (restored_tabs, session_worker) = match SessionStoreWorker::spawn_windows(session_policy) {
        Ok(result) => (result.0, Some(result.1)),
        Err(error) => {
            tracing::warn!(%error, "could not start session store; using one new tab");
            (Vec::new(), None)
        }
    };
    let restored_tabs = Rc::new(RefCell::new(VecDeque::from(restored_tabs)));
    let session_worker = Rc::new(RefCell::new(session_worker));
    let browser_controller = Rc::new(RefCell::new(Vec::<Weak<BrowserController>>::new()));
    let application_state = match ApplicationState::new() {
        Ok(state) => Rc::new(state),
        Err(error) => {
            tracing::error!(%error, "could not start application filesystem services");
            return glib::ExitCode::FAILURE;
        }
    };
    let operation_event_hub = OperationEventHub::new(Rc::clone(&application_state));

    let application = adw::Application::builder()
        .application_id(APPLICATION_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    let shared_preferences_for_activate = shared_preferences.clone();
    let shared_bookmarks_for_activate = shared_bookmarks.clone();
    let restored_tabs_for_activate = Rc::clone(&restored_tabs);
    let session_worker_for_activate = Rc::clone(&session_worker);
    let browser_for_activate = Rc::clone(&browser_controller);
    let preferences_for_activate = view_preferences.clone();
    let application_state_for_activate = Rc::clone(&application_state);
    let event_hub_for_activate = Rc::clone(&operation_event_hub);
    application.connect_activate(move |application| {
        let window_count = restored_tabs_for_activate.borrow().len().max(1);
        for _ in 0..window_count {
            let _controller = build_window(
                application,
                preferences_for_activate.clone(),
                Some(shared_preferences_for_activate.clone()),
                shared_bookmarks_for_activate.clone(),
                &restored_tabs_for_activate,
                &session_worker_for_activate,
                &browser_for_activate,
                &application_state_for_activate,
                &event_hub_for_activate,
                None,
            );
        }
    });

    let shared_preferences_for_open = shared_preferences.clone();
    let shared_bookmarks_for_open = shared_bookmarks.clone();
    let restored_tabs_for_open = Rc::clone(&restored_tabs);
    let session_worker_for_open = Rc::clone(&session_worker);
    let browser_for_open = Rc::clone(&browser_controller);
    let preferences_for_open = view_preferences.clone();
    let application_state_for_open = Rc::clone(&application_state);
    let event_hub_for_open = Rc::clone(&operation_event_hub);
    application.connect_open(move |application, files, _hint| {
        let controller = build_window(
            application,
            preferences_for_open.clone(),
            Some(shared_preferences_for_open.clone()),
            shared_bookmarks_for_open.clone(),
            &restored_tabs_for_open,
            &session_worker_for_open,
            &browser_for_open,
            &application_state_for_open,
            &event_hub_for_open,
            None,
        );
        match local_open_target(files) {
            Ok(path) => {
                route_new_window_target(controller, path, |controller, path| {
                    controller.queue_cli_target(path);
                });
            }
            Err(message) => {
                if let Some(controller) = controller {
                    controller.show_external_message(message, 5);
                }
            }
        }
    });

    let new_window = gio::SimpleAction::new(NEW_WINDOW_ACTION, None);
    let application_weak = application.downgrade();
    let shared_preferences_for_new = shared_preferences.clone();
    let shared_bookmarks_for_new = shared_bookmarks.clone();
    let restored_tabs_for_new = Rc::clone(&restored_tabs);
    let session_worker_for_new = Rc::clone(&session_worker);
    let browser_for_new = Rc::clone(&browser_controller);
    let preferences_for_new = view_preferences.clone();
    let application_state_for_new = Rc::clone(&application_state);
    let event_hub_for_new = Rc::clone(&operation_event_hub);
    new_window.connect_activate(move |_, _| {
        if let Some(application) = application_weak.upgrade() {
            let _controller = build_window(
                &application,
                preferences_for_new.clone(),
                Some(shared_preferences_for_new.clone()),
                shared_bookmarks_for_new.clone(),
                &restored_tabs_for_new,
                &session_worker_for_new,
                &browser_for_new,
                &application_state_for_new,
                &event_hub_for_new,
                None,
            );
        }
    });
    application.add_action(&new_window);
    application.set_accels_for_action("app.new-window", &[NEW_WINDOW_ACCELERATOR]);

    let open_new_window =
        gio::SimpleAction::new(OPEN_NEW_WINDOW_ACTION, Some(glib::VariantTy::BYTE_STRING));
    let application_weak = application.downgrade();
    let shared_preferences_for_target = shared_preferences.clone();
    let shared_bookmarks_for_target = shared_bookmarks.clone();
    let restored_tabs_for_target = Rc::clone(&restored_tabs);
    let session_worker_for_target = Rc::clone(&session_worker);
    let browser_for_target = Rc::clone(&browser_controller);
    let preferences_for_target = view_preferences.clone();
    let application_state_for_target = Rc::clone(&application_state);
    let event_hub_for_target = Rc::clone(&operation_event_hub);
    open_new_window.connect_activate(move |_, parameter| {
        let Some(bytes) = parameter.and_then(glib::Variant::get::<Vec<u8>>) else {
            return;
        };
        if bytes.is_empty() || bytes.contains(&0) {
            return;
        }
        let path = std::path::PathBuf::from(std::ffi::OsString::from_vec(bytes));
        let Some(application) = application_weak.upgrade() else {
            return;
        };
        let controller = build_window(
            &application,
            preferences_for_target.clone(),
            Some(shared_preferences_for_target.clone()),
            shared_bookmarks_for_target.clone(),
            &restored_tabs_for_target,
            &session_worker_for_target,
            &browser_for_target,
            &application_state_for_target,
            &event_hub_for_target,
            None,
        );
        route_new_window_target(controller, path, |controller, path| {
            controller.queue_cli_target(path);
        });
    });
    application.add_action(&open_new_window);

    let quit = gio::SimpleAction::new("quit", None);
    let application_weak = application.downgrade();
    let application_state_for_quit = Rc::clone(&application_state);
    let browsers_for_quit = Rc::clone(&browser_controller);
    quit.connect_activate(move |_, _| {
        if let Some(application) = application_weak.upgrade() {
            if application_quit_allowed(application_state_for_quit.has_active_jobs()) {
                application.quit();
            } else if let Some(browser) = most_recent_browser(&browsers_for_quit) {
                browser.show_active_operation_close_message();
            }
        }
    });
    application.add_action(&quit);
    application.set_accels_for_action("app.quit", &["<Control>q"]);

    let browsers_for_shutdown = Rc::clone(&browser_controller);
    let session_worker_for_shutdown = Rc::clone(&session_worker);
    let preference_worker_for_shutdown = Rc::clone(&preference_worker);
    let shared_preferences_for_shutdown = shared_preferences.clone();
    let application_state_for_shutdown = Rc::clone(&application_state);
    application.connect_shutdown(move |_| {
        let snapshots = {
            let mut registry = browsers_for_shutdown.borrow_mut();
            registry.retain(|browser| browser.strong_count() > 0);
            let live = registry
                .iter()
                .filter_map(Weak::upgrade)
                .collect::<Vec<_>>();
            live.iter()
                .map(|browser| browser.session_snapshot())
                .collect::<Vec<_>>()
        };
        if !snapshots.is_empty()
            && let Some(mut worker) = session_worker_for_shutdown.borrow_mut().take()
            && let Err(error) = worker.save_windows_before_shutdown(snapshots)
        {
            tracing::warn!(%error, "could not submit final multi-window session");
        }
        if let Some(worker) = preference_worker_for_shutdown.borrow().as_ref()
            && let Err(error) =
                worker.save_before_shutdown(shared_preferences_for_shutdown.snapshot())
        {
            tracing::warn!(%error, "could not submit final view preferences");
        }
        application_state_for_shutdown.cleanup_selection_transient_state();
    });

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
    let restored_tabs = Rc::new(RefCell::new(VecDeque::new()));
    let session_worker = Rc::new(RefCell::new(None));
    let browser_controller = Rc::new(RefCell::new(Vec::<Weak<BrowserController>>::new()));
    let completion = Rc::new(RefCell::new(None));
    let application_state = match ApplicationState::new_selection_mode() {
        Ok(state) => Rc::new(state),
        Err(error) => {
            tracing::error!(%error, "could not start Selection Mode services");
            return glib::ExitCode::FAILURE;
        }
    };
    let operation_event_hub = OperationEventHub::new(Rc::clone(&application_state));
    let launch = SelectionLaunch {
        config,
        completion: Rc::clone(&completion),
    };
    let application_id = selection_application_id(std::process::id());
    let application = adw::Application::builder()
        .application_id(&application_id)
        .build();
    let preferences_for_activate = view_preferences.clone();
    let application_state_for_activate = Rc::clone(&application_state);
    let event_hub_for_activate = Rc::clone(&operation_event_hub);
    let restored_tabs_for_activate = Rc::clone(&restored_tabs);
    let session_worker_for_activate = Rc::clone(&session_worker);
    let browser_for_activate = Rc::clone(&browser_controller);
    application.connect_activate(move |application| {
        let _controller = build_window(
            application,
            preferences_for_activate.clone(),
            None,
            None,
            &restored_tabs_for_activate,
            &session_worker_for_activate,
            &browser_for_activate,
            &application_state_for_activate,
            &event_hub_for_activate,
            Some(launch.clone()),
        );
    });
    let accept = gio::SimpleAction::new("accept-selection", None);
    let browser_for_accept = Rc::clone(&browser_controller);
    accept.connect_activate(move |_, _| {
        if let Some(controller) = most_recent_browser(&browser_for_accept) {
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
        SelectionProcessOutput::AcceptedWithOptions(uris, options) => {
            let Some(encoded) = encode_option_result(&options) else {
                return glib::ExitCode::FAILURE;
            };
            println!("floe-chooser-options-v1:{encoded}");
            for uri in uris {
                println!("{uri}");
            }
            glib::ExitCode::SUCCESS
        }
        SelectionProcessOutput::Cancelled => glib::ExitCode::FAILURE,
        SelectionProcessOutput::Failed => process_exit,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_window(
    application: &adw::Application,
    view_preferences: ViewPreferences,
    shared_preferences: Option<SharedPreferences>,
    shared_bookmarks: Option<SharedBookmarks>,
    restored_tabs: &Rc<RefCell<VecDeque<floe_core::BrowserTabs>>>,
    _session_worker: &Rc<RefCell<Option<SessionStoreWorker>>>,
    browser_controller: &BrowserRegistry,
    application_state: &Rc<ApplicationState>,
    operation_event_hub: &Rc<OperationEventHub>,
    selection_launch: Option<SelectionLaunch>,
) -> Option<Rc<BrowserController>> {
    let view_preferences = shared_preferences
        .as_ref()
        .map(SharedPreferences::snapshot)
        .unwrap_or(view_preferences);
    let existing_controller = selection_launch
        .is_none()
        .then(|| most_recent_browser(browser_controller))
        .flatten();
    let appearance = Appearance::from_environment_or(view_preferences.appearance);
    if let Some(display) = gtk::gdk::Display::default() {
        iconography::register(&display);
    }

    let places = locations::standard_locations();
    let restored_tabs = restored_tabs.borrow_mut().pop_front();
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
                return None;
            }
            if let Some(existing) = existing_controller.as_ref() {
                existing
                    .show_external_message(&format!("Could not open another window: {error}"), 7);
            } else {
                widgets.window.present();
            }
            return None;
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
    let device_monitor = DeviceMonitor::new();
    let application_state = Rc::clone(application_state);
    let window_runtime_id = match operation_event_hub.register() {
        Ok(id) => id,
        Err(error) => {
            if fail_selection_launch(&selection_launch) {
                application.quit();
                return None;
            }
            if let Some(existing) = existing_controller.as_ref() {
                existing
                    .show_external_message(&format!("Could not open another window: {error}"), 7);
            }
            return None;
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
            shared_bookmarks,
            device_monitor,
            shared_preferences,
            None,
        ),
        view_preferences,
        Rc::clone(&application_state),
        Rc::clone(operation_event_hub),
        window_runtime_id,
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
    browser_controller
        .borrow_mut()
        .push(Rc::downgrade(&controller));
    let browser = Rc::downgrade(&controller);
    let browser_for_guardrails = browser.clone();
    let browser_for_shutdown = Rc::downgrade(&controller);
    let application_state_for_shutdown = Rc::downgrade(&application_state);
    application.connect_shutdown(move |_| {
        if let Some(browser) = browser_for_shutdown.upgrade() {
            browser.persist_for_shutdown();
        }
        if let Some(application_state) = application_state_for_shutdown.upgrade() {
            application_state.cleanup_selection_transient_state();
        }
    });
    let operation_controller = OperationController::new(
        operation_window,
        operation_toasts,
        operation_widgets,
        application_state,
        Rc::clone(operation_event_hub),
        window_runtime_id,
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
            {
                let browser = Rc::downgrade(&controller);
                move |job_id| {
                    if let Some(browser) = browser.upgrade() {
                        browser.notify_operation_completed(job_id);
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
    Some(controller)
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
    fn adversarial_quit_policy_blocks_ctrl_q_while_application_jobs_are_active() {
        assert!(application_quit_allowed(false));
        assert!(!application_quit_allowed(true));
    }

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
    fn phase_23a_multi_window_actions_use_one_application_and_standard_shortcut() {
        assert_eq!(NEW_WINDOW_ACTION, "new-window");
        assert_eq!(NEW_WINDOW_ACCELERATOR, "<Control>n");
        let application = adw::Application::builder()
            .application_id("io.github.rodriguezcappsec.Floe.MultiWindowContract")
            .flags(gio::ApplicationFlags::HANDLES_OPEN)
            .build();
        assert!(
            application
                .flags()
                .contains(gio::ApplicationFlags::HANDLES_OPEN)
        );
        assert!(
            !application
                .flags()
                .contains(gio::ApplicationFlags::NON_UNIQUE)
        );
    }

    #[test]
    fn phase_23a_multi_window_model_routes_to_latest_live_window() {
        let registry = Rc::new(RefCell::new(Vec::<Weak<String>>::new()));
        let first = Rc::new("first".to_owned());
        registry.borrow_mut().push(Rc::downgrade(&first));
        let second = Rc::new("second".to_owned());
        registry.borrow_mut().push(Rc::downgrade(&second));
        assert_eq!(
            most_recent_weak(&registry).as_deref(),
            Some(&"second".to_owned())
        );

        drop(second);
        assert_eq!(
            most_recent_weak(&registry).as_deref(),
            Some(&"first".to_owned())
        );
        assert_eq!(registry.borrow().len(), 1);
    }

    #[test]
    fn phase_23_reliability_window_routing_never_falls_back_after_build_failure() {
        let old_window_routes = Rc::new(RefCell::new(Vec::<std::path::PathBuf>::new()));
        let old_window = Rc::clone(&old_window_routes);
        let target = std::path::PathBuf::from("/requested/new-window-target");

        assert!(
            !route_new_window_target::<RefCell<Vec<std::path::PathBuf>>>(
                None,
                target.clone(),
                |routes, path| routes.borrow_mut().push(path),
            )
        );
        assert!(old_window.borrow().is_empty());

        let new_window_routes = Rc::new(RefCell::new(Vec::new()));
        assert!(route_new_window_target(
            Some(Rc::clone(&new_window_routes)),
            target.clone(),
            |routes, path| routes.borrow_mut().push(path),
        ));
        assert_eq!(new_window_routes.borrow().as_slice(), [target]);
        assert!(old_window.borrow().is_empty());
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
