//! Bounded application worker for duplicate discovery.

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
    ChecksumAlgorithm, ChecksumRequest, DuplicateHashError, DuplicateScanError,
    DuplicateScanLimits, DuplicateScanOutcome, DuplicateScanRequest, DuplicateScanSummary,
    ExpectedDigest, find_duplicates,
};

use crate::checksum_executor::{ChecksumError, execute_checksum};

const REQUEST_CAPACITY: usize = 1;
const RESPONSE_CAPACITY: usize = 32;

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
    worker: Option<JoinHandle<()>>,
}

impl DuplicateFinderWorker {
    pub fn spawn() -> std::io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<WorkerRequest>(REQUEST_CAPACITY);
        let (response_sender, responses) =
            mpsc::sync_channel::<DuplicateFinderEvent>(RESPONSE_CAPACITY);
        let latest_generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&latest_generation);
        let worker = thread::Builder::new()
            .name("floe-duplicate-finder".to_owned())
            .spawn(move || {
                while let Ok(work) = requests.recv() {
                    if worker_generation.load(Ordering::Acquire) != work.generation {
                        continue;
                    }
                    let generation = work.generation;
                    let result = find_duplicates(
                        &work.request,
                        DuplicateScanLimits::default(),
                        || worker_generation.load(Ordering::Acquire) != generation,
                        |path| {
                            let request = ChecksumRequest::new(
                                vec![path.to_path_buf()],
                                ChecksumAlgorithm::Sha256,
                                None,
                            )
                            .map_err(|error| DuplicateHashError::Failed(error.to_string()))?;
                            let outcome = execute_checksum(
                                &request,
                                || worker_generation.load(Ordering::Acquire) != generation,
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
                            digest.bytes().try_into().map_err(|_| {
                                DuplicateHashError::Failed(
                                    "SHA-256 result length was invalid".to_owned(),
                                )
                            })
                        },
                        |summary| {
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
        fs::write(fixture.path().join("a"), b"same").expect("a");
        fs::write(fixture.path().join("b"), b"same").expect("b");
        let worker = DuplicateFinderWorker::spawn().expect("worker");
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
        for index in 0..100 {
            fs::write(
                fixture.path().join(format!("{index}")),
                vec![7u8; 128 * 1024],
            )
            .expect("file");
        }
        let worker = DuplicateFinderWorker::spawn().expect("worker");
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
}
