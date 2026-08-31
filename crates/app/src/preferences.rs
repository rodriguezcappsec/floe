use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread::{self, JoinHandle},
};

#[cfg(unix)]
use std::{
    ffi::OsString,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
};

use floe_core::{
    DirectoryGrouping, DirectoryPlacement, DirectorySort, SavedSearch, SavedSearchCatalog,
    SortColumn, SortDirection,
};
use thiserror::Error;

use crate::view::{
    FileViewDensity, FolderViewState, GridSize, ListColumnLayout, MillerColumnWidth, ViewMode,
};
use crate::{
    appearance::AppearancePreset, iconography::EntryIconStyle, terminal::TerminalProviderId,
};
use crate::{
    context_menu::ContextMenuPreferences,
    custom_actions::{CUSTOM_ACTION_CAPACITY, CustomActionDefinition},
    keybindings::KeybindingOverrides,
};

const PREFERENCE_QUEUE_CAPACITY: usize = 1;
const PREFERENCE_FILE_NAME: &str = "view-preferences.conf";
const PREFERENCE_FORMAT_VERSION: u16 = 18;
const MAX_PREFERENCE_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_TEMP_ATTEMPTS: u32 = 64;
pub const FOLDER_VIEW_CAPACITY: usize = 256;

/// The smallest useful sidebar width in Floe's compact density.
pub const SIDEBAR_WIDTH_MIN: u16 = 128;
/// Keeps the sidebar from starving the file view on ordinary desktop windows.
pub const SIDEBAR_WIDTH_MAX: u16 = 480;
pub const WINDOW_WIDTH_DEFAULT: u16 = 1_060;
pub const WINDOW_HEIGHT_DEFAULT: u16 = 720;
pub const WINDOW_WIDTH_MIN: u16 = 720;
pub const WINDOW_HEIGHT_MIN: u16 = 480;
pub const WINDOW_SIZE_MAX: u16 = 8_192;
pub const FONT_SCALE_MIN: u16 = 75;
pub const FONT_SCALE_MAX: u16 = 200;
pub const FONT_FAMILY_MAX_CHARS: usize = 64;
pub const COLLAPSED_GROUP_CAPACITY: usize = 32;
pub const COLLAPSED_GROUP_MAX_CHARS: usize = 80;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ColorSchemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[cfg(test)]
mod phase_21b_migration_tests {
    use std::{
        fs,
        os::unix::{fs::PermissionsExt, fs::symlink},
    };

    use tempfile::tempdir;

    use super::*;

    fn preference_path(root: &Path) -> PathBuf {
        root.join("config").join("floe").join(PREFERENCE_FILE_NAME)
    }

    fn write_fixture(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture parent");
        fs::write(path, bytes).expect("write preference fixture");
    }

    #[test]
    fn phase_21b_migration_upgrades_legacy_with_private_backup_and_rollback() {
        let fixture = tempdir().expect("temporary preferences");
        let path = preference_path(fixture.path());
        let legacy = b"version=16\nview=grid\ngrid-size=160\n";
        write_fixture(&path, legacy);

        let (loaded, worker) = PreferenceWorker::spawn_internal(path.clone(), None)
            .expect("migrate supported preferences");
        drop(worker);
        assert_eq!(loaded.mode, ViewMode::Grid);
        assert_eq!(loaded.grid_size.edge(), 160);

        let backup = path.with_extension("conf.pre-v18-legacy");
        assert_eq!(fs::read(&backup).expect("legacy backup"), legacy);
        assert!(
            fs::read_to_string(&path)
                .expect("migrated preferences")
                .starts_with("version=18\n")
        );
        assert_eq!(
            fs::metadata(path.parent().expect("preference parent"))
                .expect("parent metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        for private_file in [&path, &backup] {
            assert_eq!(
                fs::metadata(private_file)
                    .expect("private file metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }

        fs::copy(&backup, &path).expect("restore rollback backup");
        let (rolled_back, worker) = PreferenceWorker::spawn_internal(path, None)
            .expect("re-open restored legacy preferences");
        drop(worker);
        assert_eq!(rolled_back.mode, ViewMode::Grid);
    }

    #[test]
    fn phase_21b_migration_backs_up_corrupt_input_and_rejects_future_and_symlink() {
        let fixture = tempdir().expect("temporary preferences");
        let corrupt_path = preference_path(fixture.path());
        write_fixture(&corrupt_path, b"version=18\nfont-family=bad\xff\n");
        let (defaults, worker) = PreferenceWorker::spawn_internal(corrupt_path.clone(), None)
            .expect("recover corrupt preferences with backup");
        drop(worker);
        assert_eq!(defaults, ViewPreferences::default());
        assert!(corrupt_path.with_extension("conf.pre-v18-corrupt").exists());

        let future_path = fixture.path().join("future/floe/view-preferences.conf");
        write_fixture(&future_path, b"version=19\nview=grid\n");
        assert!(PreferenceWorker::spawn_internal(future_path.clone(), None).is_err());
        assert_eq!(
            fs::read(&future_path).expect("future input retained"),
            b"version=19\nview=grid\n"
        );

        let sentinel = fixture.path().join("sentinel");
        fs::write(&sentinel, b"do not replace").expect("sentinel");
        let symlink_path = fixture.path().join("symlink/floe/view-preferences.conf");
        fs::create_dir_all(symlink_path.parent().expect("symlink parent")).expect("symlink parent");
        symlink(&sentinel, &symlink_path).expect("preference symlink");
        assert!(PreferenceWorker::spawn_internal(symlink_path, None).is_err());
        assert_eq!(
            fs::read(sentinel).expect("sentinel unchanged"),
            b"do not replace"
        );
    }

    #[test]
    fn phase_21b_migration_rejects_oversize_and_ignores_interrupted_temp_residue() {
        let fixture = tempdir().expect("temporary preferences");
        let oversized_path = preference_path(fixture.path());
        write_fixture(
            &oversized_path,
            &vec![b'x'; (MAX_PREFERENCE_FILE_BYTES + 1) as usize],
        );
        assert!(PreferenceWorker::spawn_internal(oversized_path, None).is_err());

        let current_path = fixture.path().join("current/floe/view-preferences.conf");
        let current = ViewPreferences {
            mode: ViewMode::Grid,
            ..ViewPreferences::default()
        };
        write_fixture(&current_path, current.serialize().as_bytes());
        fs::write(
            current_path
                .parent()
                .expect("current parent")
                .join(".view-preferences.conf.tmp-interrupted"),
            b"partial",
        )
        .expect("interrupted residue");
        let (loaded, worker) = PreferenceWorker::spawn_internal(current_path, None)
            .expect("load current preference despite residue");
        drop(worker);
        assert_eq!(loaded.mode, ViewMode::Grid);
    }
}

impl ColorSchemePreference {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub const fn persisted(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::System => "Follow system",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.persisted() == value)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ClickPolicy {
    #[default]
    Double,
    Single,
}

impl ClickPolicy {
    pub const ALL: [Self; 2] = [Self::Double, Self::Single];

    pub const fn persisted(self) -> &'static str {
        match self {
            Self::Double => "double",
            Self::Single => "single",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Double => "Double-click to open",
            Self::Single => "Single-click to open",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.persisted() == value)
    }

    pub const fn activates_on_single_click(self) -> bool {
        matches!(self, Self::Single)
    }
}

pub fn validated_font_family(value: &str) -> Option<String> {
    if value.chars().any(|character| character.is_control()) {
        return None;
    }
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.chars().count() > FONT_FAMILY_MAX_CHARS {
        return None;
    }
    Some(value.to_owned())
}

pub fn clamp_font_scale(percent: u16) -> u16 {
    percent.clamp(FONT_SCALE_MIN, FONT_SCALE_MAX)
}

/// The last normal (neither maximized nor fullscreen) top-level window size.
///
/// Wayland intentionally does not expose portable application-controlled
/// placement, so Floe persists size only. One tuple avoids restoring a width
/// from one configuration with a height from another.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowSize {
    width: u16,
    height: u16,
}

impl WindowSize {
    pub fn from_normal_allocation(
        width: i32,
        height: i32,
        maximized: bool,
        fullscreen: bool,
    ) -> Option<Self> {
        if maximized || fullscreen || width <= 0 || height <= 0 {
            return None;
        }
        Some(Self {
            width: width.clamp(i32::from(WINDOW_WIDTH_MIN), i32::from(WINDOW_SIZE_MAX)) as u16,
            height: height.clamp(i32::from(WINDOW_HEIGHT_MIN), i32::from(WINDOW_SIZE_MAX)) as u16,
        })
    }

    fn from_persisted(value: &str) -> Option<Self> {
        let (width, height) = value.split_once('x')?;
        Self::from_normal_allocation(width.parse().ok()?, height.parse().ok()?, false, false)
    }

    fn persisted(self) -> String {
        format!("{}x{}", self.width, self.height)
    }

    pub const fn width(self) -> i32 {
        self.width as i32
    }

    pub const fn height(self) -> i32 {
        self.height as i32
    }
}

impl Default for WindowSize {
    fn default() -> Self {
        Self {
            width: WINDOW_WIDTH_DEFAULT,
            height: WINDOW_HEIGHT_DEFAULT,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SidebarDensity {
    #[default]
    Compact,
    Balanced,
    Comfortable,
}

impl SidebarDensity {
    pub const fn persisted(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Balanced => "balanced",
            Self::Comfortable => "comfortable",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        match value {
            "compact" => Some(Self::Compact),
            "balanced" => Some(Self::Balanced),
            "comfortable" => Some(Self::Comfortable),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolderViewOverride {
    path: PathBuf,
    state: FolderViewState,
}

impl FolderViewOverride {
    pub fn state(&self) -> FolderViewState {
        self.state
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewPreferences {
    pub window_size: Option<WindowSize>,
    pub mode: ViewMode,
    pub grid_size: GridSize,
    pub sidebar_density: SidebarDensity,
    pub sidebar_width: Option<u16>,
    pub sidebar_collapsed: bool,
    pub completion_notifications: bool,
    pub miller_column_width: MillerColumnWidth,
    pub inspector_width: MillerColumnWidth,
    pub file_density: FileViewDensity,
    pub sort: DirectorySort,
    pub columns: ListColumnLayout,
    pub color_scheme: ColorSchemePreference,
    pub click_policy: ClickPolicy,
    pub font_family: Option<String>,
    pub font_scale_percent: u16,
    pub reduced_motion: bool,
    pub collapsed_groups: Vec<String>,
    pub remember_per_folder: bool,
    pub keybindings: KeybindingOverrides,
    pub vim_mode: bool,
    pub preferred_terminal: Option<TerminalProviderId>,
    pub context_menu: ContextMenuPreferences,
    pub appearance: AppearancePreset,
    pub icon_style: EntryIconStyle,
    pub saved_searches: SavedSearchCatalog,
    pub search_index_enabled: bool,
    pub metadata_sort_cache_enabled: bool,
    pub privileged_access_enabled: bool,
    pub custom_actions: Vec<CustomActionDefinition>,
    folder_views: Vec<FolderViewOverride>,
}

impl Default for ViewPreferences {
    fn default() -> Self {
        let state = FolderViewState::default();
        Self {
            window_size: None,
            mode: state.mode,
            grid_size: state.grid_size,
            sidebar_density: SidebarDensity::default(),
            sidebar_width: None,
            sidebar_collapsed: false,
            completion_notifications: true,
            miller_column_width: MillerColumnWidth::default(),
            inspector_width: MillerColumnWidth::default(),
            file_density: state.density,
            sort: state.sort,
            columns: state.columns,
            color_scheme: ColorSchemePreference::System,
            click_policy: ClickPolicy::Double,
            font_family: None,
            font_scale_percent: 100,
            reduced_motion: false,
            collapsed_groups: Vec::new(),
            remember_per_folder: false,
            keybindings: KeybindingOverrides::default(),
            vim_mode: false,
            preferred_terminal: None,
            context_menu: ContextMenuPreferences::default(),
            appearance: AppearancePreset::Frosted,
            icon_style: EntryIconStyle::FloeColor,
            saved_searches: SavedSearchCatalog::default(),
            search_index_enabled: false,
            metadata_sort_cache_enabled: true,
            privileged_access_enabled: false,
            custom_actions: Vec::new(),
            folder_views: Vec::new(),
        }
    }
}

impl ViewPreferences {
    pub fn global_state(&self) -> FolderViewState {
        FolderViewState {
            mode: self.mode,
            grid_size: self.grid_size,
            density: self.file_density,
            sort: self.sort,
            columns: self.columns,
        }
    }

    pub fn set_global_state(&mut self, state: FolderViewState) {
        self.mode = state.mode;
        self.grid_size = state.grid_size;
        self.file_density = state.density;
        self.sort = state.sort;
        self.columns = state.columns;
    }

    pub fn effective_state(&self, path: &Path) -> FolderViewState {
        if self.remember_per_folder {
            self.folder_views
                .iter()
                .rev()
                .find(|item| item.path == path)
                .map(FolderViewOverride::state)
                .unwrap_or_else(|| self.global_state())
        } else {
            self.global_state()
        }
    }

    pub fn remember_folder_state(&mut self, path: PathBuf, state: FolderViewState) {
        if !self.remember_per_folder {
            self.set_global_state(state);
            return;
        }
        self.folder_views.retain(|item| item.path != path);
        self.folder_views.push(FolderViewOverride { path, state });
        if self.folder_views.len() > FOLDER_VIEW_CAPACITY {
            let excess = self.folder_views.len() - FOLDER_VIEW_CAPACITY;
            self.folder_views.drain(..excess);
        }
    }

    pub fn clear_all_folder_states(&mut self) {
        self.folder_views.clear();
    }

    #[cfg(test)]
    pub fn folder_view_count(&self) -> usize {
        self.folder_views.len()
    }

    pub(crate) fn parse(contents: &str) -> Self {
        let mut preferences = Self::default();
        for line in contents.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "window-size" => {
                    preferences.window_size = WindowSize::from_persisted(value);
                }
                "view" => {
                    if let Some(mode) = ViewMode::from_persisted(value) {
                        preferences.mode = mode;
                    }
                }
                "grid-size" => {
                    if let Ok(edge) = value.parse::<u16>()
                        && let Some(size) = GridSize::from_persisted(edge)
                    {
                        preferences.grid_size = size;
                    }
                }
                "sidebar-density" => {
                    if let Some(density) = SidebarDensity::from_persisted(value) {
                        preferences.sidebar_density = density;
                    }
                }
                "sidebar-width" => {
                    if let Ok(width) = value.parse::<u16>() {
                        preferences.sidebar_width = Some(clamp_sidebar_width(width));
                    }
                }
                "sidebar-collapsed" => preferences.sidebar_collapsed = value == "true",
                "completion-notifications" => {
                    preferences.completion_notifications = value != "false"
                }
                "miller-column-width" => {
                    if let Ok(width) = value.parse::<u16>() {
                        preferences.miller_column_width = MillerColumnWidth::new(width);
                    }
                }
                "inspector-width" => {
                    if let Ok(width) = value.parse::<u16>() {
                        preferences.inspector_width = MillerColumnWidth::new(width);
                    }
                }
                "file-density" => {
                    if let Some(density) = FileViewDensity::from_persisted(value) {
                        preferences.file_density = density;
                    }
                }
                "sort-column" => {
                    if let Some(column) = SortColumn::from_persisted(value) {
                        preferences.sort.column = column;
                    }
                }
                "sort-direction" => {
                    if let Some(direction) = SortDirection::from_persisted(value) {
                        preferences.sort.direction = direction;
                    }
                }
                "directories" => {
                    if let Some(placement) = DirectoryPlacement::from_persisted(value) {
                        preferences.sort.directories = placement;
                    }
                }
                "grouping" => {
                    if let Some(grouping) = DirectoryGrouping::from_persisted(value) {
                        preferences.sort.grouping = grouping;
                    }
                }
                "hidden-last" => preferences.sort.hidden_last = value == "true",
                "columns" => {
                    preferences.columns = ListColumnLayout::parse_visible(value);
                }
                "column-widths" => preferences.columns.apply_widths_text(value),
                "column-order" => preferences.columns.apply_order_text(value),
                "color-scheme" => {
                    if let Some(scheme) = ColorSchemePreference::from_persisted(value) {
                        preferences.color_scheme = scheme;
                    }
                }
                "click-policy" => {
                    if let Some(policy) = ClickPolicy::from_persisted(value) {
                        preferences.click_policy = policy;
                    }
                }
                "font-family" => preferences.font_family = validated_font_family(value),
                "font-scale" => {
                    if let Ok(scale) = value.parse::<u16>() {
                        preferences.font_scale_percent = clamp_font_scale(scale);
                    }
                }
                "reduced-motion" => preferences.reduced_motion = value == "true",
                "collapsed-group" => {
                    let valid = !value.is_empty()
                        && value.chars().count() <= COLLAPSED_GROUP_MAX_CHARS
                        && !value.chars().any(char::is_control);
                    if valid
                        && preferences.collapsed_groups.len() < COLLAPSED_GROUP_CAPACITY
                        && !preferences
                            .collapsed_groups
                            .iter()
                            .any(|group| group == value)
                    {
                        preferences.collapsed_groups.push(value.to_owned());
                    }
                }
                "remember-per-folder" => {
                    preferences.remember_per_folder = value == "true";
                }
                "keybinding" => {
                    preferences.keybindings.apply_record(value);
                }
                "vim-mode" => preferences.vim_mode = value == "true",
                "preferred-terminal" => {
                    preferences.preferred_terminal = TerminalProviderId::from_persisted(value);
                }
                "context-menu-groups" => {
                    preferences.context_menu = ContextMenuPreferences::parse(value);
                }
                "appearance" => {
                    if let Some(appearance) = AppearancePreset::from_persisted(value) {
                        preferences.appearance = appearance;
                    }
                }
                "icon-style" => {
                    if let Some(style) = EntryIconStyle::from_persisted(value) {
                        preferences.icon_style = style;
                    }
                }
                "saved-search" => {
                    if let Some(saved) = SavedSearch::parse_record(value) {
                        let _ = preferences.saved_searches.add(saved);
                    }
                }
                "search-index-enabled" => {
                    preferences.search_index_enabled = value == "true";
                }
                "metadata-sort-cache-enabled" => {
                    preferences.metadata_sort_cache_enabled = value == "true";
                }
                "privileged-access-enabled" => {
                    preferences.privileged_access_enabled = value == "true";
                }
                "custom-action" => {
                    if let Some(action) = CustomActionDefinition::parse_record(value) {
                        preferences
                            .custom_actions
                            .retain(|existing| existing.id != action.id);
                        if preferences.custom_actions.len() < CUSTOM_ACTION_CAPACITY {
                            preferences.custom_actions.push(action);
                        }
                    }
                }
                "folder" => {
                    if let Some(folder) = parse_folder_override(value) {
                        preferences
                            .folder_views
                            .retain(|item| item.path != folder.path);
                        preferences.folder_views.push(folder);
                        if preferences.folder_views.len() > FOLDER_VIEW_CAPACITY {
                            preferences.folder_views.remove(0);
                        }
                    }
                }
                _ => {}
            }
        }
        preferences
    }

    pub(crate) fn serialize(&self) -> String {
        let mut serialized = format!(
            "version=18\nappearance={}\nicon-style={}\ncolor-scheme={}\nclick-policy={}\nfont-scale={}\nreduced-motion={}\nview={}\ngrid-size={}\nsidebar-density={}\nsidebar-collapsed={}\ncompletion-notifications={}\nmiller-column-width={}\ninspector-width={}\nfile-density={}\nsort-column={}\nsort-direction={}\ndirectories={}\ngrouping={}\nhidden-last={}\ncolumns={}\ncolumn-widths={}\ncolumn-order={}\nremember-per-folder={}\nvim-mode={}\ncontext-menu-groups={}\nsearch-index-enabled={}\nmetadata-sort-cache-enabled={}\nprivileged-access-enabled={}\n",
            self.appearance.persisted(),
            self.icon_style.persisted(),
            self.color_scheme.persisted(),
            self.click_policy.persisted(),
            self.font_scale_percent,
            self.reduced_motion,
            self.mode.persisted(),
            self.grid_size.edge(),
            self.sidebar_density.persisted(),
            self.sidebar_collapsed,
            self.completion_notifications,
            self.miller_column_width.get(),
            self.inspector_width.get(),
            self.file_density.persisted(),
            self.sort.column.persisted(),
            self.sort.direction.persisted(),
            self.sort.directories.persisted(),
            self.sort.grouping.persisted(),
            self.sort.hidden_last,
            self.columns.visible_names(),
            self.columns.widths_text(),
            self.columns.order_text(),
            self.remember_per_folder,
            self.vim_mode,
            self.context_menu.persisted(),
            self.search_index_enabled,
            self.metadata_sort_cache_enabled,
            self.privileged_access_enabled,
        );
        if let Some(font_family) = self.font_family.as_deref() {
            serialized.push_str("font-family=");
            serialized.push_str(font_family);
            serialized.push('\n');
        }
        for group in self.collapsed_groups.iter().take(COLLAPSED_GROUP_CAPACITY) {
            serialized.push_str("collapsed-group=");
            serialized.push_str(group);
            serialized.push('\n');
        }
        if let Some(window_size) = self.window_size {
            serialized.push_str("window-size=");
            serialized.push_str(&window_size.persisted());
            serialized.push('\n');
        }
        if let Some(width) = self.sidebar_width {
            serialized.push_str(&format!("sidebar-width={}\n", clamp_sidebar_width(width)));
        }
        if let Some(provider) = self.preferred_terminal {
            serialized.push_str("preferred-terminal=");
            serialized.push_str(provider.persisted());
            serialized.push('\n');
        }
        for record in self.keybindings.serialize_records() {
            serialized.push_str("keybinding=");
            serialized.push_str(&record);
            serialized.push('\n');
        }
        for saved in self.saved_searches.entries() {
            serialized.push_str("saved-search=");
            serialized.push_str(&saved.serialize_record());
            serialized.push('\n');
        }
        for action in self.custom_actions.iter().take(CUSTOM_ACTION_CAPACITY) {
            if let Some(record) = action.serialize_record() {
                serialized.push_str("custom-action=");
                serialized.push_str(&record);
                serialized.push('\n');
            }
        }
        for folder in &self.folder_views {
            if let Some(encoded) = serialize_folder_override(folder) {
                serialized.push_str("folder=");
                serialized.push_str(&encoded);
                serialized.push('\n');
            }
        }
        serialized
    }
}

pub fn clamp_sidebar_width(width: u16) -> u16 {
    width.clamp(SIDEBAR_WIDTH_MIN, SIDEBAR_WIDTH_MAX)
}

#[cfg(unix)]
fn serialize_folder_override(folder: &FolderViewOverride) -> Option<String> {
    let path = hex_encode(folder.path.as_os_str().as_bytes());
    let state = folder.state;
    Some(format!(
        "{path}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        state.mode.persisted(),
        state.grid_size.edge(),
        state.density.persisted(),
        state.sort.column.persisted(),
        state.sort.direction.persisted(),
        state.sort.directories.persisted(),
        state.sort.grouping.persisted(),
        state.sort.hidden_last,
        state.columns.visible_names(),
        state.columns.widths_text(),
        state.columns.order_text(),
    ))
}

#[cfg(not(unix))]
fn serialize_folder_override(_folder: &FolderViewOverride) -> Option<String> {
    None
}

#[cfg(unix)]
fn parse_folder_override(value: &str) -> Option<FolderViewOverride> {
    let mut fields = value.split('\t');
    let path = PathBuf::from(OsString::from_vec(hex_decode(fields.next()?)?));
    if !path.is_absolute() {
        return None;
    }
    let mode = ViewMode::from_persisted(fields.next()?)?;
    let grid_size = GridSize::from_persisted(fields.next()?.parse().ok()?)?;
    let density = FileViewDensity::from_persisted(fields.next()?)?;
    let column = SortColumn::from_persisted(fields.next()?)?;
    let direction = SortDirection::from_persisted(fields.next()?)?;
    let directories = DirectoryPlacement::from_persisted(fields.next()?)?;
    let grouping = DirectoryGrouping::from_persisted(fields.next()?)?;
    let remaining = fields.collect::<Vec<_>>();
    let (hidden_last, visible, widths, order) = match remaining.as_slice() {
        [visible, widths] => (false, *visible, *widths, None),
        [hidden_last, visible, widths] => (
            (*hidden_last).parse::<bool>().ok()?,
            *visible,
            *widths,
            None,
        ),
        [hidden_last, visible, widths, order] => (
            (*hidden_last).parse::<bool>().ok()?,
            *visible,
            *widths,
            Some(*order),
        ),
        _ => return None,
    };
    let mut columns = ListColumnLayout::parse_visible(visible);
    columns.apply_widths_text(widths);
    if let Some(order) = order {
        columns.apply_order_text(order);
    }
    Some(FolderViewOverride {
        path,
        state: FolderViewState {
            mode,
            grid_size,
            density,
            sort: DirectorySort::new(column, direction)
                .with_directories(directories)
                .with_grouping(grouping)
                .with_hidden_last(hidden_last),
            columns,
        },
    })
}

#[cfg(not(unix))]
fn parse_folder_override(_value: &str) -> Option<FolderViewOverride> {
    None
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_nibble(pair[0])?;
            let low = hex_nibble(pair[1])?;
            Some((high << 4) | low)
        })
        .collect()
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[derive(Debug, Error)]
pub enum PreferenceSubmitError {
    #[error("preference worker queue is full")]
    Full(Box<ViewPreferences>),
    #[error("preference worker is disconnected")]
    Disconnected,
}

pub struct PreferenceWorker {
    sender: Option<SyncSender<ViewPreferences>>,
    worker: Option<JoinHandle<()>>,
}

impl PreferenceWorker {
    /// Loads the user's familiar presentation without running migrations or
    /// creating configuration files. Selection Mode uses this read-only path
    /// so chooser-local adjustments remain process-local.
    pub fn load_read_only() -> io::Result<ViewPreferences> {
        let path = gtk::glib::user_config_dir()
            .join("floe")
            .join(PREFERENCE_FILE_NAME);
        load_preferences(&path).map(|loaded| loaded.preferences)
    }

    pub fn spawn() -> io::Result<(ViewPreferences, Self)> {
        let path = gtk::glib::user_config_dir()
            .join("floe")
            .join(PREFERENCE_FILE_NAME);
        Self::spawn_internal(path, None)
    }

    fn spawn_internal(
        path: PathBuf,
        start_gate: Option<Receiver<()>>,
    ) -> io::Result<(ViewPreferences, Self)> {
        let loaded = load_preferences(&path)?;
        if let Some(backup_suffix) = loaded.backup_suffix {
            persist_migrated_preferences(
                &path,
                &loaded.source_bytes,
                backup_suffix,
                &loaded.preferences,
            )?;
        }
        let initial = loaded.preferences;
        let (sender, receiver) = mpsc::sync_channel(PREFERENCE_QUEUE_CAPACITY);
        let worker = thread::Builder::new()
            .name("floe-view-preferences".to_owned())
            .spawn(move || {
                if let Some(start_gate) = start_gate
                    && start_gate.recv().is_err()
                {
                    return;
                }
                while let Ok(mut preferences) = receiver.recv() {
                    while let Ok(newer) = receiver.try_recv() {
                        preferences = newer;
                    }
                    if let Err(error) = persist_preferences(&path, &preferences) {
                        tracing::warn!(%error, "could not persist view preferences");
                    }
                }
            })?;
        Ok((
            initial,
            Self {
                sender: Some(sender),
                worker: Some(worker),
            },
        ))
    }

    pub fn try_save(&self, preferences: ViewPreferences) -> Result<(), PreferenceSubmitError> {
        let Some(sender) = self.sender.as_ref() else {
            return Err(PreferenceSubmitError::Disconnected);
        };
        match sender.try_send(preferences) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(preferences)) => {
                Err(PreferenceSubmitError::Full(Box::new(preferences)))
            }
            Err(TrySendError::Disconnected(_)) => Err(PreferenceSubmitError::Disconnected),
        }
    }

    pub fn save_before_shutdown(
        &self,
        preferences: ViewPreferences,
    ) -> Result<(), PreferenceSubmitError> {
        let Some(sender) = self.sender.as_ref() else {
            return Err(PreferenceSubmitError::Disconnected);
        };
        sender
            .send(preferences)
            .map_err(|_| PreferenceSubmitError::Disconnected)
    }
}

impl Drop for PreferenceWorker {
    fn drop(&mut self) {
        self.sender.take();
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("view preference worker panicked during shutdown");
        }
    }
}

struct LoadedPreferences {
    preferences: ViewPreferences,
    source_bytes: Vec<u8>,
    backup_suffix: Option<&'static str>,
}

fn load_preferences(path: &Path) -> io::Result<LoadedPreferences> {
    let mut options = OpenOptions::new();
    options.read(true);
    options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(LoadedPreferences {
                preferences: ViewPreferences::default(),
                source_bytes: Vec::new(),
                backup_suffix: None,
            });
        }
        Err(error) => return Err(error),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.len() > MAX_PREFERENCE_FILE_BYTES
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "view preference storage is not a bounded owned regular file",
        ));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    Read::by_ref(&mut file)
        .take(MAX_PREFERENCE_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_PREFERENCE_FILE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "view preference storage exceeds its size limit",
        ));
    }
    let contents = match std::str::from_utf8(&bytes) {
        Ok(contents) if !contents.contains('\0') => contents,
        _ => {
            return Ok(LoadedPreferences {
                preferences: ViewPreferences::default(),
                source_bytes: bytes,
                backup_suffix: Some("corrupt"),
            });
        }
    };
    let version = stored_preference_version(contents)?;
    if version > PREFERENCE_FORMAT_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("view preference version {version} is newer than supported"),
        ));
    }
    Ok(LoadedPreferences {
        preferences: ViewPreferences::parse(contents),
        source_bytes: bytes,
        backup_suffix: (version < PREFERENCE_FORMAT_VERSION).then_some("legacy"),
    })
}

fn stored_preference_version(contents: &str) -> io::Result<u16> {
    let mut version = None;
    for line in contents.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "version" {
            continue;
        }
        if version.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "view preference storage has duplicate version records",
            ));
        }
        version = Some(value.trim().parse::<u16>().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "view preference storage has an invalid version",
            )
        })?);
    }
    Ok(version.unwrap_or(0))
}

fn persist_migrated_preferences(
    path: &Path,
    source_bytes: &[u8],
    backup_suffix: &str,
    preferences: &ViewPreferences,
) -> io::Result<()> {
    let backup = path.with_extension(format!(
        "conf.pre-v{PREFERENCE_FORMAT_VERSION}-{backup_suffix}"
    ));
    if !backup.exists() {
        write_private_file(&backup, source_bytes, false)?;
    }
    persist_preferences(path, preferences)
}

fn persist_preferences(path: &Path, preferences: &ViewPreferences) -> io::Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && (!metadata.is_file() || metadata.file_type().is_symlink())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "view preference destination is not a regular file",
        ));
    }
    write_private_file(path, preferences.serialize().as_bytes(), true)
}

fn write_private_file(path: &Path, bytes: &[u8], replace: bool) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "view preference path has no parent",
        )
    })?;
    prepare_private_parent(parent)?;
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "view preference path has no file name",
        )
    })?;
    let mut temporary = None;
    for attempt in 0..MAX_TEMP_ATTEMPTS {
        let candidate = parent.join(format!(
            ".{}.tmp-{}-{attempt}",
            name.to_string_lossy(),
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
        match options.open(&candidate) {
            Ok(file) => {
                temporary = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    let (temporary_path, mut file) = temporary.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no private preference temporary name is available",
        )
    })?;
    let result = (|| -> io::Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if replace {
            fs::rename(&temporary_path, path)?;
        } else {
            match fs::hard_link(&temporary_path, path) {
                Ok(()) => fs::remove_file(&temporary_path)?,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    fs::remove_file(&temporary_path)?;
                }
                Err(error) => return Err(error),
            }
        }
        File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn prepare_private_parent(parent: &Path) -> io::Result<()> {
    match fs::symlink_metadata(parent) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "view preference parent is not a private directory",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir(parent)?,
        Err(error) => return Err(error),
    }
    let metadata = fs::symlink_metadata(parent)?;
    if metadata.uid() != rustix::process::geteuid().as_raw() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "view preference parent belongs to another user",
        ));
    }
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[cfg(unix)]
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    };

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn phase_23c_natural_sort_and_phase_23e_sidebar_policy_persist_with_notifications() {
        let preferences = ViewPreferences {
            sort: DirectorySort::new(SortColumn::NaturalName, SortDirection::Descending),
            sidebar_width: Some(312),
            sidebar_collapsed: true,
            completion_notifications: false,
            ..ViewPreferences::default()
        };
        let restored = ViewPreferences::parse(&preferences.serialize());
        assert_eq!(restored.sort.column, SortColumn::NaturalName);
        assert_eq!(restored.sort.direction, SortDirection::Descending);
        assert_eq!(restored.sidebar_width, Some(312));
        assert!(restored.sidebar_collapsed);
        assert!(!restored.completion_notifications);

        let legacy = ViewPreferences::parse("version=18\nsidebar-width=280\n");
        assert_eq!(legacy.sidebar_width, Some(280));
        assert!(!legacy.sidebar_collapsed);
        assert!(legacy.completion_notifications);
    }
    use crate::view::ListColumn;

    #[test]
    fn phase_22a_read_only_preferences_do_not_create_or_migrate_files() {
        let fixture = tempdir().expect("preference fixture");
        let missing = fixture.path().join("missing/floe/view-preferences.conf");
        assert_eq!(
            load_preferences(&missing)
                .expect("missing preferences")
                .preferences,
            ViewPreferences::default()
        );
        assert!(!missing.parent().expect("missing parent").exists());

        let legacy = fixture.path().join("legacy/floe/view-preferences.conf");
        fs::create_dir_all(legacy.parent().expect("legacy parent")).expect("legacy directory");
        let bytes = b"version=16\nview=grid\ngrid-size=160\n";
        fs::write(&legacy, bytes).expect("legacy preferences");
        let loaded = load_preferences(&legacy).expect("read legacy preferences");
        assert_eq!(loaded.preferences.mode, ViewMode::Grid);
        assert_eq!(fs::read(&legacy).expect("legacy remains"), bytes);
        assert!(!legacy.with_extension("conf.pre-v18-legacy").exists());
    }

    #[test]
    fn phase_6d_preferences_parse_valid_fields_and_reject_invalid_values() {
        let parsed = ViewPreferences::parse("view=grid\ngrid-size=160\nunknown=value\n");
        assert_eq!(parsed.mode, ViewMode::Grid);
        assert_eq!(parsed.grid_size.edge(), 160);

        let invalid = ViewPreferences::parse("view=tiles\ngrid-size=100\n");
        assert_eq!(invalid, ViewPreferences::default());
    }

    #[test]
    fn phase_20b2a_window_size_preferences_migrate_validate_and_round_trip() {
        assert_eq!(ViewPreferences::parse("version=16\n").window_size, None);
        assert_eq!(
            ViewPreferences::parse("window-size=1440x900\n").window_size,
            WindowSize::from_normal_allocation(1440, 900, false, false)
        );
        assert_eq!(
            ViewPreferences::parse("window-size=1x2\n").window_size,
            WindowSize::from_normal_allocation(1, 2, false, false)
        );
        for malformed in ["", "0x720", "1060x0", "wide", "1060x720x1"] {
            assert_eq!(
                ViewPreferences::parse(&format!("window-size={malformed}\n")).window_size,
                None,
                "{malformed:?}"
            );
        }

        let preferences = ViewPreferences {
            window_size: WindowSize::from_normal_allocation(1728, 972, false, false),
            ..ViewPreferences::default()
        };
        let serialized = preferences.serialize();
        assert!(serialized.starts_with("version=18\n"));
        assert!(serialized.contains("window-size=1728x972\n"));
        assert_eq!(ViewPreferences::parse(&serialized), preferences);
    }

    #[test]
    fn phase_20b2_appearance_click_group_and_column_preferences_migrate_safely() {
        let mut preferences = ViewPreferences {
            color_scheme: ColorSchemePreference::Dark,
            click_policy: ClickPolicy::Single,
            font_family: Some("Noto Sans".to_owned()),
            font_scale_percent: 135,
            reduced_motion: true,
            collapsed_groups: vec!["Folders".to_owned(), "2026-08-29".to_owned()],
            ..ViewPreferences::default()
        };
        preferences.columns.move_column(ListColumn::Size, -1);
        let serialized = preferences.serialize();
        assert!(serialized.starts_with("version=18\n"));
        assert!(serialized.contains("color-scheme=dark\n"));
        assert!(serialized.contains("click-policy=single\n"));
        assert!(serialized.contains("column-order=name,size,type"));
        assert_eq!(ViewPreferences::parse(&serialized), preferences);

        let hostile = ViewPreferences::parse(
            "color-scheme=unknown\nclick-policy=triple\nfont-scale=9999\nfont-family=bad\nname\ncollapsed-group=ok\ncollapsed-group=bad\nvalue\n",
        );
        assert_eq!(hostile.color_scheme, ColorSchemePreference::System);
        assert_eq!(hostile.click_policy, ClickPolicy::Double);
        assert_eq!(hostile.font_scale_percent, FONT_SCALE_MAX);
        assert_eq!(hostile.font_family, Some("bad".to_owned()));
        assert_eq!(
            hostile.collapsed_groups,
            vec!["ok".to_owned(), "bad".to_owned()]
        );
        assert_eq!(validated_font_family("\nunsafe"), None);
        assert_eq!(validated_font_family(&"x".repeat(65)), None);
    }

    #[test]
    fn phase_20b2_click_policy_is_explicit_persisted_and_keyboard_independent() {
        assert!(!ClickPolicy::Double.activates_on_single_click());
        assert!(ClickPolicy::Single.activates_on_single_click());
        assert_eq!(
            ClickPolicy::from_persisted("single"),
            Some(ClickPolicy::Single)
        );
        assert_eq!(ClickPolicy::from_persisted("enter"), None);
        let restored = ViewPreferences::parse("click-policy=single\n");
        assert_eq!(restored.click_policy, ClickPolicy::Single);
    }

    #[test]
    fn phase_20b2_appearance_validates_scheme_font_scale_motion_and_reset_defaults() {
        assert_eq!(ColorSchemePreference::ALL.len(), 3);
        assert_eq!(clamp_font_scale(1), FONT_SCALE_MIN);
        assert_eq!(clamp_font_scale(500), FONT_SCALE_MAX);
        assert_eq!(
            validated_font_family("  Noto Sans  ").as_deref(),
            Some("Noto Sans")
        );
        let defaults = ViewPreferences::default();
        assert_eq!(defaults.color_scheme, ColorSchemePreference::System);
        assert_eq!(defaults.font_scale_percent, 100);
        assert!(!defaults.reduced_motion);
    }

    #[test]
    fn phase_20b2_columns_and_group_collapse_round_trip_without_path_reconstruction() {
        let mut preferences = ViewPreferences {
            collapsed_groups: vec!["Folders".to_owned(), ".rs".to_owned()],
            ..ViewPreferences::default()
        };
        preferences.columns.move_column(ListColumn::Modified, -2);
        preferences
            .columns
            .autosize_from_max_chars(ListColumn::Name, 40);
        let restored = ViewPreferences::parse(&preferences.serialize());
        assert_eq!(restored.columns, preferences.columns);
        assert_eq!(restored.collapsed_groups, preferences.collapsed_groups);
    }

    #[test]
    fn phase_6d_preference_worker_persists_without_blocking_submitter() {
        let directory = tempdir().expect("temporary directory");
        let path = directory.path().join("nested").join(PREFERENCE_FILE_NAME);
        let (gate_sender, gate_receiver) = mpsc::channel();
        let (initial, worker) = PreferenceWorker::spawn_internal(path.clone(), Some(gate_receiver))
            .expect("preference worker");
        assert_eq!(initial, ViewPreferences::default());
        let preferences = ViewPreferences {
            mode: ViewMode::Grid,
            grid_size: GridSize::from_persisted(192).expect("grid size"),
            ..ViewPreferences::default()
        };
        worker.try_save(preferences.clone()).expect("first save");
        assert!(matches!(
            worker.try_save(ViewPreferences::default()),
            Err(PreferenceSubmitError::Full(_))
        ));
        gate_sender.send(()).expect("release worker");
        drop(worker);

        let saved = fs::read_to_string(&path).expect("saved preferences");
        assert_eq!(ViewPreferences::parse(&saved), preferences);
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn phase_6k2_preference_width_is_optional_clamped_and_backward_compatible() {
        let legacy = ViewPreferences::parse("view=grid\ngrid-size=160\n");
        assert_eq!(legacy.mode, ViewMode::Grid);
        assert_eq!(legacy.grid_size.edge(), 160);
        assert_eq!(legacy.sidebar_density, SidebarDensity::Compact);
        assert_eq!(legacy.sidebar_width, None);

        assert_eq!(
            ViewPreferences::parse("sidebar-width=1\n").sidebar_width,
            Some(SIDEBAR_WIDTH_MIN)
        );
        assert_eq!(
            ViewPreferences::parse("sidebar-width=65535\n").sidebar_width,
            Some(SIDEBAR_WIDTH_MAX)
        );
    }

    #[test]
    fn phase_6t_preferences_migrate_and_round_trip_complete_global_policy() {
        let mut preferences = ViewPreferences::parse("view=grid\ngrid-size=160\n");
        preferences.file_density = FileViewDensity::Compact;
        preferences.sort = DirectorySort::new(SortColumn::Extension, SortDirection::Descending)
            .with_directories(DirectoryPlacement::Last)
            .with_grouping(DirectoryGrouping::Extension);
        preferences.columns.set_visible(ListColumn::Mime, true);
        preferences.columns.set_width(ListColumn::Mime, 248);
        preferences.miller_column_width = MillerColumnWidth::new(360);
        preferences.inspector_width = MillerColumnWidth::new(420);
        preferences.remember_per_folder = true;

        let serialized = preferences.serialize();
        let restored = ViewPreferences::parse(&serialized);
        assert_eq!(restored, preferences);
        assert!(serialized.starts_with("version=18\n"));
        assert!(serialized.contains("miller-column-width=360\n"));
        assert!(serialized.contains("inspector-width=420\n"));
    }

    #[test]
    fn phase_12f_context_preferences_migrate_round_trip_and_preserve_explicit_empty() {
        let legacy = ViewPreferences::parse("version=7\nview=grid\n");
        assert_eq!(
            legacy.context_menu,
            ContextMenuPreferences::default(),
            "legacy files receive the reviewed compact defaults"
        );

        let customized = ViewPreferences::parse(
            "version=8\ncontext-menu-groups=archives,checksums,archives,unknown\n",
        );
        assert_eq!(customized.context_menu.persisted(), "archives,checksums");
        let serialized = customized.serialize();
        assert!(serialized.starts_with("version=18\n"));
        assert!(serialized.contains("context-menu-groups=archives,checksums\n"));
        assert_eq!(ViewPreferences::parse(&serialized), customized);

        let empty = ViewPreferences::parse("version=8\ncontext-menu-groups=\n");
        assert_eq!(empty.context_menu, ContextMenuPreferences::empty());
        assert_eq!(ViewPreferences::parse(&empty.serialize()), empty);
    }

    #[test]
    fn phase_10a_inspector_preferences_clamp_persist_and_migrate_version_three() {
        let legacy = ViewPreferences::parse("version=3\nmiller-column-width=360\n");
        assert_eq!(legacy.miller_column_width, MillerColumnWidth::new(360));
        assert_eq!(legacy.inspector_width, MillerColumnWidth::default());

        let narrow = ViewPreferences::parse("version=4\ninspector-width=1\n");
        assert_eq!(narrow.inspector_width, MillerColumnWidth::new(1));
        let wide = ViewPreferences::parse("version=4\ninspector-width=65535\n");
        assert_eq!(wide.inspector_width, MillerColumnWidth::new(u16::MAX));

        let mut preferences = legacy;
        preferences.inspector_width = MillerColumnWidth::new(440);
        let serialized = preferences.serialize();
        assert!(serialized.starts_with("version=18\n"));
        assert!(serialized.contains("inspector-width=440\n"));
        assert_eq!(
            ViewPreferences::parse(&serialized).inspector_width,
            MillerColumnWidth::new(440)
        );
    }

    #[test]
    fn appearance_preferences_migrate_validate_and_round_trip_all_presets() {
        let legacy = ViewPreferences::parse("version=8\nview=grid\n");
        assert_eq!(legacy.appearance, AppearancePreset::Frosted);
        assert_eq!(
            ViewPreferences::parse("version=11\nappearance=unknown\n").appearance,
            AppearancePreset::Frosted
        );

        for preset in AppearancePreset::ALL {
            let preferences = ViewPreferences {
                appearance: preset,
                ..ViewPreferences::default()
            };
            let serialized = preferences.serialize();
            assert!(serialized.starts_with("version=18\n"));
            assert!(serialized.contains(&format!("appearance={}\n", preset.persisted())));
            assert_eq!(ViewPreferences::parse(&serialized).appearance, preset);
        }
    }

    #[test]
    fn post_phase_14_icon_style_migrates_validates_and_round_trips() {
        let legacy = ViewPreferences::parse("version=11\nappearance=glass\n");
        assert_eq!(legacy.icon_style, EntryIconStyle::FloeColor);
        assert_eq!(
            ViewPreferences::parse("version=12\nicon-style=unknown\n").icon_style,
            EntryIconStyle::FloeColor
        );

        for style in EntryIconStyle::ALL {
            let preferences = ViewPreferences {
                icon_style: style,
                ..ViewPreferences::default()
            };
            let serialized = preferences.serialize();
            assert!(serialized.starts_with("version=18\n"));
            assert!(serialized.contains(&format!("icon-style={}\n", style.persisted())));
            assert_eq!(ViewPreferences::parse(&serialized).icon_style, style);
        }
    }

    #[cfg(unix)]
    #[test]
    fn phase_13e_saved_search_preferences_are_versioned_exact_and_corruption_tolerant() {
        use floe_core::{
            AdvancedFilter, FilenameSearchScope, FolderFilterMode, SavedSearch, SearchKind,
            SearchQuery,
        };

        let raw = PathBuf::from("/tmp").join(OsString::from_vec(vec![b's', 0x80]));
        let query = SearchQuery::new(
            raw.clone(),
            SearchKind::Contents,
            "Needle".to_owned(),
            FilenameSearchScope::Subtree,
            true,
            FolderFilterMode::Regex,
            AdvancedFilter {
                minimum_size: Some(10),
                match_case: true,
                ..AdvancedFilter::default()
            },
        )
        .expect("saved query");
        let saved = SavedSearch::new(1, "Raw root".to_owned(), query).expect("saved search");
        let mut preferences = ViewPreferences::default();
        preferences.saved_searches.add(saved).expect("catalog add");
        let serialized = preferences.serialize();
        assert!(serialized.starts_with("version=18\n"));
        assert!(serialized.contains("saved-search="));
        let restored = ViewPreferences::parse(&serialized);
        assert_eq!(restored.saved_searches, preferences.saved_searches);
        assert_eq!(
            restored.saved_searches.entries()[0]
                .query_definition()
                .root(),
            raw
        );
        let corrupt = ViewPreferences::parse(&format!(
            "version=11\nsaved-search=bad\n{}",
            serialized
                .lines()
                .find(|line| line.starts_with("saved-search="))
                .expect("saved record")
        ));
        assert_eq!(corrupt.saved_searches.entries().len(), 1);
        let directory = tempdir().expect("saved-search preference directory");
        let path = directory.path().join("floe").join(PREFERENCE_FILE_NAME);
        persist_preferences(&path, &preferences).expect("persist saved searches");
        assert_eq!(
            fs::metadata(&path)
                .expect("saved-search preference metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            load_preferences(&path)
                .expect("load saved-search preferences")
                .preferences
                .saved_searches,
            preferences.saved_searches
        );
    }

    #[cfg(unix)]
    #[test]
    fn phase_13f_search_index_opt_in_is_versioned_and_defaults_off() {
        let defaults = ViewPreferences::parse("version=10\n");
        assert!(!defaults.search_index_enabled);
        let mut enabled = defaults;
        enabled.search_index_enabled = true;
        let serialized = enabled.serialize();
        assert!(serialized.starts_with("version=18\n"));
        assert!(serialized.contains("search-index-enabled=true\n"));
        assert!(ViewPreferences::parse(&serialized).search_index_enabled);
        assert!(!ViewPreferences::parse("search-index-enabled=invalid\n").search_index_enabled);
    }

    #[test]
    fn phase_6t_preferences_preserve_raw_folder_paths_and_bound_history() {
        let raw = PathBuf::from("/tmp").join(OsString::from_vec(vec![b'f', 0x80]));
        let mut preferences = ViewPreferences {
            remember_per_folder: true,
            ..ViewPreferences::default()
        };
        let state = FolderViewState {
            mode: ViewMode::Grid,
            density: FileViewDensity::Spacious,
            ..FolderViewState::default()
        };
        preferences.remember_folder_state(raw.clone(), state);
        for index in 0..FOLDER_VIEW_CAPACITY {
            preferences.remember_folder_state(PathBuf::from(format!("/folder/{index}")), state);
        }
        assert_eq!(preferences.folder_view_count(), FOLDER_VIEW_CAPACITY);
        assert_eq!(
            preferences.effective_state(&raw),
            preferences.global_state()
        );

        preferences.remember_folder_state(raw.clone(), state);
        let restored = ViewPreferences::parse(&preferences.serialize());
        assert_eq!(restored.effective_state(&raw), state);
        assert_eq!(restored.folder_view_count(), FOLDER_VIEW_CAPACITY);
    }

    #[test]
    fn phase_6t_preferences_inherit_global_until_per_folder_is_enabled() {
        let path = PathBuf::from("/tmp/project");
        let mut preferences = ViewPreferences::default();
        let folder = FolderViewState {
            mode: ViewMode::Grid,
            ..FolderViewState::default()
        };
        preferences.remember_folder_state(path.clone(), folder);
        assert_eq!(preferences.global_state(), folder);

        preferences.remember_per_folder = true;
        let override_state = FolderViewState {
            density: FileViewDensity::Compact,
            ..folder
        };
        preferences.remember_folder_state(path.clone(), override_state);
        assert_eq!(preferences.effective_state(&path), override_state);
        assert_eq!(
            preferences.effective_state(Path::new("/unrecorded")),
            preferences.global_state()
        );
    }

    #[test]
    fn phase_19b_custom_action_store_uses_bounded_versioned_preferences() {
        let mut preferences = ViewPreferences::default();
        preferences.custom_actions.push(CustomActionDefinition {
            id: 1,
            name: "Inspect PDF".to_owned(),
            executable: "pdfinfo".to_owned(),
            arguments: vec!["%f".to_owned()],
            target: crate::custom_actions::CustomActionTarget::Files,
            mime_patterns: vec!["application/pdf".to_owned()],
            allow_multiple: false,
        });
        let serialized = preferences.serialize();
        assert!(serialized.starts_with("version=18\n"));
        assert!(serialized.contains("custom-action="));
        assert_eq!(ViewPreferences::parse(&serialized), preferences);

        let hostile = "custom-action=1\tfiles\tfalse\tnot-hex\n";
        assert!(ViewPreferences::parse(hostile).custom_actions.is_empty());
    }

    #[test]
    fn phase_14b_state_privileged_access_opt_in_is_private_and_versioned() {
        let defaults = ViewPreferences::default();
        assert!(!defaults.privileged_access_enabled);
        let legacy = ViewPreferences::parse("version=13\nprivileged-access-enabled=true\n");
        assert!(legacy.privileged_access_enabled);

        let mut enabled = defaults;
        enabled.privileged_access_enabled = true;
        let serialized = enabled.serialize();
        assert!(serialized.starts_with("version=18\n"));
        assert!(serialized.contains("privileged-access-enabled=true\n"));
        assert!(ViewPreferences::parse(&serialized).privileged_access_enabled);
    }

    #[test]
    fn phase_20b1_sort_persistence_round_trips_and_migrates_hidden_last() {
        let legacy =
            ViewPreferences::parse("version=14\nsort-column=created\nsort-direction=descending\n");
        assert_eq!(legacy.sort.column, SortColumn::Created);
        assert_eq!(legacy.sort.direction, SortDirection::Descending);
        assert!(!legacy.sort.hidden_last);

        let path = PathBuf::from("/tmp/hidden-last-folder");
        let mut preferences = ViewPreferences {
            sort: DirectorySort::new(SortColumn::Accessed, SortDirection::Descending)
                .with_hidden_last(true),
            remember_per_folder: true,
            ..ViewPreferences::default()
        };
        preferences.remember_folder_state(
            path.clone(),
            FolderViewState {
                sort: DirectorySort::new(SortColumn::Comment, SortDirection::Ascending)
                    .with_directories(DirectoryPlacement::Last)
                    .with_hidden_last(true),
                ..FolderViewState::default()
            },
        );

        let serialized = preferences.serialize();
        assert!(serialized.starts_with("version=18\n"));
        assert!(serialized.contains("hidden-last=true\n"));
        let restored = ViewPreferences::parse(&serialized);
        assert!(restored.sort.hidden_last);
        let folder = restored.effective_state(&path);
        assert_eq!(folder.sort.column, SortColumn::Comment);
        assert_eq!(folder.sort.directories, DirectoryPlacement::Last);
        assert!(folder.sort.hidden_last);
    }
}
