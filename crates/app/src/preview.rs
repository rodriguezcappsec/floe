//! Bounded, GTK-independent Quick Preview provider orchestration.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime},
};

use floe_core::{DirectoryEntry, EntryKind};
use thiserror::Error;

pub const PREVIEW_QUEUE_CAPACITY: usize = 16;
pub const PREVIEW_PROVIDER_CAPACITY: usize = 32;
pub const PREVIEW_MEMORY_CACHE_CAPACITY: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreviewLimits {
    pub max_source_bytes: u64,
    pub max_output_bytes: u64,
    pub max_text_bytes: usize,
    pub max_archive_entries: usize,
    pub deadline: Duration,
}

impl Default for PreviewLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 256 * 1024 * 1024,
            max_output_bytes: 128 * 1024 * 1024,
            max_text_bytes: 2 * 1024 * 1024,
            max_archive_entries: 4_096,
            deadline: Duration::from_secs(15),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PreviewCachePolicy {
    Disabled,
    #[default]
    MemoryOnly,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PreviewKind {
    Image,
    Text,
    Document,
    Media,
    Font,
    Archive,
    Unknown,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PreviewSourceKey {
    path: PathBuf,
    size: Option<u64>,
    modified: Option<SystemTime>,
}

impl PreviewSourceKey {
    pub fn from_entry(entry: &DirectoryEntry) -> Option<Self> {
        matches!(
            entry.kind(),
            EntryKind::RegularFile
                | EntryKind::SymbolicLink {
                    target_is_directory: false
                }
        )
        .then(|| Self {
            path: entry.path().to_path_buf(),
            size: entry.size(),
            modified: entry.modified(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewRequest {
    generation: u64,
    source: PreviewSourceKey,
    limits: PreviewLimits,
    cache_policy: PreviewCachePolicy,
}

impl PreviewRequest {
    pub fn new(
        generation: u64,
        source: PreviewSourceKey,
        limits: PreviewLimits,
        cache_policy: PreviewCachePolicy,
    ) -> Option<Self> {
        (generation != 0).then_some(Self {
            generation,
            source,
            limits,
            cache_policy,
        })
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn source(&self) -> &PreviewSourceKey {
        &self.source
    }

    pub const fn limits(&self) -> PreviewLimits {
        self.limits
    }

    pub const fn cache_policy(&self) -> PreviewCachePolicy {
        self.cache_policy
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewPayload {
    pub provider_id: &'static str,
    pub kind: PreviewKind,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PreviewProviderError {
    #[error("preview was cancelled")]
    Cancelled,
    #[error("preview source exceeds configured limits")]
    LimitExceeded,
    #[error("preview source changed")]
    SourceChanged,
    #[error("preview provider failed: {0}")]
    Failed(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewOutcome {
    Ready(PreviewPayload),
    Unsupported,
    Cancelled,
    Failed(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewResponse {
    pub generation: u64,
    pub source: PreviewSourceKey,
    pub outcome: PreviewOutcome,
}

#[derive(Clone)]
pub struct PreviewCancellation {
    active_generation: Arc<AtomicU64>,
    generation: u64,
    started: Instant,
    deadline: Duration,
}

impl PreviewCancellation {
    pub fn is_cancelled(&self) -> bool {
        self.active_generation.load(Ordering::Acquire) != self.generation
            || self.started.elapsed() >= self.deadline
    }
}

pub trait PreviewProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn supports(&self, request: &PreviewRequest) -> bool;
    fn load(
        &self,
        request: &PreviewRequest,
        cancellation: &PreviewCancellation,
    ) -> Result<PreviewPayload, PreviewProviderError>;
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum PreviewRegistryError {
    #[error("preview provider registry is full")]
    Full,
    #[error("duplicate preview provider id: {0}")]
    Duplicate(&'static str),
}

#[derive(Default)]
pub struct PreviewProviderRegistry {
    providers: Vec<Arc<dyn PreviewProvider>>,
    ids: HashSet<&'static str>,
}

impl PreviewProviderRegistry {
    pub fn register(
        &mut self,
        provider: Arc<dyn PreviewProvider>,
    ) -> Result<(), PreviewRegistryError> {
        if self.providers.len() == PREVIEW_PROVIDER_CAPACITY {
            return Err(PreviewRegistryError::Full);
        }
        let id = provider.id();
        if !self.ids.insert(id) {
            return Err(PreviewRegistryError::Duplicate(id));
        }
        self.providers.push(provider);
        Ok(())
    }

    fn load(&self, request: &PreviewRequest, cancellation: &PreviewCancellation) -> PreviewOutcome {
        if cancellation.is_cancelled() {
            return PreviewOutcome::Cancelled;
        }
        let mut selected = None;
        for provider in &self.providers {
            match catch_unwind(AssertUnwindSafe(|| provider.supports(request))) {
                Ok(true) => {
                    selected = Some(provider);
                    break;
                }
                Ok(false) => {}
                Err(_) => return PreviewOutcome::Failed("preview provider panicked".to_owned()),
            }
        }
        let Some(provider) = selected else {
            return PreviewOutcome::Unsupported;
        };
        match catch_unwind(AssertUnwindSafe(|| provider.load(request, cancellation))) {
            Ok(Ok(_payload)) if cancellation.is_cancelled() => PreviewOutcome::Cancelled,
            Ok(Ok(payload)) => PreviewOutcome::Ready(payload),
            Ok(Err(PreviewProviderError::Cancelled)) => PreviewOutcome::Cancelled,
            Ok(Err(error)) => PreviewOutcome::Failed(error.to_string()),
            Err(_) => PreviewOutcome::Failed("preview provider panicked".to_owned()),
        }
    }
}

#[derive(Debug, Error)]
pub enum PreviewSubmitError {
    #[error("preview queue is full")]
    Full(PreviewRequest),
    #[error("preview worker disconnected")]
    Disconnected,
    #[error("preview request generation is stale")]
    Stale(PreviewRequest),
}

pub struct PreviewWorker {
    sender: Option<SyncSender<PreviewRequest>>,
    receiver: Receiver<PreviewResponse>,
    active_generation: Arc<AtomicU64>,
    worker: Option<JoinHandle<()>>,
}

impl PreviewWorker {
    pub fn spawn(registry: PreviewProviderRegistry) -> std::io::Result<Self> {
        Self::spawn_internal(registry, PREVIEW_QUEUE_CAPACITY, None)
    }

    fn spawn_internal(
        registry: PreviewProviderRegistry,
        capacity: usize,
        start_gate: Option<Receiver<()>>,
    ) -> std::io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<PreviewRequest>(capacity);
        let (responses, receiver) = mpsc::channel();
        let active_generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&active_generation);
        let worker = thread::Builder::new()
            .name("floe-preview".to_owned())
            .spawn(move || {
                if let Some(gate) = start_gate
                    && gate.recv().is_err()
                {
                    return;
                }
                let mut cache = PreviewMemoryCache::default();
                while let Ok(request) = requests.recv() {
                    let cancellation = PreviewCancellation {
                        active_generation: Arc::clone(&worker_generation),
                        generation: request.generation,
                        started: Instant::now(),
                        deadline: request.limits.deadline,
                    };
                    let outcome = if cancellation.is_cancelled() {
                        PreviewOutcome::Cancelled
                    } else if request.cache_policy == PreviewCachePolicy::MemoryOnly {
                        cache.get(&request.source).map_or_else(
                            || {
                                let outcome = registry.load(&request, &cancellation);
                                if let PreviewOutcome::Ready(payload) = &outcome {
                                    cache.insert(request.source.clone(), payload.clone());
                                }
                                outcome
                            },
                            PreviewOutcome::Ready,
                        )
                    } else {
                        registry.load(&request, &cancellation)
                    };
                    if responses
                        .send(PreviewResponse {
                            generation: request.generation,
                            source: request.source,
                            outcome,
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            receiver,
            active_generation,
            worker: Some(worker),
        })
    }

    pub fn begin_generation(&self) -> u64 {
        let mut current = self.active_generation.load(Ordering::Acquire);
        loop {
            let next = current.wrapping_add(1).max(1);
            match self.active_generation.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return next,
                Err(observed) => current = observed,
            }
        }
    }

    pub fn cancel(&self) {
        let _ = self.begin_generation();
    }

    pub fn submit(&self, request: PreviewRequest) -> Result<(), PreviewSubmitError> {
        if request.generation != self.active_generation.load(Ordering::Acquire) {
            return Err(PreviewSubmitError::Stale(request));
        }
        let Some(sender) = self.sender.as_ref() else {
            return Err(PreviewSubmitError::Disconnected);
        };
        match sender.try_send(request) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(request)) => Err(PreviewSubmitError::Full(request)),
            Err(TrySendError::Disconnected(_)) => Err(PreviewSubmitError::Disconnected),
        }
    }

    pub fn try_response(&self) -> Option<PreviewResponse> {
        self.receiver.try_recv().ok()
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.active_generation.load(Ordering::Acquire) == generation
    }
}

impl Drop for PreviewWorker {
    fn drop(&mut self) {
        self.cancel();
        self.sender.take();
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::warn!("preview worker panicked during shutdown");
        }
    }
}

#[derive(Default)]
struct PreviewMemoryCache {
    values: HashMap<PreviewSourceKey, PreviewPayload>,
    order: VecDeque<PreviewSourceKey>,
}

impl PreviewMemoryCache {
    fn get(&self, key: &PreviewSourceKey) -> Option<PreviewPayload> {
        self.values.get(key).cloned()
    }

    fn insert(&mut self, key: PreviewSourceKey, payload: PreviewPayload) {
        self.order.retain(|candidate| candidate != &key);
        self.order.push_back(key.clone());
        self.values.insert(key, payload);
        while self.values.len() > PREVIEW_MEMORY_CACHE_CAPACITY {
            if let Some(expired) = self.order.pop_front() {
                self.values.remove(&expired);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::ffi::{OsStrExt, OsStringExt},
        sync::atomic::{AtomicUsize, Ordering},
    };

    use floe_core::enumerate_directory;
    use tempfile::tempdir;

    use super::*;

    struct TestProvider {
        id: &'static str,
        calls: Arc<AtomicUsize>,
        wait_for_cancel: bool,
        fail: bool,
    }

    impl PreviewProvider for TestProvider {
        fn id(&self) -> &'static str {
            self.id
        }

        fn supports(&self, _: &PreviewRequest) -> bool {
            true
        }

        fn load(
            &self,
            _: &PreviewRequest,
            cancellation: &PreviewCancellation,
        ) -> Result<PreviewPayload, PreviewProviderError> {
            self.calls.fetch_add(1, Ordering::AcqRel);
            while self.wait_for_cancel && !cancellation.is_cancelled() {
                thread::yield_now();
            }
            if cancellation.is_cancelled() {
                return Err(PreviewProviderError::Cancelled);
            }
            if self.fail {
                return Err(PreviewProviderError::Failed("fixture".to_owned()));
            }
            Ok(PreviewPayload {
                provider_id: self.id,
                kind: PreviewKind::Unknown,
            })
        }
    }

    fn source_fixture() -> (tempfile::TempDir, PreviewSourceKey) {
        let root = tempdir().expect("root");
        let path = root
            .path()
            .join(std::ffi::OsString::from_vec(b"raw-\xff".to_vec()));
        fs::write(&path, b"preview").expect("fixture");
        let entry = enumerate_directory(root.path())
            .expect("listing")
            .into_entries()
            .remove(0);
        (root, PreviewSourceKey::from_entry(&entry).expect("source"))
    }

    fn request(generation: u64, source: PreviewSourceKey) -> PreviewRequest {
        PreviewRequest::new(
            generation,
            source,
            PreviewLimits::default(),
            PreviewCachePolicy::MemoryOnly,
        )
        .expect("request")
    }

    #[test]
    fn phase_9a_contract_preserves_raw_identity_limits_order_and_fallback() {
        let (_root, source) = source_fixture();
        assert_eq!(source.path().as_os_str().as_bytes().last(), Some(&0xff));
        assert_eq!(
            PreviewCachePolicy::default(),
            PreviewCachePolicy::MemoryOnly
        );
        assert_eq!(PreviewLimits::default().max_archive_entries, 4_096);
        assert!(
            PreviewRequest::new(
                0,
                source.clone(),
                PreviewLimits::default(),
                PreviewCachePolicy::Disabled
            )
            .is_none()
        );
        let registry = PreviewProviderRegistry::default();
        let active = Arc::new(AtomicU64::new(1));
        let outcome = registry.load(
            &request(1, source),
            &PreviewCancellation {
                active_generation: active,
                generation: 1,
                started: Instant::now(),
                deadline: Duration::from_secs(1),
            },
        );
        assert_eq!(outcome, PreviewOutcome::Unsupported);

        let calls = Arc::new(AtomicUsize::new(0));
        let mut ordered = PreviewProviderRegistry::default();
        ordered
            .register(Arc::new(TestProvider {
                id: "first",
                calls: Arc::clone(&calls),
                wait_for_cancel: false,
                fail: false,
            }))
            .expect("first provider");
        ordered
            .register(Arc::new(TestProvider {
                id: "second",
                calls: Arc::clone(&calls),
                wait_for_cancel: false,
                fail: false,
            }))
            .expect("second provider");
        assert!(matches!(
            ordered.register(Arc::new(TestProvider {
                id: "first",
                calls: Arc::clone(&calls),
                wait_for_cancel: false,
                fail: false,
            })),
            Err(PreviewRegistryError::Duplicate("first"))
        ));
        let active = Arc::new(AtomicU64::new(2));
        let outcome = ordered.load(
            &request(2, source_fixture().1),
            &PreviewCancellation {
                active_generation: active,
                generation: 2,
                started: Instant::now(),
                deadline: Duration::from_secs(1),
            },
        );
        assert!(matches!(
            outcome,
            PreviewOutcome::Ready(PreviewPayload {
                provider_id: "first",
                ..
            })
        ));
        assert_eq!(calls.load(Ordering::Acquire), 1);
    }

    #[test]
    fn phase_9a_worker_cancels_stale_work_bounds_queue_and_caches_memory_only() {
        let (_root, source) = source_fixture();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = PreviewProviderRegistry::default();
        registry
            .register(Arc::new(TestProvider {
                id: "test",
                calls: Arc::clone(&calls),
                wait_for_cancel: false,
                fail: false,
            }))
            .expect("provider");
        let worker = PreviewWorker::spawn(registry).expect("worker");
        let generation = worker.begin_generation();
        worker
            .submit(request(generation, source.clone()))
            .expect("first");
        let first = loop {
            if let Some(response) = worker.try_response() {
                break response;
            }
            thread::yield_now();
        };
        assert!(matches!(first.outcome, PreviewOutcome::Ready(_)));
        let generation = worker.begin_generation();
        worker
            .submit(request(generation, source.clone()))
            .expect("cached");
        while worker.try_response().is_none() {
            thread::yield_now();
        }
        assert_eq!(calls.load(Ordering::Acquire), 1);

        let (gate_send, gate_receive) = mpsc::channel();
        let blocked = PreviewWorker::spawn_internal(
            PreviewProviderRegistry::default(),
            1,
            Some(gate_receive),
        )
        .expect("blocked worker");
        let generation = blocked.begin_generation();
        blocked
            .submit(request(generation, source.clone()))
            .expect("queued");
        assert!(matches!(
            blocked.submit(request(generation, source)),
            Err(PreviewSubmitError::Full(_))
        ));
        drop(gate_send);

        let (_root, source) = source_fixture();
        let cancel_calls = Arc::new(AtomicUsize::new(0));
        let mut registry = PreviewProviderRegistry::default();
        registry
            .register(Arc::new(TestProvider {
                id: "cancel",
                calls: Arc::clone(&cancel_calls),
                wait_for_cancel: true,
                fail: false,
            }))
            .expect("cancel provider");
        let cancelling = PreviewWorker::spawn(registry).expect("cancel worker");
        let generation = cancelling.begin_generation();
        cancelling
            .submit(request(generation, source))
            .expect("cancel request");
        while cancel_calls.load(Ordering::Acquire) == 0 {
            thread::yield_now();
        }
        cancelling.cancel();
        let cancelled = loop {
            if let Some(response) = cancelling.try_response() {
                break response;
            }
            thread::yield_now();
        };
        assert_eq!(cancelled.outcome, PreviewOutcome::Cancelled);

        let (_root, source) = source_fixture();
        let mut registry = PreviewProviderRegistry::default();
        registry
            .register(Arc::new(TestProvider {
                id: "failure",
                calls: Arc::new(AtomicUsize::new(0)),
                wait_for_cancel: false,
                fail: true,
            }))
            .expect("failure provider");
        let failing = PreviewWorker::spawn(registry).expect("failure worker");
        let generation = failing.begin_generation();
        failing
            .submit(request(generation, source.clone()))
            .expect("failure request");
        assert!(matches!(
            failing.submit(request(generation.wrapping_add(1), source)),
            Err(PreviewSubmitError::Stale(_))
        ));
        let failed = loop {
            if let Some(response) = failing.try_response() {
                break response;
            }
            thread::yield_now();
        };
        assert!(matches!(failed.outcome, PreviewOutcome::Failed(_)));
    }
}
