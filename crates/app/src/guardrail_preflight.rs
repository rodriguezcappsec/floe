//! Bounded, no-follow destructive-operation preflight for Phase 18X.
//!
//! This layer observes operation scale and risk only. It does not mutate the
//! filesystem and cannot weaken the operation engine's existing no-overwrite,
//! identity-revalidation, mount-boundary, or irreversible-confirmation rules.

use std::{
    fs, io,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

use floe_core::{
    DestructiveFacts, DestructiveScope, PREFLIGHT_DEPTH_CAPACITY, PREFLIGHT_DIRECTORY_CAPACITY,
    PREFLIGHT_ENTRY_CAPACITY, PreflightDecision, PreflightScanState, ProtectedIntersection,
    ProtectedRoots, ProtectedRootsError, validate_guardrail_path,
};
use thiserror::Error;

pub const GUARDRAIL_PREFLIGHT_QUEUE_CAPACITY: usize = 1;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PreflightEnvironment {
    home: Option<PathBuf>,
    mount_roots: Vec<PathBuf>,
}

impl PreflightEnvironment {
    pub fn new(
        home: Option<PathBuf>,
        mut mount_roots: Vec<PathBuf>,
    ) -> Result<Self, ProtectedRootsError> {
        if let Some(home) = home.as_deref() {
            validate_guardrail_path(home)?;
        }
        for mount_root in &mount_roots {
            validate_guardrail_path(mount_root)?;
        }
        mount_roots.sort_by(|left, right| left.as_os_str().cmp(right.as_os_str()));
        mount_roots.dedup();
        Ok(Self { home, mount_roots })
    }

    pub fn home(&self) -> Option<&Path> {
        self.home.as_deref()
    }

    pub fn mount_roots(&self) -> &[PathBuf] {
        &self.mount_roots
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuardrailPreflightOutcome {
    scope: DestructiveScope,
    policy_generation: u64,
    facts: DestructiveFacts,
    decision: PreflightDecision,
}

impl GuardrailPreflightOutcome {
    pub fn scope(&self) -> &DestructiveScope {
        &self.scope
    }

    pub const fn policy_generation(&self) -> u64 {
        self.policy_generation
    }

    pub fn facts(&self) -> &DestructiveFacts {
        &self.facts
    }

    pub fn decision(&self) -> &PreflightDecision {
        &self.decision
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn deterministic_for_test(scope: DestructiveScope, policy_generation: u64) -> Self {
        let facts = DestructiveFacts::new(
            Some(1),
            Some(0),
            Some(0),
            PreflightScanState::Complete,
            Vec::new(),
            false,
            false,
            false,
        );
        let decision = PreflightDecision::evaluate(scope.action(), &facts);
        Self {
            scope,
            policy_generation,
            facts,
            decision,
        }
    }
}

#[derive(Debug, Error)]
pub enum GuardrailPreflightError {
    #[error("guardrail preflight was cancelled")]
    Cancelled,
    #[error("guardrail preflight target is unavailable: {}", path.display())]
    TargetUnavailable {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("guardrail preflight could not read directory: {}", path.display())]
    ReadDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Policy(#[from] ProtectedRootsError),
}

pub fn scan_guardrail_preflight(
    scope: DestructiveScope,
    policy: &ProtectedRoots,
    environment: &PreflightEnvironment,
    cancelled: impl Fn() -> bool,
) -> Result<GuardrailPreflightOutcome, GuardrailPreflightError> {
    let mut accumulator = ScanAccumulator::default();
    for target in scope.targets() {
        if cancelled() {
            return Err(GuardrailPreflightError::Cancelled);
        }
        accumulator
            .protected_intersections
            .extend(policy.intersections(target)?);
        accumulator.touches_filesystem_root |= target == Path::new("/");
        accumulator.touches_home |= environment
            .home()
            .is_some_and(|home| paths_intersect(target, home));
        accumulator.touches_mount_root |= environment
            .mount_roots()
            .iter()
            .any(|mount| paths_intersect(target, mount));

        if !scan_target(target, environment, &cancelled, &mut accumulator)? {
            break;
        }
    }
    if let Some(destination) = scope.destination() {
        accumulator
            .protected_intersections
            .extend(policy.intersections(destination)?);
        accumulator.touches_filesystem_root |= destination == Path::new("/");
        accumulator.touches_home |= environment
            .home()
            .is_some_and(|home| paths_intersect(destination, home));
        accumulator.touches_mount_root |= environment
            .mount_roots()
            .iter()
            .any(|mount| paths_intersect(destination, mount));
    }

    accumulator.protected_intersections.sort_by(|left, right| {
        left.target()
            .as_os_str()
            .cmp(right.target().as_os_str())
            .then_with(|| {
                left.protected_root()
                    .as_os_str()
                    .cmp(right.protected_root().as_os_str())
            })
    });
    accumulator.protected_intersections.dedup();
    let facts = accumulator.into_facts();
    let decision = PreflightDecision::evaluate(scope.action(), &facts);
    Ok(GuardrailPreflightOutcome {
        scope,
        policy_generation: policy.generation(),
        facts,
        decision,
    })
}

fn paths_intersect(left: &Path, right: &Path) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

#[derive(Debug, Default)]
struct ScanAccumulator {
    item_count: u64,
    byte_count: u64,
    max_depth: u32,
    directory_count: u64,
    state: Option<PreflightScanState>,
    protected_intersections: Vec<ProtectedIntersection>,
    touches_filesystem_root: bool,
    touches_mount_root: bool,
    touches_home: bool,
}

impl ScanAccumulator {
    fn count_entry(&mut self, metadata: &fs::Metadata, depth: u32) -> bool {
        let Some(item_count) = self.item_count.checked_add(1) else {
            self.state = Some(PreflightScanState::ArithmeticOverflow);
            return false;
        };
        self.item_count = item_count;
        if self.item_count > PREFLIGHT_ENTRY_CAPACITY {
            self.state = Some(PreflightScanState::EntryLimitExceeded);
            return false;
        }
        self.max_depth = self.max_depth.max(depth);
        if depth > PREFLIGHT_DEPTH_CAPACITY {
            self.state = Some(PreflightScanState::DepthLimitExceeded);
            return false;
        }
        if metadata.file_type().is_file() {
            let Some(byte_count) = self.byte_count.checked_add(metadata.len()) else {
                self.state = Some(PreflightScanState::ArithmeticOverflow);
                return false;
            };
            self.byte_count = byte_count;
        }
        if metadata.file_type().is_dir() {
            let Some(directory_count) = self.directory_count.checked_add(1) else {
                self.state = Some(PreflightScanState::ArithmeticOverflow);
                return false;
            };
            self.directory_count = directory_count;
            if self.directory_count > PREFLIGHT_DIRECTORY_CAPACITY {
                self.state = Some(PreflightScanState::DirectoryLimitExceeded);
                return false;
            }
        }
        true
    }

    fn into_facts(self) -> DestructiveFacts {
        let state = self.state.unwrap_or(PreflightScanState::Complete);
        let (item_count, byte_count, max_depth) = if state == PreflightScanState::ArithmeticOverflow
        {
            (None, None, None)
        } else {
            (
                Some(self.item_count),
                Some(self.byte_count),
                Some(self.max_depth),
            )
        };
        DestructiveFacts::new(
            item_count,
            byte_count,
            max_depth,
            state,
            self.protected_intersections,
            self.touches_filesystem_root,
            self.touches_mount_root,
            self.touches_home,
        )
    }
}

fn scan_target(
    target: &Path,
    environment: &PreflightEnvironment,
    cancelled: &impl Fn() -> bool,
    accumulator: &mut ScanAccumulator,
) -> Result<bool, GuardrailPreflightError> {
    let root_metadata = fs::symlink_metadata(target).map_err(|source| {
        GuardrailPreflightError::TargetUnavailable {
            path: target.to_path_buf(),
            source,
        }
    })?;
    let root_device = root_metadata.dev();
    let mut pending = vec![(target.to_path_buf(), 0u32, root_metadata)];

    while let Some((path, depth, metadata)) = pending.pop() {
        if cancelled() {
            return Err(GuardrailPreflightError::Cancelled);
        }
        if !accumulator.count_entry(&metadata, depth) {
            return Ok(false);
        }
        if !metadata.file_type().is_dir() {
            continue;
        }

        if path != target
            && (metadata.dev() != root_device
                || environment.mount_roots().iter().any(|mount| mount == &path))
        {
            accumulator.touches_mount_root = true;
            accumulator
                .state
                .get_or_insert(PreflightScanState::Incomplete);
            continue;
        }

        let next_depth = match depth.checked_add(1) {
            Some(next_depth) => next_depth,
            None => {
                accumulator.state = Some(PreflightScanState::ArithmeticOverflow);
                return Ok(false);
            }
        };
        if next_depth > PREFLIGHT_DEPTH_CAPACITY {
            accumulator.state = Some(PreflightScanState::DepthLimitExceeded);
            return Ok(false);
        }

        let mut children = fs::read_dir(&path)
            .map_err(|source| GuardrailPreflightError::ReadDirectory {
                path: path.clone(),
                source,
            })?
            .map(|entry| {
                entry.map(|entry| entry.path()).map_err(|source| {
                    GuardrailPreflightError::ReadDirectory {
                        path: path.clone(),
                        source,
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        children.sort_by(|left, right| right.as_os_str().cmp(left.as_os_str()));
        for child in children {
            let metadata = fs::symlink_metadata(&child).map_err(|source| {
                GuardrailPreflightError::TargetUnavailable {
                    path: child.clone(),
                    source,
                }
            })?;
            pending.push((child, next_depth, metadata));
        }
    }
    Ok(true)
}

#[derive(Debug)]
pub struct GuardrailPreflightResult {
    pub generation: u64,
    pub outcome: Result<GuardrailPreflightOutcome, GuardrailPreflightError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GuardrailPreflightSubmission {
    generation: u64,
}

impl GuardrailPreflightSubmission {
    pub const fn generation(self) -> u64 {
        self.generation
    }
}

#[derive(Debug, Error)]
pub enum GuardrailPreflightSpawnError {
    #[error("could not spawn guardrail preflight worker")]
    Thread(#[source] io::Error),
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum GuardrailPreflightSubmitError {
    #[error("guardrail preflight queue is full")]
    QueueFull(GuardrailPreflightSubmission),
    #[error("guardrail preflight worker has stopped")]
    WorkerStopped(GuardrailPreflightSubmission),
}

struct WorkerTask {
    generation: u64,
    scope: DestructiveScope,
    policy: ProtectedRoots,
    environment: PreflightEnvironment,
    cancelled: Arc<AtomicBool>,
}

pub struct GuardrailPreflightWorker {
    sender: Option<SyncSender<WorkerTask>>,
    latest_generation: Arc<AtomicU64>,
    active_cancellation: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    latest_result: Arc<Mutex<Option<GuardrailPreflightResult>>>,
    worker: Option<JoinHandle<()>>,
}

impl GuardrailPreflightWorker {
    pub fn spawn() -> Result<Self, GuardrailPreflightSpawnError> {
        let (sender, receiver) =
            mpsc::sync_channel::<WorkerTask>(GUARDRAIL_PREFLIGHT_QUEUE_CAPACITY);
        let latest_generation = Arc::new(AtomicU64::new(0));
        let active_cancellation = Arc::new(Mutex::new(None));
        let latest_result = Arc::new(Mutex::new(None));
        let worker = thread::Builder::new()
            .name("floe-guardrail-preflight".to_owned())
            .spawn({
                let latest_generation = Arc::clone(&latest_generation);
                let active_cancellation = Arc::clone(&active_cancellation);
                let latest_result = Arc::clone(&latest_result);
                move || {
                    while let Ok(task) = receiver.recv() {
                        let outcome = scan_guardrail_preflight(
                            task.scope,
                            &task.policy,
                            &task.environment,
                            || task.cancelled.load(Ordering::Acquire),
                        );
                        if latest_generation.load(Ordering::Acquire) == task.generation {
                            *latest_result
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                Some(GuardrailPreflightResult {
                                    generation: task.generation,
                                    outcome,
                                });
                            active_cancellation
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .take();
                        }
                    }
                }
            })
            .map_err(GuardrailPreflightSpawnError::Thread)?;
        Ok(Self {
            sender: Some(sender),
            latest_generation,
            active_cancellation,
            latest_result,
            worker: Some(worker),
        })
    }

    pub fn submit(
        &self,
        scope: DestructiveScope,
        policy: ProtectedRoots,
        environment: PreflightEnvironment,
    ) -> Result<GuardrailPreflightSubmission, GuardrailPreflightSubmitError> {
        let generation = self
            .latest_generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
            .max(1);
        let submission = GuardrailPreflightSubmission { generation };
        self.cancel_active();
        let cancelled = Arc::new(AtomicBool::new(false));
        *self
            .active_cancellation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Arc::clone(&cancelled));
        let task = WorkerTask {
            generation,
            scope,
            policy,
            environment,
            cancelled,
        };
        let Some(sender) = self.sender.as_ref() else {
            self.clear_active(generation);
            return Err(GuardrailPreflightSubmitError::WorkerStopped(submission));
        };
        match sender.try_send(task) {
            Ok(()) => Ok(submission),
            Err(TrySendError::Full(_)) => {
                self.clear_active(generation);
                Err(GuardrailPreflightSubmitError::QueueFull(submission))
            }
            Err(TrySendError::Disconnected(_)) => {
                self.clear_active(generation);
                Err(GuardrailPreflightSubmitError::WorkerStopped(submission))
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

    pub fn take_result(&self, generation: u64) -> Option<GuardrailPreflightResult> {
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

    fn clear_active(&self, generation: u64) {
        if self.latest_generation.load(Ordering::Acquire) == generation {
            self.active_cancellation
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
        }
    }
}

impl Drop for GuardrailPreflightWorker {
    fn drop(&mut self) {
        self.cancel_active();
        self.sender.take();
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("guardrail preflight worker panicked during shutdown");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::symlink},
        time::{Duration, Instant},
    };

    use floe_core::{DestructiveAction, PreflightRisk, ProtectedRelation};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn phase_18x_preflight_scans_raw_paths_without_following_links() {
        let fixture = tempdir().expect("fixture");
        let root = fixture.path().join("selected");
        fs::create_dir(&root).expect("root");
        let raw = root.join(OsString::from_vec(b"raw-\xff".to_vec()));
        fs::write(&raw, b"1234").expect("raw file");
        let outside = fixture.path().join("outside");
        fs::create_dir(&outside).expect("outside");
        fs::write(outside.join("large"), vec![0u8; 128]).expect("outside file");
        symlink(&outside, root.join("link")).expect("link");
        let policy = ProtectedRoots::new(vec![raw.clone()]).expect("policy");
        let scope = DestructiveScope::new(DestructiveAction::Trash, vec![root.clone()], None)
            .expect("scope");

        let outcome =
            scan_guardrail_preflight(scope, &policy, &PreflightEnvironment::default(), || false)
                .expect("scan");
        assert_eq!(outcome.facts().item_count(), Some(3));
        assert_eq!(outcome.facts().byte_count(), Some(4));
        assert_eq!(
            outcome.facts().protected_intersections()[0].relation(),
            ProtectedRelation::Ancestor
        );
        assert_eq!(outcome.decision().risks(), &[PreflightRisk::ProtectedPath]);
    }

    #[test]
    fn phase_18x_preflight_marks_home_mount_and_root_boundaries() {
        let fixture = tempdir().expect("fixture");
        let home = fixture.path().join("home");
        let mount = home.join("media");
        fs::create_dir_all(&mount).expect("roots");
        let environment = PreflightEnvironment::new(Some(home.clone()), vec![mount.clone()])
            .expect("environment");
        let scope = DestructiveScope::new(
            DestructiveAction::Move,
            vec![home.clone()],
            Some(fixture.path().join("destination")),
        )
        .expect("scope");
        let outcome =
            scan_guardrail_preflight(scope, &ProtectedRoots::default(), &environment, || false)
                .expect("scan");
        assert!(outcome.facts().touches_home());
        assert!(outcome.facts().touches_mount_root());
        assert_eq!(outcome.facts().scan_state(), PreflightScanState::Incomplete);
        assert_eq!(
            outcome.decision().risks(),
            &[
                PreflightRisk::MountRoot,
                PreflightRisk::HomeDirectory,
                PreflightRisk::IncompleteScan,
            ]
        );

        let root_scope =
            DestructiveScope::new(DestructiveAction::Trash, vec![PathBuf::from("/")], None)
                .expect("root scope");
        assert!(paths_intersect(root_scope.targets()[0].as_path(), &home));
    }

    #[test]
    fn phase_18x_preflight_protects_an_exact_destination_without_scanning_it() {
        let fixture = tempdir().expect("fixture");
        let source = fixture.path().join("source");
        fs::write(&source, b"source").expect("source");
        let protected_destination = fixture.path().join("protected/new-name");
        let policy = ProtectedRoots::new(vec![fixture.path().join("protected")]).expect("policy");
        let scope = DestructiveScope::new(
            DestructiveAction::Move,
            vec![source],
            Some(protected_destination),
        )
        .expect("scope");
        let outcome =
            scan_guardrail_preflight(scope, &policy, &PreflightEnvironment::default(), || false)
                .expect("scan");
        assert_eq!(outcome.facts().item_count(), Some(1));
        assert_eq!(outcome.decision().risks(), &[PreflightRisk::ProtectedPath]);
    }

    #[test]
    fn phase_18x_preflight_protects_ancestor_descendant_and_raw_name_scopes() {
        let fixture = tempdir().expect("fixture");
        let parent = fixture.path().join("parent");
        let protected = parent.join(OsString::from_vec(b"protected-\xff".to_vec()));
        let descendant = protected.join("descendant");
        fs::create_dir_all(&descendant).expect("protected tree");
        fs::write(descendant.join("file"), b"content").expect("file");
        let policy = ProtectedRoots::new(vec![protected.clone()]).expect("policy");

        for selected in [parent, protected, descendant] {
            let scope = DestructiveScope::new(DestructiveAction::Trash, vec![selected], None)
                .expect("scope");
            let outcome =
                scan_guardrail_preflight(scope, &policy, &PreflightEnvironment::default(), || {
                    false
                })
                .expect("scan");
            assert_eq!(outcome.decision().risks(), &[PreflightRisk::ProtectedPath]);
        }
    }

    #[test]
    fn phase_18x_preflight_destination_mount_boundary_confirms_without_reading_destination() {
        let fixture = tempdir().expect("fixture");
        let source = fixture.path().join("source");
        fs::write(&source, b"source").expect("source");
        let mount = fixture.path().join("not-present-mount");
        let environment =
            PreflightEnvironment::new(None, vec![mount.clone()]).expect("environment");
        let scope = DestructiveScope::new(
            DestructiveAction::Move,
            vec![source],
            Some(mount.join("new-name")),
        )
        .expect("scope");
        let outcome =
            scan_guardrail_preflight(scope, &ProtectedRoots::default(), &environment, || false)
                .expect("scan");
        assert_eq!(outcome.decision().risks(), &[PreflightRisk::MountRoot]);
    }

    #[test]
    fn phase_18x_preflight_excludes_small_unprotected_recoverable_operations() {
        let fixture = tempdir().expect("fixture");
        let source = fixture.path().join("source");
        fs::write(&source, b"source").expect("source");
        for (action, destination) in [
            (DestructiveAction::Trash, None),
            (
                DestructiveAction::Move,
                Some(fixture.path().join("move-destination")),
            ),
            (
                DestructiveAction::Rename,
                Some(fixture.path().join("rename-destination")),
            ),
        ] {
            let scope = DestructiveScope::new(action, vec![source.clone()], destination)
                .expect("safe scope");
            let outcome = scan_guardrail_preflight(
                scope,
                &ProtectedRoots::default(),
                &PreflightEnvironment::default(),
                || false,
            )
            .expect("safe scan");
            assert_eq!(outcome.decision(), &PreflightDecision::Proceed);
        }

        for action in [
            DestructiveAction::PermanentDelete,
            DestructiveAction::Overwrite,
        ] {
            let scope = DestructiveScope::new(action, vec![source.clone()], None)
                .expect("irreversible scope");
            let outcome = scan_guardrail_preflight(
                scope,
                &ProtectedRoots::default(),
                &PreflightEnvironment::default(),
                || false,
            )
            .expect("irreversible scan");
            assert_eq!(
                outcome.decision().risks(),
                &[PreflightRisk::IrreversibleAction]
            );
        }
    }

    #[test]
    fn phase_18x_preflight_cancellation_is_explicit() {
        let fixture = tempdir().expect("fixture");
        let scope = DestructiveScope::new(
            DestructiveAction::Trash,
            vec![fixture.path().to_path_buf()],
            None,
        )
        .expect("scope");
        assert!(matches!(
            scan_guardrail_preflight(
                scope,
                &ProtectedRoots::default(),
                &PreflightEnvironment::default(),
                || true
            ),
            Err(GuardrailPreflightError::Cancelled)
        ));
    }

    #[test]
    fn phase_18x_preflight_worker_returns_only_requested_generation() {
        let fixture = tempdir().expect("fixture");
        let worker = GuardrailPreflightWorker::spawn().expect("worker");
        let scope = DestructiveScope::new(
            DestructiveAction::Trash,
            vec![fixture.path().to_path_buf()],
            None,
        )
        .expect("scope");
        let submission = worker
            .submit(
                scope,
                ProtectedRoots::default(),
                PreflightEnvironment::default(),
            )
            .expect("submit");
        let deadline = Instant::now() + Duration::from_secs(2);
        let result = loop {
            if let Some(result) = worker.take_result(submission.generation()) {
                break result;
            }
            assert!(Instant::now() < deadline, "worker result timed out");
            thread::yield_now();
        };
        assert_eq!(result.generation, submission.generation());
        assert!(result.outcome.is_ok());
    }
}
