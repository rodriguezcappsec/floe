//! Bounded application worker for streaming local filename-search results.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use floe_core::{
    DirectoryEntry, FilenameSearchError, FilenameSearchLimits, FilenameSearchRequest,
    FilenameSearchSummary, search_filenames_with_mime,
};
use gtk::gio;

const SEARCH_REQUEST_CAPACITY: usize = 1;
const SEARCH_RESPONSE_CAPACITY: usize = 32;

struct SearchRequest {
    generation: u64,
    request: FilenameSearchRequest,
}

#[derive(Debug)]
pub enum FilenameSearchEventKind {
    Batch {
        entries: Vec<Arc<DirectoryEntry>>,
        summary: FilenameSearchSummary,
    },
    Finished(FilenameSearchSummary),
    Failed(FilenameSearchError),
}

#[derive(Debug)]
pub struct FilenameSearchEvent {
    pub generation: u64,
    pub kind: FilenameSearchEventKind,
}

#[derive(Debug)]
pub enum FilenameSearchSubmitError {
    Busy(Box<FilenameSearchRequest>),
    Stopped,
}

pub struct FilenameSearchWorker {
    sender: Option<SyncSender<SearchRequest>>,
    responses: Receiver<FilenameSearchEvent>,
    latest_generation: Arc<AtomicU64>,
    worker: Option<JoinHandle<()>>,
}

impl FilenameSearchWorker {
    pub fn spawn() -> std::io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<SearchRequest>(SEARCH_REQUEST_CAPACITY);
        let (response_sender, responses) =
            mpsc::sync_channel::<FilenameSearchEvent>(SEARCH_RESPONSE_CAPACITY);
        let latest_generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&latest_generation);
        let worker = thread::Builder::new()
            .name("floe-filename-search".to_owned())
            .spawn(move || {
                while let Ok(request) = requests.recv() {
                    if worker_generation.load(Ordering::Acquire) != request.generation {
                        continue;
                    }
                    let generation = request.generation;
                    let result = search_filenames_with_mime(
                        &request.request,
                        FilenameSearchLimits::default(),
                        || worker_generation.load(Ordering::Acquire) != generation,
                        |path| {
                            let (content_type, _) =
                                gio::content_type_guess(Some(path), None::<&[u8]>);
                            (!content_type.is_empty()).then(|| content_type.to_string())
                        },
                        |entries, summary| {
                            send_event(
                                &response_sender,
                                &worker_generation,
                                FilenameSearchEvent {
                                    generation,
                                    kind: FilenameSearchEventKind::Batch {
                                        entries: entries.into_iter().map(Arc::new).collect(),
                                        summary,
                                    },
                                },
                            )
                        },
                    );
                    let kind = match result {
                        Ok(summary) => FilenameSearchEventKind::Finished(summary),
                        Err(
                            FilenameSearchError::Cancelled | FilenameSearchError::ConsumerStopped,
                        ) if worker_generation.load(Ordering::Acquire) != generation => {
                            continue;
                        }
                        Err(error) => FilenameSearchEventKind::Failed(error),
                    };
                    if !send_event(
                        &response_sender,
                        &worker_generation,
                        FilenameSearchEvent { generation, kind },
                    ) {
                        continue;
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

    pub fn submit(
        &self,
        generation: u64,
        request: FilenameSearchRequest,
    ) -> Result<(), FilenameSearchSubmitError> {
        self.latest_generation.store(generation, Ordering::Release);
        let Some(sender) = &self.sender else {
            return Err(FilenameSearchSubmitError::Stopped);
        };
        match sender.try_send(SearchRequest {
            generation,
            request,
        }) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(request)) => {
                Err(FilenameSearchSubmitError::Busy(Box::new(request.request)))
            }
            Err(TrySendError::Disconnected(_)) => Err(FilenameSearchSubmitError::Stopped),
        }
    }

    pub fn cancel(&self, generation: u64) {
        self.latest_generation.store(generation, Ordering::Release);
    }

    pub fn try_event(&self) -> Option<FilenameSearchEvent> {
        self.responses.try_recv().ok()
    }
}

impl Drop for FilenameSearchWorker {
    fn drop(&mut self) {
        self.latest_generation.store(u64::MAX, Ordering::Release);
        self.sender.take();
        self.worker.take();
    }
}

fn send_event(
    sender: &SyncSender<FilenameSearchEvent>,
    latest_generation: &AtomicU64,
    mut event: FilenameSearchEvent,
) -> bool {
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
    use std::{fs, time::Duration};

    use floe_core::{AdvancedFilter, FilenameSearchScope, FolderFilterMode, HiddenFilter};

    use super::*;

    fn request(root: &std::path::Path, query: &str) -> FilenameSearchRequest {
        FilenameSearchRequest::new(
            root.to_path_buf(),
            query.to_owned(),
            FilenameSearchScope::Subtree,
            false,
        )
        .expect("valid search request")
    }

    fn events_until_finished(worker: &FilenameSearchWorker) -> Vec<FilenameSearchEvent> {
        let mut events = Vec::new();
        for _ in 0..400 {
            while let Some(event) = worker.try_event() {
                let finished = matches!(
                    event.kind,
                    FilenameSearchEventKind::Finished(_) | FilenameSearchEventKind::Failed(_)
                );
                events.push(event);
                if finished {
                    return events;
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("search worker did not finish");
    }

    #[test]
    fn phase_13b_filename_search_worker_streams_bounded_batches_and_summary() {
        let root = tempfile::tempdir().expect("search fixture");
        for index in 0..300 {
            fs::write(root.path().join(format!("result-{index}.txt")), b"x").expect("fixture file");
        }
        let worker = FilenameSearchWorker::spawn().expect("search worker");
        worker
            .submit(7, request(root.path(), "result"))
            .expect("submit search");
        let events = events_until_finished(&worker);
        let batches = events
            .iter()
            .filter_map(|event| match &event.kind {
                FilenameSearchEventKind::Batch { entries, .. } => Some(entries.len()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(batches, [128, 128, 44]);
        assert!(events.iter().all(|event| event.generation == 7));
        assert!(matches!(
            events.last().map(|event| &event.kind),
            Some(FilenameSearchEventKind::Finished(summary)) if summary.matched == 300
        ));
    }

    #[test]
    fn phase_13b_filename_search_worker_supersedes_stale_generation() {
        let root = tempfile::tempdir().expect("search fixture");
        for index in 0..1_000 {
            fs::write(root.path().join(format!("old-{index}.txt")), b"x").expect("fixture file");
        }
        fs::write(root.path().join("new-result.txt"), b"x").expect("new fixture");
        let worker = FilenameSearchWorker::spawn().expect("search worker");
        worker
            .submit(10, request(root.path(), "old"))
            .expect("old search");
        let mut pending = request(root.path(), "new-result");
        let mut submitted = false;
        for _ in 0..400 {
            match worker.submit(11, pending) {
                Ok(()) => {
                    submitted = true;
                    break;
                }
                Err(FilenameSearchSubmitError::Busy(returned)) => {
                    pending = *returned;
                    thread::sleep(Duration::from_millis(2));
                }
                Err(FilenameSearchSubmitError::Stopped) => panic!("worker stopped"),
            }
        }
        assert!(
            submitted,
            "new generation should eventually enter bounded queue"
        );
        let events = events_until_finished(&worker);
        assert!(events.iter().all(|event| event.generation == 11));
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            FilenameSearchEventKind::Batch { entries, .. }
                if entries.iter().any(|entry| entry.display_name_lossy() == "new-result.txt")
        )));
    }

    #[test]
    fn phase_13b_filename_search_worker_request_and_response_queues_are_fixed() {
        assert_eq!(SEARCH_REQUEST_CAPACITY, 1);
        assert_eq!(SEARCH_RESPONSE_CAPACITY, 32);
        let root = tempfile::tempdir().expect("search fixture");
        let worker = FilenameSearchWorker::spawn().expect("search worker");
        worker.cancel(99);
        assert_eq!(worker.latest_generation.load(Ordering::Acquire), 99);
        worker
            .submit(100, request(root.path(), "missing"))
            .expect("submit request");
        assert_eq!(worker.latest_generation.load(Ordering::Acquire), 100);
    }

    #[test]
    fn phase_13c_filename_search_worker_accepts_predicate_only_hidden_search() {
        let root = tempfile::tempdir().expect("advanced search fixture");
        fs::write(root.path().join("visible.txt"), b"visible").expect("visible");
        fs::write(root.path().join(".hidden.txt"), b"hidden").expect("hidden");
        let request = FilenameSearchRequest::new_with_filter(
            root.path().to_path_buf(),
            String::new(),
            FilenameSearchScope::Subtree,
            false,
            FolderFilterMode::Text,
            AdvancedFilter {
                hidden: HiddenFilter::Only,
                ..AdvancedFilter::default()
            },
        )
        .expect("predicate-only request");
        let worker = FilenameSearchWorker::spawn().expect("search worker");
        worker.submit(120, request).expect("submit advanced search");
        let events = events_until_finished(&worker);
        let names = events
            .iter()
            .filter_map(|event| match &event.kind {
                FilenameSearchEventKind::Batch { entries, .. } => Some(entries),
                _ => None,
            })
            .flatten()
            .map(|entry| entry.display_name_lossy())
            .collect::<Vec<_>>();
        assert_eq!(names, [".hidden.txt"]);
        assert!(events.iter().all(|event| event.generation == 120));
    }
}
