use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use gtk::{gio, glib, prelude::*};
use thiserror::Error;

const COALESCE_DELAY: Duration = Duration::from_millis(140);
const MONITOR_RATE_LIMIT_MS: i32 = 50;
const MAX_EVENTS_PER_BATCH: usize = 16_384;
const MAX_PATHS_PER_BATCH: usize = 4_096;
const MAX_RENAMES_PER_BATCH: usize = 1_024;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WatchChangeKind {
    Content,
    Created,
    Deleted,
    Attributes,
    Renamed,
    MovedIn,
    MovedOut,
    Invalidated,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct WatchChange {
    kind: WatchChangeKind,
    path: PathBuf,
    other_path: Option<PathBuf>,
}

impl WatchChange {
    fn new(kind: WatchChangeKind, path: PathBuf, other_path: Option<PathBuf>) -> Self {
        Self {
            kind,
            path,
            other_path,
        }
    }

    #[cfg(test)]
    pub const fn kind(&self) -> WatchChangeKind {
        self.kind
    }

    #[cfg(test)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[cfg(test)]
    pub fn other_path(&self) -> Option<&Path> {
        self.other_path.as_deref()
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RenamePair {
    from: PathBuf,
    to: PathBuf,
}

impl RenamePair {
    pub fn from(&self) -> &Path {
        &self.from
    }

    pub fn to(&self) -> &Path {
        &self.to
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WatchBatch {
    generation: u64,
    directory: PathBuf,
    changed_paths: Vec<PathBuf>,
    renames: Vec<RenamePair>,
    event_count: usize,
    overflowed: bool,
}

impl WatchBatch {
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn changed_paths(&self) -> &[PathBuf] {
        &self.changed_paths
    }

    pub fn renames(&self) -> &[RenamePair] {
        &self.renames
    }

    pub const fn event_count(&self) -> usize {
        self.event_count
    }

    pub const fn overflowed(&self) -> bool {
        self.overflowed
    }
}

#[derive(Default)]
struct EventAccumulator {
    changed_paths: Vec<PathBuf>,
    changed_seen: HashSet<PathBuf>,
    renames: Vec<RenamePair>,
    rename_seen: HashSet<RenamePair>,
    event_count: usize,
    overflowed: bool,
}

impl EventAccumulator {
    fn record(&mut self, change: WatchChange) {
        self.event_count = self.event_count.saturating_add(1);
        if self.event_count > MAX_EVENTS_PER_BATCH {
            self.overflowed = true;
            return;
        }
        self.record_path(change.path.clone());
        if let Some(other) = change.other_path {
            self.record_path(other.clone());
            if matches!(change.kind, WatchChangeKind::Renamed)
                && self.renames.len() < MAX_RENAMES_PER_BATCH
            {
                let rename = RenamePair {
                    from: change.path,
                    to: other,
                };
                if self.rename_seen.insert(rename.clone()) {
                    self.renames.push(rename);
                }
            } else if matches!(change.kind, WatchChangeKind::Renamed) {
                self.overflowed = true;
            }
        }
    }

    fn record_path(&mut self, path: PathBuf) {
        if self.changed_seen.contains(&path) {
            return;
        }
        if self.changed_paths.len() >= MAX_PATHS_PER_BATCH {
            self.overflowed = true;
            return;
        }
        self.changed_seen.insert(path.clone());
        self.changed_paths.push(path);
    }

    fn take_batch(&mut self, generation: u64, directory: PathBuf) -> Option<WatchBatch> {
        if self.event_count == 0 {
            return None;
        }
        let pending = std::mem::take(self);
        Some(WatchBatch {
            generation,
            directory,
            changed_paths: pending.changed_paths,
            renames: pending.renames,
            event_count: pending.event_count,
            overflowed: pending.overflowed,
        })
    }
}

type WatchHandler = Rc<dyn Fn(WatchBatch)>;

#[derive(Default)]
struct WatcherInner {
    monitor: Option<gio::FileMonitor>,
    generation: u64,
    directory: Option<PathBuf>,
    pending: EventAccumulator,
    flush_source: Option<glib::SourceId>,
    handler: Option<WatchHandler>,
}

#[derive(Clone, Default)]
pub struct FileWatcher {
    inner: Rc<RefCell<WatcherInner>>,
}

impl FileWatcher {
    pub fn bind(&self, handler: impl Fn(WatchBatch) + 'static) {
        self.inner.borrow_mut().handler = Some(Rc::new(handler));
    }

    pub fn watch_directory(&self, directory: PathBuf) -> Result<u64, WatchStartError> {
        if !directory.is_absolute() {
            return Err(WatchStartError::InvalidDirectory(directory));
        }
        self.stop();
        let generation = {
            let inner = self.inner.borrow();
            inner.generation.wrapping_add(1).max(1)
        };
        let file = gio::File::for_path(&directory);
        let monitor = file
            .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
            .map_err(WatchStartError::Monitor)?;
        monitor.set_rate_limit(MONITOR_RATE_LIMIT_MS);

        let weak_inner = Rc::downgrade(&self.inner);
        monitor.connect_changed(move |_, file, other_file, event| {
            let Some(inner) = weak_inner.upgrade() else {
                return;
            };
            let change = map_monitor_event(file, other_file, event);
            let should_schedule = {
                let mut inner = inner.borrow_mut();
                if inner.generation != generation {
                    return;
                }
                match change {
                    Some(change) => inner.pending.record(change),
                    None if !matches!(event, gio::FileMonitorEvent::ChangesDoneHint) => {
                        inner.pending.event_count = inner.pending.event_count.saturating_add(1);
                        inner.pending.overflowed = true;
                    }
                    None => return,
                }
                inner.flush_source.is_none()
            };
            if should_schedule {
                schedule_flush(&inner, generation);
            }
        });

        let mut inner = self.inner.borrow_mut();
        inner.generation = generation;
        inner.directory = Some(directory);
        inner.monitor = Some(monitor);
        Ok(generation)
    }

    pub fn stop(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.generation = inner.generation.wrapping_add(1).max(1);
        if let Some(source) = inner.flush_source.take() {
            source.remove();
        }
        if let Some(monitor) = inner.monitor.take() {
            monitor.cancel();
        }
        inner.directory = None;
        inner.pending = EventAccumulator::default();
    }

    pub fn generation(&self) -> u64 {
        self.inner.borrow().generation
    }

    #[cfg(test)]
    pub fn directory(&self) -> Option<PathBuf> {
        self.inner.borrow().directory.clone()
    }
}

impl Drop for FileWatcher {
    fn drop(&mut self) {
        if Rc::strong_count(&self.inner) == 1 {
            self.stop();
        }
    }
}

fn schedule_flush(inner: &Rc<RefCell<WatcherInner>>, generation: u64) {
    let weak_inner = Rc::downgrade(inner);
    let source = glib::timeout_add_local_once(COALESCE_DELAY, move || {
        let Some(inner) = weak_inner.upgrade() else {
            return;
        };
        let (handler, batch) = {
            let mut inner = inner.borrow_mut();
            inner.flush_source.take();
            if inner.generation != generation {
                return;
            }
            let Some(directory) = inner.directory.clone() else {
                return;
            };
            let batch = inner.pending.take_batch(generation, directory);
            (inner.handler.clone(), batch)
        };
        if let (Some(handler), Some(batch)) = (handler, batch) {
            handler(batch);
        }
    });
    inner.borrow_mut().flush_source = Some(source);
}

fn map_monitor_event(
    file: &gio::File,
    other_file: Option<&gio::File>,
    event: gio::FileMonitorEvent,
) -> Option<WatchChange> {
    let path = file.path()?;
    let other_path = other_file.and_then(gio::File::path);
    let kind = match event {
        gio::FileMonitorEvent::Changed => WatchChangeKind::Content,
        gio::FileMonitorEvent::Created => WatchChangeKind::Created,
        gio::FileMonitorEvent::Deleted => WatchChangeKind::Deleted,
        gio::FileMonitorEvent::AttributeChanged => WatchChangeKind::Attributes,
        gio::FileMonitorEvent::Moved | gio::FileMonitorEvent::Renamed => WatchChangeKind::Renamed,
        gio::FileMonitorEvent::MovedIn => WatchChangeKind::MovedIn,
        gio::FileMonitorEvent::MovedOut => WatchChangeKind::MovedOut,
        gio::FileMonitorEvent::PreUnmount | gio::FileMonitorEvent::Unmounted => {
            WatchChangeKind::Invalidated
        }
        gio::FileMonitorEvent::ChangesDoneHint => return None,
        _ => return None,
    };
    Some(WatchChange::new(kind, path, other_path))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewStateSnapshot {
    pub selected_paths: Vec<PathBuf>,
    pub anchor_path: Option<PathBuf>,
    pub anchor_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconciledViewState {
    pub selected_paths: Vec<PathBuf>,
    pub anchor_index: Option<usize>,
}

pub fn reconcile_view_state(
    snapshot: ViewStateSnapshot,
    renames: &[RenamePair],
    current_paths: &[PathBuf],
) -> ReconciledViewState {
    let current_indices = current_paths
        .iter()
        .enumerate()
        .map(|(index, path)| (path.as_path(), index))
        .collect::<HashMap<_, _>>();
    let rename_map = renames
        .iter()
        .map(|rename| (rename.from(), rename.to()))
        .collect::<HashMap<_, _>>();
    let translate = |path: &Path| {
        let mut current = path;
        let mut seen = HashSet::with_capacity(rename_map.len().min(8));
        while seen.insert(current) {
            let Some(next) = rename_map.get(current).copied() else {
                break;
            };
            current = next;
        }
        current.to_path_buf()
    };

    let mut selected_seen = HashSet::with_capacity(snapshot.selected_paths.len());
    let selected_paths = snapshot
        .selected_paths
        .iter()
        .filter_map(|path| {
            let translated = translate(path);
            current_indices
                .get(translated.as_path())
                .map(|index| current_paths[*index].clone())
        })
        .filter(|path| selected_seen.insert(path.clone()))
        .collect();
    let anchor_index = snapshot
        .anchor_path
        .as_deref()
        .map(translate)
        .and_then(|path| current_indices.get(path.as_path()).copied())
        .or_else(|| {
            (!current_paths.is_empty()).then(|| snapshot.anchor_index.min(current_paths.len() - 1))
        });
    ReconciledViewState {
        selected_paths,
        anchor_index,
    }
}

pub fn scroll_anchor_index(
    value: f64,
    lower: f64,
    upper: f64,
    page_size: f64,
    entry_count: usize,
) -> usize {
    if entry_count <= 1 {
        return 0;
    }
    let scrollable = (upper - page_size - lower).max(0.0);
    if scrollable == 0.0 {
        return 0;
    }
    let ratio = ((value - lower) / scrollable).clamp(0.0, 1.0);
    (ratio * (entry_count - 1) as f64).round() as usize
}

pub fn batch_is_current(batch: &WatchBatch, generation: u64, directory: &Path) -> bool {
    batch.generation() == generation && batch.directory() == directory
}

pub fn watch_failure_message(error: &WatchStartError) -> String {
    format!("Live updates unavailable; use Refresh. {error}")
}

#[derive(Debug, Error)]
pub enum WatchStartError {
    #[error("watch location is not an absolute directory: {0:?}")]
    InvalidDirectory(PathBuf),
    #[error("directory monitor could not start: {0}")]
    Monitor(glib::Error),
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    use tempfile::tempdir;

    use super::*;

    fn batch(generation: u64, directory: &Path) -> WatchBatch {
        WatchBatch {
            generation,
            directory: directory.to_path_buf(),
            changed_paths: Vec::new(),
            renames: Vec::new(),
            event_count: 1,
            overflowed: false,
        }
    }

    #[test]
    fn phase_6s_monitor_replaces_exact_directory_and_stops_cleanly() {
        let first = tempdir().expect("first directory");
        let second = tempdir().expect("second directory");
        let watcher = FileWatcher::default();
        let first_generation = watcher
            .watch_directory(first.path().to_path_buf())
            .expect("first monitor");
        assert_eq!(watcher.directory().as_deref(), Some(first.path()));
        let second_generation = watcher
            .watch_directory(second.path().to_path_buf())
            .expect("replacement monitor");
        assert_ne!(first_generation, second_generation);
        assert_eq!(watcher.directory().as_deref(), Some(second.path()));
        watcher.stop();
        assert!(watcher.directory().is_none());
        assert_ne!(watcher.generation(), second_generation);
    }

    #[test]
    fn phase_6s_coalescer_deduplicates_and_bounds_storms() {
        let mut pending = EventAccumulator::default();
        for _ in 0..100 {
            pending.record(WatchChange::new(
                WatchChangeKind::Content,
                PathBuf::from("/tmp/item"),
                None,
            ));
        }
        assert_eq!(pending.changed_paths.len(), 1);
        assert_eq!(pending.event_count, 100);
        for index in 0..=MAX_EVENTS_PER_BATCH {
            pending.record(WatchChange::new(
                WatchChangeKind::Created,
                PathBuf::from(format!("/tmp/{index}")),
                None,
            ));
        }
        assert!(pending.overflowed);
        assert!(pending.changed_paths.len() <= MAX_PATHS_PER_BATCH);
        let batch = pending.take_batch(7, PathBuf::from("/tmp")).expect("batch");
        assert!(batch.overflowed());
        assert_eq!(pending.event_count, 0);
    }

    #[test]
    fn phase_6s_events_preserve_raw_rename_and_map_common_kinds() {
        let from = PathBuf::from(OsString::from_vec(b"/tmp/from-\xff".to_vec()));
        let to = PathBuf::from(OsString::from_vec(b"/tmp/to-\xfe".to_vec()));
        let change = map_monitor_event(
            &gio::File::for_path(&from),
            Some(&gio::File::for_path(&to)),
            gio::FileMonitorEvent::Renamed,
        )
        .expect("rename");
        assert_eq!(change.kind(), WatchChangeKind::Renamed);
        assert_eq!(change.path(), from);
        assert_eq!(change.other_path(), Some(to.as_path()));
        for (event, kind) in [
            (gio::FileMonitorEvent::Created, WatchChangeKind::Created),
            (gio::FileMonitorEvent::Deleted, WatchChangeKind::Deleted),
            (
                gio::FileMonitorEvent::AttributeChanged,
                WatchChangeKind::Attributes,
            ),
            (gio::FileMonitorEvent::MovedIn, WatchChangeKind::MovedIn),
            (gio::FileMonitorEvent::MovedOut, WatchChangeKind::MovedOut),
        ] {
            assert_eq!(
                map_monitor_event(&gio::File::for_path("/tmp/item"), None, event)
                    .expect("mapped event")
                    .kind(),
                kind
            );
        }
    }

    #[test]
    fn phase_6s_dispatch_accepts_only_current_generation_and_directory() {
        let directory = Path::new("/tmp/current");
        assert!(batch_is_current(&batch(4, directory), 4, directory));
        assert!(!batch_is_current(&batch(3, directory), 4, directory));
        assert!(!batch_is_current(
            &batch(4, Path::new("/tmp/old")),
            4,
            directory
        ));
    }

    #[test]
    fn phase_6s_reconcile_preserves_selection_anchor_rename_and_scales_to_100k() {
        let mut paths = (0..100_000)
            .map(|index| PathBuf::from(format!("/large/item-{index:06}")))
            .collect::<Vec<_>>();
        let old_selected = paths[42_000].clone();
        let old_anchor = paths[75_000].clone();
        let renamed = PathBuf::from("/large/renamed");
        paths[42_000] = renamed.clone();
        let state = reconcile_view_state(
            ViewStateSnapshot {
                selected_paths: vec![old_selected.clone(), PathBuf::from("/large/deleted")],
                anchor_path: Some(old_anchor.clone()),
                anchor_index: 75_000,
            },
            &[RenamePair {
                from: old_selected,
                to: renamed.clone(),
            }],
            &paths,
        );
        assert_eq!(state.selected_paths, vec![renamed]);
        assert_eq!(state.anchor_index, Some(75_000));
        assert_eq!(scroll_anchor_index(50.0, 0.0, 110.0, 10.0, 101), 50);

        let chained = reconcile_view_state(
            ViewStateSnapshot {
                selected_paths: vec![PathBuf::from("/large/old")],
                anchor_path: Some(PathBuf::from("/large/old")),
                anchor_index: 0,
            },
            &[
                RenamePair {
                    from: PathBuf::from("/large/old"),
                    to: PathBuf::from("/large/middle"),
                },
                RenamePair {
                    from: PathBuf::from("/large/middle"),
                    to: paths[42_000].clone(),
                },
            ],
            &paths,
        );
        assert_eq!(chained.selected_paths, vec![paths[42_000].clone()]);
        assert_eq!(chained.anchor_index, Some(42_000));
    }

    #[test]
    fn phase_6s_failure_is_recoverable_and_stale_generations_are_rejected() {
        let error = WatchStartError::InvalidDirectory(PathBuf::from("relative"));
        let message = watch_failure_message(&error);
        assert!(message.contains("use Refresh"));
        assert!(!message.to_ascii_lowercase().contains("integrity"));
        assert!(
            FileWatcher::default()
                .watch_directory(PathBuf::from("relative"))
                .is_err()
        );
    }
}
