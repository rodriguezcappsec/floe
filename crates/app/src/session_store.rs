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
    Save(BrowserTabs),
    Suppress,
}

pub struct SessionStoreWorker {
    sender: Option<SyncSender<StoreCommand>>,
    join: Option<JoinHandle<()>>,
    policy: SessionTracePolicy,
}

impl SessionStoreWorker {
    pub fn spawn(
        policy: SessionTracePolicy,
    ) -> io::Result<(Option<BrowserTabs>, SessionStoreWorker)> {
        let path = glib::user_config_dir().join("floe").join(SESSION_FILE_NAME);
        Self::spawn_at(path, policy)
    }

    fn spawn_at(
        path: PathBuf,
        policy: SessionTracePolicy,
    ) -> io::Result<(Option<BrowserTabs>, SessionStoreWorker)> {
        let (sender, receiver) = mpsc::sync_channel(SESSION_QUEUE_CAPACITY);
        let (startup_sender, startup_receiver) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("floe-session-store".to_owned())
            .spawn(move || {
                let restored = if policy.allows_persistence() {
                    match load_workspace(&path) {
                        Ok(workspace) => workspace,
                        Err(error) => {
                            tracing::warn!(%error, "browser session could not be restored");
                            None
                        }
                    }
                } else {
                    if let Err(error) = remove_session_file(&path) {
                        tracing::warn!(%error, "browser session trace could not be removed");
                    }
                    None
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
        let Some(sender) = self.sender.take() else {
            return Err(StoreError::Stopped);
        };
        let command = if self.policy.allows_persistence() {
            StoreCommand::Save(workspace)
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
}

fn load_workspace(path: &Path) -> io::Result<Option<BrowserTabs>> {
    let mut options = OpenOptions::new();
    options.read(true);
    options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
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
    BrowserTabs::decode_workspace(&bytes)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn save_workspace(path: &Path, workspace: &BrowserTabs) -> io::Result<()> {
    let bytes = workspace
        .encode_workspace()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
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
        assert!(load_workspace(&path).is_err());
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
}
