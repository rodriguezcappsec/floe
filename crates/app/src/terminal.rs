//! Bounded, no-shell terminal-provider discovery and launch worker.

use std::{
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
    thread::{self, JoinHandle},
    time::Duration,
};

use floe_core::DirectoryEntry;
use thiserror::Error;

pub const TERMINAL_PROVIDER_CAPACITY: usize = 9;
pub const TERMINAL_REQUEST_CAPACITY: usize = 4;
pub const TERMINAL_RESULT_CAPACITY: usize = 8;
pub const TERMINAL_CHILD_CAPACITY: usize = 32;
pub const TERMINAL_PATH_COMPONENT_CAPACITY: usize = 256;
pub const TERMINAL_TARGET_BYTE_CAPACITY: usize = 1_048_576;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TerminalProviderId {
    XdgTerminalExec,
    Ptyxis,
    Kgx,
    Foot,
    Kitty,
    Alacritty,
    WezTerm,
    Konsole,
    Xterm,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalProvider {
    pub id: TerminalProviderId,
    pub name: &'static str,
    pub executable: &'static str,
}

pub const TERMINAL_PROVIDERS: [TerminalProvider; TERMINAL_PROVIDER_CAPACITY] = [
    TerminalProvider {
        id: TerminalProviderId::XdgTerminalExec,
        name: "System Default (xdg-terminal-exec)",
        executable: "xdg-terminal-exec",
    },
    TerminalProvider {
        id: TerminalProviderId::Ptyxis,
        name: "Ptyxis",
        executable: "ptyxis",
    },
    TerminalProvider {
        id: TerminalProviderId::Kgx,
        name: "GNOME Console",
        executable: "kgx",
    },
    TerminalProvider {
        id: TerminalProviderId::Foot,
        name: "foot",
        executable: "foot",
    },
    TerminalProvider {
        id: TerminalProviderId::Kitty,
        name: "kitty",
        executable: "kitty",
    },
    TerminalProvider {
        id: TerminalProviderId::Alacritty,
        name: "Alacritty",
        executable: "alacritty",
    },
    TerminalProvider {
        id: TerminalProviderId::WezTerm,
        name: "WezTerm",
        executable: "wezterm-gui",
    },
    TerminalProvider {
        id: TerminalProviderId::Konsole,
        name: "Konsole",
        executable: "konsole",
    },
    TerminalProvider {
        id: TerminalProviderId::Xterm,
        name: "XTerm",
        executable: "xterm",
    },
];

impl TerminalProviderId {
    pub const fn persisted(self) -> &'static str {
        match self {
            Self::XdgTerminalExec => "xdg-terminal-exec",
            Self::Ptyxis => "ptyxis",
            Self::Kgx => "kgx",
            Self::Foot => "foot",
            Self::Kitty => "kitty",
            Self::Alacritty => "alacritty",
            Self::WezTerm => "wezterm",
            Self::Konsole => "konsole",
            Self::Xterm => "xterm",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        TERMINAL_PROVIDERS
            .iter()
            .find(|provider| provider.id.persisted() == value)
            .map(|provider| provider.id)
    }

    pub fn definition(self) -> &'static TerminalProvider {
        TERMINAL_PROVIDERS
            .iter()
            .find(|provider| provider.id == self)
            .expect("every provider ID has one reviewed definition")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalAvailability {
    pub id: TerminalProviderId,
    pub available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DiscoveredTerminal {
    provider: &'static TerminalProvider,
    executable: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalLaunchRequest {
    pub id: u64,
    pub target: PathBuf,
    pub preferred: Option<TerminalProviderId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalLaunchSuccess {
    pub id: u64,
    pub provider: TerminalProviderId,
    pub preferred_unavailable: bool,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum TerminalLaunchError {
    #[error("Trash is not a local terminal working directory.")]
    Trash,
    #[error("The terminal target must be an absolute local path.")]
    NonLocal,
    #[error("The terminal target exceeds Floe's path limit.")]
    TargetTooLong,
    #[error("The terminal target no longer exists or is not a directory.")]
    InvalidTarget,
    #[error("No reviewed terminal application is available.")]
    NoProvider,
    #[error("Too many terminal processes are still being tracked.")]
    TooManyChildren,
    #[error("The terminal application could not be started: {0}")]
    Spawn(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalEvent {
    Discovery(Vec<TerminalAvailability>),
    Launch(Result<TerminalLaunchSuccess, TerminalLaunchError>),
}

enum TerminalRequest {
    Discover,
    Launch(TerminalLaunchRequest),
}

#[derive(Debug, Error)]
pub enum TerminalSubmitError {
    #[error("terminal worker queue is full")]
    Full,
    #[error("terminal worker disconnected")]
    Disconnected,
}

pub struct TerminalWorker {
    sender: Option<SyncSender<TerminalRequest>>,
    events: Receiver<TerminalEvent>,
    worker: Option<JoinHandle<()>>,
}

impl TerminalWorker {
    pub fn spawn() -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(TERMINAL_REQUEST_CAPACITY);
        let (event_sender, events) = mpsc::sync_channel(TERMINAL_RESULT_CAPACITY);
        let worker = thread::Builder::new()
            .name("floe-terminal-launcher".to_owned())
            .spawn(move || terminal_worker_loop(receiver, event_sender))?;
        Ok(Self {
            sender: Some(sender),
            events,
            worker: Some(worker),
        })
    }

    pub fn try_discover(&self) -> Result<(), TerminalSubmitError> {
        self.try_send(TerminalRequest::Discover)
    }

    pub fn try_launch(&self, request: TerminalLaunchRequest) -> Result<(), TerminalSubmitError> {
        self.try_send(TerminalRequest::Launch(request))
    }

    pub fn try_event(&self) -> Option<TerminalEvent> {
        self.events.try_recv().ok()
    }

    fn try_send(&self, request: TerminalRequest) -> Result<(), TerminalSubmitError> {
        let Some(sender) = self.sender.as_ref() else {
            return Err(TerminalSubmitError::Disconnected);
        };
        sender.try_send(request).map_err(|error| match error {
            TrySendError::Full(_) => TerminalSubmitError::Full,
            TrySendError::Disconnected(_) => TerminalSubmitError::Disconnected,
        })
    }
}

impl Drop for TerminalWorker {
    fn drop(&mut self) {
        self.sender.take();
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("terminal worker panicked during shutdown");
        }
    }
}

pub fn terminal_target(
    current_directory: &Path,
    selected: &[std::sync::Arc<DirectoryEntry>],
    trash_active: bool,
) -> Result<PathBuf, TerminalLaunchError> {
    if trash_active {
        return Err(TerminalLaunchError::Trash);
    }
    let target = if selected.len() == 1 && selected[0].is_navigable_directory() {
        selected[0].path()
    } else {
        current_directory
    };
    validate_target_shape(target)?;
    Ok(target.to_path_buf())
}

pub fn provider_choices(availability: &[TerminalAvailability]) -> Vec<TerminalAvailability> {
    TERMINAL_PROVIDERS
        .iter()
        .map(|provider| TerminalAvailability {
            id: provider.id,
            available: availability
                .iter()
                .any(|item| item.id == provider.id && item.available),
        })
        .collect()
}

fn terminal_worker_loop(
    receiver: Receiver<TerminalRequest>,
    event_sender: SyncSender<TerminalEvent>,
) {
    let mut children = Vec::<Child>::new();
    loop {
        reap_children(&mut children);
        match receiver.recv_timeout(Duration::from_millis(250)) {
            Ok(TerminalRequest::Discover) => {
                let discovered = discover_terminals(std::env::var_os("PATH").as_deref());
                let event = TerminalEvent::Discovery(
                    TERMINAL_PROVIDERS
                        .iter()
                        .map(|provider| TerminalAvailability {
                            id: provider.id,
                            available: discovered
                                .iter()
                                .any(|item| item.provider.id == provider.id),
                        })
                        .collect(),
                );
                if event_sender.try_send(event).is_err() {
                    tracing::warn!("terminal discovery result queue is unavailable");
                }
            }
            Ok(TerminalRequest::Launch(request)) => {
                let result = launch_request(request, &mut children);
                if event_sender
                    .try_send(TerminalEvent::Launch(result))
                    .is_err()
                {
                    tracing::warn!("terminal launch result queue is unavailable");
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn launch_request(
    request: TerminalLaunchRequest,
    children: &mut Vec<Child>,
) -> Result<TerminalLaunchSuccess, TerminalLaunchError> {
    validate_target_shape(&request.target)?;
    if !fs::metadata(&request.target).is_ok_and(|metadata| metadata.is_dir()) {
        return Err(TerminalLaunchError::InvalidTarget);
    }
    reap_children(children);
    if children.len() >= TERMINAL_CHILD_CAPACITY {
        return Err(TerminalLaunchError::TooManyChildren);
    }
    let discovered = discover_terminals(std::env::var_os("PATH").as_deref());
    let (selected, preferred_unavailable) =
        select_discovered_terminal(&discovered, request.preferred)?;
    let child = spawn_terminal(&selected.executable, &request.target)
        .map_err(|error| TerminalLaunchError::Spawn(error.to_string()))?;
    children.push(child);
    Ok(TerminalLaunchSuccess {
        id: request.id,
        provider: selected.provider.id,
        preferred_unavailable,
    })
}

fn select_discovered_terminal(
    discovered: &[DiscoveredTerminal],
    preferred: Option<TerminalProviderId>,
) -> Result<(&DiscoveredTerminal, bool), TerminalLaunchError> {
    let selected = preferred
        .and_then(|preferred| discovered.iter().find(|item| item.provider.id == preferred))
        .or_else(|| discovered.first())
        .ok_or(TerminalLaunchError::NoProvider)?;
    Ok((
        selected,
        preferred.is_some_and(|preferred| preferred != selected.provider.id),
    ))
}

fn discover_terminals(path: Option<&OsStr>) -> Vec<DiscoveredTerminal> {
    let directories = path
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .filter(|directory| directory.is_absolute())
        .take(TERMINAL_PATH_COMPONENT_CAPACITY)
        .collect::<Vec<_>>();
    TERMINAL_PROVIDERS
        .iter()
        .filter_map(|provider| {
            directories.iter().find_map(|directory| {
                let executable = directory.join(provider.executable);
                executable_is_runnable(&executable).then_some(DiscoveredTerminal {
                    provider,
                    executable,
                })
            })
        })
        .collect()
}

fn executable_is_runnable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
    }
}

fn spawn_terminal(executable: &Path, target: &Path) -> io::Result<Child> {
    Command::new(executable)
        .current_dir(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

fn validate_target_shape(path: &Path) -> Result<(), TerminalLaunchError> {
    if !path.is_absolute() {
        return Err(TerminalLaunchError::NonLocal);
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        if path.as_os_str().as_bytes().len() > TERMINAL_TARGET_BYTE_CAPACITY {
            return Err(TerminalLaunchError::TargetTooLong);
        }
    }
    #[cfg(not(unix))]
    if path.as_os_str().to_string_lossy().len() > TERMINAL_TARGET_BYTE_CAPACITY {
        return Err(TerminalLaunchError::TargetTooLong);
    }
    Ok(())
}

fn reap_children(children: &mut Vec<Child>) {
    children.retain_mut(|child| child.try_wait().ok().flatten().is_none());
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn phase_11e_terminal_policy_is_bounded_selection_aware_and_local() {
        let root = tempdir().expect("temporary directory");
        let current = root.path().to_path_buf();
        fs::create_dir(current.join("folder")).expect("folder fixture");
        fs::write(current.join("file"), b"fixture").expect("file fixture");
        let listing = floe_core::enumerate_directory(&current).expect("listing fixture");
        let folder = listing
            .entries()
            .iter()
            .find(|entry| entry.path() == current.join("folder"))
            .cloned()
            .expect("folder entry");
        let file = listing
            .entries()
            .iter()
            .find(|entry| entry.path() == current.join("file"))
            .cloned()
            .expect("file entry");
        assert_eq!(terminal_target(&current, &[], false), Ok(current.clone()));
        assert_eq!(
            terminal_target(&current, &[folder.into()], false),
            Ok(current.join("folder"))
        );
        assert_eq!(
            terminal_target(&current, &[file.into()], false),
            Ok(current.clone())
        );
        assert_eq!(
            terminal_target(&current, &[], true),
            Err(TerminalLaunchError::Trash)
        );
        assert_eq!(
            terminal_target(Path::new("relative"), &[], false),
            Err(TerminalLaunchError::NonLocal)
        );
        assert_eq!(TERMINAL_PROVIDERS.len(), TERMINAL_PROVIDER_CAPACITY);
    }

    #[cfg(unix)]
    #[test]
    fn phase_11e_terminal_worker_preserves_hostile_and_non_utf8_working_directory() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};

        let root = tempdir().expect("temporary directory");
        let target = root.path().join(OsString::from_vec(vec![
            b's', b'p', b'a', b'c', b'e', b' ', b'$', 0x80,
        ]));
        fs::create_dir(&target).expect("hostile directory fixture");
        let executable = Path::new("/bin/true");
        assert!(executable_is_runnable(executable));
        let mut child = spawn_terminal(executable, &target).expect("direct argv spawn");
        assert!(child.wait().expect("wait fixture").success());
        assert_eq!(validate_target_shape(&target), Ok(()));
    }

    #[test]
    fn phase_11e_terminal_worker_queue_is_nonblocking_and_bounded() {
        let worker = TerminalWorker::spawn().expect("worker");
        let started = Instant::now();
        let mut accepted = 0;
        for _ in 0..TERMINAL_REQUEST_CAPACITY + 8 {
            match worker.try_discover() {
                Ok(()) => accepted += 1,
                Err(TerminalSubmitError::Full) => break,
                Err(TerminalSubmitError::Disconnected) => panic!("worker disconnected"),
            }
        }
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(accepted > 0);
    }

    #[test]
    fn phase_11e_terminal_policy_provider_persistence_is_stable() {
        for provider in TERMINAL_PROVIDERS {
            assert_eq!(
                TerminalProviderId::from_persisted(provider.id.persisted()),
                Some(provider.id)
            );
            assert!(!provider.name.is_empty());
            assert!(!provider.executable.contains(char::is_whitespace));
        }
        assert_eq!(TerminalProviderId::from_persisted("sh -c"), None);

        let discovered = [DiscoveredTerminal {
            provider: TerminalProviderId::Foot.definition(),
            executable: PathBuf::from("/usr/bin/foot"),
        }];
        let (selected, fallback) =
            select_discovered_terminal(&discovered, Some(TerminalProviderId::Konsole))
                .expect("reviewed fallback");
        assert_eq!(selected.provider.id, TerminalProviderId::Foot);
        assert!(fallback);
        assert_eq!(
            select_discovered_terminal(&[], None),
            Err(TerminalLaunchError::NoProvider)
        );
    }

    #[test]
    fn phase_11e_terminal_preferences_migrate_and_round_trip_reviewed_provider() {
        let legacy = crate::preferences::ViewPreferences::parse("version=6\nview=list\n");
        assert_eq!(legacy.preferred_terminal, None);
        let mut preferences = legacy;
        preferences.preferred_terminal = Some(TerminalProviderId::Konsole);
        let serialized = preferences.serialize();
        assert!(serialized.starts_with("version=18\n"));
        assert!(serialized.contains("preferred-terminal=konsole\n"));
        assert_eq!(
            crate::preferences::ViewPreferences::parse(&serialized).preferred_terminal,
            Some(TerminalProviderId::Konsole)
        );
        assert_eq!(
            crate::preferences::ViewPreferences::parse("preferred-terminal=sh -c\n")
                .preferred_terminal,
            None
        );
    }
}
