//! Native, searchable settings surface over Floe's authoritative preferences.

use adw::prelude::*;

use crate::{
    appearance::AppearancePreset,
    iconography::EntryIconStyle,
    preferences::{SidebarDensity, ViewPreferences},
    view::{FileViewDensity, GRID_SIZES, GridSize, ViewMode},
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum SettingsSection {
    Appearance,
    Browsing,
    ViewsLayout,
    SearchPreview,
    OperationsSafety,
    Applications,
    ShortcutsMenus,
    Accessibility,
}

impl SettingsSection {
    const ALL: [Self; 8] = [
        Self::Appearance,
        Self::Browsing,
        Self::ViewsLayout,
        Self::SearchPreview,
        Self::OperationsSafety,
        Self::Applications,
        Self::ShortcutsMenus,
        Self::Accessibility,
    ];

    const fn title(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Browsing => "Browsing",
            Self::ViewsLayout => "Views & Layout",
            Self::SearchPreview => "Search & Preview",
            Self::OperationsSafety => "Operations & Safety",
            Self::Applications => "Applications",
            Self::ShortcutsMenus => "Shortcuts & Menus",
            Self::Accessibility => "Accessibility",
        }
    }

    const fn description(self) -> &'static str {
        match self {
            Self::Appearance => "Choose Floe's visual preset and file icon language.",
            Self::Browsing => "Set the default way folders open and how navigation behaves.",
            Self::ViewsLayout => "Tune file spacing, icon size, and sidebar density.",
            Self::SearchPreview => "Control private search acceleration and preview state.",
            Self::OperationsSafety => "Review operation recovery and data-loss guardrails.",
            Self::Applications => "Choose external applications and inspect desktop support.",
            Self::ShortcutsMenus => "Customize keyboard workflows and right-click menus.",
            Self::Accessibility => "Understand system accessibility behavior and keyboard access.",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum SettingId {
    AppearancePreset,
    IconStyle,
    DefaultView,
    RememberFolderView,
    VimNavigation,
    GridSize,
    FileDensity,
    SidebarDensity,
    SearchIndex,
    ClearPreviewCache,
    OperationHistory,
    RecoveryCenter,
    ProtectedFolders,
    PreferredTerminal,
    DesktopIntegration,
    KeyboardShortcuts,
    ContextMenus,
    CommandPalette,
    SystemAccessibility,
    ReducedMotion,
}

#[derive(Clone, Copy, Debug)]
struct SettingDefinition {
    id: SettingId,
    section: SettingsSection,
    title: &'static str,
    description: &'static str,
    keywords: &'static str,
    action: Option<&'static str>,
}

const SETTINGS: [SettingDefinition; 20] = [
    setting(
        SettingId::AppearancePreset,
        SettingsSection::Appearance,
        "Appearance preset",
        "Choose Native, Glass, Frosted, Minimal, or Compact styling.",
        "theme glass frosted transparency compact native visual",
        None,
    ),
    setting(
        SettingId::IconStyle,
        SettingsSection::Appearance,
        "File and folder icons",
        "Choose Floe Color, Phosphor Monochrome, or your system icon theme.",
        "folder style phosphor freedesktop mime symbols color",
        None,
    ),
    setting(
        SettingId::DefaultView,
        SettingsSection::Browsing,
        "Default folder view",
        "Choose List, Grid, or spatial Miller columns for folders without a remembered view.",
        "list grid columns browse folder",
        None,
    ),
    setting(
        SettingId::RememberFolderView,
        SettingsSection::Browsing,
        "Remember each folder's view",
        "Store a bounded per-folder view choice instead of applying one global default.",
        "per folder memory layout remember",
        None,
    ),
    setting(
        SettingId::VimNavigation,
        SettingsSection::Browsing,
        "Vim navigation",
        "Enable optional h, j, k, l, g, G, and o navigation without replacing normal shortcuts.",
        "keyboard power user keys vim",
        None,
    ),
    setting(
        SettingId::GridSize,
        SettingsSection::ViewsLayout,
        "Grid icon size",
        "Set the default icon and thumbnail size used in Grid view.",
        "zoom thumbnail tile size",
        None,
    ),
    setting(
        SettingId::FileDensity,
        SettingsSection::ViewsLayout,
        "File spacing",
        "Choose compact, comfortable, or spacious rows and tiles.",
        "density padding compact comfortable spacious",
        None,
    ),
    setting(
        SettingId::SidebarDensity,
        SettingsSection::ViewsLayout,
        "Sidebar spacing",
        "Adjust spacing between Places, Bookmarks, and Devices while preserving your sidebar width.",
        "places devices bookmarks padding left panel",
        None,
    ),
    setting(
        SettingId::SearchIndex,
        SettingsSection::SearchPreview,
        "Private filename index",
        "Optionally accelerate filename searches with local metadata only; live search remains the fallback.",
        "search fast local privacy filenames metadata",
        None,
    ),
    setting(
        SettingId::ClearPreviewCache,
        SettingsSection::SearchPreview,
        "Clear preview memory",
        "Discard Floe's memory-only Quick Preview cache.",
        "thumbnail quick preview cache reset",
        Some("win.preview-clear-cache"),
    ),
    setting(
        SettingId::OperationHistory,
        SettingsSection::OperationsSafety,
        "Operation history",
        "Review completed and active file operations and safe Undo availability.",
        "jobs copy move undo progress",
        Some("win.operation-history"),
    ),
    setting(
        SettingId::RecoveryCenter,
        SettingsSection::OperationsSafety,
        "Recovery Center",
        "Review conservatively journaled operations left by an interrupted Floe session.",
        "restart interrupted journal retry resolve",
        Some("win.recovery-center"),
    ),
    setting(
        SettingId::ProtectedFolders,
        SettingsSection::OperationsSafety,
        "Protected Folders",
        "Review accidental-change guardrails and their current coverage.",
        "delete guardrail safety data loss",
        Some("win.protected-folders"),
    ),
    setting(
        SettingId::PreferredTerminal,
        SettingsSection::Applications,
        "Preferred terminal",
        "Choose the reviewed terminal application used by Open Terminal Here.",
        "shell console external app",
        Some("win.terminal-preferences"),
    ),
    setting(
        SettingId::DesktopIntegration,
        SettingsSection::Applications,
        "Desktop integration",
        "Inspect the generic Wayland and desktop capabilities available to Floe.",
        "gio xdg portal wayland plasma niri",
        Some("win.desktop-integration-status"),
    ),
    setting(
        SettingId::KeyboardShortcuts,
        SettingsSection::ShortcutsMenus,
        "Keyboard shortcuts",
        "Review every shortcut and customize supported key bindings.",
        "hotkeys keys discover commands",
        Some("win.keyboard-shortcuts"),
    ),
    setting(
        SettingId::ContextMenus,
        SettingsSection::ShortcutsMenus,
        "Context menu contents",
        "Choose useful reviewed command groups shown in file and folder right-click menus.",
        "right click popup actions customize",
        Some("win.context-menu-settings"),
    ),
    setting(
        SettingId::CommandPalette,
        SettingsSection::ShortcutsMenus,
        "Command palette",
        "Search Floe's human-readable commands from one keyboard-first surface.",
        "actions commands ctrl shift p",
        Some("win.command-palette"),
    ),
    setting(
        SettingId::SystemAccessibility,
        SettingsSection::Accessibility,
        "System text, contrast, and input",
        "Floe follows GTK and desktop accessibility settings for text, contrast, focus, and assistive technology.",
        "screen reader high contrast font scale at spi",
        None,
    ),
    setting(
        SettingId::ReducedMotion,
        SettingsSection::Accessibility,
        "Reduced motion",
        "Animations follow the GTK system preference; change it in your desktop accessibility settings.",
        "animation disable reduce motion accessibility",
        None,
    ),
];

const fn setting(
    id: SettingId,
    section: SettingsSection,
    title: &'static str,
    description: &'static str,
    keywords: &'static str,
    action: Option<&'static str>,
) -> SettingDefinition {
    SettingDefinition {
        id,
        section,
        title,
        description,
        keywords,
        action,
    }
}

fn definition(id: SettingId) -> &'static SettingDefinition {
    SETTINGS
        .iter()
        .find(|definition| definition.id == id)
        .expect("every settings row has metadata")
}

fn normalized_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|term| term.to_lowercase())
        .collect()
}

fn setting_matches(definition: &SettingDefinition, terms: &[String]) -> bool {
    if terms.is_empty() {
        return true;
    }
    let haystack = format!(
        "{} {} {} {}",
        definition.section.title(),
        definition.title,
        definition.description,
        definition.keywords
    )
    .to_lowercase();
    terms.iter().all(|term| haystack.contains(term))
}

#[cfg(test)]
fn matching_settings(query: &str) -> Vec<SettingId> {
    let terms = normalized_terms(query);
    SETTINGS
        .iter()
        .filter(|definition| setting_matches(definition, &terms))
        .map(|definition| definition.id)
        .collect()
}

#[derive(Clone)]
struct FilterGroup {
    group: adw::PreferencesGroup,
    rows: Vec<(gtk::Widget, SettingId)>,
}

pub struct SettingsCenterWidgets {
    pub dialog: adw::Dialog,
    pub search: gtk::SearchEntry,
    pub appearance: gtk::DropDown,
    pub icon_style: gtk::DropDown,
    pub default_view: gtk::DropDown,
    pub remember_folder_view: gtk::Switch,
    pub vim_navigation: gtk::Switch,
    pub grid_size: gtk::DropDown,
    pub file_density: gtk::DropDown,
    pub sidebar_density: gtk::DropDown,
    pub search_index: gtk::Switch,
    pub action_buttons: Vec<(&'static str, gtk::Button)>,
    #[cfg(test)]
    pub no_results: gtk::Label,
}

pub fn build(preferences: &ViewPreferences) -> SettingsCenterWidgets {
    let dialog = adw::Dialog::builder()
        .title("Settings")
        .content_width(760)
        .content_height(680)
        .build();
    dialog.update_property(&[
        gtk::accessible::Property::Label("Floe Settings"),
        gtk::accessible::Property::Description(
            "Search and customize Floe's appearance, browsing, applications, shortcuts, and accessibility behavior",
        ),
    ]);

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(
        &gtk::Label::builder()
            .label("Settings")
            .css_classes(["title-3"])
            .build(),
    ));
    toolbar.add_top_bar(&header);

    let search = gtk::SearchEntry::builder()
        .placeholder_text("Search settings")
        .hexpand(true)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(18)
        .margin_end(18)
        .build();
    search.set_tooltip_text(Some("Search setting names and plain-language descriptions"));
    search.update_property(&[
        gtk::accessible::Property::Label("Search settings"),
        gtk::accessible::Property::Description(
            "Filter settings by name, category, description, or common terms",
        ),
    ]);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
    content.set_margin_top(6);
    content.set_margin_bottom(24);
    content.set_margin_start(18);
    content.set_margin_end(18);

    let appearance = dropdown(
        &AppearancePreset::ALL.map(AppearancePreset::label),
        index_of(&AppearancePreset::ALL, preferences.appearance),
        "Appearance preset",
        definition(SettingId::AppearancePreset).description,
    );
    let icon_style = dropdown(
        &EntryIconStyle::ALL.map(EntryIconStyle::label),
        index_of(&EntryIconStyle::ALL, preferences.icon_style),
        "File and folder icon style",
        definition(SettingId::IconStyle).description,
    );
    let default_view = dropdown(
        &["List", "Grid", "Miller columns"],
        match preferences.mode {
            ViewMode::List => 0,
            ViewMode::Grid => 1,
            ViewMode::Miller => 2,
        },
        "Default folder view",
        definition(SettingId::DefaultView).description,
    );
    let remember_folder_view = settings_switch(
        preferences.remember_per_folder,
        "Remember each folder's view",
        definition(SettingId::RememberFolderView).description,
    );
    let vim_navigation = settings_switch(
        preferences.vim_mode,
        "Vim navigation",
        definition(SettingId::VimNavigation).description,
    );
    let grid_labels = GRID_SIZES.map(|edge| format!("{edge} pixels"));
    let grid_label_refs = grid_labels.iter().map(String::as_str).collect::<Vec<_>>();
    let grid_size = dropdown(
        &grid_label_refs,
        preferences.grid_size.index(),
        "Grid icon size",
        definition(SettingId::GridSize).description,
    );
    let file_density = dropdown(
        &["Compact", "Comfortable", "Spacious"],
        index_of(&FileViewDensity::ALL, preferences.file_density),
        "File spacing",
        definition(SettingId::FileDensity).description,
    );
    let sidebar_density = dropdown(
        &["Compact", "Balanced", "Comfortable"],
        match preferences.sidebar_density {
            SidebarDensity::Compact => 0,
            SidebarDensity::Balanced => 1,
            SidebarDensity::Comfortable => 2,
        },
        "Sidebar spacing",
        definition(SettingId::SidebarDensity).description,
    );
    let search_index = settings_switch(
        preferences.search_index_enabled,
        "Private filename index",
        definition(SettingId::SearchIndex).description,
    );

    let mut action_buttons = Vec::new();
    let mut filter_groups = Vec::new();
    for section in SettingsSection::ALL {
        let group = adw::PreferencesGroup::builder()
            .title(gtk::glib::markup_escape_text(section.title()))
            .description(section.description())
            .build();
        let mut rows = Vec::new();
        for item in SETTINGS.iter().filter(|item| item.section == section) {
            let row = match item.id {
                SettingId::AppearancePreset => control_row(item, &appearance),
                SettingId::IconStyle => control_row(item, &icon_style),
                SettingId::DefaultView => control_row(item, &default_view),
                SettingId::RememberFolderView => control_row(item, &remember_folder_view),
                SettingId::VimNavigation => control_row(item, &vim_navigation),
                SettingId::GridSize => control_row(item, &grid_size),
                SettingId::FileDensity => control_row(item, &file_density),
                SettingId::SidebarDensity => control_row(item, &sidebar_density),
                SettingId::SearchIndex => control_row(item, &search_index),
                SettingId::SystemAccessibility | SettingId::ReducedMotion => info_row(item),
                _ => {
                    let button = gtk::Button::builder()
                        .label(if item.id == SettingId::ClearPreviewCache {
                            "Clear"
                        } else if item.id == SettingId::CommandPalette {
                            "Show"
                        } else {
                            "Open"
                        })
                        .valign(gtk::Align::Center)
                        .build();
                    button.update_property(&[
                        gtk::accessible::Property::Label(item.title),
                        gtk::accessible::Property::Description(item.description),
                    ]);
                    if let Some(action) = item.action {
                        action_buttons.push((action, button.clone()));
                    }
                    control_row(item, &button)
                }
            };
            group.add(&row);
            rows.push((row.upcast::<gtk::Widget>(), item.id));
        }
        content.append(&group);
        filter_groups.push(FilterGroup { group, rows });
    }

    let no_results = gtk::Label::builder()
        .label("No settings match this search")
        .wrap(true)
        .visible(false)
        .css_classes(["dim-label", "title-4"])
        .margin_top(48)
        .margin_bottom(48)
        .build();
    no_results.set_accessible_role(gtk::AccessibleRole::Status);
    content.append(&no_results);

    let groups_for_search = filter_groups.clone();
    let no_results_for_search = no_results.clone();
    search.connect_search_changed(move |entry| {
        let terms = normalized_terms(entry.text().as_str());
        let mut visible_count = 0usize;
        for filter_group in &groups_for_search {
            let mut group_visible = false;
            for (row, id) in &filter_group.rows {
                let visible = setting_matches(definition(*id), &terms);
                row.set_visible(visible);
                group_visible |= visible;
                visible_count += usize::from(visible);
            }
            filter_group.group.set_visible(group_visible);
        }
        no_results_for_search.set_visible(visible_count == 0);
    });

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&content)
        .build();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.append(&search);
    body.append(&scroll);
    toolbar.set_content(Some(&body));
    dialog.set_child(Some(&toolbar));

    SettingsCenterWidgets {
        dialog,
        search,
        appearance,
        icon_style,
        default_view,
        remember_folder_view,
        vim_navigation,
        grid_size,
        file_density,
        sidebar_density,
        search_index,
        action_buttons,
        #[cfg(test)]
        no_results,
    }
}

fn dropdown(
    labels: &[&str],
    selected: usize,
    accessible_label: &str,
    accessible_description: &str,
) -> gtk::DropDown {
    let model = gtk::StringList::new(labels);
    let dropdown = gtk::DropDown::builder()
        .model(&model)
        .selected(selected as u32)
        .valign(gtk::Align::Center)
        .build();
    dropdown.update_property(&[
        gtk::accessible::Property::Label(accessible_label),
        gtk::accessible::Property::Description(accessible_description),
    ]);
    dropdown
}

fn settings_switch(active: bool, label: &str, description: &str) -> gtk::Switch {
    let toggle = gtk::Switch::builder()
        .active(active)
        .valign(gtk::Align::Center)
        .build();
    toggle.update_property(&[
        gtk::accessible::Property::Label(label),
        gtk::accessible::Property::Description(description),
    ]);
    toggle
}

fn control_row(definition: &SettingDefinition, control: &impl IsA<gtk::Widget>) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(definition.title)
        .subtitle(definition.description)
        .build();
    row.add_suffix(control);
    row.set_activatable_widget(Some(control));
    row
}

fn info_row(definition: &SettingDefinition) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(definition.title)
        .subtitle(definition.description)
        .activatable(false)
        .build()
}

fn index_of<T: Copy + PartialEq>(values: &[T], selected: T) -> usize {
    values
        .iter()
        .position(|value| *value == selected)
        .unwrap_or(0)
}

pub fn grid_size_at(index: usize) -> Option<GridSize> {
    GridSize::from_index(index)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn phase_20a_settings_center_model_has_complete_unique_sections() {
        assert_eq!(SettingsSection::ALL.len(), 8);
        let ids = SETTINGS.iter().map(|item| item.id).collect::<HashSet<_>>();
        assert_eq!(ids.len(), SETTINGS.len());
        for section in SettingsSection::ALL {
            assert!(SETTINGS.iter().any(|item| item.section == section));
            assert!(!section.title().trim().is_empty());
            assert!(!section.description().trim().is_empty());
        }
    }

    #[test]
    fn phase_20a_settings_center_search_uses_plain_language_and_all_terms() {
        assert!(matching_settings("FROSTED").contains(&SettingId::AppearancePreset));
        assert!(matching_settings("right click").contains(&SettingId::ContextMenus));
        assert!(matching_settings("reduce animation").contains(&SettingId::ReducedMotion));
        assert_eq!(matching_settings("unrelated impossible phrase"), Vec::new());
        assert_eq!(
            matching_settings(""),
            SETTINGS.iter().map(|item| item.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn phase_20a_settings_center_actions_link_existing_specialized_surfaces() {
        let actions = SETTINGS
            .iter()
            .filter_map(|item| item.action)
            .collect::<HashSet<_>>();
        for action in &actions {
            assert!(
                crate::command_registry::command(action).is_some(),
                "Settings links an unregistered command: {action}"
            );
        }
        for action in [
            "win.keyboard-shortcuts",
            "win.context-menu-settings",
            "win.terminal-preferences",
            "win.command-palette",
            "win.operation-history",
            "win.recovery-center",
            "win.protected-folders",
        ] {
            assert!(actions.contains(action), "missing Settings link: {action}");
        }
    }

    #[test]
    fn phase_20a_settings_center_preferences_reflect_authoritative_defaults() {
        let preferences = ViewPreferences::default();
        assert_eq!(index_of(&AppearancePreset::ALL, preferences.appearance), 2);
        assert_eq!(index_of(&EntryIconStyle::ALL, preferences.icon_style), 0);
        assert_eq!(index_of(&FileViewDensity::ALL, preferences.file_density), 1);
        assert_eq!(
            grid_size_at(preferences.grid_size.index()),
            Some(preferences.grid_size)
        );
        assert!(!preferences.remember_per_folder);
        assert!(!preferences.search_index_enabled);
    }

    #[test]
    #[ignore = "requires a real disposable GTK display"]
    fn phase_testing_gtk_phase_20a_settings_center_accessibility_contract() {
        gtk::init().expect("GTK display");
        let widgets = build(&ViewPreferences::default());
        assert_eq!(
            widgets.dialog.accessible_role(),
            gtk::AccessibleRole::Dialog
        );
        assert_eq!(
            widgets.search.accessible_role(),
            gtk::AccessibleRole::SearchBox
        );
        assert_eq!(
            widgets.appearance.accessible_role(),
            gtk::AccessibleRole::ComboBox
        );
        assert_eq!(
            widgets.remember_folder_view.accessible_role(),
            gtk::AccessibleRole::Switch
        );
        assert_eq!(widgets.action_buttons.len(), 9);
        assert_eq!(
            widgets.no_results.accessible_role(),
            gtk::AccessibleRole::Status
        );
    }
}
