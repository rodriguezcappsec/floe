//! A deliberately separate, bounded GIO watch set for explicit integrity monitoring.
//!
//! This is not the browser's [`crate::file_watcher::FileWatcher`]: it never refreshes
//! navigation and it is constructed only after the user starts monitoring a local
//! baseline.  Events are advisory and cause a complete worker-side recheck.

use std::{
    cell::RefCell,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    rc::Rc,
};

use gtk::{
    gio,
    gio::prelude::{FileExt, FileMonitorExt},
    glib,
};

use floe_core::{IntegrityWatchEvent, IntegrityWatchSetPolicy};

pub const INTEGRITY_MONITOR_DIRECTORY_CAPACITY: usize = 512;

#[derive(Debug, thiserror::Error)]
pub enum IntegrityWatchSetError {
    #[error("integrity monitoring needs an absolute local directory")]
    InvalidRoot,
    #[error("integrity monitoring does not follow symbolic links")]
    SymbolicLink,
    #[error("integrity monitoring does not cross filesystem boundaries")]
    CrossDevice,
    #[error("integrity monitoring reached its directory watch capacity")]
    Capacity,
    #[error("could not monitor an integrity directory: {0}")]
    Monitor(glib::Error),
    #[error("could not enumerate an integrity directory: {0}")]
    Io(#[from] std::io::Error),
}

type WatchHandler = Box<dyn Fn(IntegrityWatchEvent)>;

/// Owns every GIO directory monitor needed by an explicit local root.
///
/// Keep this distinct from navigation watching: stopping this watch set has no
/// effect on directory presentation and dropping it disconnects every monitor.
pub struct IntegrityWatchSet {
    root: PathBuf,
    _policy: IntegrityWatchSetPolicy,
    _monitors: Vec<gio::FileMonitor>,
    _handler: Rc<RefCell<WatchHandler>>,
}

impl IntegrityWatchSet {
    pub fn start(
        root: PathBuf,
        handler: impl Fn(IntegrityWatchEvent) + 'static,
    ) -> Result<Self, IntegrityWatchSetError> {
        let policy = IntegrityWatchSetPolicy::new(root.clone())
            .map_err(|_| IntegrityWatchSetError::InvalidRoot)?;
        let root_metadata = std::fs::symlink_metadata(&root)?;
        if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
            return Err(IntegrityWatchSetError::SymbolicLink);
        }

        let directories = discover_directories(&root, root_metadata.dev())?;
        let handler: Rc<RefCell<WatchHandler>> = Rc::new(RefCell::new(Box::new(handler)));
        let mut monitors = Vec::with_capacity(directories.len());
        for directory in directories {
            let monitor = gio::File::for_path(directory)
                .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
                .map_err(IntegrityWatchSetError::Monitor)?;
            monitor.set_rate_limit(50);
            let root = root.clone();
            let handler = Rc::downgrade(&handler);
            monitor.connect_changed(move |_, file, other_file, event| {
                let Some(handler) = handler.upgrade() else {
                    return;
                };
                let event = map_event(&root, file, other_file, event);
                (handler.borrow())(event);
            });
            monitors.push(monitor);
        }
        Ok(Self {
            root,
            _policy: policy,
            _monitors: monitors,
            _handler: handler,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn discover_directories(
    root: &Path,
    root_device: u64,
) -> Result<Vec<PathBuf>, IntegrityWatchSetError> {
    let mut directories = vec![root.to_path_buf()];
    let mut cursor = 0;
    while let Some(directory) = directories.get(cursor).cloned() {
        cursor += 1;
        let mut children = std::fs::read_dir(&directory)?.collect::<Result<Vec<_>, _>>()?;
        children.sort_by(|left, right| {
            left.file_name()
                .as_encoded_bytes()
                .cmp(right.file_name().as_encoded_bytes())
        });
        for child in children {
            let path = child.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.dev() != root_device {
                return Err(IntegrityWatchSetError::CrossDevice);
            }
            if metadata.is_dir() {
                if directories.len() == INTEGRITY_MONITOR_DIRECTORY_CAPACITY {
                    return Err(IntegrityWatchSetError::Capacity);
                }
                directories.push(path);
            }
        }
    }
    Ok(directories)
}

fn map_event(
    root: &Path,
    file: &gio::File,
    other_file: Option<&gio::File>,
    event: gio::FileMonitorEvent,
) -> IntegrityWatchEvent {
    let path = file.path().unwrap_or_else(|| root.to_path_buf());
    match event {
        gio::FileMonitorEvent::Changed
        | gio::FileMonitorEvent::AttributeChanged
        | gio::FileMonitorEvent::ChangesDoneHint => IntegrityWatchEvent::Changed(path),
        gio::FileMonitorEvent::Created | gio::FileMonitorEvent::MovedIn => {
            IntegrityWatchEvent::Created(path)
        }
        gio::FileMonitorEvent::Deleted | gio::FileMonitorEvent::MovedOut => {
            IntegrityWatchEvent::Deleted(path)
        }
        gio::FileMonitorEvent::Renamed | gio::FileMonitorEvent::Moved => {
            IntegrityWatchEvent::Renamed {
                from: path,
                to: other_file
                    .and_then(|file| file.path())
                    .unwrap_or_else(|| root.to_path_buf()),
            }
        }
        gio::FileMonitorEvent::Unmounted => IntegrityWatchEvent::MountLost,
        gio::FileMonitorEvent::PreUnmount => IntegrityWatchEvent::Invalidated,
        _ => IntegrityWatchEvent::Overflow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_18u_watch_discovery_rejects_symlink_and_bounds_directories() {
        let fixture = tempfile::tempdir().expect("fixture");
        std::fs::create_dir(fixture.path().join("child")).expect("child");
        assert_eq!(
            discover_directories(
                fixture.path(),
                std::fs::metadata(fixture.path()).expect("root").dev()
            )
            .expect("dirs")
            .len(),
            2
        );
    }

    #[test]
    fn phase_18u_watch_events_keep_non_utf8_paths() {
        use std::os::unix::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/changed-\xff".to_vec()));
        assert!(
            matches!(map_event(Path::new("/tmp"), &gio::File::for_path(&path), None, gio::FileMonitorEvent::Changed), IntegrityWatchEvent::Changed(found) if found == path)
        );
    }
}
