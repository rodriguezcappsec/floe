use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

#[cfg(unix)]
use std::{
    ffi::OsString,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{OpenOptionsExt, PermissionsExt},
    },
};
use thiserror::Error;

const BOOKMARK_FILE_NAME: &str = "bookmarks.bin";
const BOOKMARK_MAGIC: &[u8; 8] = b"FLOEBMKS";
const BOOKMARK_FORMAT_VERSION: u16 = 1;
const BOOKMARK_QUEUE_CAPACITY: usize = 8;
const MAX_BOOKMARKS: usize = 512;
const MAX_PATH_BYTES: usize = 1024 * 1024;
const MAX_BOOKMARK_FILE_BYTES: u64 = 8 * 1024 * 1024;
const DROP_SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(250);
static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Bookmarks {
    paths: Vec<PathBuf>,
}

impl Bookmarks {
    pub fn validate(paths: Vec<PathBuf>) -> Result<Self, BookmarkValidationError> {
        Self::validate_with(paths, |path| path.is_dir())
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    fn validate_with(
        paths: Vec<PathBuf>,
        mut is_directory: impl FnMut(&Path) -> bool,
    ) -> Result<Self, BookmarkValidationError> {
        let mut seen = HashSet::new();
        let mut validated = Vec::with_capacity(paths.len().min(MAX_BOOKMARKS));

        for path in paths {
            if !path.is_absolute() {
                return Err(BookmarkValidationError::NotAbsolute(path));
            }
            if !seen.insert(path.clone()) {
                continue;
            }
            if validated.len() == MAX_BOOKMARKS {
                return Err(BookmarkValidationError::TooMany {
                    maximum: MAX_BOOKMARKS,
                });
            }
            if !is_directory(&path) {
                return Err(BookmarkValidationError::NotDirectory(path));
            }
            validated.push(path);
        }

        Ok(Self { paths: validated })
    }
}

#[derive(Debug, Error)]
pub enum BookmarkValidationError {
    #[error("bookmark path is not absolute: {0:?}")]
    NotAbsolute(PathBuf),
    #[error("bookmark path is not an existing directory: {0:?}")]
    NotDirectory(PathBuf),
    #[error("bookmark list exceeds the limit of {maximum} distinct paths")]
    TooMany { maximum: usize },
}

#[derive(Debug, Error)]
pub enum BookmarkFormatError {
    #[error("bookmark file header is not recognized")]
    InvalidMagic,
    #[error("bookmark format version {found} is unsupported")]
    UnsupportedVersion { found: u16 },
    #[error("bookmark file is truncated")]
    Truncated,
    #[error("bookmark file declares {found} records, above the limit of {maximum}")]
    TooManyRecords { found: u32, maximum: usize },
    #[error("bookmark path has {found} bytes, above the limit of {maximum}")]
    PathTooLong { found: usize, maximum: usize },
    #[error("bookmark file has {found} bytes, above the limit of {maximum}")]
    FileTooLarge { found: u64, maximum: u64 },
    #[error("bookmark file contains a relative path")]
    RelativePath,
    #[error("bookmark file contains a duplicate path")]
    DuplicatePath,
    #[error("bookmark file contains trailing data")]
    TrailingData,
    #[cfg(not(unix))]
    #[error("bookmark persistence requires Unix path-byte support")]
    UnsupportedPlatform,
}

#[derive(Debug, Error)]
pub enum BookmarkPersistenceError {
    #[error(transparent)]
    Validation(#[from] BookmarkValidationError),
    #[error(transparent)]
    Format(#[from] BookmarkFormatError),
    #[error("could not {operation} bookmark data")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug)]
pub enum BookmarkWorkerEvent {
    Loaded(Result<Bookmarks, BookmarkPersistenceError>),
    Saved {
        revision: u64,
        result: Result<Bookmarks, BookmarkPersistenceError>,
    },
}

#[derive(Debug, Error)]
pub enum BookmarkSubmitError {
    #[error("bookmark worker queue is full")]
    Full { revision: u64, paths: Vec<PathBuf> },
    #[error("bookmark worker is disconnected")]
    Disconnected { revision: u64, paths: Vec<PathBuf> },
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum BookmarkShutdownError {
    #[error("bookmark worker did not stop within {0:?}")]
    TimedOut(Duration),
    #[error("bookmark worker panicked during shutdown")]
    WorkerPanicked,
}

#[derive(Debug)]
struct BookmarkSaveRequest {
    revision: u64,
    paths: Vec<PathBuf>,
}

pub struct BookmarkWorker {
    sender: Option<SyncSender<BookmarkSaveRequest>>,
    events: Receiver<BookmarkWorkerEvent>,
    completed: Receiver<()>,
    worker: Option<JoinHandle<()>>,
}

impl BookmarkWorker {
    pub fn spawn() -> io::Result<Self> {
        let path = gtk::glib::user_config_dir()
            .join("floe")
            .join(BOOKMARK_FILE_NAME);
        Self::spawn_internal(path, BOOKMARK_QUEUE_CAPACITY, None)
    }

    fn spawn_internal(
        path: PathBuf,
        queue_capacity: usize,
        start_gate: Option<Receiver<()>>,
    ) -> io::Result<Self> {
        let event_capacity = queue_capacity.saturating_add(1).max(1);
        let (sender, receiver) = mpsc::sync_channel::<BookmarkSaveRequest>(queue_capacity);
        let (event_sender, events) = mpsc::sync_channel(event_capacity);
        let (completed_sender, completed) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("floe-bookmark-persistence".to_owned())
            .spawn(move || {
                if start_gate.is_some_and(|gate| gate.recv().is_err()) {
                    let _ = completed_sender.try_send(());
                    return;
                }

                if event_sender
                    .send(BookmarkWorkerEvent::Loaded(load_bookmarks(&path)))
                    .is_err()
                {
                    let _ = completed_sender.try_send(());
                    return;
                }

                while let Ok(request) = receiver.recv() {
                    let result = Bookmarks::validate(request.paths)
                        .map_err(BookmarkPersistenceError::from)
                        .and_then(|bookmarks| {
                            persist_bookmarks(&path, &bookmarks)?;
                            Ok(bookmarks)
                        });
                    if event_sender
                        .send(BookmarkWorkerEvent::Saved {
                            revision: request.revision,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }

                let _ = completed_sender.try_send(());
            })?;

        Ok(Self {
            sender: Some(sender),
            events,
            completed,
            worker: Some(worker),
        })
    }

    pub fn try_save(&self, revision: u64, paths: Vec<PathBuf>) -> Result<(), BookmarkSubmitError> {
        let request = BookmarkSaveRequest { revision, paths };
        let Some(sender) = self.sender.as_ref() else {
            return Err(BookmarkSubmitError::Disconnected {
                revision: request.revision,
                paths: request.paths,
            });
        };

        match sender.try_send(request) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(request)) => Err(BookmarkSubmitError::Full {
                revision: request.revision,
                paths: request.paths,
            }),
            Err(TrySendError::Disconnected(request)) => Err(BookmarkSubmitError::Disconnected {
                revision: request.revision,
                paths: request.paths,
            }),
        }
    }

    pub fn try_event(&self) -> Result<BookmarkWorkerEvent, TryRecvError> {
        self.events.try_recv()
    }

    pub fn shutdown(&mut self, timeout: Duration) -> Result<(), BookmarkShutdownError> {
        self.sender.take();
        if self.worker.is_none() {
            return Ok(());
        }
        let completion = self.completed.recv_timeout(timeout);
        match completion {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => self.join_worker(),
            Err(RecvTimeoutError::Timeout) => {
                self.worker.take();
                Err(BookmarkShutdownError::TimedOut(timeout))
            }
        }
    }

    fn join_worker(&mut self) -> Result<(), BookmarkShutdownError> {
        match self.worker.take() {
            Some(worker) => worker
                .join()
                .map_err(|_| BookmarkShutdownError::WorkerPanicked),
            None => Ok(()),
        }
    }
}

impl Drop for BookmarkWorker {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown(DROP_SHUTDOWN_TIMEOUT) {
            tracing::warn!(%error, "bookmark worker did not shut down cleanly");
        }
    }
}

fn load_bookmarks(path: &Path) -> Result<Bookmarks, BookmarkPersistenceError> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Bookmarks::default()),
        Err(source) => {
            return Err(BookmarkPersistenceError::Io {
                operation: "open",
                source,
            });
        }
    };
    let length = file
        .metadata()
        .map_err(|source| BookmarkPersistenceError::Io {
            operation: "inspect",
            source,
        })?
        .len();
    if length > MAX_BOOKMARK_FILE_BYTES {
        return Err(BookmarkFormatError::FileTooLarge {
            found: length,
            maximum: MAX_BOOKMARK_FILE_BYTES,
        }
        .into());
    }

    let mut bytes = Vec::with_capacity(length as usize);
    file.read_to_end(&mut bytes)
        .map_err(|source| BookmarkPersistenceError::Io {
            operation: "read",
            source,
        })?;
    decode_bookmarks(&bytes).map_err(BookmarkPersistenceError::from)
}

fn persist_bookmarks(path: &Path, bookmarks: &Bookmarks) -> Result<(), BookmarkPersistenceError> {
    let encoded = encode_bookmarks(bookmarks)?;
    let parent = path.parent().ok_or_else(|| BookmarkPersistenceError::Io {
        operation: "resolve the parent directory for",
        source: io::Error::new(io::ErrorKind::InvalidInput, "bookmark path has no parent"),
    })?;

    fs::create_dir_all(parent).map_err(|source| BookmarkPersistenceError::Io {
        operation: "create the parent directory for",
        source,
    })?;
    set_private_directory_permissions(parent)?;

    let (temporary, mut file) = create_private_temporary(parent)?;

    let result = (|| {
        file.write_all(&encoded)
            .map_err(|source| BookmarkPersistenceError::Io {
                operation: "write",
                source,
            })?;
        set_private_file_permissions(&file)?;
        file.sync_all()
            .map_err(|source| BookmarkPersistenceError::Io {
                operation: "synchronize",
                source,
            })?;
        drop(file);
        fs::rename(&temporary, path).map_err(|source| BookmarkPersistenceError::Io {
            operation: "replace",
            source,
        })?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|source| BookmarkPersistenceError::Io {
                operation: "synchronize the parent directory for",
                source,
            })
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn create_private_temporary(parent: &Path) -> Result<(PathBuf, File), BookmarkPersistenceError> {
    const ATTEMPTS: usize = 16;
    let mut last_collision = None;

    for _ in 0..ATTEMPTS {
        let temporary = parent.join(format!(
            ".floe-bookmarks-{}-{}.temporary",
            std::process::id(),
            TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        match options.open(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                last_collision = Some(error);
            }
            Err(source) => {
                return Err(BookmarkPersistenceError::Io {
                    operation: "create a temporary",
                    source,
                });
            }
        }
    }

    Err(BookmarkPersistenceError::Io {
        operation: "create a unique temporary",
        source: last_collision.unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "temporary bookmark filename attempts were exhausted",
            )
        }),
    })
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<(), BookmarkPersistenceError> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
        BookmarkPersistenceError::Io {
            operation: "set private parent permissions for",
            source,
        }
    })
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> Result<(), BookmarkPersistenceError> {
    Err(BookmarkFormatError::UnsupportedPlatform.into())
}

#[cfg(unix)]
fn set_private_file_permissions(file: &File) -> Result<(), BookmarkPersistenceError> {
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|source| BookmarkPersistenceError::Io {
            operation: "set private file permissions for",
            source,
        })
}

#[cfg(not(unix))]
fn set_private_file_permissions(_file: &File) -> Result<(), BookmarkPersistenceError> {
    Err(BookmarkFormatError::UnsupportedPlatform.into())
}

#[cfg(unix)]
fn encode_bookmarks(bookmarks: &Bookmarks) -> Result<Vec<u8>, BookmarkFormatError> {
    if bookmarks.paths.len() > MAX_BOOKMARKS {
        return Err(BookmarkFormatError::TooManyRecords {
            found: u32::try_from(bookmarks.paths.len()).unwrap_or(u32::MAX),
            maximum: MAX_BOOKMARKS,
        });
    }
    let count =
        u32::try_from(bookmarks.paths.len()).map_err(|_| BookmarkFormatError::TooManyRecords {
            found: u32::MAX,
            maximum: MAX_BOOKMARKS,
        })?;
    let mut encoded = Vec::new();
    encoded.extend_from_slice(BOOKMARK_MAGIC);
    encoded.extend_from_slice(&BOOKMARK_FORMAT_VERSION.to_le_bytes());
    encoded.extend_from_slice(&count.to_le_bytes());

    for path in &bookmarks.paths {
        let bytes = path.as_os_str().as_bytes();
        if bytes.len() > MAX_PATH_BYTES {
            return Err(BookmarkFormatError::PathTooLong {
                found: bytes.len(),
                maximum: MAX_PATH_BYTES,
            });
        }
        let length = u32::try_from(bytes.len()).map_err(|_| BookmarkFormatError::PathTooLong {
            found: bytes.len(),
            maximum: MAX_PATH_BYTES,
        })?;
        encoded.extend_from_slice(&length.to_le_bytes());
        encoded.extend_from_slice(bytes);
    }

    Ok(encoded)
}

#[cfg(not(unix))]
fn encode_bookmarks(_bookmarks: &Bookmarks) -> Result<Vec<u8>, BookmarkFormatError> {
    Err(BookmarkFormatError::UnsupportedPlatform)
}

#[cfg(unix)]
fn decode_bookmarks(bytes: &[u8]) -> Result<Bookmarks, BookmarkFormatError> {
    let mut cursor = BinaryCursor::new(bytes);
    if cursor.take(BOOKMARK_MAGIC.len())? != BOOKMARK_MAGIC {
        return Err(BookmarkFormatError::InvalidMagic);
    }
    let version = cursor.read_u16()?;
    if version != BOOKMARK_FORMAT_VERSION {
        return Err(BookmarkFormatError::UnsupportedVersion { found: version });
    }
    let count = cursor.read_u32()?;
    if count as usize > MAX_BOOKMARKS {
        return Err(BookmarkFormatError::TooManyRecords {
            found: count,
            maximum: MAX_BOOKMARKS,
        });
    }

    let mut seen = HashSet::new();
    let mut paths = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let length = cursor.read_u32()? as usize;
        if length > MAX_PATH_BYTES {
            return Err(BookmarkFormatError::PathTooLong {
                found: length,
                maximum: MAX_PATH_BYTES,
            });
        }
        let path = PathBuf::from(OsString::from_vec(cursor.take(length)?.to_vec()));
        if !path.is_absolute() {
            return Err(BookmarkFormatError::RelativePath);
        }
        if !seen.insert(path.clone()) {
            return Err(BookmarkFormatError::DuplicatePath);
        }
        paths.push(path);
    }
    if !cursor.remaining().is_empty() {
        return Err(BookmarkFormatError::TrailingData);
    }

    Ok(Bookmarks { paths })
}

#[cfg(not(unix))]
fn decode_bookmarks(_bytes: &[u8]) -> Result<Bookmarks, BookmarkFormatError> {
    Err(BookmarkFormatError::UnsupportedPlatform)
}

struct BinaryCursor<'a> {
    remaining: &'a [u8],
}

impl<'a> BinaryCursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], BookmarkFormatError> {
        if self.remaining.len() < length {
            return Err(BookmarkFormatError::Truncated);
        }
        let (taken, remaining) = self.remaining.split_at(length);
        self.remaining = remaining;
        Ok(taken)
    }

    fn read_u16(&mut self) -> Result<u16, BookmarkFormatError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn read_u32(&mut self) -> Result<u32, BookmarkFormatError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn remaining(&self) -> &'a [u8] {
        self.remaining
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::{ffi::OsStringExt, fs::PermissionsExt},
        time::{Duration, Instant},
    };

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn phase_6k_bookmark_validation_deduplicates_exact_existing_directories() {
        let directory = tempdir().expect("temporary directory should be created");
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        fs::create_dir_all(&first).expect("first directory should be created");
        fs::create_dir_all(&second).expect("second directory should be created");

        let bookmarks = Bookmarks::validate(vec![first.clone(), first.clone(), second.clone()])
            .expect("existing absolute directories should validate");
        assert_eq!(bookmarks.paths(), &[first, second]);

        assert!(matches!(
            Bookmarks::validate(vec![PathBuf::from("relative")]),
            Err(BookmarkValidationError::NotAbsolute(_))
        ));
        assert!(matches!(
            Bookmarks::validate(vec![directory.path().join("missing")]),
            Err(BookmarkValidationError::NotDirectory(_))
        ));
    }

    #[test]
    fn phase_6k_bookmark_binary_round_trips_raw_non_utf8_unix_paths() {
        let directory = tempdir().expect("temporary directory should be created");
        let raw_name = OsString::from_vec(vec![b'r', b'a', b'w', b'-', 0x80, 0xff]);
        let raw_path = directory.path().join(raw_name);
        fs::create_dir(&raw_path).expect("raw-byte directory should be created");

        let bookmarks = Bookmarks::validate(vec![raw_path.clone()])
            .expect("raw-byte directory should validate");
        let encoded = encode_bookmarks(&bookmarks).expect("bookmarks should encode");
        let decoded = decode_bookmarks(&encoded).expect("bookmarks should decode");

        assert_eq!(decoded, bookmarks);
        assert_eq!(
            decoded.paths()[0].as_os_str().as_bytes(),
            raw_path.as_os_str().as_bytes()
        );
        assert_eq!(
            u16::from_le_bytes([encoded[8], encoded[9]]),
            BOOKMARK_FORMAT_VERSION
        );
    }

    #[test]
    fn phase_6k_bookmark_format_rejects_wrong_version_truncation_and_trailing_data() {
        let bookmarks = Bookmarks {
            paths: vec![PathBuf::from("/valid")],
        };
        let encoded = encode_bookmarks(&bookmarks).expect("bookmarks should encode");

        let mut wrong_version = encoded.clone();
        wrong_version[8..10].copy_from_slice(&(BOOKMARK_FORMAT_VERSION + 1).to_le_bytes());
        assert!(matches!(
            decode_bookmarks(&wrong_version),
            Err(BookmarkFormatError::UnsupportedVersion { .. })
        ));
        assert!(matches!(
            decode_bookmarks(&encoded[..encoded.len() - 1]),
            Err(BookmarkFormatError::Truncated)
        ));
        let mut trailing = encoded;
        trailing.push(0);
        assert!(matches!(
            decode_bookmarks(&trailing),
            Err(BookmarkFormatError::TrailingData)
        ));
    }

    #[test]
    fn bookmark_worker_uses_bounded_queue_private_atomic_file_and_clean_shutdown() {
        let directory = tempdir().expect("temporary directory should be created");
        let path = directory.path().join("private").join(BOOKMARK_FILE_NAME);
        let bookmark = directory.path().join("bookmark");
        fs::create_dir(&bookmark).expect("bookmark directory should be created");
        let (gate_sender, gate_receiver) = mpsc::channel();
        let mut worker = BookmarkWorker::spawn_internal(path.clone(), 1, Some(gate_receiver))
            .expect("bookmark worker should start");

        worker
            .try_save(7, vec![bookmark.clone(), bookmark.clone()])
            .expect("first request should fit bounded queue");
        assert!(matches!(
            worker.try_save(8, vec![bookmark.clone()]),
            Err(BookmarkSubmitError::Full { revision: 8, .. })
        ));
        gate_sender.send(()).expect("worker should be released");

        let loaded = worker
            .events
            .recv_timeout(Duration::from_secs(2))
            .expect("load event should arrive");
        assert!(matches!(loaded, BookmarkWorkerEvent::Loaded(Ok(_))));
        let saved = worker
            .events
            .recv_timeout(Duration::from_secs(2))
            .expect("save event should arrive");
        match saved {
            BookmarkWorkerEvent::Saved { revision, result } => {
                assert_eq!(revision, 7);
                assert_eq!(
                    result.expect("save should succeed").paths(),
                    std::slice::from_ref(&bookmark)
                );
            }
            BookmarkWorkerEvent::Loaded(_) => panic!("expected save event"),
        }
        worker
            .shutdown(Duration::from_secs(2))
            .expect("worker should shut down cleanly");

        let persisted = load_bookmarks(&path).expect("saved bookmarks should load");
        assert_eq!(persisted.paths(), std::slice::from_ref(&bookmark));
        assert_eq!(
            fs::metadata(path.parent().expect("path should have parent"))
                .expect("parent metadata should exist")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path)
                .expect("bookmark metadata should exist")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::read_dir(path.parent().expect("path should have parent"))
                .expect("parent should be readable")
                .count(),
            1,
            "atomic write must not leave a temporary file"
        );
    }

    #[test]
    fn bookmark_worker_reports_validation_failures_as_structured_events() {
        let directory = tempdir().expect("temporary directory should be created");
        let path = directory.path().join("private").join(BOOKMARK_FILE_NAME);
        let missing = directory.path().join("missing");
        let mut worker =
            BookmarkWorker::spawn_internal(path, 1, None).expect("bookmark worker should start");

        assert!(matches!(
            worker
                .events
                .recv_timeout(Duration::from_secs(2))
                .expect("load event should arrive"),
            BookmarkWorkerEvent::Loaded(Ok(_))
        ));
        worker
            .try_save(11, vec![missing.clone()])
            .expect("request should be accepted asynchronously");
        match worker
            .events
            .recv_timeout(Duration::from_secs(2))
            .expect("failure event should arrive")
        {
            BookmarkWorkerEvent::Saved { revision, result } => {
                assert_eq!(revision, 11);
                assert!(matches!(
                    result,
                    Err(BookmarkPersistenceError::Validation(
                        BookmarkValidationError::NotDirectory(path)
                    )) if path == missing
                ));
            }
            BookmarkWorkerEvent::Loaded(_) => panic!("expected save event"),
        }
        worker
            .shutdown(Duration::from_secs(2))
            .expect("worker should shut down cleanly");
    }

    #[test]
    fn bookmark_worker_shutdown_is_bounded_when_worker_cannot_finish() {
        let directory = tempdir().expect("temporary directory should be created");
        let path = directory.path().join("private").join(BOOKMARK_FILE_NAME);
        let (gate_sender, gate_receiver) = mpsc::channel();
        let mut worker = BookmarkWorker::spawn_internal(path, 1, Some(gate_receiver))
            .expect("bookmark worker should start");
        let timeout = Duration::from_millis(20);
        let started = Instant::now();

        assert_eq!(
            worker.shutdown(timeout),
            Err(BookmarkShutdownError::TimedOut(timeout))
        );
        assert!(started.elapsed() < Duration::from_millis(500));
        drop(gate_sender);
    }

    #[test]
    fn bookmark_worker_public_event_poll_is_nonblocking() {
        let directory = tempdir().expect("temporary directory should be created");
        let path = directory.path().join("private").join(BOOKMARK_FILE_NAME);
        let (gate_sender, gate_receiver) = mpsc::channel();
        let mut worker = BookmarkWorker::spawn_internal(path, 1, Some(gate_receiver))
            .expect("bookmark worker should start");

        assert!(matches!(worker.try_event(), Err(TryRecvError::Empty)));
        drop(gate_sender);
        worker
            .shutdown(Duration::from_secs(2))
            .expect("worker should shut down after gate disconnects");
    }
}
