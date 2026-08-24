use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread::{self, JoinHandle},
};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use thiserror::Error;

use crate::view::{GridSize, ViewMode};

const PREFERENCE_QUEUE_CAPACITY: usize = 1;
const PREFERENCE_FILE_NAME: &str = "view-preferences.conf";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ViewPreferences {
    pub mode: ViewMode,
    pub grid_size: GridSize,
}

impl ViewPreferences {
    fn parse(contents: &str) -> Self {
        let mut preferences = Self::default();
        for line in contents.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "view" => {
                    if let Some(mode) = ViewMode::from_persisted(value.trim()) {
                        preferences.mode = mode;
                    }
                }
                "grid-size" => {
                    if let Ok(edge) = value.trim().parse::<u16>()
                        && let Some(size) = GridSize::from_persisted(edge)
                    {
                        preferences.grid_size = size;
                    }
                }
                _ => {}
            }
        }
        preferences
    }

    fn serialize(self) -> String {
        format!(
            "view={}\ngrid-size={}\n",
            self.mode.persisted(),
            self.grid_size.edge()
        )
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
                    if let Err(error) = persist_preferences(&path, preferences) {
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

fn persist_preferences(path: &Path, preferences: ViewPreferences) -> io::Result<()> {
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

    use tempfile::tempdir;

    use super::*;

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
        let directory = tempdir().expect("temporary directory should be created");
        let path = directory.path().join("nested").join(PREFERENCE_FILE_NAME);
        let (gate_sender, gate_receiver) = mpsc::channel();
        let (initial, worker) = PreferenceWorker::spawn_internal(path.clone(), Some(gate_receiver))
            .expect("preference worker should start");
        assert_eq!(initial, ViewPreferences::default());
        let preferences = ViewPreferences {
            mode: ViewMode::Grid,
            grid_size: GridSize::from_persisted(192).expect("grid size should be valid"),
        };
        worker
            .try_save(preferences)
            .expect("first preference save should enter bounded queue");
        assert!(matches!(
            worker.try_save(ViewPreferences::default()),
            Err(PreferenceSubmitError::Full(_))
        ));
        gate_sender.send(()).expect("worker should be released");
        drop(worker);

        let saved = fs::read_to_string(&path).expect("preference file should be written");
        assert_eq!(ViewPreferences::parse(&saved), preferences);
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn phase_6d_shutdown_submission_preserves_latest_full_queue_value() {
        let directory = tempdir().expect("temporary directory should be created");
        let path = directory.path().join(PREFERENCE_FILE_NAME);
        let (gate_sender, gate_receiver) = mpsc::channel();
        let (_, worker) = PreferenceWorker::spawn_internal(path.clone(), Some(gate_receiver))
            .expect("preference worker should start");
        worker
            .try_save(ViewPreferences::default())
            .expect("first preference save should fill the queue");
        let latest = ViewPreferences {
            mode: ViewMode::Grid,
            grid_size: GridSize::from_persisted(160).expect("grid size should be valid"),
        };
        gate_sender.send(()).expect("worker should be released");
        worker
            .save_before_shutdown(latest)
            .expect("shutdown save should wait only for bounded queue capacity");
        drop(worker);

        let saved = fs::read_to_string(path).expect("latest preferences should be written");
        assert_eq!(ViewPreferences::parse(&saved), latest);
    }
}
