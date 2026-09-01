//! Optional local ClamAV scanning through `clamd`'s Unix `INSTREAM` protocol.
//!
//! Floe never links `libclamav`, uploads content, executes the target, or treats
//! a no-signature response as proof that a file is safe.

use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::{
        fs::FileTypeExt,
        fs::{MetadataExt, OpenOptionsExt},
        net::UnixStream,
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use thiserror::Error;

const REQUEST_CAPACITY: usize = 1;
const RESULT_CAPACITY: usize = 1;
const ROOT_CAPACITY: usize = 128;
const FILE_CAPACITY: usize = 100_000;
const DIRECTORY_CAPACITY: usize = 100_000;
const DEPTH_CAPACITY: usize = 128;
const RETAINED_RESULT_CAPACITY: usize = 4_096;
const MIB: u64 = 1_048_576;
const GIB: u64 = 1_073_741_824;
pub const CLAMAV_FILE_LIMIT_MIB_MIN: u32 = 1;
pub const CLAMAV_FILE_LIMIT_MIB_DEFAULT: u32 = 1_024;
pub const CLAMAV_FILE_LIMIT_MIB_MAX: u32 = 16_384;
pub const CLAMAV_TOTAL_LIMIT_GIB_MIN: u32 = 1;
pub const CLAMAV_TOTAL_LIMIT_GIB_DEFAULT: u32 = 16;
pub const CLAMAV_TOTAL_LIMIT_GIB_MAX: u32 = 1_024;
const STREAM_CHUNK_BYTES: usize = 256 * 1024;
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const RESPONSE_POLL: Duration = Duration::from_millis(200);

const SOCKET_CANDIDATES: [&str; 4] = [
    "/run/clamav/clamd.ctl",
    "/run/clamav/clamd.sock",
    "/run/clamd.scan/clamd.sock",
    "/var/run/clamav/clamd.ctl",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThreatScanLimits {
    max_file_bytes: u64,
    max_total_bytes: u64,
}

impl ThreatScanLimits {
    pub fn from_preferences(file_mib: u32, total_gib: u32) -> Self {
        let file_mib = file_mib.clamp(CLAMAV_FILE_LIMIT_MIB_MIN, CLAMAV_FILE_LIMIT_MIB_MAX);
        let total_gib = total_gib.clamp(CLAMAV_TOTAL_LIMIT_GIB_MIN, CLAMAV_TOTAL_LIMIT_GIB_MAX);
        let max_file_bytes = u64::from(file_mib) * MIB;
        let max_total_bytes = (u64::from(total_gib) * GIB).max(max_file_bytes);
        Self {
            max_file_bytes,
            max_total_bytes,
        }
    }

    pub const fn max_file_bytes(self) -> u64 {
        self.max_file_bytes
    }

    pub const fn max_total_bytes(self) -> u64 {
        self.max_total_bytes
    }
}

impl Default for ThreatScanLimits {
    fn default() -> Self {
        Self::from_preferences(
            CLAMAV_FILE_LIMIT_MIB_DEFAULT,
            CLAMAV_TOTAL_LIMIT_GIB_DEFAULT,
        )
    }
}

pub fn format_scan_limit(bytes: u64) -> String {
    if bytes % GIB == 0 {
        format!("{} GiB", bytes / GIB)
    } else {
        format!("{} MiB", bytes / MIB)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreatScanRequest {
    pub generation: u64,
    pub roots: Vec<PathBuf>,
    pub limits: ThreatScanLimits,
}

impl ThreatScanRequest {
    pub fn new(generation: u64, roots: Vec<PathBuf>) -> Result<Self, ThreatScanError> {
        Self::with_limits(generation, roots, ThreatScanLimits::default())
    }

    pub fn with_limits(
        generation: u64,
        roots: Vec<PathBuf>,
        limits: ThreatScanLimits,
    ) -> Result<Self, ThreatScanError> {
        if roots.is_empty() {
            return Err(ThreatScanError::InvalidRequest("select at least one item"));
        }
        if roots.len() > ROOT_CAPACITY {
            return Err(ThreatScanError::InvalidRequest("too many selected roots"));
        }
        if roots.iter().any(|path| !path.is_absolute()) {
            return Err(ThreatScanError::InvalidRequest(
                "scan roots must be absolute",
            ));
        }
        Ok(Self {
            generation,
            roots,
            limits,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThreatFileStatus {
    NoKnownSignature,
    Detected { signature: String },
    NotScanned { reason: String },
    Changed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreatFileResult {
    pub path: PathBuf,
    pub status: ThreatFileStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreatScanOutcome {
    pub generation: u64,
    pub limits: ThreatScanLimits,
    pub engine: String,
    pub scanned_files: u64,
    pub no_known_signature: u64,
    pub detections: u64,
    pub not_scanned: u64,
    pub retained_results: Vec<ThreatFileResult>,
    pub truncated: bool,
    pub cancelled: bool,
}

#[derive(Debug)]
pub struct ThreatScanResult {
    pub generation: u64,
    pub outcome: Result<ThreatScanOutcome, ThreatScanError>,
}

#[derive(Debug, Error)]
pub enum ThreatScanError {
    #[error("invalid threat scan request: {0}")]
    InvalidRequest(&'static str),
    #[error("ClamAV daemon is unavailable")]
    Unavailable,
    #[error("ClamAV communication failed: {0}")]
    Io(#[from] io::Error),
    #[error("ClamAV returned a malformed response")]
    MalformedResponse,
    #[error("ClamAV response timed out")]
    TimedOut,
    #[error("local ClamAV scan was cancelled")]
    Cancelled,
    #[error("threat scan worker queue is full")]
    QueueFull,
    #[error("threat scan worker has stopped")]
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileFingerprint {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

impl FileFingerprint {
    fn from_file(file: &File) -> io::Result<Self> {
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "scan target is not a regular file",
            ));
        }
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
        })
    }
}

#[derive(Clone, Debug)]
struct ClamAvClient {
    socket: PathBuf,
    #[cfg(test)]
    fixture_streams: Option<Arc<std::sync::Mutex<VecDeque<FixtureStream>>>>,
}

enum ClamAvStream {
    Socket(UnixStream),
    #[cfg(test)]
    Fixture(FixtureStream),
}

impl Read for ClamAvStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Socket(stream) => stream.read(buffer),
            #[cfg(test)]
            Self::Fixture(stream) => stream.read(buffer),
        }
    }
}

impl Write for ClamAvStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Self::Socket(stream) => stream.write(buffer),
            #[cfg(test)]
            Self::Fixture(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Socket(stream) => stream.flush(),
            #[cfg(test)]
            Self::Fixture(stream) => stream.flush(),
        }
    }
}

#[cfg(test)]
#[derive(Debug)]
struct FixtureStream {
    response: std::io::Cursor<Vec<u8>>,
    writes: Arc<std::sync::Mutex<Vec<u8>>>,
}

#[cfg(test)]
impl Read for FixtureStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let fragment = buffer.len().min(3);
        self.response.read(&mut buffer[..fragment])
    }
}

#[cfg(test)]
impl Write for FixtureStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.writes
            .lock()
            .map_err(|_| io::Error::other("fake clamd write lock poisoned"))?
            .extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl ClamAvClient {
    fn discover() -> Result<Self, ThreatScanError> {
        SOCKET_CANDIDATES
            .iter()
            .map(PathBuf::from)
            .find(|path| {
                fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_socket())
            })
            .map(|socket| Self {
                socket,
                #[cfg(test)]
                fixture_streams: None,
            })
            .ok_or(ThreatScanError::Unavailable)
    }

    fn version(&self, cancelled: &AtomicBool) -> Result<String, ThreatScanError> {
        let mut stream = self.connect()?;
        stream.write_all(b"zVERSION\0")?;
        read_response(&mut stream, cancelled)
    }

    fn scan_file(
        &self,
        path: &Path,
        cancelled: &AtomicBool,
    ) -> Result<ThreatFileStatus, ThreatScanError> {
        let mut options = OpenOptions::new();
        options
            .read(true)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
        let mut file = options.open(path)?;
        let before = FileFingerprint::from_file(&file)?;
        let mut stream = self.connect()?;
        stream.write_all(b"zINSTREAM\0")?;
        let mut buffer = vec![0; STREAM_CHUNK_BYTES];
        loop {
            if cancelled.load(Ordering::Relaxed) {
                return Err(ThreatScanError::Cancelled);
            }
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            let length = u32::try_from(count).expect("stream chunk fits u32");
            stream.write_all(&length.to_be_bytes())?;
            stream.write_all(&buffer[..count])?;
        }
        stream.write_all(&0u32.to_be_bytes())?;
        let response = read_response(&mut stream, cancelled)?;
        let after = FileFingerprint::from_file(&file)?;
        if before != after {
            return Ok(ThreatFileStatus::Changed);
        }
        parse_scan_response(&response)
    }

    fn connect(&self) -> Result<ClamAvStream, ThreatScanError> {
        #[cfg(test)]
        if let Some(streams) = self.fixture_streams.as_ref() {
            let stream = streams
                .lock()
                .map_err(|_| io::Error::other("fake clamd stream lock poisoned"))?
                .pop_front()
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::ConnectionRefused, "no fake clamd stream")
                })?;
            return Ok(ClamAvStream::Fixture(stream));
        }
        let stream = UnixStream::connect(&self.socket)?;
        stream.set_read_timeout(Some(RESPONSE_POLL))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        Ok(ClamAvStream::Socket(stream))
    }
}

fn read_response(
    stream: &mut ClamAvStream,
    cancelled: &AtomicBool,
) -> Result<String, ThreatScanError> {
    let started = Instant::now();
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 1024];
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err(ThreatScanError::Cancelled);
        }
        if started.elapsed() >= RESPONSE_TIMEOUT {
            return Err(ThreatScanError::TimedOut);
        }
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                bytes.extend_from_slice(&buffer[..count]);
                if bytes.len() > MAX_RESPONSE_BYTES {
                    return Err(ThreatScanError::MalformedResponse);
                }
                if bytes.contains(&0) {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(error) => return Err(error.into()),
        }
    }
    if let Some(nul) = bytes.iter().position(|byte| *byte == 0) {
        bytes.truncate(nul);
    }
    let response = String::from_utf8(bytes).map_err(|_| ThreatScanError::MalformedResponse)?;
    if response.trim().is_empty() {
        return Err(ThreatScanError::MalformedResponse);
    }
    Ok(response.trim().to_owned())
}

fn parse_scan_response(response: &str) -> Result<ThreatFileStatus, ThreatScanError> {
    let (_, result) = response
        .rsplit_once(": ")
        .ok_or(ThreatScanError::MalformedResponse)?;
    if result == "OK" {
        return Ok(ThreatFileStatus::NoKnownSignature);
    }
    if let Some(signature) = result.strip_suffix(" FOUND") {
        if signature.is_empty() || signature.chars().count() > 512 {
            return Err(ThreatScanError::MalformedResponse);
        }
        return Ok(ThreatFileStatus::Detected {
            signature: signature.to_owned(),
        });
    }
    if let Some(reason) = result.strip_suffix(" ERROR") {
        return Ok(ThreatFileStatus::NotScanned {
            reason: reason.chars().take(512).collect(),
        });
    }
    Err(ThreatScanError::MalformedResponse)
}

fn execute_scan(
    client: &ClamAvClient,
    request: ThreatScanRequest,
    cancelled: &AtomicBool,
) -> Result<ThreatScanOutcome, ThreatScanError> {
    let engine = client.version(cancelled)?;
    let mut outcome = ThreatScanOutcome {
        generation: request.generation,
        limits: request.limits,
        engine,
        scanned_files: 0,
        no_known_signature: 0,
        detections: 0,
        not_scanned: 0,
        retained_results: Vec::new(),
        truncated: false,
        cancelled: false,
    };
    let mut queue = request
        .roots
        .into_iter()
        .map(|path| (path, 0usize, None))
        .collect::<VecDeque<_>>();
    let mut directories = 0usize;
    let mut total_bytes = 0u64;
    while let Some((path, depth, root_device)) = queue.pop_front() {
        if cancelled.load(Ordering::Relaxed) {
            outcome.cancelled = true;
            break;
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                retain_result(
                    &mut outcome,
                    ThreatFileResult {
                        path,
                        status: ThreatFileStatus::NotScanned {
                            reason: error.to_string().chars().take(512).collect(),
                        },
                    },
                );
                outcome.not_scanned += 1;
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            outcome.not_scanned += 1;
            retain_result(
                &mut outcome,
                ThreatFileResult {
                    path,
                    status: ThreatFileStatus::NotScanned {
                        reason: "symbolic links are not followed".to_owned(),
                    },
                },
            );
            continue;
        }
        if metadata.is_dir() {
            if depth >= DEPTH_CAPACITY || directories >= DIRECTORY_CAPACITY {
                outcome.truncated = true;
                continue;
            }
            let root_device = root_device.unwrap_or_else(|| metadata.dev());
            if metadata.dev() != root_device {
                outcome.not_scanned += 1;
                continue;
            }
            directories += 1;
            let mut children = match fs::read_dir(&path) {
                Ok(children) => children.filter_map(Result::ok).collect::<Vec<_>>(),
                Err(error) => {
                    outcome.not_scanned += 1;
                    retain_result(
                        &mut outcome,
                        ThreatFileResult {
                            path,
                            status: ThreatFileStatus::NotScanned {
                                reason: error.to_string().chars().take(512).collect(),
                            },
                        },
                    );
                    continue;
                }
            };
            children.sort_by_key(|entry| entry.file_name());
            for child in children {
                queue.push_back((child.path(), depth + 1, Some(root_device)));
            }
            continue;
        }
        if !metadata.is_file() {
            outcome.not_scanned += 1;
            continue;
        }
        if metadata.len() > request.limits.max_file_bytes {
            outcome.not_scanned += 1;
            retain_result(
                &mut outcome,
                ThreatFileResult {
                    path,
                    status: ThreatFileStatus::NotScanned {
                        reason: format!(
                            "file exceeds your configured {} per-file limit",
                            format_scan_limit(request.limits.max_file_bytes)
                        ),
                    },
                },
            );
            continue;
        }
        if outcome.scanned_files as usize >= FILE_CAPACITY {
            outcome.truncated = true;
            break;
        }
        if total_bytes.saturating_add(metadata.len()) > request.limits.max_total_bytes {
            outcome.not_scanned += 1;
            outcome.truncated = true;
            retain_result(
                &mut outcome,
                ThreatFileResult {
                    path,
                    status: ThreatFileStatus::NotScanned {
                        reason: format!(
                            "request reached your configured {} total scan limit",
                            format_scan_limit(request.limits.max_total_bytes)
                        ),
                    },
                },
            );
            break;
        }
        total_bytes = total_bytes.saturating_add(metadata.len());
        let status = match client.scan_file(&path, cancelled) {
            Ok(status) => status,
            Err(ThreatScanError::Cancelled) => {
                outcome.cancelled = true;
                break;
            }
            Err(error) => ThreatFileStatus::NotScanned {
                reason: error.to_string().chars().take(512).collect(),
            },
        };
        outcome.scanned_files += 1;
        match &status {
            ThreatFileStatus::NoKnownSignature => outcome.no_known_signature += 1,
            ThreatFileStatus::Detected { .. } => outcome.detections += 1,
            ThreatFileStatus::NotScanned { .. } | ThreatFileStatus::Changed => {
                outcome.not_scanned += 1;
            }
        }
        if !matches!(status, ThreatFileStatus::NoKnownSignature) {
            retain_result(&mut outcome, ThreatFileResult { path, status });
        }
    }
    Ok(outcome)
}

fn retain_result(outcome: &mut ThreatScanOutcome, result: ThreatFileResult) {
    if outcome.retained_results.len() < RETAINED_RESULT_CAPACITY {
        outcome.retained_results.push(result);
    } else {
        outcome.truncated = true;
    }
}

pub struct ThreatScanWorker {
    sender: Option<SyncSender<ThreatScanRequest>>,
    results: Receiver<ThreatScanResult>,
    cancelled: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for ThreatScanWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ThreatScanWorker")
            .finish_non_exhaustive()
    }
}

impl ThreatScanWorker {
    pub fn spawn() -> Result<Self, ThreatScanError> {
        Self::spawn_with_client(ClamAvClient::discover()?).map_err(ThreatScanError::Io)
    }

    fn spawn_with_client(client: ClamAvClient) -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<ThreatScanRequest>(REQUEST_CAPACITY);
        let (result_sender, results) = mpsc::sync_channel(RESULT_CAPACITY);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let join = thread::Builder::new()
            .name("floe-threat-scan".to_owned())
            .spawn(move || {
                while let Ok(request) = receiver.recv() {
                    worker_cancelled.store(false, Ordering::Relaxed);
                    let generation = request.generation;
                    let outcome = execute_scan(&client, request, &worker_cancelled);
                    let _ = result_sender.send(ThreatScanResult {
                        generation,
                        outcome,
                    });
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            results,
            cancelled,
            join: Some(join),
        })
    }

    pub fn submit(&self, request: ThreatScanRequest) -> Result<(), ThreatScanError> {
        let Some(sender) = self.sender.as_ref() else {
            return Err(ThreatScanError::Stopped);
        };
        match sender.try_send(request) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(ThreatScanError::QueueFull),
            Err(TrySendError::Disconnected(_)) => Err(ThreatScanError::Stopped),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn try_result(&self) -> Option<ThreatScanResult> {
        self.results.try_recv().ok()
    }
}

impl Drop for ThreatScanWorker {
    fn drop(&mut self) {
        self.cancel();
        self.sender.take();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixListener;

    use tempfile::tempdir;

    use super::*;

    fn fake_clamd(
        responses: Vec<&'static str>,
    ) -> (ClamAvClient, Vec<Arc<std::sync::Mutex<Vec<u8>>>>) {
        let mut streams = VecDeque::with_capacity(responses.len());
        let mut writes = Vec::with_capacity(responses.len());
        for response in responses {
            let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
            streams.push_back(FixtureStream {
                response: std::io::Cursor::new(format!("{response}\0").into_bytes()),
                writes: Arc::clone(&recorded),
            });
            writes.push(recorded);
        }
        let client = ClamAvClient {
            socket: PathBuf::from("/fixture/does-not-connect.sock"),
            fixture_streams: Some(Arc::new(std::sync::Mutex::new(streams))),
        };
        (client, writes)
    }

    #[test]
    fn phase_18n_clamav_parses_fragmented_ok_detection_and_error() {
        let fixture = tempdir().expect("temporary directory");
        let (client, writes) = fake_clamd(vec![
            "ClamAV 1.4/test-db/now",
            "stream: OK",
            "stream: Eicar-Test-Signature FOUND",
            "stream: INSTREAM size limit exceeded ERROR",
        ]);
        let cancelled = AtomicBool::new(false);
        assert!(
            client
                .version(&cancelled)
                .expect("version")
                .starts_with("ClamAV")
        );
        let clean = fixture.path().join("clean");
        fs::write(&clean, b"clean").expect("clean fixture");
        assert_eq!(
            client.scan_file(&clean, &cancelled).expect("clean scan"),
            ThreatFileStatus::NoKnownSignature
        );
        let detected = fixture.path().join("detected");
        fs::write(&detected, b"EICAR").expect("detected fixture");
        assert!(matches!(
            client.scan_file(&detected, &cancelled).expect("detected scan"),
            ThreatFileStatus::Detected { signature } if signature == "Eicar-Test-Signature"
        ));
        let limited = fixture.path().join("limited");
        fs::write(&limited, b"large").expect("limited fixture");
        assert!(matches!(
            client.scan_file(&limited, &cancelled).expect("limited scan"),
            ThreatFileStatus::NotScanned { reason } if reason.contains("size limit")
        ));
        assert_eq!(
            writes[0].lock().expect("version writes").as_slice(),
            b"zVERSION\0"
        );
        for write in &writes[1..] {
            let write = write.lock().expect("scan writes");
            assert!(write.starts_with(b"zINSTREAM\0"));
            assert!(write.ends_with(&0_u32.to_be_bytes()));
        }
    }

    #[test]
    fn phase_18n_clamav_rejects_malformed_response_and_symlink() {
        let fixture = tempdir().expect("temporary directory");
        assert!(parse_scan_response("not a clamd response").is_err());
        let socket = fixture.path().join("unused-clamd.sock");
        let target = fixture.path().join("target");
        let link = fixture.path().join("link");
        fs::write(&target, b"target").expect("target");
        std::os::unix::fs::symlink(&target, &link).expect("link");
        let error = ClamAvClient {
            socket,
            fixture_streams: Some(Arc::new(std::sync::Mutex::new(VecDeque::new()))),
        }
        .scan_file(&link, &AtomicBool::new(false))
        .expect_err("symlink must be rejected before streaming");
        assert!(matches!(error, ThreatScanError::Io(_)));
    }

    #[test]
    fn phase_18n_clamav_cancellation_stays_distinct_from_failure() {
        let (client, writes) = fake_clamd(vec!["ClamAV fixture"]);
        let cancelled = AtomicBool::new(true);
        assert!(matches!(
            client.version(&cancelled),
            Err(ThreatScanError::Cancelled)
        ));
        assert_eq!(
            writes[0].lock().expect("cancelled writes").as_slice(),
            b"zVERSION\0"
        );
    }

    #[test]
    fn phase_18n_clamav_path_socket_connector_when_host_permits_bind() {
        let fixture = tempdir().expect("temporary directory");
        let socket = fixture.path().join("clamd.sock");
        let listener = match UnixListener::bind(&socket) {
            Ok(listener) => listener,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::PermissionDenied | io::ErrorKind::Unsupported
                ) =>
            {
                eprintln!("SKIP path-bound fake clamd: host sandbox denied Unix bind: {error}");
                return;
            }
            Err(error) => panic!("path-bound fake clamd: {error}"),
        };
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("fake clamd accept");
            let mut command = [0_u8; 9];
            stream.read_exact(&mut command).expect("version command");
            assert_eq!(&command, b"zVERSION\0");
            stream
                .write_all(b"ClamAV path-socket fixture\0")
                .expect("version response");
        });
        let client = ClamAvClient {
            socket,
            fixture_streams: None,
        };
        assert_eq!(
            client
                .version(&AtomicBool::new(false))
                .expect("path socket version"),
            "ClamAV path-socket fixture"
        );
        server.join().expect("path socket server");
    }

    #[test]
    fn phase_18n_scan_workflow_recurses_without_following_links() {
        let fixture = tempdir().expect("temporary directory");
        let root = fixture.path().join("root");
        fs::create_dir(&root).expect("root");
        fs::write(root.join("one"), b"one").expect("one");
        fs::write(root.join("two"), b"two").expect("two");
        std::os::unix::fs::symlink(root.join("one"), root.join("alias")).expect("alias");
        let (client, writes) = fake_clamd(vec![
            "ClamAV test",
            "stream: OK",
            "stream: Test.Finding FOUND",
        ]);
        let outcome = execute_scan(
            &client,
            ThreatScanRequest::new(7, vec![root]).expect("request"),
            &AtomicBool::new(false),
        )
        .expect("scan");
        assert_eq!(outcome.scanned_files, 2);
        assert_eq!(outcome.detections, 1);
        assert_eq!(outcome.not_scanned, 1);
        assert_eq!(outcome.retained_results.len(), 2);
        assert_eq!(writes.len(), 3);
        assert_eq!(
            writes[0].lock().expect("version writes").as_slice(),
            b"zVERSION\0"
        );
    }

    #[test]
    fn clamav_configured_limits_are_immutable_bounded_and_explained() {
        let normalized = ThreatScanLimits::from_preferences(u32::MAX, 0);
        assert_eq!(
            normalized.max_file_bytes(),
            u64::from(CLAMAV_FILE_LIMIT_MIB_MAX) * MIB
        );
        assert_eq!(normalized.max_total_bytes(), normalized.max_file_bytes());

        let fixture = tempdir().expect("temporary directory");
        let oversized = fixture.path().join("oversized");
        File::create(&oversized)
            .expect("create sparse fixture")
            .set_len(MIB + 1)
            .expect("size sparse fixture");
        let (client, writes) = fake_clamd(vec!["ClamAV configured-limit fixture"]);
        let limits = ThreatScanLimits::from_preferences(1, 1);
        let request = ThreatScanRequest::with_limits(19, vec![oversized.clone()], limits)
            .expect("configured request");
        let outcome = execute_scan(&client, request, &AtomicBool::new(false))
            .expect("configured scan outcome");
        assert_eq!(outcome.limits, limits);
        assert_eq!(outcome.scanned_files, 0);
        assert_eq!(outcome.not_scanned, 1);
        assert!(matches!(
            &outcome.retained_results[0],
            ThreatFileResult {
                path,
                status: ThreatFileStatus::NotScanned { reason },
            } if path.as_path() == oversized.as_path()
                && reason == "file exceeds your configured 1 MiB per-file limit"
        ));
        assert_eq!(writes.len(), 1, "over-limit bytes never reach clamd");

        let (client, _) = fake_clamd(vec!["ClamAV configured-total fixture"]);
        let request = ThreatScanRequest::with_limits(
            20,
            vec![oversized],
            ThreatScanLimits {
                max_file_bytes: 2 * MIB,
                max_total_bytes: MIB,
            },
        )
        .expect("internal total-limit fixture request");
        let outcome = execute_scan(&client, request, &AtomicBool::new(false))
            .expect("total-limited scan outcome");
        assert!(outcome.truncated);
        assert_eq!(outcome.scanned_files, 0);
        assert_eq!(outcome.not_scanned, 1);
        assert!(matches!(
            &outcome.retained_results[0].status,
            ThreatFileStatus::NotScanned { reason }
                if reason == "request reached your configured 1 MiB total scan limit"
        ));
    }
}
