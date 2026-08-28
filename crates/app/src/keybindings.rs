//! Bounded shortcut overrides layered over the central command registry.

use std::collections::{HashMap, HashSet};

use gtk::prelude::GtkApplicationExt;
use thiserror::Error;

use crate::command_registry::{self, CommandDefinition, CommandRisk};

pub const KEYBINDING_OVERRIDE_CAPACITY: usize = 96;
pub const KEYBINDINGS_PER_COMMAND_CAPACITY: usize = 4;
pub const KEYBINDING_TEXT_CAPACITY: usize = 64;
const LOCAL_FILE_VIEW_SHORTCUTS: [(&str, &str); 1] = [("win.quick-preview", "space")];

#[derive(Clone, Debug, Eq, PartialEq)]
struct KeybindingOverride {
    action: &'static str,
    accelerators: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KeybindingOverrides {
    overrides: Vec<KeybindingOverride>,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum KeybindingError {
    #[error("That command is not available for shortcut customization.")]
    UnknownCommand,
    #[error("This security-sensitive command keeps its reviewed shortcut.")]
    ProtectedCommand,
    #[error("Use at most {KEYBINDINGS_PER_COMMAND_CAPACITY} shortcuts for one command.")]
    TooManyBindings,
    #[error("That shortcut is empty, too long, or is not valid GTK accelerator syntax.")]
    InvalidAccelerator,
    #[error("Unmodified typing keys cannot be assigned as custom shortcuts.")]
    UnsafeUnmodifiedKey,
    #[error("{accelerator} is already assigned to {command}.")]
    Conflict {
        accelerator: String,
        command: &'static str,
    },
}

impl KeybindingOverrides {
    pub fn effective(&self, definition: &CommandDefinition) -> Vec<String> {
        self.overrides
            .iter()
            .find(|item| item.action == definition.action)
            .map(|item| item.accelerators.clone())
            .unwrap_or_else(|| {
                definition
                    .default_shortcuts
                    .iter()
                    .map(|shortcut| {
                        canonicalize_accelerator(shortcut)
                            .unwrap_or_else(|_| (*shortcut).to_owned())
                    })
                    .collect()
            })
    }

    pub fn is_overridden(&self, action: &str) -> bool {
        self.overrides.iter().any(|item| item.action == action)
    }

    pub fn set_from_text(&mut self, action: &str, text: &str) -> Result<(), KeybindingError> {
        let bindings = if text.trim().is_empty() {
            Vec::new()
        } else {
            text.split(',')
                .map(canonicalize_accelerator)
                .collect::<Result<Vec<_>, _>>()?
        };
        self.set(action, bindings)
    }

    pub fn set(&mut self, action: &str, accelerators: Vec<String>) -> Result<(), KeybindingError> {
        let Some(definition) = command_registry::command(action) else {
            return Err(KeybindingError::UnknownCommand);
        };
        if matches!(
            definition.risk,
            CommandRisk::ConfirmationRequired | CommandRisk::Irreversible
        ) {
            return Err(KeybindingError::ProtectedCommand);
        }
        if accelerators.len() > KEYBINDINGS_PER_COMMAND_CAPACITY {
            return Err(KeybindingError::TooManyBindings);
        }

        let mut canonical = Vec::with_capacity(accelerators.len());
        let mut unique = HashSet::with_capacity(accelerators.len());
        for accelerator in accelerators {
            let accelerator = canonicalize_accelerator(&accelerator)?;
            if unique.insert(accelerator.clone()) {
                canonical.push(accelerator);
            }
        }

        let mut candidate = self.clone();
        candidate
            .overrides
            .retain(|item| item.action != definition.action);
        if candidate.overrides.len() >= KEYBINDING_OVERRIDE_CAPACITY {
            return Err(KeybindingError::TooManyBindings);
        }
        candidate.overrides.push(KeybindingOverride {
            action: definition.action,
            accelerators: canonical,
        });
        candidate.sort();
        candidate.validate_conflicts()?;
        *self = candidate;
        Ok(())
    }

    pub fn reset(&mut self, action: &str) -> bool {
        let before = self.overrides.len();
        self.overrides.retain(|item| item.action != action);
        self.overrides.len() != before
    }

    pub fn reset_all(&mut self) -> bool {
        let changed = !self.overrides.is_empty();
        self.overrides.clear();
        changed
    }

    pub fn serialize_records(&self) -> Vec<String> {
        self.overrides
            .iter()
            .map(|item| {
                let bindings = if item.accelerators.is_empty() {
                    "!".to_owned()
                } else {
                    item.accelerators.join(",")
                };
                format!("{}\t{bindings}", item.action)
            })
            .collect()
    }

    pub fn apply_record(&mut self, record: &str) -> bool {
        let Some((action, bindings)) = record.split_once('\t') else {
            return false;
        };
        let parsed = if bindings == "!" {
            Vec::new()
        } else {
            bindings.split(',').map(str::to_owned).collect()
        };
        self.set(action, parsed).is_ok()
    }

    fn validate_conflicts(&self) -> Result<(), KeybindingError> {
        let mut assigned: HashMap<String, &'static CommandDefinition> = HashMap::new();
        for definition in command_registry::COMMANDS {
            for accelerator in self.effective(definition) {
                if let Some(existing) = assigned.insert(accelerator.clone(), definition)
                    && existing.action != definition.action
                {
                    return Err(KeybindingError::Conflict {
                        accelerator,
                        command: existing.name,
                    });
                }
            }
        }
        Ok(())
    }

    fn sort(&mut self) {
        self.overrides.sort_by_key(|item| item.action);
    }
}

pub fn install_effective_window_shortcuts(
    application: &adw::Application,
    overrides: &KeybindingOverrides,
) {
    for definition in command_registry::COMMANDS {
        let shortcuts = application_shortcuts(overrides, definition);
        let borrowed = shortcuts.iter().map(String::as_str).collect::<Vec<_>>();
        application.set_accels_for_action(definition.action, &borrowed);
    }
}

pub fn local_file_view_shortcut_enabled(
    overrides: &KeybindingOverrides,
    action: &str,
    accelerator: &str,
) -> bool {
    let Some(definition) = command_registry::command(action) else {
        return false;
    };
    is_local_file_view_shortcut(action, accelerator)
        && overrides
            .effective(definition)
            .iter()
            .any(|effective| effective == accelerator)
}

fn application_shortcuts(
    overrides: &KeybindingOverrides,
    definition: &CommandDefinition,
) -> Vec<String> {
    overrides
        .effective(definition)
        .into_iter()
        .filter(|accelerator| !is_local_file_view_shortcut(definition.action, accelerator))
        .collect()
}

fn is_local_file_view_shortcut(action: &str, accelerator: &str) -> bool {
    LOCAL_FILE_VIEW_SHORTCUTS
        .iter()
        .any(|local| local == &(action, accelerator))
}

fn canonicalize_accelerator(value: &str) -> Result<String, KeybindingError> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > KEYBINDING_TEXT_CAPACITY
        || trimmed.contains([',', '\t', '\n', '\r'])
    {
        return Err(KeybindingError::InvalidAccelerator);
    }
    let mut remainder = trimmed;
    let mut modifiers = HashSet::new();
    while let Some(after_open) = remainder.strip_prefix('<') {
        let Some(end) = after_open.find('>') else {
            return Err(KeybindingError::InvalidAccelerator);
        };
        let modifier =
            canonical_modifier(&after_open[..end]).ok_or(KeybindingError::InvalidAccelerator)?;
        if !modifiers.insert(modifier) {
            return Err(KeybindingError::InvalidAccelerator);
        }
        remainder = &after_open[end + 1..];
    }
    let key = canonical_key(remainder).ok_or(KeybindingError::InvalidAccelerator)?;
    if modifiers.is_empty() && (key.chars().count() == 1 || key == "space") {
        return Err(KeybindingError::UnsafeUnmodifiedKey);
    }
    let mut canonical = String::new();
    for modifier in ["Control", "Shift", "Alt", "Super", "Meta", "Hyper"] {
        if modifiers.contains(modifier) {
            canonical.push('<');
            canonical.push_str(modifier);
            canonical.push('>');
        }
    }
    canonical.push_str(&key);
    Ok(canonical)
}

fn canonical_modifier(value: &str) -> Option<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "control" | "ctrl" | "primary" => Some("Control"),
        "shift" => Some("Shift"),
        "alt" | "mod1" => Some("Alt"),
        "super" | "mod4" => Some("Super"),
        "meta" => Some("Meta"),
        "hyper" => Some("Hyper"),
        _ => None,
    }
}

fn canonical_key(value: &str) -> Option<String> {
    if value.chars().count() == 1 {
        let character = value.chars().next()?;
        if character.is_ascii_alphanumeric() {
            return Some(character.to_ascii_lowercase().to_string());
        }
    }
    let lower = value.to_ascii_lowercase();
    if let Some(number) = lower
        .strip_prefix('f')
        .and_then(|part| part.parse::<u8>().ok())
        && (1..=35).contains(&number)
    {
        return Some(format!("F{number}"));
    }
    let canonical = match lower.as_str() {
        "left" => "Left",
        "right" => "Right",
        "up" => "Up",
        "down" => "Down",
        "home" => "Home",
        "end" => "End",
        "page_up" | "pageup" => "Page_Up",
        "page_down" | "pagedown" => "Page_Down",
        "return" | "enter" => "Return",
        "kp_enter" => "KP_Enter",
        "escape" | "esc" => "Escape",
        "delete" | "del" => "Delete",
        "backspace" => "BackSpace",
        "space" => "space",
        "tab" => "Tab",
        "iso_left_tab" => "ISO_Left_Tab",
        "plus" => "plus",
        "minus" => "minus",
        "equal" => "equal",
        "question" => "question",
        "comma" => "comma",
        "period" => "period",
        "slash" => "slash",
        _ => return None,
    };
    Some(canonical.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_11c_keybinding_model_validates_conflicts_resets_and_risk() {
        let mut overrides = KeybindingOverrides::default();
        overrides
            .set_from_text("win.back", "<Control>b, <Alt>b")
            .expect("valid override");
        assert_eq!(
            overrides.effective(command_registry::command("win.back").expect("back")),
            ["<Control>b", "<Alt>b"]
        );
        assert!(matches!(
            overrides.set_from_text("win.forward", "<Control>b"),
            Err(KeybindingError::Conflict { .. })
        ));
        assert_eq!(
            overrides.set_from_text("win.permanent-delete", "<Control>Delete"),
            Err(KeybindingError::ProtectedCommand)
        );
        assert_eq!(
            overrides.set_from_text("win.back", "x"),
            Err(KeybindingError::UnsafeUnmodifiedKey)
        );
        assert!(overrides.reset("win.back"));
        assert!(!overrides.is_overridden("win.back"));
        assert!(!overrides.reset_all());
    }

    #[test]
    fn phase_11c_keybinding_model_parser_is_bounded_and_rejects_hostile_records() {
        let mut overrides = KeybindingOverrides::default();
        assert!(overrides.apply_record("win.back\t<Control>b"));
        assert!(overrides.apply_record("win.forward\t!"));
        assert!(!overrides.apply_record("win.unknown\t<Control>u"));
        assert!(!overrides.apply_record("win.parent\tbad accelerator"));
        assert!(!overrides.apply_record("win.parent\t<Control>p\nwin.trash\t!"));
        assert!(overrides.serialize_records().len() <= KEYBINDING_OVERRIDE_CAPACITY);
    }

    #[test]
    fn phase_11c_effective_accelerators_preserve_defaults_without_overrides() {
        let overrides = KeybindingOverrides::default();
        assert!(command_registry::COMMANDS.len() <= KEYBINDING_OVERRIDE_CAPACITY);
        for definition in command_registry::COMMANDS {
            assert!(definition.default_shortcuts.iter().all(|shortcut| {
                canonicalize_accelerator(shortcut).is_ok() || *shortcut == "space"
            }));
        }
        assert_eq!(
            overrides.effective(command_registry::command("win.back").expect("back")),
            ["<Alt>Left"]
        );
        assert_eq!(
            overrides.effective(command_registry::command("win.refresh").expect("refresh")),
            ["F5", "<Control>r"]
        );
    }

    #[test]
    fn phase_9f_bare_space_is_scoped_to_file_views_not_application_text_input() {
        let quick_preview =
            command_registry::command("win.quick-preview").expect("quick preview command");
        let defaults = KeybindingOverrides::default();
        assert!(local_file_view_shortcut_enabled(
            &defaults,
            "win.quick-preview",
            "space"
        ));
        assert!(application_shortcuts(&defaults, quick_preview).is_empty());
        let application = adw::Application::builder()
            .application_id("io.github.floe.ShortcutTest")
            .build();
        install_effective_window_shortcuts(&application, &defaults);
        assert!(
            application
                .accels_for_action("win.quick-preview")
                .is_empty()
        );

        let mut customized = KeybindingOverrides::default();
        customized
            .set_from_text("win.quick-preview", "<Control>p")
            .expect("modified quick preview binding");
        assert!(!local_file_view_shortcut_enabled(
            &customized,
            "win.quick-preview",
            "space"
        ));
        assert_eq!(
            application_shortcuts(&customized, quick_preview),
            ["<Control>p"]
        );
        install_effective_window_shortcuts(&application, &customized);
        assert_eq!(
            application.accels_for_action("win.quick-preview"),
            ["<Control>p"]
        );

        customized
            .set_from_text("win.quick-preview", "")
            .expect("disabled quick preview binding");
        assert!(application_shortcuts(&customized, quick_preview).is_empty());
    }

    #[test]
    fn phase_11c_keybinding_preferences_round_trip_and_migrate_legacy_defaults() {
        let legacy = crate::preferences::ViewPreferences::parse("version=4\nview=list\n");
        assert!(!legacy.keybindings.is_overridden("win.back"));

        let mut preferences = legacy;
        preferences
            .keybindings
            .set_from_text("win.back", "<Control>b")
            .expect("valid override");
        preferences
            .keybindings
            .set_from_text("win.forward", "")
            .expect("disabled binding");
        let serialized = preferences.serialize();
        assert!(serialized.starts_with("version=13\n"));
        assert!(serialized.contains("keybinding=win.back\t<Control>b\n"));
        assert!(serialized.contains("keybinding=win.forward\t!\n"));
        assert_eq!(
            crate::preferences::ViewPreferences::parse(&serialized).keybindings,
            preferences.keybindings
        );
    }
}
