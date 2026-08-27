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
    ContentSearchError, ContentSearchLimits, ContentSearchMatch, ContentSearchRequest,
    ContentSearchSummary, search_contents_with_mime,
};
use gtk::gio;

const CONTENT_REQUEST_CAPACITY: usize = 1;
const CONTENT_RESPONSE_CAPACITY: usize = 32;

#[derive(Debug)]
struct SearchRequest {
    generation: u64,
    request: ContentSearchRequest,
}

#[derive(Debug)]
pub enum ContentSearchEventKind {
    Batch {
        matches: Vec<Arc<ContentSearchMatch>>,
        summary: ContentSearchSummary,
    },
    Finished(ContentSearchSummary),
    Failed(ContentSearchError),
}

#[derive(Debug)]
pub struct ContentSearchEvent {
    pub generation: u64,
    pub kind: ContentSearchEventKind,
}

#[derive(Debug)]
pub enum ContentSearchSubmitError {
    Busy(Box<ContentSearchRequest>),
    Stopped,
}

pub struct ContentSearchWorker {
    sender: Option<SyncSender<SearchRequest>>,
    responses: Receiver<ContentSearchEvent>,
    latest_generation: Arc<AtomicU64>,
    worker: Option<JoinHandle<()>>,
}

impl ContentSearchWorker {
    pub fn spawn() -> std::io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<SearchRequest>(CONTENT_REQUEST_CAPACITY);
        let (response_sender, responses) =
            mpsc::sync_channel::<ContentSearchEvent>(CONTENT_RESPONSE_CAPACITY);
        let latest_generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&latest_generation);
        let worker = thread::Builder::new()
            .name("floe-content-search".to_owned())
            .spawn(move || {
                while let Ok(request) = requests.recv() {
                    if worker_generation.load(Ordering::Acquire) != request.generation {
                        continue;
                    }
                    let generation = request.generation;
                    let result = search_contents_with_mime(
                        &request.request,
                        ContentSearchLimits::default(),
                        || worker_generation.load(Ordering::Acquire) != generation,
                        |path| {
                            let (content_type, _) =
                                gio::content_type_guess(Some(path), None::<&[u8]>);
                            (!content_type.is_empty()).then(|| content_type.to_string())
                        },
                        |matches, summary| {
                            send_event(
                                &response_sender,
                                &worker_generation,
                                ContentSearchEvent {
                                    generation,
                                    kind: ContentSearchEventKind::Batch {
                                        matches: matches.into_iter().map(Arc::new).collect(),
                                        summary,
                                    },
                                },
                            )
                        },
                    );
                    let kind = match result {
                        Ok(summary) => ContentSearchEventKind::Finished(summary),
                        Err(
                            ContentSearchError::Cancelled | ContentSearchError::ConsumerStopped,
                        ) if worker_generation.load(Ordering::Acquire) != generation => {
                            continue;
                        }
                        Err(error) => ContentSearchEventKind::Failed(error),
                    };
                    if !send_event(
                        &response_sender,
                        &worker_generation,
                        ContentSearchEvent { generation, kind },
                    ) {
                        break;
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
        request: ContentSearchRequest,
    ) -> Result<(), ContentSearchSubmitError> {
        self.latest_generation.store(generation, Ordering::Release);
        let Some(sender) = &self.sender else {
            return Err(ContentSearchSubmitError::Stopped);
        };
        match sender.try_send(SearchRequest {
            generation,
            request,
        }) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(request)) => {
                Err(ContentSearchSubmitError::Busy(Box::new(request.request)))
            }
            Err(TrySendError::Disconnected(_)) => Err(ContentSearchSubmitError::Stopped),
        }
    }

    pub fn cancel(&self, generation: u64) {
        self.latest_generation.store(generation, Ordering::Release);
    }

    pub fn try_event(&self) -> Option<ContentSearchEvent> {
        self.responses.try_recv().ok()
    }
}

impl Drop for ContentSearchWorker {
    fn drop(&mut self) {
        self.latest_generation.store(u64::MAX, Ordering::Release);
        self.sender.take();
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("content-search worker panicked during shutdown");
        }
    }
}

fn send_event(
    sender: &SyncSender<ContentSearchEvent>,
    latest_generation: &AtomicU64,
    mut event: ContentSearchEvent,
) -> bool {
    loop {
        if latest_generation.load(Ordering::Acquire) != event.generation {
            return true;
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
    use std::{fs, path::Path, time::Duration};

    use floe_core::{AdvancedFilter, FilenameSearchScope, FolderFilterMode};

    use super::*;

    fn request(root: &Path, query: &str) -> ContentSearchRequest {
        ContentSearchRequest::new(
            root.to_path_buf(),
            query.to_owned(),
            FilenameSearchScope::Subtree,
            false,
            FolderFilterMode::Text,
            AdvancedFilter::default(),
        )
        .expect("valid request")
    }

    fn events_until_terminal(worker: &ContentSearchWorker) -> Vec<ContentSearchEvent> {
        let mut events = Vec::new();
        for _ in 0..400 {
            while let Some(event) = worker.try_event() {
                let terminal = matches!(
                    event.kind,
                    ContentSearchEventKind::Finished(_) | ContentSearchEventKind::Failed(_)
                );
                events.push(event);
                if terminal {
                    return events;
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("content-search worker did not finish");
    }

    #[test]
    fn phase_13d_content_search_worker_streams_results_and_summary() {
        let root = tempfile::tempdir().expect("worker fixture");
        fs::write(root.path().join("one.txt"), b"needle here").expect("fixture");
        let worker = ContentSearchWorker::spawn().expect("worker");
        worker
            .submit(7, request(root.path(), "needle"))
            .expect("submit");
        let events = events_until_terminal(&worker);
        assert!(events.iter().all(|event| event.generation == 7));
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            ContentSearchEventKind::Batch { matches, .. } if matches.len() == 1
        )));
        assert!(matches!(
            events.last().map(|event| &event.kind),
            Some(ContentSearchEventKind::Finished(summary)) if summary.matched == 1
        ));
    }

    #[test]
    fn phase_13d_content_search_worker_supersedes_stale_generation() {
        let root = tempfile::tempdir().expect("generation fixture");
        fs::write(root.path().join("one.txt"), b"needle here").expect("fixture");
        let worker = ContentSearchWorker::spawn().expect("worker");
        worker
            .submit(10, request(root.path(), "needle"))
            .expect("submit");
        worker.cancel(11);
        thread::sleep(Duration::from_millis(20));
        assert!(worker.try_event().is_none());
        worker
            .submit(12, request(root.path(), "needle"))
            .expect("submit current");
        assert!(
            events_until_terminal(&worker)
                .iter()
                .all(|event| event.generation == 12)
        );
    }
}
