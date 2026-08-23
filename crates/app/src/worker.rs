use std::{
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
};

use floe_core::{DirectoryError, DirectoryListing, enumerate_directory_with_cancel};

struct Request {
    generation: u64,
    path: PathBuf,
}

pub struct Response {
    pub generation: u64,
    pub path: PathBuf,
    pub result: Result<DirectoryListing, DirectoryError>,
}

/// A single bounded-concurrency filesystem worker with cooperative supersession.
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
                    let result = enumerate_directory_with_cancel(&path, || {
                        worker_generation.load(Ordering::Acquire) != generation
                    });
                    if response_sender
                        .send(Response {
                            generation,
                            path,
                            result,
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

    pub fn request(&mut self, path: PathBuf) -> u64 {
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        let generation = self.next_generation;
        self.latest_generation.store(generation, Ordering::Release);
        if let Err(error) = self.requests.send(Request { generation, path }) {
            tracing::error!(%error, "directory worker stopped accepting requests");
        }
        generation
    }

    pub fn try_response(&self) -> Option<Response> {
        self.responses.try_recv().ok()
    }
}
