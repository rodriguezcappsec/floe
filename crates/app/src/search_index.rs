//! Bounded application worker for Floe's optional filename/metadata index.

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use floe_core::{
    DirectoryEntry, FILENAME_SEARCH_BATCH_CAPACITY, FilenameSearchRequest, FilenameSearchSummary,
    SearchIndex, SearchIndexBuildRequest, SearchIndexBuildSummary, SearchIndexError,
    SearchIndexLimits, build_search_index,
};
use gtk::gio;

const REQUEST_CAPACITY: usize = 1;
const RESPONSE_CAPACITY: usize = 32;
const CACHE_FILE_NAME: &str = "search-index-v1";

#[derive(Debug)]
enum SearchIndexCommand {
    Build(SearchIndexBuildRequest),
    Query(Box<FilenameSearchRequest>),
    Clear,
}

#[derive(Debug)]
struct SearchIndexRequest {
    generation: u64,
    command: SearchIndexCommand,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchIndexFallbackReason {
    Missing,
    Stale,
    Ineligible,
    Corrupt,
}

impl SearchIndexFallbackReason {
    pub const fn description(self) -> &'static str {
        match self {
            Self::Missing => "no current index",
            Self::Stale => "the index was stale",
            Self::Ineligible => "this query is outside the index policy",
            Self::Corrupt => "the index could not be read safely",
        }
    }
}

#[derive(Debug)]
pub enum SearchIndexEventKind {
    Built(SearchIndexBuildSummary),
    Batch(Vec<Arc<DirectoryEntry>>, FilenameSearchSummary),
    Finished(FilenameSearchSummary),
    Fallback {
        request: Box<FilenameSearchRequest>,
        reason: SearchIndexFallbackReason,
    },
    Cleared,
    Failed(String),
}

#[derive(Debug)]
pub struct SearchIndexEvent {
    pub generation: u64,
    pub kind: SearchIndexEventKind,
}

#[derive(Debug)]
pub enum SearchIndexSubmitError {
    Busy,
    Stopped,
}

pub struct SearchIndexWorker {
    sender: Option<SyncSender<SearchIndexRequest>>,
    responses: Receiver<SearchIndexEvent>,
    latest_generation: Arc<AtomicU64>,
    worker: Option<JoinHandle<()>>,
}

impl SearchIndexWorker {
    pub fn spawn() -> io::Result<Self> {
        Self::spawn_internal(glib::user_cache_dir().join("floe").join(CACHE_FILE_NAME))
    }

    fn spawn_internal(path: PathBuf) -> io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<SearchIndexRequest>(REQUEST_CAPACITY);
        let (response_sender, responses) =
            mpsc::sync_channel::<SearchIndexEvent>(RESPONSE_CAPACITY);
        let latest_generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&latest_generation);
        let worker = thread::Builder::new()
            .name("floe-search-index".to_owned())
            .spawn(move || {
                let mut loaded = load_index(&path);
                while let Ok(request) = requests.recv() {
                    if worker_generation.load(Ordering::Acquire) != request.generation {
                        continue;
                    }
                    let generation = request.generation;
                    match request.command {
                        SearchIndexCommand::Build(build_request) => {
                            let result = build_search_index(
                                &build_request,
                                SearchIndexLimits::default(),
                                || worker_generation.load(Ordering::Acquire) != generation,
                            )
                            .and_then(|index| {
                                persist_index(&path, &index)
                                    .map_err(SearchIndexError::RootMetadata)?;
                                Ok(index)
                            });
                            match result {
                                Ok(index) => {
                                    let summary = index.summary();
                                    loaded = LoadedIndex::Ready(index);
                                    send_event(
                                        &response_sender,
                                        &worker_generation,
                                        SearchIndexEvent {
                                            generation,
                                            kind: SearchIndexEventKind::Built(summary),
                                        },
                                    );
                                }
                                Err(SearchIndexError::Cancelled) => {}
                                Err(error) => {
                                    send_event(
                                        &response_sender,
                                        &worker_generation,
                                        SearchIndexEvent {
                                            generation,
                                            kind: SearchIndexEventKind::Failed(error.to_string()),
                                        },
                                    );
                                }
                            }
                        }
                        SearchIndexCommand::Query(search_request) => {
                            let reason = match &loaded {
                                LoadedIndex::Ready(index) => match index.search_with_mime(
                                    &search_request,
                                    || worker_generation.load(Ordering::Acquire) != generation,
                                    |path| {
                                        let (content_type, _) =
                                            gio::content_type_guess(Some(path), None::<&[u8]>);
                                        (!content_type.is_empty()).then(|| content_type.to_string())
                                    },
                                ) {
                                    Ok((entries, summary)) => {
                                        for batch in entries.chunks(FILENAME_SEARCH_BATCH_CAPACITY)
                                        {
                                            if !send_event(
                                                &response_sender,
                                                &worker_generation,
                                                SearchIndexEvent {
                                                    generation,
                                                    kind: SearchIndexEventKind::Batch(
                                                        batch
                                                            .iter()
                                                            .cloned()
                                                            .map(Arc::new)
                                                            .collect(),
                                                        summary,
                                                    ),
                                                },
                                            ) {
                                                break;
                                            }
                                        }
                                        send_event(
                                            &response_sender,
                                            &worker_generation,
                                            SearchIndexEvent {
                                                generation,
                                                kind: SearchIndexEventKind::Finished(summary),
                                            },
                                        );
                                        continue;
                                    }
                                    Err(SearchIndexError::Cancelled) => continue,
                                    Err(SearchIndexError::Stale) => {
                                        loaded = LoadedIndex::Missing;
                                        let _ = remove_index(&path);
                                        SearchIndexFallbackReason::Stale
                                    }
                                    Err(SearchIndexError::Ineligible) => {
                                        SearchIndexFallbackReason::Ineligible
                                    }
                                    Err(_) => SearchIndexFallbackReason::Corrupt,
                                },
                                LoadedIndex::Missing => SearchIndexFallbackReason::Missing,
                                LoadedIndex::Corrupt => SearchIndexFallbackReason::Corrupt,
                            };
                            send_event(
                                &response_sender,
                                &worker_generation,
                                SearchIndexEvent {
                                    generation,
                                    kind: SearchIndexEventKind::Fallback {
                                        request: search_request,
                                        reason,
                                    },
                                },
                            );
                        }
                        SearchIndexCommand::Clear => match remove_index(&path) {
                            Ok(()) => {
                                loaded = LoadedIndex::Missing;
                                send_event(
                                    &response_sender,
                                    &worker_generation,
                                    SearchIndexEvent {
                                        generation,
                                        kind: SearchIndexEventKind::Cleared,
                                    },
                                );
                            }
                            Err(error) => {
                                send_event(
                                    &response_sender,
                                    &worker_generation,
                                    SearchIndexEvent {
                                        generation,
                                        kind: SearchIndexEventKind::Failed(error.to_string()),
                                    },
                                );
                            }
                        },
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            responses,
            latest_generation,
            worker: Some(worker),
        })
    }

    pub fn build(
        &self,
        generation: u64,
        request: SearchIndexBuildRequest,
    ) -> Result<(), SearchIndexSubmitError> {
        self.submit(generation, SearchIndexCommand::Build(request))
    }

    pub fn query(
        &self,
        generation: u64,
        request: FilenameSearchRequest,
    ) -> Result<(), SearchIndexSubmitError> {
        self.submit(generation, SearchIndexCommand::Query(Box::new(request)))
    }

    pub fn clear(&self, generation: u64) -> Result<(), SearchIndexSubmitError> {
        self.submit(generation, SearchIndexCommand::Clear)
    }

    fn submit(
        &self,
        generation: u64,
        command: SearchIndexCommand,
    ) -> Result<(), SearchIndexSubmitError> {
        self.latest_generation.store(generation, Ordering::Release);
        let Some(sender) = self.sender.as_ref() else {
            return Err(SearchIndexSubmitError::Stopped);
        };
        match sender.try_send(SearchIndexRequest {
            generation,
            command,
        }) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(SearchIndexSubmitError::Busy),
            Err(TrySendError::Disconnected(_)) => Err(SearchIndexSubmitError::Stopped),
        }
    }

    pub fn cancel(&self, generation: u64) {
        self.latest_generation.store(generation, Ordering::Release);
    }

    pub fn try_event(&self) -> Option<SearchIndexEvent> {
        self.responses.try_recv().ok()
    }
}

impl Drop for SearchIndexWorker {
    fn drop(&mut self) {
        self.latest_generation.store(u64::MAX, Ordering::Release);
        self.sender.take();
        self.worker.take();
    }
}

enum LoadedIndex {
    Ready(SearchIndex),
    Missing,
    Corrupt,
}

fn load_index(path: &Path) -> LoadedIndex {
    if fs::metadata(path)
        .ok()
        .is_some_and(|metadata| metadata.permissions().mode() & 0o077 != 0)
    {
        return LoadedIndex::Corrupt;
    }
    match fs::read(path) {
        Ok(bytes) => SearchIndex::parse(&bytes)
            .map(LoadedIndex::Ready)
            .unwrap_or(LoadedIndex::Corrupt),
        Err(error) if error.kind() == io::ErrorKind::NotFound => LoadedIndex::Missing,
        Err(_) => LoadedIndex::Corrupt,
    }
}

fn persist_index(path: &Path, index: &SearchIndex) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "index path has no parent"))?;
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder.create(parent)?;
    let temporary = path.with_extension("tmp");
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true).mode(0o600);
    let mut file = options.open(&temporary)?;
    file.write_all(&index.serialize().map_err(io::Error::other)?)?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

fn remove_index(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn send_event(
    sender: &SyncSender<SearchIndexEvent>,
    latest_generation: &AtomicU64,
    event: SearchIndexEvent,
) -> bool {
    if latest_generation.load(Ordering::Acquire) != event.generation {
        return false;
    }
    let mut event = event;
    loop {
        if latest_generation.load(Ordering::Acquire) != event.generation {
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

    use floe_core::{FilenameSearchScope, SearchIndexBuildRequest};
    use tempfile::tempdir;

    use super::*;

    fn event(worker: &SearchIndexWorker) -> SearchIndexEvent {
        for _ in 0..400 {
            if let Some(event) = worker.try_event() {
                return event;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("worker event timed out")
    }

    #[test]
    fn phase_13f_search_index_worker_persists_privately_queries_and_clears() {
        let fixture = tempdir().expect("fixture");
        let root = fixture.path().join("root");
        fs::create_dir(&root).expect("root");
        fs::write(root.join("report.txt"), b"report").expect("file");
        let cache = fixture.path().join("cache/search-index-v1");
        let worker = SearchIndexWorker::spawn_internal(cache.clone()).expect("worker");
        worker
            .build(
                1,
                SearchIndexBuildRequest::new(root.clone()).expect("request"),
            )
            .expect("build");
        assert!(matches!(
            event(&worker).kind,
            SearchIndexEventKind::Built(_)
        ));
        assert_eq!(
            fs::metadata(&cache)
                .expect("cache metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        worker
            .query(
                2,
                FilenameSearchRequest::new(
                    root,
                    "report".to_owned(),
                    FilenameSearchScope::Subtree,
                    false,
                )
                .expect("search request"),
            )
            .expect("query");
        assert!(matches!(
            event(&worker).kind,
            SearchIndexEventKind::Batch(_, _)
        ));
        assert!(matches!(
            event(&worker).kind,
            SearchIndexEventKind::Finished(_)
        ));
        worker.clear(3).expect("clear");
        assert!(matches!(event(&worker).kind, SearchIndexEventKind::Cleared));
        assert!(!cache.exists());
    }

    #[test]
    fn phase_13f_search_index_worker_returns_explicit_missing_and_stale_fallbacks() {
        let fixture = tempdir().expect("fixture");
        let root = fixture.path().join("root");
        fs::create_dir(&root).expect("root");
        fs::write(root.join("alpha"), b"one").expect("file");
        let worker =
            SearchIndexWorker::spawn_internal(fixture.path().join("index")).expect("worker");
        let search = || {
            FilenameSearchRequest::new(
                root.clone(),
                "alpha".to_owned(),
                FilenameSearchScope::Subtree,
                false,
            )
            .expect("search")
        };
        worker.query(1, search()).expect("missing query");
        assert!(matches!(
            event(&worker).kind,
            SearchIndexEventKind::Fallback {
                reason: SearchIndexFallbackReason::Missing,
                ..
            }
        ));
        worker
            .build(
                2,
                SearchIndexBuildRequest::new(root.clone()).expect("request"),
            )
            .expect("build");
        assert!(matches!(
            event(&worker).kind,
            SearchIndexEventKind::Built(_)
        ));
        fs::write(root.join("new"), b"two").expect("change root");
        worker.query(3, search()).expect("stale query");
        assert!(matches!(
            event(&worker).kind,
            SearchIndexEventKind::Fallback {
                reason: SearchIndexFallbackReason::Stale,
                ..
            }
        ));
    }
}
