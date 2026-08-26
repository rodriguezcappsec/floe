use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use rustix::{
    fs::{CWD, RenameFlags, renameat_with},
    io::Errno,
};
use thiserror::Error;

pub const BATCH_RENAME_CAPACITY: usize = 4_096;
const STAGE_ATTEMPTS: usize = 128;
static STAGE_NONCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchRenamePair {
    source: PathBuf,
    destination: PathBuf,
}

impl BatchRenamePair {
    pub fn new(source: PathBuf, destination: PathBuf) -> Result<Self, BatchRenameRequestError> {
        validate_path(&source)?;
        validate_path(&destination)?;
        if source.parent() != destination.parent() || source == destination {
            return Err(BatchRenameRequestError::InvalidPair {
                source_path: source,
                destination,
            });
        }
        Ok(Self {
            source,
            destination,
        })
    }

    pub fn source(&self) -> &Path {
        &self.source
    }

    pub fn destination(&self) -> &Path {
        &self.destination
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchRenameRequest {
    pairs: Arc<[BatchRenamePair]>,
}

impl BatchRenameRequest {
    pub fn new(pairs: Vec<BatchRenamePair>) -> Result<Self, BatchRenameRequestError> {
        if pairs.is_empty() || pairs.len() > BATCH_RENAME_CAPACITY {
            return Err(BatchRenameRequestError::InvalidCount(pairs.len()));
        }
        let mut sources = HashSet::with_capacity(pairs.len());
        let mut destinations = HashSet::with_capacity(pairs.len());
        for pair in &pairs {
            if !sources.insert(pair.source.clone()) {
                return Err(BatchRenameRequestError::DuplicateSource(
                    pair.source.clone(),
                ));
            }
            if !destinations.insert(pair.destination.clone()) {
                return Err(BatchRenameRequestError::DuplicateDestination(
                    pair.destination.clone(),
                ));
            }
        }
        Ok(Self {
            pairs: pairs.into(),
        })
    }

    pub fn pairs(&self) -> &[BatchRenamePair] {
        &self.pairs
    }

    pub fn inverse(&self) -> Result<Self, BatchRenameRequestError> {
        Self::new(
            self.pairs
                .iter()
                .map(|pair| BatchRenamePair::new(pair.destination.clone(), pair.source.clone()))
                .collect::<Result<Vec<_>, _>>()?,
        )
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BatchRenameRequestError {
    #[error("batch rename requires between one and {BATCH_RENAME_CAPACITY} pairs, received {0}")]
    InvalidCount(usize),
    #[error("batch rename path must be absolute, normalized, and non-root: {}", .0.display())]
    InvalidPath(PathBuf),
    #[error("batch rename pair must change only the final name: {} -> {}", source_path.display(), destination.display())]
    InvalidPair {
        source_path: PathBuf,
        destination: PathBuf,
    },
    #[error("duplicate batch rename source: {}", .0.display())]
    DuplicateSource(PathBuf),
    #[error("duplicate batch rename destination: {}", .0.display())]
    DuplicateDestination(PathBuf),
}

#[derive(Clone, Debug, Default)]
pub struct BatchRenameCancellation(Arc<AtomicBool>);

impl BatchRenameCancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchRenameOutcome {
    completed: Arc<[BatchRenamePair]>,
}

impl BatchRenameOutcome {
    pub fn completed(&self) -> &[BatchRenamePair] {
        &self.completed
    }

    pub fn undo_request(&self) -> Result<BatchRenameRequest, BatchRenameRequestError> {
        BatchRenameRequest {
            pairs: Arc::clone(&self.completed),
        }
        .inverse()
    }
}

#[derive(Debug, Error)]
pub enum BatchRenameError {
    #[error("batch rename cancelled before any name changed")]
    Cancelled,
    #[error("batch rename source is missing or changed: {}", .0.display())]
    SourceChanged(PathBuf),
    #[error("batch rename destination already exists: {}", .0.display())]
    DestinationExists(PathBuf),
    #[error("batch rename failed at {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("batch rename partially committed and rollback was incomplete: {message}")]
    Partial { message: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Identity {
    device: u64,
    inode: u64,
    file_type: fs::FileType,
}

pub fn execute_batch_rename(
    request: &BatchRenameRequest,
    cancellation: &BatchRenameCancellation,
    mut progress: impl FnMut(u64, u64),
) -> Result<BatchRenameOutcome, BatchRenameError> {
    use std::os::unix::fs::MetadataExt;

    if cancellation.is_cancelled() {
        return Err(BatchRenameError::Cancelled);
    }
    let source_set = request
        .pairs
        .iter()
        .map(|pair| pair.source.clone())
        .collect::<HashSet<_>>();
    let mut identities = Vec::with_capacity(request.pairs.len());
    for pair in request.pairs.iter() {
        let metadata = fs::symlink_metadata(&pair.source)
            .map_err(|_| BatchRenameError::SourceChanged(pair.source.clone()))?;
        identities.push(Identity {
            device: metadata.dev(),
            inode: metadata.ino(),
            file_type: metadata.file_type(),
        });
        if !source_set.contains(&pair.destination)
            && fs::symlink_metadata(&pair.destination).is_ok()
        {
            return Err(BatchRenameError::DestinationExists(
                pair.destination.clone(),
            ));
        }
    }
    if cancellation.is_cancelled() {
        return Err(BatchRenameError::Cancelled);
    }

    let stages = request
        .pairs
        .iter()
        .enumerate()
        .map(|(index, pair)| stage_path(pair.source.parent().expect("validated parent"), index))
        .collect::<Result<Vec<_>, _>>()?;

    let mut staged = 0usize;
    for (index, pair) in request.pairs.iter().enumerate() {
        revalidate(&pair.source, identities[index])?;
        if let Err(error) = rename_noreplace(&pair.source, &stages[index]) {
            if rollback_stages(&request.pairs, &stages, staged).is_err() {
                return Err(BatchRenameError::Partial {
                    message: format!("{error}; rollback could not restore every source"),
                });
            }
            return Err(error);
        }
        staged += 1;
        progress(staged as u64, (request.pairs.len() * 2) as u64);
    }

    let mut published = 0usize;
    for (index, pair) in request.pairs.iter().enumerate() {
        if let Err(error) = rename_noreplace(&stages[index], &pair.destination) {
            if rollback_published(&request.pairs, &stages, published).is_err() {
                return Err(BatchRenameError::Partial {
                    message: format!("{}; rollback could not restore every source", error),
                });
            }
            return Err(error);
        }
        published += 1;
        progress(
            (request.pairs.len() + published) as u64,
            (request.pairs.len() * 2) as u64,
        );
    }
    Ok(BatchRenameOutcome {
        completed: Arc::clone(&request.pairs),
    })
}

fn validate_path(path: &Path) -> Result<(), BatchRenameRequestError> {
    if !path.is_absolute()
        || path.parent().is_none()
        || path.components().any(|component| {
            matches!(
                component,
                Component::CurDir | Component::ParentDir | Component::Prefix(_)
            )
        })
    {
        Err(BatchRenameRequestError::InvalidPath(path.to_path_buf()))
    } else {
        Ok(())
    }
}

fn stage_path(parent: &Path, index: usize) -> Result<PathBuf, BatchRenameError> {
    for _ in 0..STAGE_ATTEMPTS {
        let nonce = STAGE_NONCE.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".floe-batch-rename-{}-{nonce}-{index}",
            std::process::id()
        ));
        if fs::symlink_metadata(&path).is_err() {
            return Ok(path);
        }
    }
    Err(BatchRenameError::Io {
        path: parent.to_path_buf(),
        source: std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not allocate private batch rename staging name",
        ),
    })
}

fn rename_noreplace(source: &Path, destination: &Path) -> Result<(), BatchRenameError> {
    renameat_with(CWD, source, CWD, destination, RenameFlags::NOREPLACE).map_err(|error| {
        if error == Errno::EXIST {
            BatchRenameError::DestinationExists(destination.to_path_buf())
        } else {
            BatchRenameError::Io {
                path: source.to_path_buf(),
                source: std::io::Error::from_raw_os_error(error.raw_os_error()),
            }
        }
    })
}

fn revalidate(path: &Path, expected: Identity) -> Result<(), BatchRenameError> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| BatchRenameError::SourceChanged(path.to_path_buf()))?;
    if metadata.dev() == expected.device
        && metadata.ino() == expected.inode
        && metadata.file_type() == expected.file_type
    {
        Ok(())
    } else {
        Err(BatchRenameError::SourceChanged(path.to_path_buf()))
    }
}

fn rollback_stages(
    pairs: &[BatchRenamePair],
    stages: &[PathBuf],
    staged: usize,
) -> Result<(), BatchRenameError> {
    for index in (0..staged).rev() {
        rename_noreplace(&stages[index], &pairs[index].source)?;
    }
    Ok(())
}

fn rollback_published(
    pairs: &[BatchRenamePair],
    stages: &[PathBuf],
    published: usize,
) -> Result<(), BatchRenameError> {
    for index in (0..published).rev() {
        rename_noreplace(&pairs[index].destination, &pairs[index].source)?;
    }
    for index in published..pairs.len() {
        rename_noreplace(&stages[index], &pairs[index].source)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn phase_12c_batch_rename_jobs_apply_cycles_conflicts_cancel_and_undo() {
        let root = tempdir().expect("root");
        let a = root.path().join("a");
        let b = root.path().join("b");
        fs::write(&a, b"A").expect("a");
        fs::write(&b, b"B").expect("b");
        let request = BatchRenameRequest::new(vec![
            BatchRenamePair::new(a.clone(), b.clone()).expect("pair"),
            BatchRenamePair::new(b.clone(), a.clone()).expect("pair"),
        ])
        .expect("request");
        let outcome =
            execute_batch_rename(&request, &BatchRenameCancellation::default(), |_, _| {})
                .expect("rename");
        assert_eq!(fs::read(&a).expect("a"), b"B");
        assert_eq!(fs::read(&b).expect("b"), b"A");
        execute_batch_rename(
            &outcome.undo_request().expect("undo"),
            &BatchRenameCancellation::default(),
            |_, _| {},
        )
        .expect("undo apply");
        assert_eq!(fs::read(&a).expect("a"), b"A");
        assert_eq!(fs::read(&b).expect("b"), b"B");

        let occupied = root.path().join("occupied");
        fs::write(&occupied, b"keep").expect("occupied");
        let conflict = BatchRenameRequest::new(vec![
            BatchRenamePair::new(a.clone(), occupied.clone()).expect("pair"),
        ])
        .expect("request");
        assert!(matches!(
            execute_batch_rename(&conflict, &BatchRenameCancellation::default(), |_, _| {}),
            Err(BatchRenameError::DestinationExists(path)) if path == occupied
        ));
        let cancellation = BatchRenameCancellation::default();
        cancellation.cancel();
        assert!(matches!(
            execute_batch_rename(&request, &cancellation, |_, _| {}),
            Err(BatchRenameError::Cancelled)
        ));
    }
}
