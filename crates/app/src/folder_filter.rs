use std::{
    io,
    sync::{
        Arc, Mutex,
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

use floe_core::{DirectoryEntry, FolderFilterError, FolderFilterMode, FolderFilterPattern};

const FILTER_REQUEST_CAPACITY: usize = 1;

#[derive(Debug)]
struct FilterRequest {
    generation: u64,
    mode: FolderFilterMode,
    query: String,
    entries: Arc<[Arc<DirectoryEntry>]>,
}

#[derive(Debug)]
pub struct FilterResponse {
    pub generation: u64,
    pub result: Result<Vec<Arc<DirectoryEntry>>, FolderFilterError>,
}

#[derive(Debug)]
pub enum FilterSubmitError {
    Busy(Arc<[Arc<DirectoryEntry>]>),
    Stopped,
}

pub struct FolderFilterWorker {
    sender: Option<SyncSender<FilterRequest>>,
    latest_response: Arc<Mutex<Option<FilterResponse>>>,
    worker: Option<JoinHandle<()>>,
}

impl FolderFilterWorker {
    pub fn spawn() -> io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<FilterRequest>(FILTER_REQUEST_CAPACITY);
        let latest_response = Arc::new(Mutex::new(None::<FilterResponse>));
        let worker_response = Arc::clone(&latest_response);
        let worker = thread::Builder::new()
            .name("floe-folder-filter".to_owned())
            .spawn(move || {
                while let Ok(request) = requests.recv() {
                    let result =
                        FolderFilterPattern::compile(request.mode, &request.query).map(|pattern| {
                            request
                                .entries
                                .iter()
                                .filter(|entry| pattern.matches(entry.display_name()))
                                .cloned()
                                .collect()
                        });
                    let response = FilterResponse {
                        generation: request.generation,
                        result,
                    };
                    let Ok(mut latest) = worker_response.lock() else {
                        break;
                    };
                    if latest
                        .as_ref()
                        .is_none_or(|previous| response.generation >= previous.generation)
                    {
                        *latest = Some(response);
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            latest_response,
            worker: Some(worker),
        })
    }

    pub fn submit(
        &self,
        generation: u64,
        mode: FolderFilterMode,
        query: String,
        entries: Arc<[Arc<DirectoryEntry>]>,
    ) -> Result<(), FilterSubmitError> {
        let Some(sender) = &self.sender else {
            return Err(FilterSubmitError::Stopped);
        };
        match sender.try_send(FilterRequest {
            generation,
            mode,
            query,
            entries,
        }) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(request)) => Err(FilterSubmitError::Busy(request.entries)),
            Err(TrySendError::Disconnected(_)) => Err(FilterSubmitError::Stopped),
        }
    }

    pub fn try_response(&self) -> Option<FilterResponse> {
        self.latest_response.lock().ok()?.take()
    }
}

impl Drop for FolderFilterWorker {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::warn!("folder filter worker panicked during shutdown");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, time::Duration};

    use super::*;

    fn entries(count: usize) -> Arc<[Arc<DirectoryEntry>]> {
        let directory = tempfile::tempdir().expect("temporary filter fixture");
        fs::write(directory.path().join("item-999999.txt"), b"test").expect("write filter fixture");
        let entry = Arc::new(
            floe_core::enumerate_directory(directory.path())
                .expect("enumerate filter fixture")
                .into_entries()
                .pop()
                .expect("one filter fixture entry"),
        );
        vec![entry; count].into()
    }

    #[test]
    fn phase_13a_filter_worker_handles_one_hundred_thousand_entries_in_order() {
        let worker = FolderFilterWorker::spawn().expect("folder filter worker");
        worker
            .submit(
                7,
                FolderFilterMode::Glob,
                "*9999*.txt".to_owned(),
                entries(100_000),
            )
            .expect("submit large filter");
        let response = (0..200)
            .find_map(|_| {
                thread::sleep(Duration::from_millis(5));
                worker.try_response()
            })
            .expect("bounded filter should complete");
        assert_eq!(response.generation, 7);
        let matches = response.result.expect("successful filter response");
        assert_eq!(matches.len(), 100_000);
        assert!(
            matches
                .windows(2)
                .all(|pair| Arc::ptr_eq(&pair[0], &pair[1]))
        );
    }

    #[test]
    fn phase_13a_filter_worker_has_bounded_request_capacity() {
        let worker = FolderFilterWorker::spawn().expect("folder filter worker");
        worker
            .submit(
                1,
                FolderFilterMode::Text,
                "no-match".to_owned(),
                entries(100_000),
            )
            .expect("submit capacity fixture");
        let mut observed_busy = false;
        for generation in 2..100 {
            if worker
                .submit(
                    generation,
                    FolderFilterMode::Text,
                    "item".to_owned(),
                    entries(1),
                )
                .is_err_and(|error| matches!(error, FilterSubmitError::Busy(_)))
            {
                observed_busy = true;
                break;
            }
        }
        assert!(observed_busy);
    }

    #[test]
    fn phase_13a_filter_worker_latest_generation_supersedes_older_response() {
        let worker = FolderFilterWorker::spawn().expect("folder filter worker");
        worker
            .submit(40, FolderFilterMode::Text, "item".to_owned(), entries(1))
            .expect("submit first generation");
        loop {
            match worker.submit(41, FolderFilterMode::Text, "item".to_owned(), entries(1)) {
                Ok(()) => break,
                Err(FilterSubmitError::Busy(_)) => thread::yield_now(),
                Err(FilterSubmitError::Stopped) => panic!("filter worker stopped"),
            }
        }
        thread::sleep(Duration::from_millis(20));
        let response = worker.try_response().expect("latest response");
        assert_eq!(response.generation, 41);
    }
}
