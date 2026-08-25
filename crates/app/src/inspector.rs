//! Bounded GTK-independent Inspector selection aggregation.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

use floe_core::{DirectoryEntry, EntryKind};
use thiserror::Error;

pub const INSPECTOR_SELECTION_CAPACITY: usize = 4_096;
pub const INSPECTOR_QUEUE_CAPACITY: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectorEntryKey {
    path: PathBuf,
    kind: EntryKind,
    size: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectorRequest {
    pub generation: u64,
    pub directory: PathBuf,
    pub entries: Arc<[InspectorEntryKey]>,
}

impl InspectorRequest {
    pub fn from_entries(
        generation: u64,
        directory: PathBuf,
        entries: &[Arc<DirectoryEntry>],
    ) -> Result<Self, InspectorRequestError> {
        if generation == 0 || entries.is_empty() || entries.len() > INSPECTOR_SELECTION_CAPACITY {
            return Err(InspectorRequestError::InvalidSelection);
        }
        let mut keys = Vec::with_capacity(entries.len());
        for entry in entries {
            if entry.path().parent() != Some(directory.as_path()) {
                return Err(InspectorRequestError::OutsideDirectory);
            }
            keys.push(InspectorEntryKey {
                path: entry.path().to_path_buf(),
                kind: entry.kind(),
                size: entry.size(),
            });
        }
        Ok(Self {
            generation,
            directory,
            entries: keys.into(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectorFacts {
    pub selection_paths: Arc<[PathBuf]>,
    pub regular_files: usize,
    pub directories: usize,
    pub symbolic_links: usize,
    pub other_entries: usize,
    pub known_bytes: u64,
    pub unknown_sizes: usize,
    pub bytes_overflowed: bool,
    pub common_parent: PathBuf,
}

impl InspectorFacts {
    pub fn selection_count(&self) -> usize {
        self.selection_paths.len()
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum InspectorRequestError {
    #[error("Inspector selection is empty, oversized, or has an invalid generation")]
    InvalidSelection,
    #[error("Inspector selection contains an item outside its exact directory")]
    OutsideDirectory,
}

#[derive(Debug, Error)]
pub enum InspectorSubmitError {
    #[error("Inspector queue is full")]
    Full(InspectorRequest),
    #[error("Inspector worker disconnected")]
    Disconnected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectorResponse {
    pub generation: u64,
    pub result: Result<InspectorFacts, InspectorRequestError>,
}

pub struct InspectorWorker {
    sender: Option<SyncSender<InspectorRequest>>,
    receiver: Receiver<InspectorResponse>,
    worker: Option<JoinHandle<()>>,
}

impl InspectorWorker {
    pub fn spawn() -> std::io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<InspectorRequest>(INSPECTOR_QUEUE_CAPACITY);
        let (responses, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("floe-inspector".to_owned())
            .spawn(move || {
                while let Ok(request) = requests.recv() {
                    let generation = request.generation;
                    let result = aggregate(request);
                    if responses
                        .send(InspectorResponse { generation, result })
                        .is_err()
                    {
                        return;
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            receiver,
            worker: Some(worker),
        })
    }

    pub fn submit(&self, request: InspectorRequest) -> Result<(), InspectorSubmitError> {
        let Some(sender) = self.sender.as_ref() else {
            return Err(InspectorSubmitError::Disconnected);
        };
        match sender.try_send(request) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(request)) => Err(InspectorSubmitError::Full(request)),
            Err(TrySendError::Disconnected(_)) => Err(InspectorSubmitError::Disconnected),
        }
    }

    pub fn try_response(&self) -> Option<InspectorResponse> {
        self.receiver.try_recv().ok()
    }
}

impl Drop for InspectorWorker {
    fn drop(&mut self) {
        self.sender.take();
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("Inspector worker panicked during shutdown");
        }
    }
}

fn aggregate(request: InspectorRequest) -> Result<InspectorFacts, InspectorRequestError> {
    if request.entries.is_empty() || request.entries.len() > INSPECTOR_SELECTION_CAPACITY {
        return Err(InspectorRequestError::InvalidSelection);
    }
    let mut facts = InspectorFacts {
        selection_paths: request
            .entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>()
            .into(),
        regular_files: 0,
        directories: 0,
        symbolic_links: 0,
        other_entries: 0,
        known_bytes: 0,
        unknown_sizes: 0,
        bytes_overflowed: false,
        common_parent: request.directory.clone(),
    };
    for entry in request.entries.iter() {
        if entry.path.parent() != Some(request.directory.as_path()) {
            return Err(InspectorRequestError::OutsideDirectory);
        }
        match entry.kind {
            EntryKind::RegularFile => facts.regular_files += 1,
            EntryKind::Directory => facts.directories += 1,
            EntryKind::SymbolicLink { .. } => facts.symbolic_links += 1,
            _ => facts.other_entries += 1,
        }
        if let Some(size) = entry.size {
            if let Some(total) = facts.known_bytes.checked_add(size) {
                facts.known_bytes = total;
            } else {
                facts.bytes_overflowed = true;
                facts.known_bytes = u64::MAX;
            }
        } else {
            facts.unknown_sizes += 1;
        }
    }
    Ok(facts)
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::ffi::OsStringExt};

    use floe_core::enumerate_directory;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn phase_10a_inspector_worker_aggregates_raw_selection_on_bounded_thread() {
        let root = tempdir().expect("Inspector root");
        let raw = root
            .path()
            .join(std::ffi::OsString::from_vec(b"raw-\xff".to_vec()));
        fs::write(&raw, b"12345").expect("raw file");
        fs::create_dir(root.path().join("folder")).expect("folder");
        let entries = enumerate_directory(root.path())
            .expect("listing")
            .into_entries()
            .into_iter()
            .map(Arc::new)
            .collect::<Vec<_>>();
        let request = InspectorRequest::from_entries(7, root.path().to_path_buf(), &entries)
            .expect("request");
        let worker = InspectorWorker::spawn().expect("worker");
        worker.submit(request).expect("submit");
        let response = loop {
            if let Some(response) = worker.try_response() {
                break response;
            }
            thread::yield_now();
        };
        let facts = response.result.expect("facts");
        assert_eq!(facts.selection_count(), 2);
        assert_eq!(facts.regular_files, 1);
        assert_eq!(facts.directories, 1);
        assert_eq!(facts.known_bytes, 5);
        assert!(
            facts
                .selection_paths
                .iter()
                .any(|path| path.as_os_str().as_encoded_bytes().contains(&0xff))
        );
        assert!(matches!(
            InspectorRequest::from_entries(0, root.path().to_path_buf(), &entries),
            Err(InspectorRequestError::InvalidSelection)
        ));
        let oversized =
            std::iter::repeat_n(Arc::clone(&entries[0]), INSPECTOR_SELECTION_CAPACITY + 1)
                .collect::<Vec<_>>();
        assert!(matches!(
            InspectorRequest::from_entries(8, root.path().to_path_buf(), &oversized),
            Err(InspectorRequestError::InvalidSelection)
        ));
    }
}
