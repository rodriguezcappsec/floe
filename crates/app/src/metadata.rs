//! Bounded, lazy metadata enrichment for virtualized browser rows.
//!
//! Initial directory enumeration intentionally stays cheap. This worker receives
//! requests only from bound visible rows when an enabled column needs details.
//! Results are keyed by the exact source path and cheap source identity, so a
//! recycled row or changed file cannot receive stale metadata.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs, io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread::{self, JoinHandle},
    time::SystemTime,
};

use floe_core::DirectoryEntry;
use gtk::gio;
use thiserror::Error;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

const METADATA_QUEUE_CAPACITY: usize = 64;
pub const METADATA_CACHE_CAPACITY: usize = 512;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MetadataKey {
    path: PathBuf,
    size: Option<u64>,
    modified: Option<SystemTime>,
}

impl MetadataKey {
    pub fn from_entry(entry: &DirectoryEntry) -> Self {
        Self {
            path: entry.path().to_path_buf(),
            size: entry.size(),
            modified: entry.modified(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataDetails {
    pub mime_type: Option<String>,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub unix_mode: Option<u32>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum MetadataError {
    #[error("the entry disappeared")]
    Missing,
    #[error("the entry changed before metadata was ready")]
    Stale,
    #[error("metadata is unavailable: {0}")]
    Unavailable(String),
}

#[derive(Clone, Debug)]
pub struct MetadataResponse {
    pub key: MetadataKey,
    pub result: Result<MetadataDetails, MetadataError>,
}

#[derive(Debug, Error)]
pub enum MetadataSubmitError {
    #[error("metadata queue is full")]
    Full(MetadataKey),
    #[error("metadata worker is disconnected")]
    Disconnected,
}

pub struct MetadataWorker {
    sender: Option<SyncSender<MetadataKey>>,
    receiver: Receiver<MetadataResponse>,
    worker: Option<JoinHandle<()>>,
}

impl MetadataWorker {
    pub fn spawn() -> io::Result<Self> {
        Self::spawn_internal(None)
    }

    fn spawn_internal(start_gate: Option<Receiver<()>>) -> io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel(METADATA_QUEUE_CAPACITY);
        let (responses, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("floe-metadata".to_owned())
            .spawn(move || {
                if let Some(start_gate) = start_gate
                    && start_gate.recv().is_err()
                {
                    return;
                }
                while let Ok(key) = requests.recv() {
                    let result = load_metadata(&key);
                    if responses.send(MetadataResponse { key, result }).is_err() {
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

    pub fn try_request(&self, key: MetadataKey) -> Result<(), MetadataSubmitError> {
        let Some(sender) = self.sender.as_ref() else {
            return Err(MetadataSubmitError::Disconnected);
        };
        match sender.try_send(key) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(key)) => Err(MetadataSubmitError::Full(key)),
            Err(TrySendError::Disconnected(_)) => Err(MetadataSubmitError::Disconnected),
        }
    }

    pub fn try_response(&self) -> Option<MetadataResponse> {
        self.receiver.try_recv().ok()
    }
}

impl Drop for MetadataWorker {
    fn drop(&mut self) {
        self.sender.take();
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("metadata worker panicked during shutdown");
        }
    }
}

fn load_metadata(key: &MetadataKey) -> Result<MetadataDetails, MetadataError> {
    let metadata = fs::symlink_metadata(&key.path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => MetadataError::Missing,
        _ => MetadataError::Unavailable(error.to_string()),
    })?;
    let current_size = metadata.is_file().then_some(metadata.len());
    let current_modified = metadata.modified().ok();
    if current_size != key.size || current_modified != key.modified {
        return Err(MetadataError::Stale);
    }

    let (content_type, _) = gio::content_type_guess(Some(key.path()), None::<&[u8]>);
    let mime_type = (!content_type.is_empty()).then(|| content_type.to_string());
    #[cfg(unix)]
    let unix_mode = Some(metadata.mode());
    #[cfg(not(unix))]
    let unix_mode = None;

    Ok(MetadataDetails {
        mime_type,
        created: metadata.created().ok(),
        accessed: metadata.accessed().ok(),
        unix_mode,
    })
}

#[derive(Default)]
pub struct MetadataCache {
    completed: HashMap<MetadataKey, Result<MetadataDetails, MetadataError>>,
    order: VecDeque<MetadataKey>,
    pending: HashSet<MetadataKey>,
    requests: VecDeque<MetadataKey>,
}

impl MetadataCache {
    pub fn request(&mut self, key: MetadataKey) -> Option<&Result<MetadataDetails, MetadataError>> {
        if !self.completed.contains_key(&key) && self.pending.insert(key.clone()) {
            self.requests.push_back(key.clone());
        }
        self.completed.get(&key)
    }

    pub fn take_request(&mut self) -> Option<MetadataKey> {
        self.requests.pop_front()
    }

    pub fn retry(&mut self, key: MetadataKey) {
        if self.pending.contains(&key) {
            self.requests.push_front(key);
        }
    }

    pub fn complete(&mut self, key: MetadataKey, result: Result<MetadataDetails, MetadataError>) {
        self.pending.remove(&key);
        self.order.retain(|candidate| candidate != &key);
        self.order.push_back(key.clone());
        self.completed.insert(key, result);
        while self.completed.len() > METADATA_CACHE_CAPACITY {
            let Some(expired) = self.order.pop_front() else {
                break;
            };
            self.completed.remove(&expired);
        }
    }

    pub fn clear_pending(&mut self) {
        self.pending.clear();
        self.requests.clear();
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.completed.len()
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::mpsc};

    use tempfile::tempdir;

    use super::*;

    fn key_for(path: &Path) -> MetadataKey {
        let metadata = fs::symlink_metadata(path).expect("fixture metadata");
        MetadataKey {
            path: path.to_path_buf(),
            size: metadata.is_file().then_some(metadata.len()),
            modified: metadata.modified().ok(),
        }
    }

    #[test]
    fn phase_6t_metadata_worker_returns_lazy_details_and_rejects_stale_identity() {
        let directory = tempdir().expect("temporary directory");
        let path = directory.path().join("notes.txt");
        fs::write(&path, b"notes").expect("fixture file");
        let key = key_for(&path);
        let worker = MetadataWorker::spawn().expect("metadata worker");
        worker.try_request(key.clone()).expect("request");
        let response = loop {
            if let Some(response) = worker.try_response() {
                break response;
            }
            std::thread::yield_now();
        };
        assert_eq!(response.key, key);
        let details = response.result.expect("metadata details");
        assert!(details.mime_type.is_some());
        assert!(details.unix_mode.is_some());

        let stale = key_for(&path);
        fs::write(&path, b"changed and longer").expect("changed fixture");
        worker.try_request(stale).expect("stale request");
        let response = loop {
            if let Some(response) = worker.try_response() {
                break response;
            }
            std::thread::yield_now();
        };
        assert_eq!(response.result, Err(MetadataError::Stale));
    }

    #[test]
    fn phase_6t_metadata_queue_is_fixed_capacity_and_non_blocking() {
        let (gate_sender, gate_receiver) = mpsc::channel();
        let worker = MetadataWorker::spawn_internal(Some(gate_receiver)).expect("metadata worker");
        for index in 0..METADATA_QUEUE_CAPACITY {
            worker
                .try_request(MetadataKey {
                    path: PathBuf::from(format!("/missing/{index}")),
                    size: None,
                    modified: None,
                })
                .expect("bounded request");
        }
        assert!(matches!(
            worker.try_request(MetadataKey {
                path: PathBuf::from("/missing/full"),
                size: None,
                modified: None,
            }),
            Err(MetadataSubmitError::Full(_))
        ));
        gate_sender.send(()).expect("release worker");
    }

    #[test]
    fn phase_6t_metadata_cache_deduplicates_and_evicts_to_bound() {
        let mut cache = MetadataCache::default();
        let details = MetadataDetails {
            mime_type: None,
            created: None,
            accessed: None,
            unix_mode: None,
        };
        for index in 0..=METADATA_CACHE_CAPACITY {
            let key = MetadataKey {
                path: PathBuf::from(format!("/entry/{index}")),
                size: Some(index as u64),
                modified: None,
            };
            assert!(cache.request(key.clone()).is_none());
            cache.complete(key, Ok(details.clone()));
        }
        assert_eq!(cache.len(), METADATA_CACHE_CAPACITY);
        assert_eq!(cache.requests.len(), METADATA_CACHE_CAPACITY + 1);
    }
}
