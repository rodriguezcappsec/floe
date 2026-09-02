//! Consistent native hover and accessibility help for Floe controls.

use gtk::prelude::*;

use crate::command_registry;

const HELP_INSTALLED_CLASS: &str = "floe-contextual-help-installed";
const MODEL_MENU_FALLBACK_HELP: &str = "Use this menu option in the current context. Disabled options are unavailable for the current selection or view.";

/// Return the central human explanation for a registered action.
pub fn help_for_action(action: &str) -> Option<&'static str> {
    let action = action.trim();
    if action.is_empty() {
        return None;
    }
    command_registry::command(action)
        .or_else(|| {
            (!action.contains('.'))
                .then(|| format!("win.{action}"))
                .as_deref()
                .and_then(command_registry::command)
        })
        .map(|definition| definition.description)
        .or_else(|| help_for_native_action(action))
        .filter(|description| !description.trim().is_empty())
}

fn help_for_native_action(action: &str) -> Option<&'static str> {
    match action {
        "navigation.pop" => Some("Return to the previous page in this dialog."),
        "window.minimize" => Some("Minimize this Floe window."),
        "window.toggle-maximized" => Some("Maximize or restore this Floe window."),
        "window.close" => Some("Close this Floe window."),
        "win.start-filename-search" => {
            Some("Run the selected filename or content search using the current scope and options.")
        }
        "win.sort-name" => Some("Sort by natural filename order, so 2 appears before 10."),
        "win.sort-type" => Some("Sort by file or folder type."),
        "win.sort-size" => Some("Sort by file size; folder sizes may be unavailable."),
        "win.sort-modified" => Some("Sort by the last modification time."),
        "win.sort-extension" => Some("Sort by the filename extension."),
        _ => None,
    }
}

/// Short explanations for ordinary dialog controls which are intentionally not
/// registered application commands.
pub fn help_for_control_label(label: &str) -> Option<&'static str> {
    let normalized = label.trim().trim_end_matches(['…', '.']);
    match normalized {
        "Apply" => Some("Apply the reviewed choices and continue."),
        "Cancel" => Some("Cancel and return without continuing."),
        "Close" => Some("Close this view."),
        "Continue" => Some("Continue with the current operation."),
        "Copy" => Some("Copy the selected item without removing the original."),
        "Delete" => Some("Delete the selected item after the required confirmation."),
        "Dismiss" => Some("Dismiss this message without changing its result."),
        "Done" => Some("Finish and close this view."),
        "Open" => Some("Open the selected item or navigate into the selected folder."),
        "Refresh" => Some("Reload the latest information."),
        "Remove" => Some("Remove the selected entry after the required confirmation."),
        "Rename" => Some("Change the selected item's name without changing its contents."),
        "Reset" => Some("Restore this option to Floe's default value."),
        "Retry" => Some("Try the failed operation again."),
        "Save" => Some("Save the reviewed choices."),
        "Stop" => Some("Request cancellation of the running operation."),
        _ => None,
    }
}

fn help_for_command_label(label: &str) -> Option<&'static str> {
    let normalized = normalize_label(label);
    command_registry::COMMANDS
        .iter()
        .find(|command| normalize_label(command.name) == normalized)
        .map(|command| command.description)
        .filter(|description| !description.trim().is_empty())
}

fn normalize_label(label: &str) -> String {
    label
        .trim()
        .trim_end_matches(['…', '.'])
        .replace('_', "")
        .to_lowercase()
}

/// Apply existing explicit help and central action descriptions to the current
/// widget tree. Dialog builders call this after their controls are attached.
pub fn install_on_tree(root: &impl IsA<gtk::Widget>) {
    let root = root.as_ref();
    install_dynamic_help(root);
    install_widget_tree(root);
}

fn install_widget_tree(widget: &gtk::Widget) {
    apply_widget_help(widget);

    if let Some(menu_button) = widget.downcast_ref::<gtk::MenuButton>() {
        if let Some(popover) = menu_button.popover() {
            install_dynamic_help(popover.upcast_ref());
            install_widget_tree(popover.upcast_ref());
        }
    } else if let Some(popover) = widget.downcast_ref::<gtk::Popover>() {
        install_dynamic_help(popover.upcast_ref());
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        install_widget_tree(&current);
    }
}

fn apply_widget_help(widget: &gtk::Widget) {
    let Some(help) = resolved_widget_help(widget) else {
        return;
    };
    if widget.tooltip_text().is_none() {
        widget.set_tooltip_text(Some(&help));
    }
    widget.update_property(&[gtk::accessible::Property::Description(&help)]);
}

fn resolved_widget_help(widget: &gtk::Widget) -> Option<String> {
    widget
        .tooltip_text()
        .map(|text| text.to_string())
        .or_else(|| {
            widget_action_name(widget)
                .and_then(|action| help_for_action(&action).map(str::to_owned))
        })
        .or_else(|| {
            widget_menu_label(widget).map(|label| {
                help_for_command_label(&label)
                    .unwrap_or(MODEL_MENU_FALLBACK_HELP)
                    .to_owned()
            })
        })
        .or_else(|| {
            widget_control_label(widget)
                .and_then(|label| help_for_control_label(&label).map(str::to_owned))
        })
        .filter(|help| !help.trim().is_empty())
}

fn widget_action_name(widget: &gtk::Widget) -> Option<String> {
    widget.find_property("action-name")?;
    widget
        .property_value("action-name")
        .get::<Option<String>>()
        .ok()
        .flatten()
}

fn widget_control_label(widget: &gtk::Widget) -> Option<String> {
    if let Some(button) = widget.downcast_ref::<gtk::Button>() {
        button.label().map(|label| label.to_string())
    } else if let Some(button) = widget.downcast_ref::<gtk::CheckButton>() {
        button.label().map(|label| label.to_string())
    } else {
        None
    }
}

fn widget_menu_label(widget: &gtk::Widget) -> Option<String> {
    widget.find_property("text")?;
    widget.property_value("text").get::<String>().ok()
}

fn install_dynamic_help(root_widget: &gtk::Widget) {
    if root_widget.has_css_class(HELP_INSTALLED_CLASS) {
        return;
    }
    root_widget.add_css_class(HELP_INSTALLED_CLASS);
    root_widget.set_has_tooltip(true);
    if root_widget.find_property("focus-widget").is_some() {
        root_widget.connect_notify_local(Some("focus-widget"), |root_widget, _| {
            if let Some(focus) = root_widget.root().and_then(|root| root.focus()) {
                apply_widget_help(&focus);
            }
        });
    }
    root_widget.connect_query_tooltip(|root_widget, x, y, keyboard_mode, tooltip| {
        let target = if keyboard_mode {
            root_widget
                .root()
                .and_then(|root| root.focus())
                .filter(|focus| focus.is_ancestor(root_widget))
        } else {
            root_widget.pick(f64::from(x), f64::from(y), gtk::PickFlags::DEFAULT)
        };
        let Some(mut target) = target else {
            return false;
        };

        loop {
            if let Some(help) = resolved_widget_help(&target) {
                tooltip.set_text(Some(&help));
                target.update_property(&[gtk::accessible::Property::Description(&help)]);
                return true;
            }
            if target == *root_widget {
                break;
            }
            let Some(parent) = target.parent() else {
                break;
            };
            target = parent;
        }
        false
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_20c_help_resolution_reuses_meaningful_command_metadata() {
        for action in [
            "win.audit-permissions",
            "audit-permissions",
            "win.command-palette",
            "win.open-as-administrator",
        ] {
            let help =
                help_for_action(action).unwrap_or_else(|| panic!("missing help for {action}"));
            assert!(!help.trim().is_empty());
            assert!(!help.ends_with(action));
        }
        assert_eq!(help_for_action(""), None);
        assert_eq!(help_for_action("win.not-a-real-action"), None);
        assert!(
            help_for_control_label("Cancel")
                .expect("standard Cancel help")
                .contains("without")
        );
        assert_eq!(help_for_control_label("Unknown label"), None);
        assert!(
            help_for_command_label("Audit Permissions…")
                .expect("registered Permission Audit label help")
                .contains("Unix modes")
        );
    }

    #[test]
    fn phase_20c_action_help_covers_every_searchable_command() {
        for command in command_registry::COMMANDS
            .iter()
            .filter(|command| command.searchable)
        {
            let help = help_for_action(command.action)
                .unwrap_or_else(|| panic!("missing action help for {}", command.action));
            assert!(!help.trim().is_empty());
            assert_ne!(help.trim(), command.name.trim());
        }
        for (index, command) in command_registry::COMMANDS.iter().enumerate() {
            for sibling in &command_registry::COMMANDS[index + 1..] {
                if normalize_label(command.name) == normalize_label(sibling.name) {
                    assert_eq!(
                        command.description, sibling.description,
                        "ambiguous menu label {} has conflicting help",
                        command.name
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "requires a real disposable GTK display"]
    fn phase_testing_gtk_phase_20c_action_help_reaches_native_widgets_and_menus() {
        fn menu_label_has_help(widget: &gtk::Widget, label: &str) -> bool {
            if widget_menu_label(widget).as_deref() == Some(label)
                && widget
                    .tooltip_text()
                    .is_some_and(|help| !help.trim().is_empty())
            {
                return true;
            }
            let mut child = widget.first_child();
            while let Some(current) = child {
                child = current.next_sibling();
                if menu_label_has_help(&current, label) {
                    return true;
                }
            }
            false
        }

        gtk::init().expect("GTK initialization");
        let action_button = gtk::Button::builder()
            .label("Audit Permissions…")
            .action_name("win.audit-permissions")
            .build();
        let model = gtk::gio::Menu::new();
        model.append(Some("Audit Permissions…"), Some("win.audit-permissions"));
        model.append(Some("User-defined action"), Some("win.run-custom-action"));
        let menu = gtk::PopoverMenu::from_model(Some(&model));
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&action_button);
        root.append(&menu);

        install_on_tree(&root);

        let help = action_button
            .tooltip_text()
            .expect("registered action should receive hover help");
        assert!(help.contains("Unix modes"));
        assert!(menu.has_tooltip());
        assert!(menu.has_css_class(HELP_INSTALLED_CLASS));
        assert!(menu_label_has_help(menu.upcast_ref(), "Audit Permissions…"));
        assert!(menu_label_has_help(
            menu.upcast_ref(),
            "User-defined action"
        ));
    }

    #[test]
    #[ignore = "requires a real disposable GTK display"]
    fn phase_testing_gtk_phase_20c_main_controls_have_hover_help() {
        fn self_or_control_ancestor_has_help(widget: &gtk::Widget) -> bool {
            let mut current = Some(widget.clone());
            for _ in 0..3 {
                let Some(candidate) = current else {
                    break;
                };
                if candidate
                    .tooltip_text()
                    .is_some_and(|help| !help.trim().is_empty())
                {
                    return true;
                }
                current = candidate.parent();
            }
            false
        }

        fn collect_missing(widget: &gtk::Widget, missing: &mut Vec<String>) {
            let interactive = widget.is::<gtk::Button>()
                || widget.is::<gtk::CheckButton>()
                || widget.is::<gtk::DropDown>()
                || widget.is::<gtk::Entry>()
                || widget.is::<gtk::Scale>()
                || widget.is::<gtk::SpinButton>()
                || widget.is::<gtk::Switch>()
                || widget.type_().name() == "GtkModelButton";
            if interactive && !self_or_control_ancestor_has_help(widget) {
                missing.push(format!(
                    "{} label={:?} action={:?} classes={:?} parent={:?}",
                    widget.type_().name(),
                    widget_control_label(widget),
                    widget_action_name(widget),
                    widget.css_classes(),
                    widget.parent().map(|parent| parent.type_().name())
                ));
            }
            let mut child = widget.first_child();
            while let Some(current) = child {
                child = current.next_sibling();
                collect_missing(&current, missing);
            }
        }

        gtk::init().expect("GTK initialization");
        adw::init().expect("libadwaita initialization");
        let display = gtk::gdk::Display::default().expect("GTK display");
        crate::iconography::register(&display);
        let application = adw::Application::builder()
            .application_id("io.github.rodriguezcappsec.Floe.Phase20CHelpTest")
            .build();
        application
            .register(None::<&gtk::gio::Cancellable>)
            .expect("register component-test application");
        let widgets = crate::ui::build(
            &application,
            &[],
            crate::appearance::Appearance::for_preset(crate::appearance::AppearancePreset::Native),
            crate::preferences::ViewPreferences::default(),
        );
        let mut missing = Vec::new();
        collect_missing(widgets.window.upcast_ref(), &mut missing);
        assert!(
            missing.is_empty(),
            "missing hover help:\n{}",
            missing.join("\n")
        );
    }
}
