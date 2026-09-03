//! Bounded asynchronous filesystem-capacity queries for browser chrome.
//!
//! The GTK thread submits exact local paths and consumes immutable results. GIO
//! filesystem queries run on one application-owned worker so slow mounted
//! storage cannot stall the main loop.

use std::{
    io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread::{self, JoinHandle},
};

use gtk::{gio, gio::prelude::*};

const STORAGE_QUEUE_CAPACITY: usize = 32;
const FILESYSTEM_ATTRIBUTES: &str = "filesystem::size,filesystem::free,filesystem::readonly";

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum StorageTarget {
    CurrentLocation,
    Device(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageRequest {
    pub generation: u64,
    pub target: StorageTarget,
    pub path: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageFacts {
    pub total: Option<u64>,
    pub free: Option<u64>,
    pub read_only: Option<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageError {
    Unavailable(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageResponse {
    pub request: StorageRequest,
    pub result: Result<StorageFacts, StorageError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageSubmitError {
    Full(StorageRequest),
    Disconnected,
}

pub struct StorageWorker {
    sender: Option<SyncSender<StorageRequest>>,
    receiver: Receiver<StorageResponse>,
    worker: Option<JoinHandle<()>>,
}

impl StorageWorker {
    pub fn spawn() -> io::Result<Self> {
        Self::spawn_internal(None)
    }

    fn spawn_internal(start_gate: Option<Receiver<()>>) -> io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<StorageRequest>(STORAGE_QUEUE_CAPACITY);
        let (responses, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("floe-storage-facts".to_owned())
            .spawn(move || {
                if let Some(start_gate) = start_gate {
                    if start_gate.recv().is_err() {
                        return;
                    }
                }
                while let Ok(request) = requests.recv() {
                    let result = query_storage_facts(&request.path);
                    if responses.send(StorageResponse { request, result }).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            receiver,
            worker: Some(worker),
        })
    }

    pub fn try_request(&self, request: StorageRequest) -> Result<(), StorageSubmitError> {
        let Some(sender) = self.sender.as_ref() else {
            return Err(StorageSubmitError::Disconnected);
        };
        match sender.try_send(request) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(request)) => Err(StorageSubmitError::Full(request)),
            Err(TrySendError::Disconnected(_)) => Err(StorageSubmitError::Disconnected),
        }
    }

    pub fn try_response(&self) -> Option<StorageResponse> {
        self.receiver.try_recv().ok()
    }
}

impl Drop for StorageWorker {
    fn drop(&mut self) {
        self.sender.take();
        // Mount queries can remain blocked after a device disappears. Never
        // synchronously join this read-only worker from GTK window teardown.
        self.worker.take();
    }
}

fn query_storage_facts(path: &Path) -> Result<StorageFacts, StorageError> {
    let info = gio::File::for_path(path)
        .query_filesystem_info(FILESYSTEM_ATTRIBUTES, None::<&gio::Cancellable>)
        .map_err(|error| StorageError::Unavailable(error.to_string()))?;
    let total = info
        .has_attribute(gio::FILE_ATTRIBUTE_FILESYSTEM_SIZE)
        .then(|| info.attribute_uint64(gio::FILE_ATTRIBUTE_FILESYSTEM_SIZE));
    let free = info
        .has_attribute(gio::FILE_ATTRIBUTE_FILESYSTEM_FREE)
        .then(|| info.attribute_uint64(gio::FILE_ATTRIBUTE_FILESYSTEM_FREE));
    let read_only = info
        .has_attribute(gio::FILE_ATTRIBUTE_FILESYSTEM_READONLY)
        .then(|| info.boolean(gio::FILE_ATTRIBUTE_FILESYSTEM_READONLY));
    Ok(StorageFacts {
        total,
        free,
        read_only,
    })
}

pub fn format_storage_facts(facts: StorageFacts) -> String {
    let mut parts = Vec::with_capacity(2);
    match (facts.free, facts.total) {
        (Some(free), Some(total)) => {
            parts.push(format!(
                "{} free of {}",
                format_bytes(free),
                format_bytes(total)
            ));
        }
        (Some(free), None) => parts.push(format!("{} free", format_bytes(free))),
        (None, Some(total)) => parts.push(format!("{} total", format_bytes(total))),
        (None, None) => {}
    }
    if facts.read_only == Some(true) {
        parts.push("Read-only".to_owned());
    }
    parts.join(" · ")
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 7] = ["B", "KB", "MB", "GB", "TB", "PB", "EB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_6t_status_formats_capacity_and_read_only_honestly() {
        assert_eq!(
            format_storage_facts(StorageFacts {
                total: Some(10_000_000_000),
                free: Some(2_500_000_000),
                read_only: Some(false),
            }),
            "2.5 GB free of 10.0 GB"
        );
        assert_eq!(
            format_storage_facts(StorageFacts {
                total: None,
                free: None,
                read_only: Some(true),
            }),
            "Read-only"
        );
        assert_eq!(
            format_storage_facts(StorageFacts {
                total: None,
                free: None,
                read_only: None,
            }),
            ""
        );
    }

    #[test]
    fn phase_6t_status_storage_worker_returns_exact_request_identity() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let worker = StorageWorker::spawn().expect("storage worker");
        let request = StorageRequest {
            generation: 17,
            target: StorageTarget::CurrentLocation,
            path: directory.path().to_path_buf(),
        };
        worker.try_request(request.clone()).expect("request");
        let response = loop {
            if let Some(response) = worker.try_response() {
                break response;
            }
            std::thread::yield_now();
        };
        assert_eq!(response.request, request);
        let facts = response.result.expect("filesystem facts");
        assert!(facts.total.is_some());
        assert!(facts.free.is_some());
    }

    #[test]
    fn phase_6t_status_storage_queue_is_fixed_capacity_and_non_blocking() {
        let (gate_sender, gate_receiver) = mpsc::channel();
        let worker = StorageWorker::spawn_internal(Some(gate_receiver)).expect("storage worker");
        for index in 0..STORAGE_QUEUE_CAPACITY {
            worker
                .try_request(StorageRequest {
                    generation: 1,
                    target: StorageTarget::Device(index.to_string()),
                    path: PathBuf::from(format!("/missing/{index}")),
                })
                .expect("bounded request");
        }
        assert!(matches!(
            worker.try_request(StorageRequest {
                generation: 1,
                target: StorageTarget::CurrentLocation,
                path: PathBuf::from("/missing/full"),
            }),
            Err(StorageSubmitError::Full(_))
        ));
        gate_sender.send(()).expect("release worker");
    }
}
