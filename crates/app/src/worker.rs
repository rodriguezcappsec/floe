use std::{
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
};

use floe_core::{DirectoryEntry, DirectoryError, DirectorySort, enumerate_directory_with_cancel};

enum RequestKind {
    Enumerate {
        sort: DirectorySort,
    },
    Sort {
        entries: Vec<Arc<DirectoryEntry>>,
        sort: DirectorySort,
    },
}

struct Request {
    generation: u64,
    path: PathBuf,
    kind: RequestKind,
}

pub enum ResponseKind {
    Listing(Result<Vec<DirectoryEntry>, DirectoryError>),
    Sorted {
        entries: Vec<Arc<DirectoryEntry>>,
        sort: DirectorySort,
    },
}

pub struct Response {
    pub generation: u64,
    pub path: PathBuf,
    pub kind: ResponseKind,
}

/// A single bounded-concurrency directory and sorting worker with supersession.
pub struct BrowserWorker {
    requests: Sender<Request>,
    responses: Receiver<Response>,
    latest_generation: Arc<AtomicU64>,
    next_generation: u64,
}

impl BrowserWorker {
    pub fn spawn() -> io::Result<Self> {
        let (request_sender, request_receiver) = mpsc::channel::<Request>();
        let (response_sender, response_receiver) = mpsc::channel::<Response>();
        let latest_generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&latest_generation);

        std::thread::Builder::new()
            .name("floe-directory-worker".into())
            .spawn(move || {
                while let Ok(request) = request_receiver.recv() {
                    let generation = request.generation;
                    let path = request.path;
                    if worker_generation.load(Ordering::Acquire) != generation {
                        continue;
                    }

                    let kind = match request.kind {
                        RequestKind::Enumerate { sort } => {
                            let result = enumerate_directory_with_cancel(&path, || {
                                worker_generation.load(Ordering::Acquire) != generation
                            })
                            .map(|listing| {
                                let mut entries = listing.into_entries();
                                sort.sort_entries(&mut entries);
                                entries
                            });
                            ResponseKind::Listing(result)
                        }
                        RequestKind::Sort { mut entries, sort } => {
                            entries.sort_by(|left, right| sort.compare_entries(left, right));
                            ResponseKind::Sorted { entries, sort }
                        }
                    };

                    if worker_generation.load(Ordering::Acquire) != generation {
                        continue;
                    }
                    if response_sender
                        .send(Response {
                            generation,
                            path,
                            kind,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;

        Ok(Self {
            requests: request_sender,
            responses: response_receiver,
            latest_generation,
            next_generation: 0,
        })
    }

    pub fn request(&mut self, path: PathBuf, sort: DirectorySort) -> u64 {
        self.submit(path, RequestKind::Enumerate { sort })
    }

    pub fn request_sort(
        &mut self,
        path: PathBuf,
        entries: Vec<Arc<DirectoryEntry>>,
        sort: DirectorySort,
    ) -> u64 {
        self.submit(path, RequestKind::Sort { entries, sort })
    }

    fn submit(&mut self, path: PathBuf, kind: RequestKind) -> u64 {
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        let generation = self.next_generation;
        self.latest_generation.store(generation, Ordering::Release);
        if let Err(error) = self.requests.send(Request {
            generation,
            path,
            kind,
        }) {
            tracing::error!(%error, "directory worker stopped accepting requests");
        }
        generation
    }

    pub fn try_response(&self) -> Option<Response> {
        self.responses.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc, thread, time::Duration};

    use floe_core::{DirectorySort, SortColumn, SortDirection, enumerate_directory};
    use tempfile::tempdir;

    use super::{BrowserWorker, ResponseKind};

    #[test]
    fn phase_6b_worker_sorts_shared_entries_and_reports_policy() {
        let directory = tempdir().expect("temporary directory should be created");
        fs::create_dir(directory.path().join("folder")).expect("folder should be created");
        fs::write(directory.path().join("small"), b"1").expect("small file should be created");
        fs::write(directory.path().join("large"), b"1234").expect("large file should be created");
        let entries = enumerate_directory(directory.path())
            .expect("directory should enumerate")
            .into_entries()
            .into_iter()
            .map(Arc::new)
            .collect();
        let sort = DirectorySort::new(SortColumn::Size, SortDirection::Descending);
        let mut worker = BrowserWorker::spawn().expect("worker should start");
        let generation = worker.request_sort(directory.path().to_path_buf(), entries, sort);

        let response = (0..100)
            .find_map(|_| {
                let response = worker.try_response();
                if response.is_none() {
                    thread::sleep(Duration::from_millis(5));
                }
                response
            })
            .expect("sort response should arrive");

        assert_eq!(response.generation, generation);
        let ResponseKind::Sorted {
            entries,
            sort: response_sort,
        } = response.kind
        else {
            panic!("worker should return a sorted response");
        };
        assert_eq!(response_sort, sort);
        let names: Vec<_> = entries
            .iter()
            .map(|entry| entry.display_name_lossy())
            .collect();
        assert_eq!(names, ["folder", "large", "small"]);
    }
}
