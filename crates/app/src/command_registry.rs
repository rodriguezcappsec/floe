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
        "settings",
        "Settings",
        "Search and customize Floe settings",
        Operations,
        [
            "preferences",
            "appearance",
            "layout",
            "applications",
            "shortcuts",
            "accessibility"
        ],
        ["<Control>comma"],
        [H, W]
    ),
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
        "desktop-integration-status",
        "Desktop Integration…",
        "Show availability and limitations of standard Linux desktop services",
        Operations,
        [
            "wayland",
            "portal",
            "notification",
            "mount",
            "xdg",
            "service"
        ],
        [],
        [H]
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
        "custom-actions",
        "File Associations & Custom Actions…",
        "Manage shell-free external tools and learn where to change XDG MIME defaults",
        Operations,
        [
            "applications",
            "mime",
            "default",
            "external",
            "tools",
            "right click"
        ],
        [],
        [H]
    ),
    command!(
        "custom-action-chooser",
        "Run Custom Action…",
        "Choose an eligible configured external tool for the current selection",
        Files,
        ["external", "tool", "application", "selection"],
        [],
        [F, H]
    ),
    command!(
        "open-as-administrator",
        "Open as Administrator…",
        "Open selected or current local folder in an explicit bounded GVfs administrator view",
        Operations,
        [
            "root",
            "permissions",
            "polkit",
            "gvfs",
            "file operations",
            "privileged"
        ],
        [],
        [F, B, H]
    ),
    command!(
        "return-standard-access",
        "Return to Standard Access",
        "Cancel and close the active administrator view",
        Operations,
        ["administrator", "privileged", "close", "cancel"],
        [],
        [H]
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
        "protect-folder",
        "Protect Folder",
        "Add the selected directory or current folder to Floe's accidental-change guardrail",
        Files,
        [
            "protected folder",
            "guardrail",
            "prevent mistakes",
            "safety"
        ],
        [],
        [F, B, H]
    ),
    command!(
        "unprotect-folder",
        "Unprotect Folder",
        "Remove the selected directory or current folder from Floe's accidental-change guardrail",
        Files,
        [
            "protected folder",
            "guardrail",
            "remove protection",
            "safety"
        ],
        [],
        [F, B, H]
    ),
    command!(
        "protected-folders",
        "Protected Folders…",
        "Review Protected Folder status, current target state, and guardrail limitations",
        Operations,
        [
            "protected folder",
            "guardrail",
            "status",
            "settings",
            "limitations"
        ],
        [],
        [F, B, H]
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
        "audit-permissions",
        "Audit Permissions…",
        "Inspect local Unix modes, ownership, ACL, xattr, capability, immutable, and mount evidence without claiming complete effective access",
        Files,
        [
            "permissions",
            "owner",
            "acl",
            "xattr",
            "capabilities",
            "immutable",
            "security"
        ],
        [],
        [F, H]
    ),
    command!(
        "inspect-privacy-safety",
        "Inspect Privacy & Safety…",
        "Show explainable local filename, type, permission, and supported image metadata evidence without declaring a file safe",
        Files,
        ["privacy", "metadata", "suspicious", "exif", "local"],
        [],
        [F, H]
    ),
    command!(
        "scan-threats",
        "Scan with Local ClamAV…",
        "Stream selected local files to a separately installed clamd service; a no-signature result is not proof of safety",
        Files,
        ["malware", "virus", "clamav", "clamd", "local"],
        [],
        [F, H]
    ),
    command!(
        "cancel-threat-scan",
        "Cancel Local ClamAV Scan",
        "Stop the active bounded local ClamAV scan",
        Operations,
        ["malware", "virus", "clamav", "stop"],
        [],
        [H]
    ),
    command!(
        "create-sanitized-copy",
        "Create Sanitized Copy…",
        "Preserve the source and create a verified no-overwrite JPEG, PNG, or WebP copy without reviewed metadata blocks",
        Files,
        ["privacy", "remove metadata", "exif", "xmp", "copy"],
        [],
        [F, H]
    ),
    command!(
        "cancel-sanitization",
        "Cancel Metadata Sanitization",
        "Stop a batch after its current item while keeping already verified copies and every source unchanged",
        Operations,
        ["privacy", "metadata", "stop", "batch"],
        [],
        [H]
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
        "copy-and-verify",
        "Copy and Verify…",
        "Copy one selected item, sync it, and compare source and destination with SHA-256",
        Files,
        ["integrity", "verified copy", "sha256", "byte comparison"],
        [],
        [F, H]
    ),
    command!(
        "verified-removable-transfer",
        "Verified Removable Transfer…",
        "Copy one selected item to removable storage, verify bytes, flush, then eject or unmount",
        Files,
        [
            "usb",
            "removable",
            "verified copy",
            "flush",
            "eject",
            "sha256"
        ],
        [],
        [F, H]
    ),
    command!(
        "save-sha256-fingerprint",
        "Save SHA-256 Fingerprint",
        "Save a private SHA-256 fingerprint for one selected regular file",
        Files,
        ["integrity", "hash", "baseline", "byte changes"],
        [],
        [F, H]
    ),
    command!(
        "verify-saved-fingerprint",
        "Verify Saved Fingerprint",
        "Compare one selected regular file with its private saved SHA-256 fingerprint",
        Files,
        ["integrity", "hash", "baseline", "byte changes"],
        [],
        [F, H]
    ),
    command!(
        "generate-sha256sums",
        "Generate SHA256SUMS",
        "Generate a portable SHA256SUMS manifest without overwriting an existing one",
        Files,
        ["integrity", "manifest", "sha256", "checksums"],
        [],
        [F, H]
    ),
    command!(
        "verify-sha256sums",
        "Verify Selected Manifest",
        "Verify the selected SHA256SUMS manifest and report byte changes",
        Files,
        ["integrity", "manifest", "sha256", "changed", "missing"],
        [],
        [F, H]
    ),
    command!(
        "create-integrity-baseline",
        "Create Integrity Baseline",
        "Create a private SHA-256 baseline for the current local folder",
        Files,
        ["integrity", "baseline", "monitor", "sha256"],
        [],
        [F, B, H]
    ),
    command!(
        "update-integrity-baseline",
        "Update Integrity Baseline",
        "Replace the private baseline after reviewing changes",
        Files,
        ["integrity", "baseline", "monitor", "sha256"],
        [],
        [F, B, H]
    ),
    command!(
        "verify-integrity-baseline",
        "Verify Integrity Baseline",
        "Recheck the current folder and report matching changed missing and new files",
        Files,
        [
            "integrity",
            "baseline",
            "verify",
            "changed",
            "missing",
            "new"
        ],
        [],
        [F, B, H]
    ),
    command!(
        "delete-integrity-baseline",
        "Delete Integrity Baseline",
        "Remove Floe private baseline data for the current folder",
        Files,
        ["integrity", "baseline", "delete", "private"],
        [],
        [F, B, H]
    ),
    command!(
        "start-integrity-monitoring",
        "Start Integrity Monitoring",
        "Notice changes while Floe watches; not intrusion detection",
        Files,
        ["integrity", "monitor", "watch", "not intrusion detection"],
        [],
        [F, B, H]
    ),
    command!(
        "stop-integrity-monitoring",
        "Stop Integrity Monitoring",
        "Stop the explicit local integrity watch",
        Files,
        ["integrity", "monitor", "watch", "stop"],
        [],
        [F, B, H]
    ),
    command!(
        "check-duplicates",
        "Check for Duplicates…",
        "Scan a folder tree, find copies of one file, or compare selected items",
        Files,
        ["duplicates", "same files", "hash", "reclaim space"],
        [],
        [F, B, H]
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
        "recent-locations",
        "Recent Locations",
        "Review and reopen bounded locations from this tab's navigation history",
        Navigation,
        ["history", "folders", "back", "forward", "path"],
        ["<Alt>Down"],
        [W]
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
        "invert-selection",
        "Invert Selection",
        "Select every visible unselected item and clear every visible selected item",
        Selection,
        ["inverse", "toggle files"],
        ["<Control><Shift>i"],
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
        "preview-clear-cache",
        "Clear Preview Memory",
        "Discard Floe's memory-only Quick Preview cache",
        Preview,
        ["cache", "reset", "thumbnail", "privacy"],
        [],
        [S]
    ),
    command!(
        "clear-metadata-sort-cache",
        "Clear Advanced Sort Metadata",
        "Delete Floe's private derived metadata-sort cache",
        Preview,
        ["sort", "metadata", "cache", "privacy", "reset"],
        [],
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
    command!(
        "recovery-center",
        "Operation Recovery",
        "Review interrupted file operations without deleting uncertain data",
        Operations,
        ["crash", "journal", "partial output", "retry"],
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

    #[test]
    fn phase_18t_ui_registers_accessible_integrity_commands() {
        for action in [
            "win.save-sha256-fingerprint",
            "win.verify-saved-fingerprint",
            "win.generate-sha256sums",
            "win.verify-sha256sums",
        ] {
            let definition = command(action).unwrap_or_else(|| panic!("missing {action}"));
            assert!(definition.searchable);
            assert!(
                definition
                    .placements
                    .contains(&CommandPlacement::FileContext)
            );
            assert!(
                definition
                    .placements
                    .contains(&CommandPlacement::HeaderMenu)
            );
            assert!(!definition.name.trim().is_empty());
        }
    }

    #[test]
    fn phase_18n_ui_registers_truthful_local_analysis_commands() {
        for action in [
            "win.inspect-privacy-safety",
            "win.scan-threats",
            "win.cancel-threat-scan",
        ] {
            let definition = command(action).unwrap_or_else(|| panic!("missing {action}"));
            assert!(definition.searchable);
            assert!(!definition.name.trim().is_empty());
            assert!(!definition.description.trim().is_empty());
        }
        let scan = command("win.scan-threats").expect("local ClamAV command");
        assert!(scan.description.contains("separately installed clamd"));
        assert!(scan.description.contains("not proof of safety"));
        assert!(scan.placements.contains(&CommandPlacement::FileContext));
        assert!(scan.placements.contains(&CommandPlacement::HeaderMenu));
    }

    #[test]
    fn phase_18o_ui_explains_inspection_scope_without_a_safety_claim() {
        let inspect = command("win.inspect-privacy-safety").expect("inspection command");
        assert!(inspect.description.contains("evidence"));
        assert!(
            inspect
                .description
                .contains("without declaring a file safe")
        );
        assert!(inspect.placements.contains(&CommandPlacement::FileContext));
        assert!(inspect.placements.contains(&CommandPlacement::HeaderMenu));
    }

    #[test]
    fn phase_18p_ui_registers_source_preserving_sanitization_commands() {
        let create = command("win.create-sanitized-copy").expect("sanitized-copy command");
        let cancel = command("win.cancel-sanitization").expect("cancel sanitization command");
        assert!(create.description.contains("Preserve the source"));
        assert!(create.description.contains("no-overwrite"));
        assert!(create.description.contains("JPEG, PNG, or WebP"));
        assert!(create.placements.contains(&CommandPlacement::FileContext));
        assert!(create.placements.contains(&CommandPlacement::HeaderMenu));
        assert!(cancel.description.contains("already verified copies"));
    }

    #[test]
    fn phase_18v_ui_registers_distinct_copy_and_verify_command() {
        let definition = command("win.copy-and-verify").expect("Copy and Verify command");
        assert_eq!(definition.name, "Copy and Verify…");
        assert!(definition.description.contains("SHA-256"));
        assert!(
            definition
                .placements
                .contains(&CommandPlacement::FileContext)
        );
        assert!(
            definition
                .placements
                .contains(&CommandPlacement::HeaderMenu)
        );
        assert_ne!(definition.action, "win.copy");
    }

    #[test]
    fn phase_18w_ui_registers_distinct_verified_removable_command() {
        let definition = command("win.verified-removable-transfer")
            .expect("Verified Removable Transfer command");
        assert_eq!(definition.name, "Verified Removable Transfer…");
        assert!(definition.description.contains("flush"));
        assert!(definition.description.contains("eject or unmount"));
        assert!(
            definition
                .placements
                .contains(&CommandPlacement::FileContext)
        );
        assert!(
            definition
                .placements
                .contains(&CommandPlacement::HeaderMenu)
        );
        assert_ne!(definition.action, "win.copy-and-verify");
    }

    #[test]
    fn phase_18u_ui_registers_explicit_baseline_and_monitoring_actions() {
        for action in [
            "win.create-integrity-baseline",
            "win.update-integrity-baseline",
            "win.verify-integrity-baseline",
            "win.delete-integrity-baseline",
            "win.start-integrity-monitoring",
            "win.stop-integrity-monitoring",
        ] {
            let definition = command(action).unwrap_or_else(|| panic!("missing {action}"));
            assert!(definition.searchable);
            assert!(
                definition
                    .placements
                    .contains(&CommandPlacement::FileContext)
            );
            assert!(
                definition
                    .placements
                    .contains(&CommandPlacement::BackgroundContext)
            );
            assert!(
                definition
                    .placements
                    .contains(&CommandPlacement::HeaderMenu)
            );
        }
        let start = command("win.start-integrity-monitoring").expect("start monitoring command");
        assert!(start.description.contains("not intrusion detection"));
    }

    #[test]
    fn phase_18x_registry_exposes_protected_folder_actions_with_menu_palette_parity() {
        for action in [
            "win.protect-folder",
            "win.unprotect-folder",
            "win.protected-folders",
        ] {
            let definition = command(action).unwrap_or_else(|| panic!("missing {action}"));
            assert!(definition.searchable);
            for placement in [
                CommandPlacement::FileContext,
                CommandPlacement::BackgroundContext,
                CommandPlacement::HeaderMenu,
            ] {
                assert!(definition.placements.contains(&placement));
            }
            assert!(definition.description.contains("guardrail"));
        }
        assert!(
            command("win.protect-folder")
                .expect("Protect Folder")
                .description
                .contains("accidental-change")
        );
    }

    #[test]
    fn phase_18r_permission_ui_registers_truthful_audit_command() {
        let definition = command("win.audit-permissions").expect("permission audit command");
        assert!(definition.searchable);
        assert!(
            definition
                .placements
                .contains(&CommandPlacement::FileContext)
        );
        assert!(
            definition
                .placements
                .contains(&CommandPlacement::HeaderMenu)
        );
        for evidence in [
            "Unix modes",
            "ACL",
            "xattr",
            "capability",
            "immutable",
            "mount",
        ] {
            assert!(definition.description.contains(evidence));
        }
        assert!(definition.description.contains("without claiming complete"));
    }
}
