//! Bounded context-menu customization over reviewed Floe command groups.

use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;

pub const CONTEXT_MENU_GROUP_CAPACITY: usize = 8;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ContextMenuGroup {
    Archives,
    BatchRename,
    Links,
    CopyDetails,
    Checksums,
    Terminal,
    SplitView,
    PrivacySafety,
}

impl ContextMenuGroup {
    pub const ALL: [Self; CONTEXT_MENU_GROUP_CAPACITY] = [
        Self::Archives,
        Self::BatchRename,
        Self::Links,
        Self::CopyDetails,
        Self::Checksums,
        Self::Terminal,
        Self::SplitView,
        Self::PrivacySafety,
    ];

    pub const DEFAULT: [Self; 6] = [
        Self::Archives,
        Self::BatchRename,
        Self::Links,
        Self::Terminal,
        Self::SplitView,
        Self::PrivacySafety,
    ];

    pub const fn persisted(self) -> &'static str {
        match self {
            Self::Archives => "archives",
            Self::BatchRename => "batch-rename",
            Self::Links => "links",
            Self::CopyDetails => "copy-details",
            Self::Checksums => "checksums",
            Self::Terminal => "terminal",
            Self::SplitView => "split-view",
            Self::PrivacySafety => "privacy-safety",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        match value {
            "archives" => Some(Self::Archives),
            "batch-rename" => Some(Self::BatchRename),
            "links" => Some(Self::Links),
            "copy-details" => Some(Self::CopyDetails),
            "checksums" => Some(Self::Checksums),
            "terminal" => Some(Self::Terminal),
            "split-view" => Some(Self::SplitView),
            "privacy-safety" => Some(Self::PrivacySafety),
            _ => None,
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Archives => "Archives",
            Self::BatchRename => "Batch rename",
            Self::Links => "Links",
            Self::CopyDetails => "Copy details",
            Self::Checksums => "Checksums",
            Self::Terminal => "Terminal",
            Self::SplitView => "Split view",
            Self::PrivacySafety => "Privacy and safety",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Archives => "Extract supported archives and compress selected items",
            Self::BatchRename => "Rename multiple selected items and undo the latest batch",
            Self::Links => "Create symbolic or hard links and reveal link targets",
            Self::CopyDetails => "Copy names, paths, relative paths, and file URIs",
            Self::Checksums => "Calculate SHA-256, SHA-512, or legacy MD5 checksums",
            Self::Terminal => "Open the preferred terminal in a selected or current folder",
            Self::SplitView => "Open, transfer, switch, and resize the other pane",
            Self::PrivacySafety => {
                "Inspect local privacy and safety signals, scan with optional ClamAV, and create verified sanitized image copies"
            }
        }
    }

    const fn bit(self) -> u8 {
        match self {
            Self::Archives => 1 << 0,
            Self::BatchRename => 1 << 1,
            Self::Links => 1 << 2,
            Self::CopyDetails => 1 << 3,
            Self::Checksums => 1 << 4,
            Self::Terminal => 1 << 5,
            Self::SplitView => 1 << 6,
            Self::PrivacySafety => 1 << 7,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextMenuPreferences {
    visible: u8,
}

impl Default for ContextMenuPreferences {
    fn default() -> Self {
        let mut preferences = Self { visible: 0 };
        for group in ContextMenuGroup::DEFAULT {
            preferences.set_visible(group, true);
        }
        preferences
    }
}

impl ContextMenuPreferences {
    pub const fn empty() -> Self {
        Self { visible: 0 }
    }

    pub const fn is_visible(self, group: ContextMenuGroup) -> bool {
        self.visible & group.bit() != 0
    }

    pub fn set_visible(&mut self, group: ContextMenuGroup, visible: bool) {
        if visible {
            self.visible |= group.bit();
        } else {
            self.visible &= !group.bit();
        }
    }

    pub fn parse(value: &str) -> Self {
        let mut preferences = Self::empty();
        for item in value.split(',').map(str::trim) {
            if let Some(group) = ContextMenuGroup::from_persisted(item) {
                preferences.set_visible(group, true);
            }
        }
        preferences
    }

    pub fn persisted(self) -> String {
        ContextMenuGroup::ALL
            .into_iter()
            .filter(|group| self.is_visible(*group))
            .map(ContextMenuGroup::persisted)
            .collect::<Vec<_>>()
            .join(",")
    }

    #[cfg(test)]
    pub fn visible_groups(self) -> impl Iterator<Item = ContextMenuGroup> {
        ContextMenuGroup::ALL
            .into_iter()
            .filter(move |group| self.is_visible(*group))
    }
}

type ChangeCallback = Box<dyn Fn(ContextMenuPreferences)>;

#[derive(Clone)]
pub struct ContextMenuEditor {
    inner: Rc<ContextMenuEditorInner>,
}

struct ContextMenuEditorInner {
    window: adw::ApplicationWindow,
    dialog: adw::Dialog,
    switches: Vec<(ContextMenuGroup, gtk::Switch)>,
    status: gtk::Label,
    on_change: RefCell<Option<ChangeCallback>>,
}

impl ContextMenuEditor {
    pub fn new(window: &adw::ApplicationWindow) -> Self {
        let dialog = adw::Dialog::builder()
            .title("Customize Context Menus")
            .content_width(620)
            .content_height(620)
            .build();

        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(18);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);

        let heading = gtk::Label::builder()
            .label("Customize Context Menus")
            .xalign(0.0)
            .css_classes(["title-2"])
            .build();
        let explanation = gtk::Label::builder()
            .label("Choose optional productivity groups for file and folder-background menus. Open, editing, Trash, permanent delete, Properties, and this customization command always remain available.")
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build();

        let group = adw::PreferencesGroup::builder()
            .title("Optional command groups")
            .description(
                "Unavailable commands remain visibly disabled according to the current selection.",
            )
            .build();
        let mut switches = Vec::with_capacity(CONTEXT_MENU_GROUP_CAPACITY);
        for item in ContextMenuGroup::ALL {
            let toggle = gtk::Switch::builder().valign(gtk::Align::Center).build();
            toggle.update_property(&[
                gtk::accessible::Property::Label(item.title()),
                gtk::accessible::Property::Description(item.description()),
            ]);
            let row = adw::ActionRow::builder()
                .title(item.title())
                .subtitle(item.description())
                .activatable(true)
                .build();
            row.add_suffix(&toggle);
            row.set_activatable_widget(Some(&toggle));
            group.add(&row);
            switches.push((item, toggle));
        }

        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&group)
            .build();
        let status = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build();
        status.update_property(&[gtk::accessible::Property::Label(
            "Context menu customization status",
        )]);

        let reset = gtk::Button::with_label("Reset Defaults");
        let close = gtk::Button::with_label("Close");
        let apply = gtk::Button::with_label("Apply");
        apply.add_css_class("suggested-action");
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        controls.set_halign(gtk::Align::End);
        controls.append(&reset);
        controls.append(&close);
        controls.append(&apply);

        content.append(&heading);
        content.append(&explanation);
        content.append(&scroll);
        content.append(&status);
        content.append(&controls);
        dialog.set_child(Some(&content));

        let inner = Rc::new(ContextMenuEditorInner {
            window: window.clone(),
            dialog,
            switches,
            status,
            on_change: RefCell::new(None),
        });

        let weak = Rc::downgrade(&inner);
        apply.connect_clicked(move |_| {
            if let Some(inner) = weak.upgrade() {
                inner.apply();
            }
        });
        let weak = Rc::downgrade(&inner);
        reset.connect_clicked(move |_| {
            if let Some(inner) = weak.upgrade() {
                inner.set_preferences(ContextMenuPreferences::default());
                inner
                    .status
                    .set_label("Default groups selected. Press Apply to save.");
            }
        });
        let dialog_for_close = inner.dialog.clone();
        close.connect_clicked(move |_| {
            dialog_for_close.close();
        });

        Self { inner }
    }

    pub fn present(
        &self,
        current: ContextMenuPreferences,
        on_change: impl Fn(ContextMenuPreferences) + 'static,
    ) {
        self.inner.set_preferences(current);
        self.inner
            .status
            .set_label("Changes affect list, grid, and Miller context menus after Apply.");
        *self.inner.on_change.borrow_mut() = Some(Box::new(on_change));
        self.inner.dialog.present(Some(&self.inner.window));
        if let Some((_, first)) = self.inner.switches.first() {
            first.grab_focus();
        }
    }
}

impl ContextMenuEditorInner {
    fn set_preferences(&self, preferences: ContextMenuPreferences) {
        for (group, toggle) in &self.switches {
            toggle.set_active(preferences.is_visible(*group));
        }
    }

    fn preferences(&self) -> ContextMenuPreferences {
        let mut preferences = ContextMenuPreferences::empty();
        for (group, toggle) in &self.switches {
            preferences.set_visible(*group, toggle.is_active());
        }
        preferences
    }

    fn apply(&self) {
        let preferences = self.preferences();
        if let Some(callback) = self.on_change.borrow().as_ref() {
            callback(preferences);
        }
        self.status
            .set_label("Context menus updated and queued for saving.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_12f_context_preferences_are_bounded_stable_and_migrate_to_defaults() {
        let defaults = ContextMenuPreferences::default();
        assert!(defaults.is_visible(ContextMenuGroup::Archives));
        assert!(defaults.is_visible(ContextMenuGroup::BatchRename));
        assert!(!defaults.is_visible(ContextMenuGroup::CopyDetails));
        assert_eq!(ContextMenuGroup::ALL.len(), CONTEXT_MENU_GROUP_CAPACITY);

        let parsed = ContextMenuPreferences::parse(
            "checksums,archives,unknown,checksums,copy-details,too-many",
        );
        assert_eq!(parsed.persisted(), "archives,copy-details,checksums");
        assert_eq!(parsed.visible_groups().count(), 3);
    }

    #[test]
    fn phase_12f_context_preferences_allow_an_explicit_empty_optional_menu() {
        let empty = ContextMenuPreferences::parse("");
        assert_eq!(empty, ContextMenuPreferences::empty());
        assert_eq!(empty.persisted(), "");
        assert_eq!(empty.visible_groups().count(), 0);
    }

    #[test]
    fn phase_12f_action_integration_definitions_have_accessible_copy() {
        for group in ContextMenuGroup::ALL {
            assert!(!group.title().trim().is_empty());
            assert!(!group.description().trim().is_empty());
            assert!(ContextMenuGroup::from_persisted(group.persisted()).is_some());
        }
    }
}
