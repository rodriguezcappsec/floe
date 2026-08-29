//! Native discovery and editing surface for effective Floe shortcuts.

use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;

use crate::{
    command_registry::{self, CommandActionSource, CommandRisk},
    keybindings::KeybindingOverrides,
};

pub const SHORTCUT_SEARCH_CAPACITY: usize = 128;
pub const SHORTCUT_ROW_CAPACITY: usize = 128;

type ChangeCallback = Box<dyn Fn(KeybindingOverrides)>;

#[derive(Clone)]
pub struct KeyboardShortcuts {
    inner: Rc<KeyboardShortcutsInner>,
}

struct KeyboardShortcutsInner {
    window: adw::ApplicationWindow,
    dialog: adw::Dialog,
    search: gtk::SearchEntry,
    list: gtk::ListBox,
    editor: gtk::Entry,
    apply: gtk::Button,
    reset: gtk::Button,
    reset_all: gtk::Button,
    status: gtk::Label,
    current: RefCell<KeybindingOverrides>,
    visible_actions: RefCell<Vec<&'static str>>,
    selected_action: RefCell<Option<&'static str>>,
    on_change: RefCell<Option<ChangeCallback>>,
}

impl KeyboardShortcuts {
    pub fn new(window: &adw::ApplicationWindow) -> Self {
        let dialog = adw::Dialog::builder()
            .title("Keyboard Shortcuts")
            .content_width(760)
            .content_height(640)
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(18);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);

        let heading = gtk::Label::builder()
            .label("Keyboard Shortcuts")
            .xalign(0.0)
            .css_classes(["title-2"])
            .build();
        let explanation = gtk::Label::builder()
            .label("Search every command, then enter comma-separated GTK shortcuts. Changes are saved with Floe settings.")
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build();
        let search = gtk::SearchEntry::builder()
            .placeholder_text("Search commands or shortcuts")
            .hexpand(true)
            .build();
        search.update_property(&[gtk::accessible::Property::Label(
            "Search keyboard shortcuts",
        )]);

        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["boxed-list"])
            .build();
        list.update_property(&[
            gtk::accessible::Property::Label("Floe commands and shortcuts"),
            gtk::accessible::Property::Description(
                "Select a command to inspect or customize its shortcuts",
            ),
        ]);
        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&list)
            .build();

        let editor = gtk::Entry::builder()
            .placeholder_text("Example: <Control><Shift>k, <Alt>k")
            .hexpand(true)
            .build();
        editor.update_property(&[gtk::accessible::Property::Label(
            "Selected command shortcuts",
        )]);
        let apply = gtk::Button::with_label("Apply");
        apply.add_css_class("suggested-action");
        let reset = gtk::Button::with_label("Reset Selected");
        let reset_all = gtk::Button::with_label("Reset All");
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        controls.append(&editor);
        controls.append(&apply);
        controls.append(&reset);
        controls.append(&reset_all);
        let status = gtk::Label::builder().xalign(0.0).wrap(true).build();
        status.update_property(&[gtk::accessible::Property::Label("Shortcut editing status")]);

        content.append(&heading);
        content.append(&explanation);
        content.append(&search);
        content.append(&scroll);
        content.append(&controls);
        content.append(&status);
        dialog.set_child(Some(&content));

        let inner = Rc::new(KeyboardShortcutsInner {
            window: window.clone(),
            dialog,
            search,
            list,
            editor,
            apply,
            reset,
            reset_all,
            status,
            current: RefCell::new(KeybindingOverrides::default()),
            visible_actions: RefCell::new(Vec::new()),
            selected_action: RefCell::new(None),
            on_change: RefCell::new(None),
        });
        KeyboardShortcutsInner::wire(&inner);
        Self { inner }
    }

    pub fn present<F>(&self, current: KeybindingOverrides, on_change: F)
    where
        F: Fn(KeybindingOverrides) + 'static,
    {
        *self.inner.current.borrow_mut() = current;
        *self.inner.on_change.borrow_mut() = Some(Box::new(on_change));
        self.inner.search.set_text("");
        self.inner.refresh();
        self.inner.dialog.present(Some(&self.inner.window));
        self.inner.search.grab_focus();
    }
}

impl KeyboardShortcutsInner {
    fn wire(this: &Rc<Self>) {
        let weak = Rc::downgrade(this);
        this.search.connect_search_changed(move |_| {
            if let Some(this) = weak.upgrade() {
                this.refresh();
            }
        });

        let weak = Rc::downgrade(this);
        this.list.connect_row_selected(move |_, row| {
            if let Some(this) = weak.upgrade() {
                let action = row.and_then(|row| {
                    usize::try_from(row.index())
                        .ok()
                        .and_then(|index| this.visible_actions.borrow().get(index).copied())
                });
                this.select(action);
            }
        });

        let weak = Rc::downgrade(this);
        this.apply.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.apply_selected();
            }
        });
        let weak = Rc::downgrade(this);
        this.editor.connect_activate(move |_| {
            if let Some(this) = weak.upgrade() {
                this.apply_selected();
            }
        });

        let weak = Rc::downgrade(this);
        this.reset.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.reset_selected();
            }
        });

        let weak = Rc::downgrade(this);
        this.reset_all.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                let changed = this.current.borrow_mut().reset_all();
                if changed {
                    this.publish();
                    this.status
                        .set_label("All shortcuts restored to their defaults.");
                } else {
                    this.status
                        .set_label("All shortcuts already use their defaults.");
                }
                this.refresh();
            }
        });
    }

    fn refresh(&self) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let query: String = self
            .search
            .text()
            .trim()
            .chars()
            .take(SHORTCUT_SEARCH_CAPACITY)
            .flat_map(char::to_lowercase)
            .collect();
        let current = self.current.borrow();
        let mut visible = Vec::new();
        for definition in command_registry::COMMANDS
            .iter()
            .take(SHORTCUT_ROW_CAPACITY)
        {
            let shortcuts = current.effective(definition);
            let haystack = format!(
                "{} {} {} {} {}",
                definition.name,
                definition.description,
                definition.category.label(),
                definition.search_terms.join(" "),
                shortcuts.join(" ")
            )
            .to_lowercase();
            if !query.is_empty() && !haystack.contains(&query) {
                continue;
            }
            visible.push(definition.action);
            self.list.append(&command_row(
                definition,
                &shortcuts,
                self.window
                    .command_action(definition.action_name())
                    .is_some(),
                current.is_overridden(definition.action),
            ));
        }
        drop(current);
        *self.visible_actions.borrow_mut() = visible;
        if let Some(row) = self.list.row_at_index(0) {
            self.list.select_row(Some(&row));
        } else {
            self.select(None);
            self.status.set_label("No commands match this search.");
        }
    }

    fn select(&self, action: Option<&'static str>) {
        *self.selected_action.borrow_mut() = action;
        let Some(definition) = action.and_then(command_registry::command) else {
            self.editor.set_text("");
            self.editor.set_sensitive(false);
            self.apply.set_sensitive(false);
            self.reset.set_sensitive(false);
            return;
        };
        let protected = matches!(
            definition.risk,
            CommandRisk::ConfirmationRequired | CommandRisk::Irreversible
        );
        self.editor
            .set_text(&self.current.borrow().effective(definition).join(", "));
        self.editor.set_sensitive(!protected);
        self.apply.set_sensitive(!protected);
        self.reset
            .set_sensitive(!protected && self.current.borrow().is_overridden(definition.action));
        if protected {
            self.status.set_label(
                "This destructive command keeps its reviewed shortcut to prevent accidental activation.",
            );
        } else {
            self.status.set_label(
                "Enter up to four comma-separated shortcuts. Leave the field empty to disable this command shortcut.",
            );
        }
    }

    fn apply_selected(&self) {
        let Some(action) = *self.selected_action.borrow() else {
            return;
        };
        match self
            .current
            .borrow_mut()
            .set_from_text(action, self.editor.text().as_str())
        {
            Ok(()) => {
                self.publish();
                self.status.set_label("Shortcut updated and saved.");
                self.refresh();
            }
            Err(error) => self.status.set_label(&error.to_string()),
        }
    }

    fn reset_selected(&self) {
        let Some(action) = *self.selected_action.borrow() else {
            return;
        };
        if self.current.borrow_mut().reset(action) {
            self.publish();
            self.status
                .set_label("Selected command restored to its default shortcuts.");
            self.refresh();
        }
    }

    fn publish(&self) {
        if let Some(callback) = self.on_change.borrow().as_ref() {
            callback(self.current.borrow().clone());
        }
    }
}

fn command_row(
    definition: &command_registry::CommandDefinition,
    shortcuts: &[String],
    action_present: bool,
    overridden: bool,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    content.set_margin_start(10);
    content.set_margin_end(10);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let name = gtk::Label::builder()
        .label(definition.name)
        .xalign(0.0)
        .hexpand(true)
        .build();
    let detail = gtk::Label::builder()
        .label(format!(
            "{} · {}{}",
            definition.category.label(),
            definition.description,
            if action_present {
                ""
            } else {
                " · Unavailable"
            }
        ))
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["dim-label", "caption"])
        .build();
    text.append(&name);
    text.append(&detail);
    let shortcut_text = if shortcuts.is_empty() {
        "Unassigned".to_owned()
    } else {
        shortcuts.join("  ·  ")
    };
    let shortcut = gtk::Label::builder()
        .label(if overridden {
            format!("{shortcut_text} · Custom")
        } else {
            shortcut_text
        })
        .xalign(1.0)
        .css_classes(["dim-label"])
        .build();
    content.append(&text);
    content.append(&shortcut);
    row.set_child(Some(&content));
    row.update_property(&[
        gtk::accessible::Property::Label(definition.name),
        gtk::accessible::Property::Description(&format!(
            "{}. {}. Shortcuts: {}",
            definition.category.label(),
            definition.description,
            if shortcuts.is_empty() {
                "unassigned".to_owned()
            } else {
                shortcuts.join(", ")
            }
        )),
    ]);
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_11c_keybinding_ui_discovers_every_registered_command() {
        assert!(command_registry::COMMANDS.len() <= SHORTCUT_ROW_CAPACITY);
        assert_eq!(SHORTCUT_SEARCH_CAPACITY, 128);
        assert!(command_registry::command("win.keyboard-shortcuts").is_some());
        assert!(command_registry::COMMANDS.iter().all(|definition| {
            !definition.name.trim().is_empty() && !definition.category.label().is_empty()
        }));
    }
}
