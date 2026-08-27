//! GTK-independent Phase 18U integrity-monitor filesystem worker.
//!
//! Monitoring is explicit and local. It compares selected immutable SHA-256
//! baselines; it is not intrusion detection, malware detection, or authenticity
//! evidence.

use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

use floe_core::{
    INTEGRITY_MONITOR_ENTRY_CAPACITY, IntegrityBaseline, IntegrityBaselineDiff,
    IntegrityBaselineEntry, IntegrityBaselineError, IntegrityWatchSetPolicy,
};
use thiserror::Error;

use crate::{
    integrity::{IntegrityError, generate_sha256sums},
    integrity_monitor_store::{
        IntegrityBaselineStoragePolicy, IntegrityBaselineStore, IntegrityBaselineStoreError,
        IntegrityBaselineStoreOutcome,
    },
};

pub const INTEGRITY_MONITOR_WORK_QUEUE_CAPACITY: usize = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrityMonitorRootKind {
    Local,
    #[cfg(test)]
    Trash,
    #[cfg(test)]
    Remote,
    #[cfg(test)]
    MountRoot,
}

#[derive(Clone, Debug)]
pub enum IntegrityMonitorRequest {
    Load {
        store_path: PathBuf,
    },
    Create {
        root: PathBuf,
        root_kind: IntegrityMonitorRootKind,
        store_path: PathBuf,
        storage_policy: IntegrityBaselineStoragePolicy,
    },
    Check {
        baseline: IntegrityBaseline,
        root_kind: IntegrityMonitorRootKind,
    },
    Delete {
        store_path: PathBuf,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IntegrityMonitorOutcome {
    BaselineLoaded(Option<IntegrityBaseline>),
    BaselineCreated {
        baseline: IntegrityBaseline,
        storage: IntegrityBaselineStoreOutcome,
    },
    Checked {
        baseline: IntegrityBaseline,
        diff: IntegrityBaselineDiff,
    },
    StoredBaselineRemoved,
}

#[derive(Debug)]
pub struct IntegrityMonitorResult {
    pub generation: u64,
    pub outcome: Result<IntegrityMonitorOutcome, IntegrityMonitorError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntegrityMonitorSubmission {
    pub generation: u64,
}

#[derive(Debug, Error)]
pub enum IntegrityMonitorSubmitError {
    #[error("integrity monitor worker queue is full")]
    QueueFull(IntegrityMonitorSubmission),
    #[error("integrity monitor worker has stopped")]
    WorkerStopped(IntegrityMonitorSubmission),
}

#[derive(Debug, Error)]
pub enum IntegrityMonitorSpawnError {
    #[error("could not spawn integrity monitor worker")]
    Thread(#[source] std::io::Error),
}

#[derive(Debug, Error)]
pub enum IntegrityMonitorError {
    #[error("integrity monitor operation was cancelled")]
    Cancelled,
    #[error("integrity monitoring accepts only explicit local roots")]
    UnsupportedRoot,
    #[error("integrity monitoring does not watch symbolic links or paths through them")]
    SymbolicLink,
    #[error("integrity monitoring does not watch a mount root")]
    MountRoot,
    #[error("integrity monitoring root is not an accessible directory")]
    InvalidRoot,
    #[error("integrity baseline became stale while it was being scanned")]
    RootChangedDuringScan,
    #[error(transparent)]
    Baseline(#[from] IntegrityBaselineError),
    #[error(transparent)]
    Integrity(#[from] IntegrityError),
    #[error(transparent)]
    Store(#[from] IntegrityBaselineStoreError),
    #[error("integrity monitor filesystem access failed")]
    Io(#[source] std::io::Error),
}

struct WorkerTask {
    generation: u64,
    request: IntegrityMonitorRequest,
    cancelled: Arc<AtomicBool>,
}

pub struct IntegrityMonitorWorker {
    sender: Option<SyncSender<WorkerTask>>,
    active_cancellation: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    latest_generation: Arc<AtomicU64>,
    latest_result: Arc<Mutex<Option<IntegrityMonitorResult>>>,
    join: Option<JoinHandle<()>>,
}

impl IntegrityMonitorWorker {
    pub fn spawn() -> Result<Self, IntegrityMonitorSpawnError> {
        let (sender, receiver) = mpsc::sync_channel(INTEGRITY_MONITOR_WORK_QUEUE_CAPACITY);
        let active_cancellation = Arc::new(Mutex::new(None));
        let latest_generation = Arc::new(AtomicU64::new(0));
        let latest_result = Arc::new(Mutex::new(None));
        let worker_cancellation = Arc::clone(&active_cancellation);
        let worker_generation = Arc::clone(&latest_generation);
        let worker_result = Arc::clone(&latest_result);
        let join = thread::Builder::new()
            .name("floe-integrity-monitor".to_owned())
            .spawn(move || {
                worker_loop(
                    receiver,
                    &worker_cancellation,
                    &worker_generation,
                    &worker_result,
                );
            })
            .map_err(IntegrityMonitorSpawnError::Thread)?;
        Ok(Self {
            sender: Some(sender),
            active_cancellation,
            latest_generation,
            latest_result,
            join: Some(join),
        })
    }

    pub fn submit(
        &self,
        request: IntegrityMonitorRequest,
    ) -> Result<IntegrityMonitorSubmission, IntegrityMonitorSubmitError> {
        let generation = self
            .latest_generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
            .max(1);
        let submission = IntegrityMonitorSubmission { generation };
        self.cancel_active();
        let cancelled = Arc::new(AtomicBool::new(false));
        let task = WorkerTask {
            generation,
            request,
            cancelled: Arc::clone(&cancelled),
        };
        *self
            .active_cancellation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(cancelled);
        match self.sender.as_ref() {
            Some(sender) => match sender.try_send(task) {
                Ok(()) => Ok(submission),
                Err(TrySendError::Full(_)) => {
                    self.clear_cancellation(generation);
                    Err(IntegrityMonitorSubmitError::QueueFull(submission))
                }
                Err(TrySendError::Disconnected(_)) => {
                    self.clear_cancellation(generation);
                    Err(IntegrityMonitorSubmitError::WorkerStopped(submission))
                }
            },
            None => {
                self.clear_cancellation(generation);
                Err(IntegrityMonitorSubmitError::WorkerStopped(submission))
            }
        }
    }

    pub fn cancel_active(&self) {
        if let Some(cancelled) = self
            .active_cancellation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        {
            cancelled.store(true, Ordering::Release);
        }
    }

    /// Returns only the requested generation, discarding superseded results.
    pub fn take_result(&self, generation: u64) -> Option<IntegrityMonitorResult> {
        let mut slot = self
            .latest_result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match slot.as_ref() {
            Some(result) if result.generation == generation => slot.take(),
            Some(result) if result.generation < generation => {
                slot.take();
                None
            }
            _ => None,
        }
    }

    fn clear_cancellation(&self, generation: u64) {
        if self.latest_generation.load(Ordering::Acquire) == generation {
            self.active_cancellation
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
        }
    }
}

impl Drop for IntegrityMonitorWorker {
    fn drop(&mut self) {
        self.cancel_active();
        self.sender.take();
        if let Some(join) = self.join.take() {
            if join.join().is_err() {
                tracing::error!("integrity monitor worker panicked during shutdown");
            }
        }
    }
}

pub fn create_integrity_baseline(
    root: PathBuf,
    root_kind: IntegrityMonitorRootKind,
    cancelled: impl Fn() -> bool,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<IntegrityBaseline, IntegrityMonitorError> {
    validate_monitor_root(&root, root_kind)?;
    let baseline = scan_root(&root, &cancelled, &mut on_progress)?;
    if cancelled() {
        return Err(IntegrityMonitorError::Cancelled);
    }
    // A second complete Phase 18T pass closes discovery/hash races before an
    // immutable baseline is accepted. Any difference makes this attempt stale.
    let confirmation = scan_root(&root, &cancelled, &mut on_progress)?;
    let diff = IntegrityBaselineDiff::between(&baseline, confirmation.entries())?;
    if diff.has_changes() {
        return Err(IntegrityMonitorError::RootChangedDuringScan);
    }
    Ok(baseline)
}

pub fn check_integrity_baseline(
    baseline: &IntegrityBaseline,
    root_kind: IntegrityMonitorRootKind,
    cancelled: impl Fn() -> bool,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<IntegrityBaselineDiff, IntegrityMonitorError> {
    validate_monitor_root(baseline.root(), root_kind)?;
    let current = scan_root(baseline.root(), cancelled, &mut on_progress)?;
    IntegrityBaselineDiff::between(baseline, current.entries()).map_err(Into::into)
}

fn worker_loop(
    receiver: Receiver<WorkerTask>,
    active_cancellation: &Mutex<Option<Arc<AtomicBool>>>,
    latest_generation: &AtomicU64,
    latest_result: &Mutex<Option<IntegrityMonitorResult>>,
) {
    while let Ok(task) = receiver.recv() {
        let outcome = execute_request(&task.request, &task.cancelled);
        if task.generation == latest_generation.load(Ordering::Acquire) {
            active_cancellation
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
        }
        if task.generation == latest_generation.load(Ordering::Acquire) {
            *latest_result
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                Some(IntegrityMonitorResult {
                    generation: task.generation,
                    outcome,
                });
        }
    }
}

fn execute_request(
    request: &IntegrityMonitorRequest,
    cancelled: &AtomicBool,
) -> Result<IntegrityMonitorOutcome, IntegrityMonitorError> {
    let is_cancelled = || cancelled.load(Ordering::Acquire);
    match request {
        IntegrityMonitorRequest::Load { store_path } => {
            if is_cancelled() {
                return Err(IntegrityMonitorError::Cancelled);
            }
            Ok(IntegrityMonitorOutcome::BaselineLoaded(
                IntegrityBaselineStore::load(store_path)?,
            ))
        }
        IntegrityMonitorRequest::Create {
            root,
            root_kind,
            store_path,
            storage_policy,
        } => {
            let baseline =
                create_integrity_baseline(root.clone(), *root_kind, is_cancelled, |_, _| {})?;
            if is_cancelled() {
                return Err(IntegrityMonitorError::Cancelled);
            }
            let storage = IntegrityBaselineStore::persist(store_path, &baseline, *storage_policy)?;
            Ok(IntegrityMonitorOutcome::BaselineCreated { baseline, storage })
        }
        IntegrityMonitorRequest::Check {
            baseline,
            root_kind,
        } => Ok(IntegrityMonitorOutcome::Checked {
            diff: check_integrity_baseline(baseline, *root_kind, is_cancelled, |_, _| {})?,
            baseline: baseline.clone(),
        }),
        IntegrityMonitorRequest::Delete { store_path } => {
            if is_cancelled() {
                return Err(IntegrityMonitorError::Cancelled);
            }
            IntegrityBaselineStore::remove_private_state(store_path)?;
            Ok(IntegrityMonitorOutcome::StoredBaselineRemoved)
        }
    }
}

fn scan_root(
    root: &Path,
    cancelled: impl Fn() -> bool,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<IntegrityBaseline, IntegrityMonitorError> {
    if cancelled() {
        return Err(IntegrityMonitorError::Cancelled);
    }
    let mut targets = fs::read_dir(root)
        .map_err(IntegrityMonitorError::Io)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(IntegrityMonitorError::Io)?;
    targets.sort_by(|left, right| {
        use std::os::unix::ffi::OsStrExt;
        left.as_os_str()
            .as_bytes()
            .cmp(right.as_os_str().as_bytes())
    });
    if targets.len() > INTEGRITY_MONITOR_ENTRY_CAPACITY {
        return Err(IntegrityMonitorError::Integrity(
            IntegrityError::TooManyEntries,
        ));
    }
    if targets.is_empty() {
        return IntegrityBaseline::new(root.to_path_buf(), Vec::new()).map_err(Into::into);
    }

    let manifest = generate_sha256sums(root, &targets, &cancelled, |completed, total| {
        on_progress(completed, total)
    })?;
    let entries = manifest
        .entries()
        .iter()
        .map(|entry| {
            IntegrityBaselineEntry::new(entry.path().to_path_buf(), entry.digest().to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    IntegrityBaseline::new(root.to_path_buf(), entries).map_err(Into::into)
}

fn validate_monitor_root(
    root: &Path,
    root_kind: IntegrityMonitorRootKind,
) -> Result<(), IntegrityMonitorError> {
    if root_kind != IntegrityMonitorRootKind::Local {
        return Err(IntegrityMonitorError::UnsupportedRoot);
    }
    IntegrityWatchSetPolicy::new(root.to_path_buf())
        .map_err(|_| IntegrityMonitorError::InvalidRoot)?;
    let metadata = fs::symlink_metadata(root).map_err(IntegrityMonitorError::Io)?;
    if metadata.file_type().is_symlink() {
        return Err(IntegrityMonitorError::SymbolicLink);
    }
    if !metadata.is_dir() {
        return Err(IntegrityMonitorError::InvalidRoot);
    }
    reject_symlink_ancestors(root)?;
    let parent = root.parent().ok_or(IntegrityMonitorError::MountRoot)?;
    if parent == root {
        return Err(IntegrityMonitorError::MountRoot);
    }
    let parent_metadata = fs::symlink_metadata(parent).map_err(IntegrityMonitorError::Io)?;
    if parent_metadata.dev() != metadata.dev() {
        return Err(IntegrityMonitorError::MountRoot);
    }
    Ok(())
}

fn reject_symlink_ancestors(path: &Path) -> Result<(), IntegrityMonitorError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => current.push(Path::new("/")),
            Component::Normal(name) => current.push(name),
            Component::CurDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(IntegrityMonitorError::InvalidRoot);
            }
        }
        let metadata = fs::symlink_metadata(&current).map_err(IntegrityMonitorError::Io)?;
        if metadata.file_type().is_symlink() {
            return Err(IntegrityMonitorError::SymbolicLink);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::symlink},
        time::{Duration, Instant},
    };

    use floe_core::IntegrityEntryStatus;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn phase_18u_baseline_engine_reports_exact_matching_changed_missing_new() {
        let fixture = tempdir().expect("fixture");
        let root = fixture.path().join("watched");
        fs::create_dir(&root).expect("root");
        let matching = root.join(OsString::from_vec(b"matching-\xff".to_vec()));
        let changed = root.join("changed");
        let missing = root.join("missing");
        fs::write(&matching, b"same").expect("matching");
        fs::write(&changed, b"before").expect("changed");
        fs::write(&missing, b"gone soon").expect("missing");
        let baseline = create_integrity_baseline(
            root.clone(),
            IntegrityMonitorRootKind::Local,
            || false,
            |_, _| {},
        )
        .expect("baseline");

        fs::write(&changed, b"after").expect("change");
        fs::remove_file(&missing).expect("remove");
        fs::write(root.join("new"), b"new").expect("new");
        let diff = check_integrity_baseline(
            &baseline,
            IntegrityMonitorRootKind::Local,
            || false,
            |_, _| {},
        )
        .expect("check");

        let status = |name: &[u8]| {
            diff.entries()
                .iter()
                .find(|entry| entry.path().as_os_str().as_encoded_bytes() == name)
                .expect("entry")
                .status()
        };
        assert_eq!(status(b"matching-\xff"), &IntegrityEntryStatus::Matching);
        assert!(matches!(
            status(b"changed"),
            IntegrityEntryStatus::Changed { .. }
        ));
        assert_eq!(status(b"missing"), &IntegrityEntryStatus::Missing);
        assert_eq!(status(b"new"), &IntegrityEntryStatus::New);
    }

    #[test]
    fn phase_18u_baseline_engine_accepts_an_explicit_empty_local_root() {
        let fixture = tempdir().expect("fixture");
        let root = fixture.path().join("empty");
        fs::create_dir(&root).expect("root");
        let baseline =
            create_integrity_baseline(root, IntegrityMonitorRootKind::Local, || false, |_, _| {})
                .expect("empty baseline");
        assert!(baseline.entries().is_empty());
    }

    #[test]
    fn phase_18u_monitor_rejects_remote_trash_mount_and_links_without_following() {
        let fixture = tempdir().expect("fixture");
        let root = fixture.path().join("watched");
        fs::create_dir(&root).expect("root");
        for kind in [
            IntegrityMonitorRootKind::Remote,
            IntegrityMonitorRootKind::Trash,
            IntegrityMonitorRootKind::MountRoot,
        ] {
            assert!(matches!(
                create_integrity_baseline(root.clone(), kind, || false, |_, _| {}),
                Err(IntegrityMonitorError::UnsupportedRoot)
            ));
        }
        let link = fixture.path().join("linked-root");
        symlink(&root, &link).expect("link");
        assert!(matches!(
            create_integrity_baseline(link, IntegrityMonitorRootKind::Local, || false, |_, _| {}),
            Err(IntegrityMonitorError::SymbolicLink)
        ));
    }

    #[test]
    fn phase_18u_monitor_worker_is_capacity_one_cancellable_and_generation_safe() {
        let fixture = tempdir().expect("fixture");
        let root = fixture.path().join("watched");
        fs::create_dir(&root).expect("root");
        fs::write(root.join("item"), vec![7; 2 * 1_024 * 1_024]).expect("item");
        let store = fixture.path().join("private").join("baseline");
        let worker = IntegrityMonitorWorker::spawn().expect("worker");
        let first = worker
            .submit(IntegrityMonitorRequest::Create {
                root: root.clone(),
                root_kind: IntegrityMonitorRootKind::Local,
                store_path: store.clone(),
                storage_policy: IntegrityBaselineStoragePolicy::Persist,
            })
            .expect("first");
        worker.cancel_active();

        let deadline = Instant::now() + Duration::from_secs(10);
        let baseline = loop {
            if let Some(result) = worker.take_result(first.generation) {
                match result.outcome {
                    Ok(IntegrityMonitorOutcome::BaselineCreated { baseline, .. }) => {
                        break baseline;
                    }
                    Err(IntegrityMonitorError::Cancelled) if Instant::now() < deadline => {
                        let retry = worker
                            .submit(IntegrityMonitorRequest::Create {
                                root: root.clone(),
                                root_kind: IntegrityMonitorRootKind::Local,
                                store_path: store.clone(),
                                storage_policy: IntegrityBaselineStoragePolicy::Persist,
                            })
                            .expect("retry");
                        break wait_for_baseline(&worker, retry.generation);
                    }
                    other => panic!("unexpected first result: {other:?}"),
                }
            }
            assert!(Instant::now() < deadline, "worker timed out");
            thread::yield_now();
        };

        let check = worker
            .submit(IntegrityMonitorRequest::Check {
                baseline,
                root_kind: IntegrityMonitorRootKind::Local,
            })
            .expect("check");
        let result = wait_for_result(&worker, check.generation);
        assert_eq!(result.generation, check.generation);
        assert!(matches!(
            result.outcome,
            Ok(IntegrityMonitorOutcome::Checked { .. })
        ));

        let load = worker
            .submit(IntegrityMonitorRequest::Load {
                store_path: store.clone(),
            })
            .expect("load");
        assert!(matches!(
            wait_for_result(&worker, load.generation).outcome,
            Ok(IntegrityMonitorOutcome::BaselineLoaded(Some(_)))
        ));
        let remove = worker
            .submit(IntegrityMonitorRequest::Delete {
                store_path: store.clone(),
            })
            .expect("remove");
        assert!(matches!(
            wait_for_result(&worker, remove.generation).outcome,
            Ok(IntegrityMonitorOutcome::StoredBaselineRemoved)
        ));
        assert!(!store.exists());
    }

    fn wait_for_baseline(worker: &IntegrityMonitorWorker, generation: u64) -> IntegrityBaseline {
        match wait_for_result(worker, generation).outcome {
            Ok(IntegrityMonitorOutcome::BaselineCreated { baseline, .. }) => baseline,
            other => panic!("baseline failed: {other:?}"),
        }
    }

    fn wait_for_result(worker: &IntegrityMonitorWorker, generation: u64) -> IntegrityMonitorResult {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(result) = worker.take_result(generation) {
                return result;
            }
            assert!(Instant::now() < deadline, "worker timed out");
            thread::yield_now();
        }
    }

    #[test]
    fn phase_18u_monitor_cancelled_direct_scan_stops_before_filesystem_work() {
        let fixture = tempdir().expect("fixture");
        let root = fixture.path().join("watched");
        fs::create_dir(&root).expect("root");
        assert!(matches!(
            create_integrity_baseline(root, IntegrityMonitorRootKind::Local, || true, |_, _| {}),
            Err(IntegrityMonitorError::Cancelled)
        ));
    }
}
