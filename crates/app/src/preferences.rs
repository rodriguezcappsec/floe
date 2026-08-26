use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread::{self, JoinHandle},
};

#[cfg(unix)]
use std::{
    ffi::OsString,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::OpenOptionsExt,
    },
};

use floe_core::{DirectoryGrouping, DirectoryPlacement, DirectorySort, SortColumn, SortDirection};
use thiserror::Error;

use crate::keybindings::KeybindingOverrides;
use crate::terminal::TerminalProviderId;
use crate::view::{
    FileViewDensity, FolderViewState, GridSize, ListColumnLayout, MillerColumnWidth, ViewMode,
};

const PREFERENCE_QUEUE_CAPACITY: usize = 1;
const PREFERENCE_FILE_NAME: &str = "view-preferences.conf";
pub const FOLDER_VIEW_CAPACITY: usize = 256;

/// The smallest useful sidebar width in Floe's compact density.
pub const SIDEBAR_WIDTH_MIN: u16 = 128;
/// Keeps the sidebar from starving the file view on ordinary desktop windows.
pub const SIDEBAR_WIDTH_MAX: u16 = 480;

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
    pub mode: ViewMode,
    pub grid_size: GridSize,
    pub sidebar_density: SidebarDensity,
    pub sidebar_width: Option<u16>,
    pub miller_column_width: MillerColumnWidth,
    pub inspector_width: MillerColumnWidth,
    pub file_density: FileViewDensity,
    pub sort: DirectorySort,
    pub columns: ListColumnLayout,
    pub remember_per_folder: bool,
    pub keybindings: KeybindingOverrides,
    pub vim_mode: bool,
    pub preferred_terminal: Option<TerminalProviderId>,
    folder_views: Vec<FolderViewOverride>,
}

impl Default for ViewPreferences {
    fn default() -> Self {
        let state = FolderViewState::default();
        Self {
            mode: state.mode,
            grid_size: state.grid_size,
            sidebar_density: SidebarDensity::default(),
            sidebar_width: None,
            miller_column_width: MillerColumnWidth::default(),
            inspector_width: MillerColumnWidth::default(),
            file_density: state.density,
            sort: state.sort,
            columns: state.columns,
            remember_per_folder: false,
            keybindings: KeybindingOverrides::default(),
            vim_mode: false,
            preferred_terminal: None,
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
                "columns" => {
                    preferences.columns = ListColumnLayout::parse_visible(value);
                }
                "column-widths" => preferences.columns.apply_widths_text(value),
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
            "version=7\nview={}\ngrid-size={}\nsidebar-density={}\nmiller-column-width={}\ninspector-width={}\nfile-density={}\nsort-column={}\nsort-direction={}\ndirectories={}\ngrouping={}\ncolumns={}\ncolumn-widths={}\nremember-per-folder={}\nvim-mode={}\n",
            self.mode.persisted(),
            self.grid_size.edge(),
            self.sidebar_density.persisted(),
            self.miller_column_width.get(),
            self.inspector_width.get(),
            self.file_density.persisted(),
            self.sort.column.persisted(),
            self.sort.direction.persisted(),
            self.sort.directories.persisted(),
            self.sort.grouping.persisted(),
            self.columns.visible_names(),
            self.columns.widths_text(),
            self.remember_per_folder,
            self.vim_mode,
        );
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
        "{path}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        state.mode.persisted(),
        state.grid_size.edge(),
        state.density.persisted(),
        state.sort.column.persisted(),
        state.sort.direction.persisted(),
        state.sort.directories.persisted(),
        state.sort.grouping.persisted(),
        state.columns.visible_names(),
        state.columns.widths_text(),
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
    let mut columns = ListColumnLayout::parse_visible(fields.next()?);
    columns.apply_widths_text(fields.next()?);
    if fields.next().is_some() {
        return None;
    }
    Some(FolderViewOverride {
        path,
        state: FolderViewState {
            mode,
            grid_size,
            density,
            sort: DirectorySort::new(column, direction)
                .with_directories(directories)
                .with_grouping(grouping),
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
    Full(ViewPreferences),
    #[error("preference worker is disconnected")]
    Disconnected,
}

pub struct PreferenceWorker {
    sender: Option<SyncSender<ViewPreferences>>,
    worker: Option<JoinHandle<()>>,
}

impl PreferenceWorker {
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
        let initial = load_preferences(&path);
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
            Err(TrySendError::Full(preferences)) => Err(PreferenceSubmitError::Full(preferences)),
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

fn load_preferences(path: &Path) -> ViewPreferences {
    match fs::read_to_string(path) {
        Ok(contents) => ViewPreferences::parse(&contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => ViewPreferences::default(),
        Err(error) => {
            tracing::warn!(%error, "could not read view preferences; using defaults");
            ViewPreferences::default()
        }
    }
}

fn persist_preferences(path: &Path, preferences: &ViewPreferences) -> io::Result<()> {
    let Some(parent) = path.parent() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "view preference path has no parent",
        ));
    };
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension("tmp");
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&temporary)?;
    file.write_all(preferences.serialize().as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[cfg(unix)]
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    use tempfile::tempdir;

    use super::*;
    use crate::view::ListColumn;

    #[test]
    fn phase_6d_preferences_parse_valid_fields_and_reject_invalid_values() {
        let parsed = ViewPreferences::parse("view=grid\ngrid-size=160\nunknown=value\n");
        assert_eq!(parsed.mode, ViewMode::Grid);
        assert_eq!(parsed.grid_size.edge(), 160);

        let invalid = ViewPreferences::parse("view=tiles\ngrid-size=100\n");
        assert_eq!(invalid, ViewPreferences::default());
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
        assert!(serialized.starts_with("version=7\n"));
        assert!(serialized.contains("miller-column-width=360\n"));
        assert!(serialized.contains("inspector-width=420\n"));
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
        assert!(serialized.starts_with("version=7\n"));
        assert!(serialized.contains("inspector-width=440\n"));
        assert_eq!(
            ViewPreferences::parse(&serialized).inspector_width,
            MillerColumnWidth::new(440)
        );
    }

    #[cfg(unix)]
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
}
