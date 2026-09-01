//! Private, atomic browser-workspace persistence outside GTK callbacks.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::mpsc::{self, SyncSender},
    thread::{self, JoinHandle},
};

use floe_core::{BrowserTabs, WORKSPACE_MAX_SERIALIZED_BYTES};
use thiserror::Error;

const SESSION_FILE_NAME: &str = "browser-session-v1.bin";
const SESSION_QUEUE_CAPACITY: usize = 1;
const WINDOW_SESSION_MAGIC: &[u8; 8] = b"FLOEWINS";
const WINDOW_SESSION_VERSION: u16 = 1;
pub const WINDOW_SESSION_CAPACITY: usize = 16;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SessionTracePolicy {
    #[default]
    Normal,
    Private,
    Sensitive,
}

impl SessionTracePolicy {
    pub fn from_environment() -> Self {
        match std::env::var("FLOE_SESSION_POLICY")
            .ok()
            .as_deref()
            .map(str::trim)
        {
            Some("private") => Self::Private,
            Some("sensitive") => Self::Sensitive,
            _ => Self::Normal,
        }
    }

    pub const fn allows_persistence(self) -> bool {
        matches!(self, Self::Normal)
    }
}

enum StoreCommand {
    Save(Vec<BrowserTabs>),
    Suppress,
}

pub struct SessionStoreWorker {
    sender: Option<SyncSender<StoreCommand>>,
    join: Option<JoinHandle<()>>,
    policy: SessionTracePolicy,
}

impl SessionStoreWorker {
    pub fn spawn_windows(
        policy: SessionTracePolicy,
    ) -> io::Result<(Vec<BrowserTabs>, SessionStoreWorker)> {
        let path = glib::user_config_dir().join("floe").join(SESSION_FILE_NAME);
        Self::spawn_windows_at(path, policy)
    }

    #[cfg(test)]
    fn spawn_at(
        path: PathBuf,
        policy: SessionTracePolicy,
    ) -> io::Result<(Option<BrowserTabs>, SessionStoreWorker)> {
        let (mut restored, worker) = Self::spawn_windows_at(path, policy)?;
        Ok((restored.drain(..).next(), worker))
    }

    fn spawn_windows_at(
        path: PathBuf,
        policy: SessionTracePolicy,
    ) -> io::Result<(Vec<BrowserTabs>, SessionStoreWorker)> {
        let (sender, receiver) = mpsc::sync_channel(SESSION_QUEUE_CAPACITY);
        let (startup_sender, startup_receiver) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("floe-session-store".to_owned())
            .spawn(move || {
                let restored = if policy.allows_persistence() {
                    match load_window_workspaces(&path) {
                        Ok(workspace) => workspace,
                        Err(error) => {
                            tracing::warn!(%error, "browser session could not be restored");
                            Vec::new()
                        }
                    }
                } else {
                    if let Err(error) = remove_session_file(&path) {
                        tracing::warn!(%error, "browser session trace could not be removed");
                    }
                    Vec::new()
                };
                if startup_sender.send(restored).is_err() {
                    return;
                }
                while let Ok(command) = receiver.recv() {
                    let command = receiver.try_iter().last().unwrap_or(command);
                    let result = match command {
                        StoreCommand::Save(workspace) => save_workspace(&path, &workspace),
                        StoreCommand::Suppress => remove_session_file(&path),
                    };
                    if let Err(error) = result {
                        tracing::warn!(%error, "browser session could not be saved");
                    }
                }
            })?;
        let restored = match startup_receiver.recv() {
            Ok(restored) => restored,
            Err(_) => {
                let _ = join.join();
                return Err(io::Error::other(
                    "session worker stopped before startup restoration completed",
                ));
            }
        };
        let worker = SessionStoreWorker {
            sender: Some(sender),
            join: Some(join),
            policy,
        };
        Ok((restored, worker))
    }

    pub fn save_before_shutdown(&mut self, workspace: BrowserTabs) -> Result<(), StoreError> {
        self.save_windows_before_shutdown(vec![workspace])
    }

    pub fn save_windows_before_shutdown(
        &mut self,
        workspaces: Vec<BrowserTabs>,
    ) -> Result<(), StoreError> {
        if workspaces.len() > WINDOW_SESSION_CAPACITY {
            return Err(StoreError::WindowCapacity(WINDOW_SESSION_CAPACITY));
        }
        let Some(sender) = self.sender.take() else {
            return Err(StoreError::Stopped);
        };
        let command = if self.policy.allows_persistence() {
            StoreCommand::Save(workspaces)
        } else {
            StoreCommand::Suppress
        };
        sender.send(command).map_err(|_| StoreError::Stopped)?;
        drop(sender);
        if let Some(join) = self.join.take() {
            join.join().map_err(|_| StoreError::WorkerPanicked)?;
        }
        Ok(())
    }
}

impl Drop for SessionStoreWorker {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("session worker has stopped")]
    Stopped,
    #[error("session worker panicked")]
    WorkerPanicked,
    #[error("the maximum {0} window sessions can be saved")]
    WindowCapacity(usize),
}

fn load_window_workspaces(path: &Path) -> io::Result<Vec<BrowserTabs>> {
    let mut options = OpenOptions::new();
    options.read(true);
    options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > WORKSPACE_MAX_SERIALIZED_BYTES as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "session file is not a bounded regular file",
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    Read::by_ref(&mut file)
        .take((WORKSPACE_MAX_SERIALIZED_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > WORKSPACE_MAX_SERIALIZED_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "session file exceeds the size limit",
        ));
    }
    decode_window_workspaces(&bytes)
}

fn save_workspace(path: &Path, workspaces: &[BrowserTabs]) -> io::Result<()> {
    let bytes = encode_window_workspaces(workspaces)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "session path has no parent"))?;
    fs::create_dir_all(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    let temporary = parent.join(format!(".{SESSION_FILE_NAME}.tmp-{}", std::process::id()));
    let write_result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true).mode(0o600);
        options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
        let mut file = options.open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}

fn encode_window_workspaces(workspaces: &[BrowserTabs]) -> io::Result<Vec<u8>> {
    if workspaces.is_empty() || workspaces.len() > WINDOW_SESSION_CAPACITY {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "window session count is outside the supported range",
        ));
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(WINDOW_SESSION_MAGIC);
    bytes.extend_from_slice(&WINDOW_SESSION_VERSION.to_le_bytes());
    bytes.extend_from_slice(&(workspaces.len() as u16).to_le_bytes());
    for workspace in workspaces {
        let encoded = workspace
            .encode_workspace()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let length = u32::try_from(encoded.len()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "window workspace is too large")
        })?;
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes.extend_from_slice(&encoded);
        if bytes.len() > WORKSPACE_MAX_SERIALIZED_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "multi-window session exceeds the size limit",
            ));
        }
    }
    Ok(bytes)
}

fn decode_window_workspaces(bytes: &[u8]) -> io::Result<Vec<BrowserTabs>> {
    if !bytes.starts_with(WINDOW_SESSION_MAGIC) {
        return BrowserTabs::decode_workspace(bytes)
            .map(|workspace| vec![workspace])
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error));
    }
    if bytes.len() < 12 || bytes.len() > WORKSPACE_MAX_SERIALIZED_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "multi-window session has an invalid size",
        ));
    }
    let version = u16::from_le_bytes([bytes[8], bytes[9]]);
    if version != WINDOW_SESSION_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("multi-window session version {version} is unsupported"),
        ));
    }
    let count = usize::from(u16::from_le_bytes([bytes[10], bytes[11]]));
    if count == 0 || count > WINDOW_SESSION_CAPACITY {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "multi-window session count is invalid",
        ));
    }
    let mut cursor = 12usize;
    let mut workspaces = Vec::with_capacity(count);
    for _ in 0..count {
        let length_end = cursor
            .checked_add(4)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "session length overflow"))?;
        let length_bytes: [u8; 4] = bytes
            .get(cursor..length_end)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "truncated session"))?
            .try_into()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid session length"))?;
        cursor = length_end;
        let length = usize::try_from(u32::from_le_bytes(length_bytes))
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid workspace length"))?;
        if length == 0 || length > WORKSPACE_MAX_SERIALIZED_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "window workspace length is invalid",
            ));
        }
        let end = cursor
            .checked_add(length)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "session length overflow"))?;
        let workspace = BrowserTabs::decode_workspace(bytes.get(cursor..end).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "truncated window workspace")
        })?)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        workspaces.push(workspace);
        cursor = end;
    }
    if cursor != bytes.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "multi-window session has trailing bytes",
        ));
    }
    Ok(workspaces)
}

fn remove_session_file(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => {
            if let Some(parent) = path.parent() {
                File::open(parent)?.sync_all()?;
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use std::{os::unix::fs::PermissionsExt, path::PathBuf};

    use floe_core::{FolderViewState, TabActivation};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn phase_7c_session_store_is_private_atomic_and_round_trips() {
        let fixture = tempdir().expect("temporary directory");
        let path = fixture.path().join("config/floe/browser-session-v1.bin");
        let (_, mut worker) =
            SessionStoreWorker::spawn_at(path.clone(), SessionTracePolicy::Normal)
                .expect("store worker");
        let mut tabs = BrowserTabs::new(PathBuf::from("/one"), FolderViewState::default())
            .expect("initial tabs");
        tabs.open(
            PathBuf::from("/two"),
            FolderViewState::default(),
            TabActivation::Foreground,
        )
        .expect("second tab");
        worker
            .save_before_shutdown(tabs.clone())
            .expect("shutdown save");
        assert_eq!(
            fs::metadata(path.parent().expect("parent"))
                .expect("directory metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path)
                .expect("file metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let (restored, mut reader) =
            SessionStoreWorker::spawn_at(path, SessionTracePolicy::Normal).expect("reader worker");
        assert_eq!(restored.expect("restored tabs").len(), 2);
        reader.save_before_shutdown(tabs).expect("reader shutdown");
    }

    #[test]
    fn phase_7c_session_store_corruption_and_symlinks_fall_back_safely() {
        let fixture = tempdir().expect("temporary directory");
        let path = fixture.path().join("session.bin");
        fs::write(&path, b"corrupt").expect("corrupt fixture");
        let (restored, mut worker) =
            SessionStoreWorker::spawn_at(path.clone(), SessionTracePolicy::Normal)
                .expect("worker still starts");
        assert!(restored.is_none());
        worker
            .save_before_shutdown(
                BrowserTabs::new(PathBuf::from("/"), FolderViewState::default())
                    .expect("fallback tabs"),
            )
            .expect("save replacement");
        let target = fixture.path().join("target");
        fs::write(&target, b"data").expect("target fixture");
        fs::remove_file(&path).expect("remove session");
        std::os::unix::fs::symlink(&target, &path).expect("symlink fixture");
        assert!(load_window_workspaces(&path).is_err());
    }

    #[test]
    fn phase_7c_session_privacy_private_and_sensitive_remove_owned_trace() {
        for policy in [SessionTracePolicy::Private, SessionTracePolicy::Sensitive] {
            let fixture = tempdir().expect("temporary directory");
            let path = fixture.path().join("session.bin");
            fs::write(&path, b"old session").expect("old session fixture");
            let (restored, mut worker) =
                SessionStoreWorker::spawn_at(path.clone(), policy).expect("suppressed worker");
            assert!(restored.is_none());
            worker
                .save_before_shutdown(
                    BrowserTabs::new(PathBuf::from("/"), FolderViewState::default())
                        .expect("fallback tabs"),
                )
                .expect("suppression shutdown");
            assert!(!path.exists());
        }
    }

    #[test]
    fn phase_23h_session_round_trips_multiple_windows_and_migrates_legacy() {
        let one = BrowserTabs::new(PathBuf::from("/one"), FolderViewState::default())
            .expect("first window");
        let two = BrowserTabs::new(PathBuf::from("/two"), FolderViewState::default())
            .expect("second window");
        let bytes = encode_window_workspaces(&[one.clone(), two.clone()]).expect("encode windows");
        let decoded = decode_window_workspaces(&bytes).expect("decode windows");
        assert_eq!(decoded, vec![one.clone(), two]);

        let legacy = one.encode_workspace().expect("legacy one-window bytes");
        assert_eq!(
            decode_window_workspaces(&legacy).expect("legacy migration"),
            vec![one]
        );
    }

    #[test]
    fn phase_23h_session_rejects_counts_trailing_bytes_and_private_restore() {
        let tabs =
            BrowserTabs::new(PathBuf::from("/"), FolderViewState::default()).expect("window");
        let too_many = vec![tabs.clone(); WINDOW_SESSION_CAPACITY + 1];
        assert!(encode_window_workspaces(&too_many).is_err());

        let mut trailing =
            encode_window_workspaces(std::slice::from_ref(&tabs)).expect("encoded window");
        trailing.push(0);
        assert!(decode_window_workspaces(&trailing).is_err());

        let fixture = tempdir().expect("temporary directory");
        let path = fixture.path().join("session.bin");
        fs::write(
            &path,
            encode_window_workspaces(&[tabs]).expect("stored windows"),
        )
        .expect("write session");
        let (restored, mut worker) =
            SessionStoreWorker::spawn_windows_at(path.clone(), SessionTracePolicy::Private)
                .expect("private worker");
        assert!(restored.is_empty());
        worker
            .save_windows_before_shutdown(Vec::new())
            .expect("suppressed shutdown");
        assert!(!path.exists());
    }
}
