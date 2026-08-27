use std::{
    fs, io,
    os::unix::fs::MetadataExt,
    sync::{
        Arc, Mutex,
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

use floe_core::{
    AdvancedFilter, AdvancedFilterDecision, AdvancedFilterError, AdvancedMetadata,
    AdvancedMetadataNeeds, DirectoryEntry, FolderFilterError, FolderFilterMode,
    FolderFilterPattern,
};
use gtk::gio;
use thiserror::Error;

const FILTER_REQUEST_CAPACITY: usize = 1;

#[derive(Debug)]
struct FilterRequest {
    generation: u64,
    mode: FolderFilterMode,
    query: String,
    advanced: AdvancedFilter,
    entries: Arc<[Arc<DirectoryEntry>]>,
}

#[derive(Debug)]
pub struct FilterResponse {
    pub generation: u64,
    pub result: Result<Vec<Arc<DirectoryEntry>>, FolderFilterWorkError>,
}

#[derive(Debug, Error)]
pub enum FolderFilterWorkError {
    #[error(transparent)]
    Pattern(#[from] FolderFilterError),
    #[error(transparent)]
    Advanced(#[from] AdvancedFilterError),
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
                    let result = request
                        .advanced
                        .validate()
                        .map_err(FolderFilterWorkError::from)
                        .and_then(|()| {
                            FolderFilterPattern::compile_with_case(
                                request.mode,
                                &request.query,
                                request.advanced.match_case,
                            )
                            .map_err(FolderFilterWorkError::from)
                            .map(|pattern| {
                                request
                                    .entries
                                    .iter()
                                    .filter(|entry| {
                                        pattern.matches(entry.display_name())
                                            && advanced_matches(entry, &request.advanced)
                                    })
                                    .cloned()
                                    .collect()
                            })
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
        advanced: AdvancedFilter,
        entries: Arc<[Arc<DirectoryEntry>]>,
    ) -> Result<(), FilterSubmitError> {
        let Some(sender) = &self.sender else {
            return Err(FilterSubmitError::Stopped);
        };
        match sender.try_send(FilterRequest {
            generation,
            mode,
            query,
            advanced,
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

fn advanced_matches(entry: &DirectoryEntry, filter: &AdvancedFilter) -> bool {
    match filter.evaluate(entry, None) {
        AdvancedFilterDecision::Match => true,
        AdvancedFilterDecision::NoMatch => false,
        AdvancedFilterDecision::NeedsMetadata(needs) => {
            filter.evaluate(entry, Some(&load_advanced_metadata(entry, needs)))
                == AdvancedFilterDecision::Match
        }
    }
}

fn load_advanced_metadata(
    entry: &DirectoryEntry,
    needs: AdvancedMetadataNeeds,
) -> AdvancedMetadata {
    let owner_uid = needs
        .owner
        .then(|| {
            fs::symlink_metadata(entry.path())
                .ok()
                .map(|metadata| metadata.uid())
        })
        .flatten();
    let mime = needs
        .mime
        .then(|| {
            let (content_type, _) = gio::content_type_guess(Some(entry.path()), None::<&[u8]>);
            (!content_type.is_empty()).then(|| content_type.to_string())
        })
        .flatten();
    AdvancedMetadata { mime, owner_uid }
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
                AdvancedFilter::default(),
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
                AdvancedFilter::default(),
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
                    AdvancedFilter::default(),
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
            .submit(
                40,
                FolderFilterMode::Text,
                "item".to_owned(),
                AdvancedFilter::default(),
                entries(1),
            )
            .expect("submit first generation");
        loop {
            match worker.submit(
                41,
                FolderFilterMode::Text,
                "item".to_owned(),
                AdvancedFilter::default(),
                entries(1),
            ) {
                Ok(()) => break,
                Err(FilterSubmitError::Busy(_)) => thread::yield_now(),
                Err(FilterSubmitError::Stopped) => panic!("filter worker stopped"),
            }
        }
        thread::sleep(Duration::from_millis(20));
        let response = worker.try_response().expect("latest response");
        assert_eq!(response.generation, 41);
    }

    #[test]
    fn phase_13c_filter_worker_combines_predicates_and_lazy_owner() {
        use floe_core::{EntryTypeFilter, OwnerFilter};

        let directory = tempfile::tempdir().expect("advanced filter fixture");
        fs::write(directory.path().join("keep.TXT"), b"large enough").expect("keep");
        fs::write(directory.path().join("skip.log"), b"large enough").expect("skip");
        let entries: Arc<[Arc<DirectoryEntry>]> = floe_core::enumerate_directory(directory.path())
            .expect("enumerate advanced fixture")
            .into_entries()
            .into_iter()
            .map(Arc::new)
            .collect::<Vec<_>>()
            .into();
        let worker = FolderFilterWorker::spawn().expect("folder filter worker");
        worker
            .submit(
                80,
                FolderFilterMode::Text,
                String::new(),
                AdvancedFilter {
                    entry_type: EntryTypeFilter::File,
                    extension: Some("txt".to_owned()),
                    minimum_size: Some(5),
                    owner: Some(OwnerFilter::Uid(rustix::process::getuid().as_raw())),
                    ..AdvancedFilter::default()
                },
                entries,
            )
            .expect("submit advanced filter");
        let response = (0..200)
            .find_map(|_| {
                thread::sleep(Duration::from_millis(5));
                worker.try_response()
            })
            .expect("advanced response");
        let matches = response.result.expect("advanced filter success");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].display_name_lossy(), "keep.TXT");
    }
}
