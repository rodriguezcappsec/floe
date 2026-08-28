//! Bounded application worker for duplicate discovery.

use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use floe_core::{
    ChecksumAlgorithm, ChecksumRequest, DuplicateHashError, DuplicateHashResult,
    DuplicateScanError, DuplicateScanLimits, DuplicateScanOutcome, DuplicateScanPhase,
    DuplicateScanRequest, DuplicateScanSummary, ExpectedDigest, find_duplicates,
};

use crate::checksum_executor::{ChecksumError, execute_checksum};
use crate::duplicate_hash_cache::DuplicateHashCache;

const REQUEST_CAPACITY: usize = 1;
const RESPONSE_CAPACITY: usize = 32;
const INVALIDATION_CAPACITY: usize = 4_096;
const CACHE_FILE_NAME: &str = "duplicate-hashes-v1";

#[derive(Default)]
struct PendingInvalidations {
    paths: HashSet<PathBuf>,
    clear_all: bool,
}

impl PendingInvalidations {
    fn add(&mut self, paths: &[PathBuf]) {
        if self.clear_all {
            return;
        }
        for path in paths {
            if self.paths.len() >= INVALIDATION_CAPACITY {
                self.paths.clear();
                self.clear_all = true;
                return;
            }
            self.paths.insert(path.clone());
        }
    }

    fn take(&mut self) -> Self {
        std::mem::take(self)
    }

    fn clear_all(&mut self) {
        self.paths.clear();
        self.clear_all = true;
    }
}

struct WorkerRequest {
    generation: u64,
    request: DuplicateScanRequest,
}

#[derive(Debug)]
pub enum DuplicateFinderEventKind {
    Progress(DuplicateScanSummary),
    Finished(Arc<DuplicateScanOutcome>),
    Failed(String),
}

#[derive(Debug)]
pub struct DuplicateFinderEvent {
    pub generation: u64,
    pub kind: DuplicateFinderEventKind,
}

#[derive(Debug)]
pub enum DuplicateFinderSubmitError {
    Busy,
    Stopped,
}

pub struct DuplicateFinderWorker {
    sender: Option<SyncSender<WorkerRequest>>,
    responses: Receiver<DuplicateFinderEvent>,
    latest_generation: Arc<AtomicU64>,
    invalidations: Arc<Mutex<PendingInvalidations>>,
    worker: Option<JoinHandle<()>>,
}

impl DuplicateFinderWorker {
    pub fn spawn() -> std::io::Result<Self> {
        Self::spawn_internal(glib::user_cache_dir().join("floe").join(CACHE_FILE_NAME))
    }

    fn spawn_internal(cache_path: PathBuf) -> std::io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<WorkerRequest>(REQUEST_CAPACITY);
        let (response_sender, responses) =
            mpsc::sync_channel::<DuplicateFinderEvent>(RESPONSE_CAPACITY);
        let latest_generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&latest_generation);
        let invalidations = Arc::new(Mutex::new(PendingInvalidations::default()));
        let worker_invalidations = Arc::clone(&invalidations);
        let worker = thread::Builder::new()
            .name("floe-duplicate-finder".to_owned())
            .spawn(move || {
                let cache = Arc::new(Mutex::new(
                    DuplicateHashCache::load(&cache_path).unwrap_or_else(|error| {
                        tracing::warn!(%error, "duplicate hash cache was rejected; rebuilding safely");
                        DuplicateHashCache::rebuilding_empty()
                    }),
                ));
                while let Ok(work) = requests.recv() {
                    if worker_generation.load(Ordering::Acquire) != work.generation {
                        continue;
                    }
                    let generation = work.generation;
                    apply_pending_invalidations(&cache, &worker_invalidations);
                    let cache_for_hash = Arc::clone(&cache);
                    let hash_generation = Arc::clone(&worker_generation);
                    let mut last_progress_at = Instant::now()
                        .checked_sub(Duration::from_secs(1))
                        .unwrap_or_else(Instant::now);
                    let mut last_progress_phase: Option<DuplicateScanPhase> = None;
                    let result = find_duplicates(
                        &work.request,
                        DuplicateScanLimits::default(),
                        || worker_generation.load(Ordering::Acquire) != generation,
                        move |path| {
                            if let Some(digest) = lock(&cache_for_hash).lookup(path) {
                                return Ok(DuplicateHashResult::reused(digest));
                            }
                            let stamp_before = DuplicateHashCache::source_stamp(path).ok();
                            let request = ChecksumRequest::new(
                                vec![path.to_path_buf()],
                                ChecksumAlgorithm::Sha256,
                                None,
                            )
                            .map_err(|error| DuplicateHashError::Failed(error.to_string()))?;
                            let outcome = execute_checksum(
                                &request,
                                || hash_generation.load(Ordering::Acquire) != generation,
                                |_, _| {},
                            )
                            .map_err(|error| match error {
                                ChecksumError::Cancelled => DuplicateHashError::Cancelled,
                                other => DuplicateHashError::Failed(other.to_string()),
                            })?;
                            let item = outcome.items.first().ok_or_else(|| {
                                DuplicateHashError::Failed("SHA-256 returned no result".to_owned())
                            })?;
                            let digest =
                                ExpectedDigest::parse(ChecksumAlgorithm::Sha256, &item.digest)
                                    .map_err(|error| {
                                        DuplicateHashError::Failed(error.to_string())
                                    })?;
                            let digest = digest
                                .bytes()
                                .try_into()
                                .map_err(|_| {
                                    DuplicateHashError::Failed(
                                        "SHA-256 result length was invalid".to_owned(),
                                    )
                                })?;
                            if let Some(stamp_before) = stamp_before {
                                match lock(&cache_for_hash).insert_if_unchanged(
                                    path.to_path_buf(),
                                    digest,
                                    stamp_before,
                                ) {
                                    Ok(true) => {}
                                    Ok(false) => tracing::debug!(
                                        "duplicate hash was not cached because the file changed"
                                    ),
                                    Err(error) => {
                                        tracing::debug!(%error, "duplicate hash was not cached");
                                    }
                                }
                            }
                            Ok(DuplicateHashResult::computed(digest))
                        },
                        |summary| {
                            let now = Instant::now();
                            if last_progress_phase == Some(summary.phase)
                                && now.duration_since(last_progress_at)
                                    < Duration::from_millis(50)
                            {
                                return;
                            }
                            last_progress_phase = Some(summary.phase);
                            last_progress_at = now;
                            send_event(
                                &response_sender,
                                &worker_generation,
                                DuplicateFinderEvent {
                                    generation,
                                    kind: DuplicateFinderEventKind::Progress(summary),
                                },
                            );
                        },
                    );
                    apply_pending_invalidations(&cache, &worker_invalidations);
                    if let Err(error) = lock(&cache).persist(&cache_path) {
                        tracing::warn!(%error, "duplicate hash cache could not be persisted");
                    }
                    match result {
                        Ok(outcome) => {
                            send_event(
                                &response_sender,
                                &worker_generation,
                                DuplicateFinderEvent {
                                    generation,
                                    kind: DuplicateFinderEventKind::Finished(Arc::new(outcome)),
                                },
                            );
                        }
                        Err(DuplicateScanError::Cancelled) => {}
                        Err(error) => {
                            send_event(
                                &response_sender,
                                &worker_generation,
                                DuplicateFinderEvent {
                                    generation,
                                    kind: DuplicateFinderEventKind::Failed(error.to_string()),
                                },
                            );
                        }
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            responses,
            latest_generation,
            invalidations,
            worker: Some(worker),
        })
    }

    pub fn submit(
        &self,
        generation: u64,
        request: DuplicateScanRequest,
    ) -> Result<(), DuplicateFinderSubmitError> {
        self.latest_generation.store(generation, Ordering::Release);
        let Some(sender) = self.sender.as_ref() else {
            return Err(DuplicateFinderSubmitError::Stopped);
        };
        match sender.try_send(WorkerRequest {
            generation,
            request,
        }) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(DuplicateFinderSubmitError::Busy),
            Err(TrySendError::Disconnected(_)) => Err(DuplicateFinderSubmitError::Stopped),
        }
    }

    pub fn cancel(&self, generation: u64) {
        self.latest_generation.store(generation, Ordering::Release);
    }

    pub fn try_event(&self) -> Option<DuplicateFinderEvent> {
        self.responses.try_recv().ok()
    }

    pub fn invalidate_watcher_paths(&self, paths: &[PathBuf], overflowed: bool) {
        let mut pending = lock(&self.invalidations);
        if overflowed {
            pending.clear_all();
        } else {
            pending.add(paths);
        }
    }
}

fn apply_pending_invalidations(
    cache: &Mutex<DuplicateHashCache>,
    pending: &Mutex<PendingInvalidations>,
) {
    let pending = lock(pending).take();
    let mut cache = lock(cache);
    if pending.clear_all {
        cache.clear();
    } else {
        cache.invalidate_paths(&pending.paths.into_iter().collect::<Vec<_>>());
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Drop for DuplicateFinderWorker {
    fn drop(&mut self) {
        self.latest_generation.store(u64::MAX, Ordering::Release);
        self.sender.take();
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("duplicate finder worker panicked during shutdown");
        }
    }
}

fn send_event(
    sender: &SyncSender<DuplicateFinderEvent>,
    generation: &AtomicU64,
    event: DuplicateFinderEvent,
) -> bool {
    let mut event = event;
    loop {
        if generation.load(Ordering::Acquire) != event.generation {
            return false;
        }
        match sender.try_send(event) {
            Ok(()) => return true,
            Err(TrySendError::Full(returned)) => {
                event = returned;
                thread::sleep(Duration::from_millis(2));
            }
            Err(TrySendError::Disconnected(_)) => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, thread, time::Duration};

    use tempfile::tempdir;

    use super::*;

    fn finished(worker: &DuplicateFinderWorker) -> Arc<DuplicateScanOutcome> {
        for _ in 0..800 {
            while let Some(event) = worker.try_event() {
                match event.kind {
                    DuplicateFinderEventKind::Finished(outcome) => return outcome,
                    DuplicateFinderEventKind::Failed(error) => panic!("worker failed: {error}"),
                    DuplicateFinderEventKind::Progress(_) => {}
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("worker did not finish")
    }

    #[test]
    fn phase_13g_duplicate_worker_uses_reviewed_sha256_and_streams_progress() {
        let fixture = tempdir().expect("fixture");
        let cache = tempdir().expect("cache");
        fs::write(fixture.path().join("a"), b"same").expect("a");
        fs::write(fixture.path().join("b"), b"same").expect("b");
        let cache_path = cache.path().join("hashes");
        let worker = DuplicateFinderWorker::spawn_internal(cache_path.clone()).expect("worker");
        worker
            .submit(
                7,
                DuplicateScanRequest::new(vec![fixture.path().to_path_buf()]).expect("request"),
            )
            .expect("submit");
        let outcome = finished(&worker);
        assert_eq!(outcome.groups().len(), 1);
        assert_eq!(outcome.groups()[0].independent_copies(), 2);
        assert_eq!(outcome.summary().hashed_files, 2);
    }

    #[test]
    fn phase_13g_duplicate_worker_generation_cancel_and_shutdown_are_bounded() {
        let fixture = tempdir().expect("fixture");
        let cache = tempdir().expect("cache");
        for index in 0..100 {
            fs::write(
                fixture.path().join(format!("{index}")),
                vec![7u8; 128 * 1024],
            )
            .expect("file");
        }
        let worker =
            DuplicateFinderWorker::spawn_internal(cache.path().join("hashes")).expect("worker");
        worker
            .submit(
                1,
                DuplicateScanRequest::new(vec![fixture.path().to_path_buf()]).expect("request"),
            )
            .expect("submit");
        worker.cancel(2);
        thread::sleep(Duration::from_millis(20));
        assert!(worker.try_event().is_none());
    }

    #[test]
    fn phase_13g3_incremental_hash_cache_worker_reuses_and_invalidates_watcher_changes() {
        let fixture = tempdir().expect("fixture");
        let cache = tempdir().expect("cache");
        let cache_path = cache.path().join("hashes");
        let first = fixture.path().join("first");
        let second = fixture.path().join("second");
        fs::write(&first, b"identical").expect("first");
        fs::write(&second, b"identical").expect("second");
        let worker = DuplicateFinderWorker::spawn_internal(cache_path.clone()).expect("worker");
        let request =
            DuplicateScanRequest::for_folder(fixture.path().to_path_buf()).expect("request");

        worker.submit(1, request.clone()).expect("cold submit");
        let cold = finished(&worker);
        assert_eq!(cold.summary().hashed_files, 2);
        assert_eq!(cold.summary().reused_hashes, 0);

        drop(worker);
        let worker = DuplicateFinderWorker::spawn_internal(cache_path).expect("restarted worker");
        worker.submit(2, request.clone()).expect("warm submit");
        let warm = finished(&worker);
        assert_eq!(warm.summary().hashed_files, 0);
        assert_eq!(warm.summary().reused_hashes, 2);

        worker.invalidate_watcher_paths(std::slice::from_ref(&first), false);
        worker
            .submit(3, request.clone())
            .expect("incremental submit");
        let incremental = finished(&worker);
        assert_eq!(incremental.summary().hashed_files, 1);
        assert_eq!(incremental.summary().reused_hashes, 1);

        worker.invalidate_watcher_paths(&[], true);
        worker.submit(4, request).expect("overflow refresh");
        let overflow_refresh = finished(&worker);
        assert_eq!(overflow_refresh.summary().hashed_files, 2);
        assert_eq!(overflow_refresh.summary().reused_hashes, 0);
    }
}
