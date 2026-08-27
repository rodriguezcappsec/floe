//! Central metadata for human-invokable application commands.
//!
//! The registry deliberately owns no callbacks. Existing GActions remain the
//! sole execution and eligibility authority; later command surfaces resolve
//! these records against the live action group.

use std::collections::HashSet;

use gtk::{gio, prelude::*};

pub const COMMAND_SEARCH_TERM_CAPACITY: usize = 8;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CommandCategory {
    Navigation,
    Selection,
    Files,
    Create,
    Clipboard,
    View,
    Tabs,
    SplitView,
    Preview,
    Trash,
    Operations,
}

impl CommandCategory {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Navigation => "Navigation",
            Self::Selection => "Selection",
            Self::Files => "Files",
            Self::Create => "Create",
            Self::Clipboard => "Clipboard",
            Self::View => "View",
            Self::Tabs => "Tabs",
            Self::SplitView => "Split View",
            Self::Preview => "Preview",
            Self::Trash => "Trash",
            Self::Operations => "Operations",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CommandPlacement {
    HeaderMenu,
    FileContext,
    TrashContext,
    BackgroundContext,
    Toolbar,
    ShortcutOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandRisk {
    Normal,
    Recoverable,
    ConfirmationRequired,
    Irreversible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandDefinition {
    pub action: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub category: CommandCategory,
    pub search_terms: &'static [&'static str],
    pub default_shortcuts: &'static [&'static str],
    pub placements: &'static [CommandPlacement],
    pub risk: CommandRisk,
    pub searchable: bool,
}

impl CommandDefinition {
    pub fn action_name(self) -> &'static str {
        self.action.strip_prefix("win.").unwrap_or(self.action)
    }
}

const H: CommandPlacement = CommandPlacement::HeaderMenu;
const F: CommandPlacement = CommandPlacement::FileContext;
const T: CommandPlacement = CommandPlacement::TrashContext;
const B: CommandPlacement = CommandPlacement::BackgroundContext;
const W: CommandPlacement = CommandPlacement::Toolbar;
const S: CommandPlacement = CommandPlacement::ShortcutOnly;

macro_rules! command {
    ($action:literal, $name:literal, $description:literal, $category:ident,
     [$($term:literal),* $(,)?], [$($shortcut:literal),* $(,)?], [$($placement:expr),* $(,)?]) => {
        CommandDefinition {
            action: concat!("win.", $action),
            name: $name,
            description: $description,
            category: CommandCategory::$category,
            search_terms: &[$($term),*],
            default_shortcuts: &[$($shortcut),*],
            placements: &[$($placement),*],
            risk: CommandRisk::Normal,
            searchable: true,
        }
    };
    ($action:literal, $name:literal, $description:literal, $category:ident,
     [$($term:literal),* $(,)?], [$($shortcut:literal),* $(,)?], [$($placement:expr),* $(,)?], $risk:ident) => {
        CommandDefinition {
            risk: CommandRisk::$risk,
            ..command!($action, $name, $description, $category, [$($term),*], [$($shortcut),*], [$($placement),*])
        }
    };
}

pub static COMMANDS: &[CommandDefinition] = &[
    CommandDefinition {
        action: "win.command-palette",
        name: "Command Palette",
        description: "Search and run Floe commands",
        category: CommandCategory::Operations,
        search_terms: &["commands", "actions"],
        default_shortcuts: &["<Control><Shift>p"],
        placements: &[CommandPlacement::ShortcutOnly],
        risk: CommandRisk::Normal,
        searchable: false,
    },
    command!(
        "keyboard-shortcuts",
        "Keyboard Shortcuts",
        "View and customize every Floe keyboard shortcut",
        Operations,
        ["keys", "bindings", "hotkeys", "customize", "help"],
        ["<Control>question"],
        [H, W]
    ),
    command!(
        "context-menu-settings",
        "Customize Context Menus…",
        "Choose optional command groups shown in file and folder context menus",
        Operations,
        ["right click", "popup", "menu", "actions", "configure"],
        [],
        [F, B, H]
    ),
    command!(
        "open",
        "Open",
        "Open the selected item",
        Files,
        ["launch", "folder"],
        [],
        [F]
    ),
    command!(
        "open-with",
        "Open With…",
        "Choose an application for the selected file",
        Files,
        ["application", "association"],
        [],
        [F, H]
    ),
    command!(
        "open-terminal",
        "Open Terminal Here",
        "Open the preferred terminal in the selected or current folder",
        Files,
        ["shell", "console", "folder", "working directory"],
        [],
        [F, B, H]
    ),
    command!(
        "terminal-preferences",
        "Preferred Terminal…",
        "Choose the reviewed terminal application Floe should launch",
        Operations,
        ["shell", "console", "settings", "application"],
        [],
        [H]
    ),
    command!(
        "properties",
        "Properties",
        "Show properties for the selection",
        Files,
        ["details", "metadata", "permissions"],
        ["<Alt>Return"],
        [F, T, H]
    ),
    command!(
        "checksum",
        "Calculate Checksums…",
        "Calculate or compare file checksums",
        Files,
        ["sha256", "sha512", "md5", "hash"],
        [],
        [F, T, H]
    ),
    command!(
        "check-duplicates",
        "Check for Duplicates…",
        "Find byte-for-byte duplicate files in the explicit selection",
        Files,
        ["duplicates", "same files", "hash", "reclaim space"],
        [],
        [F, H]
    ),
    command!(
        "extract-here",
        "Extract Here",
        "Extract one supported archive beside its source",
        Operations,
        ["archive", "unpack", "zip", "tar", "7z"],
        [],
        [F, H]
    ),
    command!(
        "extract-to",
        "Extract To…",
        "Choose a local destination for one supported archive",
        Operations,
        ["archive", "unpack", "destination", "zip", "tar", "7z"],
        [],
        [F, H]
    ),
    command!(
        "compress",
        "Compress…",
        "Create a supported archive from selected files and folders",
        Operations,
        ["archive", "pack", "zip", "tar", "7z"],
        [],
        [F, H]
    ),
    command!(
        "rename",
        "Rename…",
        "Rename the selected item",
        Files,
        ["filename", "edit name"],
        ["F2"],
        [F, H]
    ),
    command!(
        "batch-rename",
        "Batch Rename…",
        "Preview and rename multiple selected items as one bounded operation",
        Files,
        ["multiple", "bulk", "regex", "sequence"],
        [],
        [F, H]
    ),
    command!(
        "undo-batch-rename",
        "Undo Last Batch Rename",
        "Undo the latest completed in-session batch rename when still safe",
        Files,
        ["revert", "bulk", "multiple"],
        [],
        [F, H]
    ),
    command!(
        "duplicate",
        "Duplicate",
        "Create no-overwrite copies of selected items",
        Files,
        ["copy here"],
        ["<Control>d"],
        [F, H]
    ),
    command!(
        "create-symbolic-link",
        "Create Symbolic Link…",
        "Create a symbolic link to the selection",
        Files,
        ["symlink", "shortcut"],
        [],
        [F, H]
    ),
    command!(
        "create-hard-link",
        "Create Hard Link…",
        "Create a hard link to one regular file",
        Files,
        ["inode", "link"],
        [],
        [F, H]
    ),
    command!(
        "reveal-link-target",
        "Reveal Link Target",
        "Navigate to a symbolic link target",
        Navigation,
        ["symlink", "show target"],
        [],
        [F, H]
    ),
    command!(
        "copy",
        "Copy",
        "Stage the selection for copying",
        Clipboard,
        ["clipboard", "transfer"],
        ["<Control>c"],
        [F, H]
    ),
    command!(
        "cut",
        "Cut",
        "Stage the selection for moving",
        Clipboard,
        ["clipboard", "move"],
        ["<Control>x"],
        [F, H]
    ),
    command!(
        "paste",
        "Paste",
        "Paste staged files into the current folder",
        Clipboard,
        ["clipboard", "transfer"],
        ["<Control>v"],
        [B, H]
    ),
    command!(
        "copy-name",
        "Copy Name",
        "Copy selected filenames as text",
        Clipboard,
        ["filename", "text"],
        [],
        [F]
    ),
    command!(
        "copy-path",
        "Copy Path",
        "Copy absolute selected paths",
        Clipboard,
        ["absolute", "location"],
        ["<Control><Shift>c"],
        [F, H]
    ),
    command!(
        "copy-relative-path",
        "Copy Relative Path",
        "Copy paths relative to the current folder",
        Clipboard,
        ["relative", "location"],
        [],
        [F]
    ),
    command!(
        "copy-uri",
        "Copy URI",
        "Copy local file URIs",
        Clipboard,
        ["file uri", "url"],
        [],
        [F]
    ),
    command!(
        "new-folder",
        "New Folder…",
        "Create a folder in the current location",
        Create,
        ["directory", "mkdir"],
        ["<Control><Shift>n"],
        [B, H]
    ),
    command!(
        "new-empty-file",
        "New Empty File…",
        "Create an empty file in the current location",
        Create,
        ["document", "touch"],
        [],
        [B, H]
    ),
    command!(
        "new-from-template",
        "New From Template…",
        "Copy a chosen template into the current location",
        Create,
        ["document", "template"],
        [],
        [B, H]
    ),
    command!(
        "trash",
        "Move to Trash",
        "Move selected items to Trash",
        Trash,
        ["delete", "remove", "Delete key"],
        [],
        [F, H],
        Recoverable
    ),
    command!(
        "restore",
        "Restore from Trash",
        "Restore selected Trash items without overwriting",
        Trash,
        ["recover", "undelete"],
        [],
        [T, H],
        Recoverable
    ),
    command!(
        "permanent-delete",
        "Delete Permanently…",
        "Permanently delete selected items after confirmation",
        Trash,
        ["remove", "irreversible"],
        ["<Shift>Delete"],
        [F, T, H],
        ConfirmationRequired
    ),
    command!(
        "empty-trash",
        "Empty Trash…",
        "Permanently delete all Trash items after confirmation",
        Trash,
        ["clear", "irreversible"],
        [],
        [H],
        Irreversible
    ),
    command!(
        "open-trash",
        "Open Trash",
        "Browse local Trash locations",
        Navigation,
        ["deleted", "wastebasket"],
        [],
        [H]
    ),
    command!(
        "back",
        "Back",
        "Navigate backward in history",
        Navigation,
        ["previous", "history"],
        ["<Alt>Left"],
        [W, H]
    ),
    command!(
        "forward",
        "Forward",
        "Navigate forward in history",
        Navigation,
        ["next", "history"],
        ["<Alt>Right"],
        [W, H]
    ),
    command!(
        "parent",
        "Parent Folder",
        "Navigate to the parent folder",
        Navigation,
        ["up", "directory"],
        ["<Alt>Up"],
        [W, H]
    ),
    command!(
        "location",
        "Edit Location",
        "Enter an absolute filesystem location",
        Navigation,
        ["path", "address"],
        ["<Control>l"],
        [B, W, H]
    ),
    command!(
        "folder-filter",
        "Search",
        "Open the unified search surface in Quick Filter mode",
        Navigation,
        [
            "find",
            "quick filter",
            "text",
            "glob",
            "regex",
            "current folder"
        ],
        ["<Control>f"],
        [W, H]
    ),
    command!(
        "filename-search",
        "Search Files…",
        "Open Search in file-search mode for this folder or its subfolders",
        Navigation,
        ["find", "filenames", "subfolders", "recursive", "name"],
        ["<Control><Shift>f"],
        [H]
    ),
    command!(
        "reveal-in-folder",
        "Reveal in Folder",
        "Open the containing folder and select the search result",
        Navigation,
        ["parent", "location", "show", "search result"],
        [],
        [F]
    ),
    command!(
        "cancel-location",
        "Cancel Location Editing",
        "Close location editing without navigating",
        Navigation,
        ["escape"],
        ["Escape"],
        [S]
    ),
    command!(
        "refresh",
        "Refresh",
        "Reload the current folder while preserving view state",
        Navigation,
        ["reload", "rescan"],
        ["F5", "<Control>r"],
        [B, H]
    ),
    command!(
        "select-all",
        "Select All",
        "Select every visible item",
        Selection,
        ["all files"],
        ["<Control>a"],
        [B, H]
    ),
    command!(
        "clear-selection",
        "Clear Selection",
        "Clear the current file selection",
        Selection,
        ["deselect"],
        ["<Control><Shift>a"],
        [S]
    ),
    command!(
        "hidden",
        "Show Hidden Files",
        "Toggle hidden file visibility",
        View,
        ["dotfiles", "visibility"],
        ["<Control>h"],
        [H]
    ),
    command!(
        "view-list",
        "List View",
        "Show files in a detailed list",
        View,
        ["details", "rows"],
        ["<Control>1"],
        [H, W]
    ),
    command!(
        "view-grid",
        "Grid View",
        "Show files in an icon grid",
        View,
        ["icons", "thumbnails"],
        ["<Control>2"],
        [H, W]
    ),
    command!(
        "view-miller",
        "Miller Columns",
        "Show spatial Miller columns",
        View,
        ["columns", "spatial"],
        [],
        [H, W]
    ),
    command!(
        "vim-mode",
        "Vim Navigation Mode",
        "Toggle opt-in h/j/k/l browser navigation",
        View,
        ["keyboard", "modal", "hjkl", "power user"],
        [],
        [H, W]
    ),
    command!(
        "zoom-out",
        "Decrease Grid Size",
        "Use smaller grid items",
        View,
        ["thumbnail", "icon size"],
        ["<Control>minus"],
        [H, W]
    ),
    command!(
        "zoom-in",
        "Increase Grid Size",
        "Use larger grid items",
        View,
        ["thumbnail", "icon size"],
        ["<Control>plus", "<Control>equal"],
        [H, W]
    ),
    command!(
        "new-tab",
        "New Tab",
        "Open a new browser tab",
        Tabs,
        ["tab"],
        ["<Control>t"],
        [H, W]
    ),
    command!(
        "close-tab-active",
        "Close Tab",
        "Close the active tab",
        Tabs,
        ["tab"],
        ["<Control>w"],
        [H]
    ),
    command!(
        "duplicate-tab-active",
        "Duplicate Tab",
        "Duplicate the active tab and navigation state",
        Tabs,
        ["clone tab"],
        [],
        [H]
    ),
    command!(
        "reopen-closed-tab",
        "Reopen Closed Tab",
        "Restore the most recently closed tab",
        Tabs,
        ["undo close"],
        ["<Control><Shift>t"],
        [H]
    ),
    command!(
        "next-tab",
        "Next Tab",
        "Activate the next tab",
        Tabs,
        ["switch tab"],
        ["<Control>Tab"],
        [S]
    ),
    command!(
        "previous-tab",
        "Previous Tab",
        "Activate the previous tab",
        Tabs,
        ["switch tab"],
        ["<Control><Shift>Tab"],
        [S]
    ),
    command!(
        "move-tab-left",
        "Move Tab Left",
        "Move the active tab one position left",
        Tabs,
        ["reorder"],
        ["<Control><Shift>Page_Up"],
        [S]
    ),
    command!(
        "move-tab-right",
        "Move Tab Right",
        "Move the active tab one position right",
        Tabs,
        ["reorder"],
        ["<Control><Shift>Page_Down"],
        [S]
    ),
    command!(
        "toggle-split",
        "Toggle Split View",
        "Open or close a second browser pane",
        SplitView,
        ["dual pane", "two panes"],
        ["F3"],
        [B, H]
    ),
    command!(
        "switch-split-side",
        "Switch Active Pane",
        "Move focus to the other split pane",
        SplitView,
        ["pane focus"],
        ["F6"],
        [B, H]
    ),
    command!(
        "swap-split-sides",
        "Swap Panes",
        "Swap the primary and secondary split sessions",
        SplitView,
        ["exchange panes"],
        [],
        [B, H]
    ),
    command!(
        "close-split",
        "Close Split",
        "Close the secondary pane",
        SplitView,
        ["single pane"],
        [],
        [B, H]
    ),
    command!(
        "narrow-primary-pane",
        "Narrow Primary Pane",
        "Decrease the primary split ratio",
        SplitView,
        ["resize pane"],
        ["<Control><Alt>Left"],
        [B, H]
    ),
    command!(
        "widen-primary-pane",
        "Widen Primary Pane",
        "Increase the primary split ratio",
        SplitView,
        ["resize pane"],
        ["<Control><Alt>Right"],
        [B, H]
    ),
    command!(
        "open-opposite-pane",
        "Open in Opposite Pane",
        "Open the selected folder in the other pane",
        SplitView,
        ["dual pane"],
        ["<Control><Shift>Return"],
        [F]
    ),
    command!(
        "copy-to-opposite-pane",
        "Copy to Opposite Pane",
        "Copy selected items to the other pane",
        SplitView,
        ["dual pane", "transfer"],
        ["<Control><Alt>c"],
        [F]
    ),
    command!(
        "move-to-opposite-pane",
        "Move to Opposite Pane",
        "Move selected items to the other pane",
        SplitView,
        ["dual pane", "transfer"],
        ["<Control><Alt>m"],
        [F]
    ),
    command!(
        "link-to-opposite-pane",
        "Link to Opposite Pane",
        "Create symbolic links in the other pane",
        SplitView,
        ["dual pane", "symlink"],
        ["<Control><Alt>l"],
        [F]
    ),
    command!(
        "quick-preview",
        "Quick Preview",
        "Toggle the selected file preview",
        Preview,
        ["space", "peek"],
        ["space"],
        [S]
    ),
    command!(
        "miller-inspector-hook",
        "Inspector",
        "Toggle read-only Inspector details in Miller view",
        Preview,
        ["metadata", "details"],
        ["<Control>i"],
        [H, S]
    ),
    command!(
        "operation-history",
        "Operation History",
        "Show retained file-operation results",
        Operations,
        ["jobs", "transfers", "errors"],
        [],
        [H]
    ),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandAvailability {
    Enabled,
    Disabled,
    Missing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedCommand {
    pub definition: &'static CommandDefinition,
    pub availability: CommandAvailability,
}

pub trait CommandActionSource {
    fn command_action(&self, name: &str) -> Option<gio::Action>;
}

impl CommandActionSource for adw::ApplicationWindow {
    fn command_action(&self, name: &str) -> Option<gio::Action> {
        self.lookup_action(name)
    }
}

impl CommandActionSource for gio::SimpleActionGroup {
    fn command_action(&self, name: &str) -> Option<gio::Action> {
        self.lookup_action(name)
    }
}

impl ResolvedCommand {
    pub const fn can_activate(self) -> bool {
        matches!(self.availability, CommandAvailability::Enabled)
    }
}

pub fn command(action: &str) -> Option<&'static CommandDefinition> {
    COMMANDS.iter().find(|command| command.action == action)
}

pub fn resolve<A: CommandActionSource>(
    group: &A,
    definition: &'static CommandDefinition,
) -> ResolvedCommand {
    let availability = group.command_action(definition.action_name()).map_or(
        CommandAvailability::Missing,
        |action| {
            if action.is_enabled() {
                CommandAvailability::Enabled
            } else {
                CommandAvailability::Disabled
            }
        },
    );
    ResolvedCommand {
        definition,
        availability,
    }
}

pub fn resolve_all<A: CommandActionSource>(group: &A) -> Vec<ResolvedCommand> {
    COMMANDS
        .iter()
        .filter(|command| command.searchable)
        .map(|command| resolve(group, command))
        .collect()
}

pub fn missing_registered_actions<A: CommandActionSource>(group: &A) -> Vec<&'static str> {
    COMMANDS
        .iter()
        .filter(|definition| group.command_action(definition.action_name()).is_none())
        .map(|definition| definition.action)
        .collect()
}

pub fn validate_contract() -> Result<(), &'static str> {
    let mut actions = HashSet::with_capacity(COMMANDS.len());
    let mut names = HashSet::with_capacity(COMMANDS.len());
    for command in COMMANDS {
        if !command.action.starts_with("win.") || command.action_name().is_empty() {
            return Err("command action must be a non-empty win.* action");
        }
        if command.name.trim().is_empty() || command.description.trim().is_empty() {
            return Err("command name and description must be non-empty");
        }
        if command.category.label().is_empty() {
            return Err("command category label must be non-empty");
        }
        if !actions.insert(command.action) {
            return Err("command actions must be unique");
        }
        if !names.insert(command.name) {
            return Err("command human names must be unique");
        }
        if command.search_terms.len() > COMMAND_SEARCH_TERM_CAPACITY {
            return Err("command search terms exceed the bound");
        }
        if command
            .search_terms
            .iter()
            .any(|term| term.trim().is_empty())
        {
            return Err("command search terms must be non-empty");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_13b_search_ui_shortcuts_open_one_surface_in_distinct_modes() {
        let filter = command("win.folder-filter").expect("folder filter command");
        assert_eq!(filter.name, "Search");
        assert_eq!(filter.default_shortcuts, ["<Control>f"]);
        assert!(filter.description.contains("Quick Filter mode"));
        assert!(filter.placements.contains(&CommandPlacement::Toolbar));
        assert!(filter.placements.contains(&CommandPlacement::HeaderMenu));

        let search = command("win.filename-search").expect("filename search command");
        assert_eq!(search.name, "Search Files…");
        assert_eq!(search.default_shortcuts, ["<Control><Shift>f"]);
        assert!(search.description.contains("file-search mode"));
        assert!(!search.placements.contains(&CommandPlacement::Toolbar));
        assert!(search.placements.contains(&CommandPlacement::HeaderMenu));

        let reveal = command("win.reveal-in-folder").expect("reveal search result command");
        assert!(reveal.placements.contains(&CommandPlacement::FileContext));
        assert!(reveal.default_shortcuts.is_empty());
    }

    #[test]
    fn phase_11a_registry_contract_is_unique_human_named_and_bounded() {
        validate_contract().expect("valid command registry");
        assert!(COMMANDS.len() >= 50);
        assert_eq!(CommandCategory::SplitView.label(), "Split View");
        assert!(
            COMMANDS
                .windows(2)
                .all(|pair| pair[0].action != pair[1].action)
        );
    }

    #[test]
    fn phase_11a_registry_eligibility_comes_only_from_live_actions() {
        let group = gio::SimpleActionGroup::new();
        let open = gio::SimpleAction::new("open", None);
        open.set_enabled(false);
        group.add_action(&open);
        let definition = command("win.open").expect("Open command");
        assert_eq!(
            resolve(&group, definition).availability,
            CommandAvailability::Disabled
        );
        open.set_enabled(true);
        assert!(resolve(&group, definition).can_activate());
        let missing = command("win.open-with").expect("Open With command");
        assert_eq!(
            resolve(&group, missing).availability,
            CommandAvailability::Missing
        );
    }

    #[test]
    fn phase_11a_registry_shortcuts_preserve_defaults_and_risk() {
        let shortcut = |action| {
            command(action)
                .expect("registered command")
                .default_shortcuts
        };
        assert_eq!(shortcut("win.back"), ["<Alt>Left"]);
        assert_eq!(shortcut("win.refresh"), ["F5", "<Control>r"]);
        assert_eq!(shortcut("win.quick-preview"), ["space"]);
        assert_eq!(shortcut("win.permanent-delete"), ["<Shift>Delete"]);
        assert_eq!(
            command("win.permanent-delete")
                .expect("permanent delete")
                .risk,
            CommandRisk::ConfirmationRequired
        );
        assert!(
            command("win.empty-trash")
                .expect("empty trash")
                .default_shortcuts
                .is_empty()
        );
    }

    #[test]
    fn phase_11a_registry_parity_covers_context_commands_and_classifies_plumbing() {
        for (actions, placement) in [
            (
                crate::ui::FILE_CONTEXT_ACTIONS.as_slice(),
                CommandPlacement::FileContext,
            ),
            (
                crate::ui::TRASH_CONTEXT_ACTIONS.as_slice(),
                CommandPlacement::TrashContext,
            ),
            (
                crate::ui::BACKGROUND_CONTEXT_ACTIONS.as_slice(),
                CommandPlacement::BackgroundContext,
            ),
        ] {
            for (_, action) in actions {
                let definition = command(action).unwrap_or_else(|| panic!("missing {action}"));
                assert!(
                    definition.placements.contains(&placement),
                    "{action} lacks {placement:?} placement"
                );
            }
        }
        for internal in [
            "win.activate-tab",
            "win.close-tab",
            "win.move-tab-before",
            "win.column-size",
            "win.sidebar-density",
            "win.file-density",
        ] {
            assert!(
                command(internal).is_none(),
                "internal action leaked: {internal}"
            );
        }
    }

    #[test]
    fn phase_12f_action_integration_registers_productivity_and_customization_commands() {
        for (action, placements) in [
            ("win.extract-here", &[F, H][..]),
            ("win.extract-to", &[F, H][..]),
            ("win.compress", &[F, H][..]),
            ("win.batch-rename", &[F, H][..]),
            ("win.undo-batch-rename", &[F, H][..]),
            ("win.context-menu-settings", &[F, B, H][..]),
        ] {
            let definition = command(action).unwrap_or_else(|| panic!("missing {action}"));
            assert!(definition.searchable);
            assert!(!definition.name.trim().is_empty());
            assert!(!definition.description.trim().is_empty());
            for placement in placements {
                assert!(definition.placements.contains(placement));
            }
        }
    }
}
