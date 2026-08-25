use std::{
    collections::HashMap,
    ffi::OsString,
    fs,
    io::Read,
    mem::MaybeUninit,
    os::{
        fd::AsRawFd,
        unix::{
            ffi::{OsStrExt, OsStringExt},
            fs::MetadataExt,
        },
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

use floe_core::{
    JobCommand, JobFailure, JobFailureKind, JobId, JobProgress, OperationId, PermissionChange,
    PermissionIdentity, PermissionRequest, PermissionScope,
};
use rustix::{
    fd::OwnedFd,
    fs::{AtFlags, FileType, Mode, OFlags, RawDir},
    process::{Gid, Uid},
};
use thiserror::Error;

use crate::job_manager::{JobManagerError, SharedJobManager};

pub const PERMISSION_QUEUE_CAPACITY: usize = 4;
pub const PERMISSION_ENTRY_CAPACITY: usize = 250_000;
const PERMISSION_DEPTH_CAPACITY: usize = 1_024;
const IDENTITY_DATABASE_CAPACITY: u64 = 1_048_576;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PermissionProgress {
    completed: u64,
    total: u64,
}
impl PermissionProgress {
    pub const fn completed(self) -> u64 {
        self.completed
    }
    pub const fn total(self) -> u64 {
        self.total
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PermissionOutcome {
    changed: u64,
    skipped_symlinks: u64,
}
impl PermissionOutcome {
    pub const fn changed(self) -> u64 {
        self.changed
    }
    pub const fn skipped_symlinks(self) -> u64 {
        self.skipped_symlinks
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PermissionError {
    #[error("permission change was cancelled before any entry changed")]
    Cancelled,
    #[error("permission target is a symbolic link and was not followed: {}", .0.display())]
    SymbolicLink(PathBuf),
    #[error("permission traversal crossed a mount boundary at {}", .0.display())]
    MountBoundary(PathBuf),
    #[error(
        "permission traversal exceeded its {PERMISSION_ENTRY_CAPACITY}-entry or {PERMISSION_DEPTH_CAPACITY}-level limit"
    )]
    LimitExceeded,
    #[error("permission preflight failed at {}: {message}", path.display())]
    Preflight { path: PathBuf, message: String },
    #[error("permission target changed before mutation: {}", .0.display())]
    SourceChanged(PathBuf),
    #[error("permission change failed at {}: {message}", path.display())]
    Io { path: PathBuf, message: String },
    #[error("local {kind} name was not found")]
    UnknownIdentity { kind: &'static str },
    #[error("local {kind} identity database could not be read: {message}")]
    IdentityDatabase { kind: &'static str, message: String },
    #[error("permission change stopped after {changed} of {total} entries; failed at {}: {message}", path.display())]
    Partial {
        changed: u64,
        total: u64,
        path: PathBuf,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlannedKind {
    File,
    Directory,
    Other,
}

#[derive(Clone, Debug)]
struct PlannedEntry {
    path: PathBuf,
    device: u64,
    inode: u64,
    kind: PlannedKind,
    mode: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ResolvedPermissionChange {
    file_mode: Option<u32>,
    directory_mode: Option<u32>,
    executable: Option<bool>,
    owner: Option<u32>,
    group: Option<u32>,
}

pub fn execute_permission_change(
    request: &PermissionRequest,
    cancelled: impl Fn() -> bool,
    mut on_progress: impl FnMut(PermissionProgress),
) -> Result<PermissionOutcome, PermissionError> {
    let change = resolve_permission_change(request.change())?;
    let mut plan = Vec::new();
    let mut skipped_symlinks = 0u64;
    for target in request.targets() {
        if cancelled() {
            return Err(PermissionError::Cancelled);
        }
        let metadata =
            fs::symlink_metadata(target).map_err(|error| PermissionError::Preflight {
                path: target.clone(),
                message: error.to_string(),
            })?;
        if metadata.file_type().is_symlink() {
            return Err(PermissionError::SymbolicLink(target.clone()));
        }
        let root_device = metadata.dev();
        push_planned(target.clone(), &metadata, &mut plan)?;
        if matches!(request.scope(), PermissionScope::Recursive) && metadata.is_dir() {
            let directory = rustix::fs::open(
                target,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
                Mode::empty(),
            )
            .map_err(|error| PermissionError::Preflight {
                path: target.clone(),
                message: error.to_string(),
            })?;
            preflight_directory(
                &directory,
                target,
                root_device,
                0,
                &cancelled,
                &mut plan,
                &mut skipped_symlinks,
            )?;
        }
    }
    let total = u64::try_from(plan.len()).unwrap_or(u64::MAX);
    let mut changed = 0u64;
    for entry in plan {
        if cancelled() {
            return if changed == 0 {
                Err(PermissionError::Cancelled)
            } else {
                Err(PermissionError::Partial {
                    changed,
                    total,
                    path: entry.path,
                    message: "cancelled after committed changes".to_owned(),
                })
            };
        }
        if let Err(error) = apply_entry(&entry, change) {
            return if changed == 0 {
                Err(error)
            } else {
                Err(PermissionError::Partial {
                    changed,
                    total,
                    path: entry.path,
                    message: error.to_string(),
                })
            };
        }
        changed += 1;
        on_progress(PermissionProgress {
            completed: changed,
            total,
        });
    }
    Ok(PermissionOutcome {
        changed,
        skipped_symlinks,
    })
}

fn resolve_permission_change(
    change: &PermissionChange,
) -> Result<ResolvedPermissionChange, PermissionError> {
    Ok(ResolvedPermissionChange {
        file_mode: change.file_mode,
        directory_mode: change.directory_mode,
        executable: change.executable,
        owner: change
            .owner
            .as_ref()
            .map(|identity| resolve_identity(identity, Path::new("/etc/passwd"), 2, "owner"))
            .transpose()?,
        group: change
            .group
            .as_ref()
            .map(|identity| resolve_identity(identity, Path::new("/etc/group"), 2, "group"))
            .transpose()?,
    })
}

fn resolve_identity(
    identity: &PermissionIdentity,
    database: &Path,
    id_field: usize,
    kind: &'static str,
) -> Result<u32, PermissionError> {
    let PermissionIdentity::LocalName(name) = identity else {
        let PermissionIdentity::Id(id) = identity else {
            unreachable!("permission identity variants are exhaustive")
        };
        return Ok(*id);
    };
    let descriptor = rustix::fs::open(
        database,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|error| PermissionError::IdentityDatabase {
        kind,
        message: error.to_string(),
    })?;
    let mut bytes = Vec::new();
    fs::File::from(descriptor)
        .take(IDENTITY_DATABASE_CAPACITY + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| PermissionError::IdentityDatabase {
            kind,
            message: error.to_string(),
        })?;
    if bytes.len() as u64 > IDENTITY_DATABASE_CAPACITY {
        return Err(PermissionError::IdentityDatabase {
            kind,
            message: "database exceeds the bounded read limit".to_owned(),
        });
    }
    for line in bytes.split(|byte| *byte == b'\n') {
        let mut fields = line.split(|byte| *byte == b':');
        if fields.next() != Some(name.as_bytes()) {
            continue;
        }
        let Some(raw_id) = fields.nth(id_field.saturating_sub(1)) else {
            break;
        };
        if let Ok(id) = std::str::from_utf8(raw_id)
            .unwrap_or_default()
            .parse::<u32>()
        {
            return Ok(id);
        }
        break;
    }
    Err(PermissionError::UnknownIdentity { kind })
}

fn push_planned(
    path: PathBuf,
    metadata: &fs::Metadata,
    plan: &mut Vec<PlannedEntry>,
) -> Result<(), PermissionError> {
    if plan.len() >= PERMISSION_ENTRY_CAPACITY {
        return Err(PermissionError::LimitExceeded);
    }
    let kind = if metadata.is_file() {
        PlannedKind::File
    } else if metadata.is_dir() {
        PlannedKind::Directory
    } else {
        PlannedKind::Other
    };
    plan.push(PlannedEntry {
        path,
        device: metadata.dev(),
        inode: metadata.ino(),
        kind,
        mode: metadata.mode(),
    });
    Ok(())
}

fn preflight_directory(
    directory: &OwnedFd,
    path: &Path,
    root_device: u64,
    depth: usize,
    cancelled: &impl Fn() -> bool,
    plan: &mut Vec<PlannedEntry>,
    skipped_symlinks: &mut u64,
) -> Result<(), PermissionError> {
    if depth >= PERMISSION_DEPTH_CAPACITY {
        return Err(PermissionError::LimitExceeded);
    }
    let mut buffer = [MaybeUninit::<u8>::uninit(); 8_192];
    let mut entries = RawDir::new(directory, &mut buffer);
    while let Some(entry) = entries.next() {
        if cancelled() {
            return Err(PermissionError::Cancelled);
        }
        let entry = entry.map_err(|error| PermissionError::Preflight {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        let bytes = entry.file_name().to_bytes();
        if bytes == b"." || bytes == b".." {
            continue;
        }
        let name = entry.file_name().to_owned();
        let child_path = path.join(OsString::from_vec(name.to_bytes().to_vec()));
        let stat = rustix::fs::statat(directory, name.as_c_str(), AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|error| PermissionError::Preflight {
                path: child_path.clone(),
                message: error.to_string(),
            })?;
        if stat.st_dev != root_device {
            return Err(PermissionError::MountBoundary(child_path));
        }
        let kind = FileType::from_raw_mode(stat.st_mode);
        if kind.is_symlink() {
            *skipped_symlinks += 1;
            continue;
        }
        if plan.len() >= PERMISSION_ENTRY_CAPACITY {
            return Err(PermissionError::LimitExceeded);
        }
        let planned_kind = if kind.is_file() {
            PlannedKind::File
        } else if kind.is_dir() {
            PlannedKind::Directory
        } else {
            PlannedKind::Other
        };
        plan.push(PlannedEntry {
            path: child_path.clone(),
            device: stat.st_dev,
            inode: stat.st_ino,
            kind: planned_kind,
            mode: stat.st_mode,
        });
        if kind.is_dir() {
            let child = rustix::fs::openat(
                directory,
                name.as_c_str(),
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
                Mode::empty(),
            )
            .map_err(|error| PermissionError::Preflight {
                path: child_path.clone(),
                message: error.to_string(),
            })?;
            preflight_directory(
                &child,
                &child_path,
                root_device,
                depth + 1,
                cancelled,
                plan,
                skipped_symlinks,
            )?;
        }
    }
    Ok(())
}

fn apply_entry(
    entry: &PlannedEntry,
    change: ResolvedPermissionChange,
) -> Result<(), PermissionError> {
    let descriptor = rustix::fs::open(
        &entry.path,
        OFlags::PATH | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|error| PermissionError::Io {
        path: entry.path.clone(),
        message: error.to_string(),
    })?;
    let stat = rustix::fs::fstat(&descriptor).map_err(|error| PermissionError::Io {
        path: entry.path.clone(),
        message: error.to_string(),
    })?;
    if stat.st_dev != entry.device
        || stat.st_ino != entry.inode
        || FileType::from_raw_mode(stat.st_mode).is_symlink()
    {
        return Err(PermissionError::SourceChanged(entry.path.clone()));
    }
    if change.owner.is_some() || change.group.is_some() {
        let descriptor_path = PathBuf::from(format!("/proc/self/fd/{}", descriptor.as_raw_fd()));
        rustix::fs::chown(
            &descriptor_path,
            change.owner.map(Uid::from_raw),
            change.group.map(Gid::from_raw),
        )
        .map_err(|error| PermissionError::Io {
            path: entry.path.clone(),
            message: error.to_string(),
        })?;
    }
    let explicit = match entry.kind {
        PlannedKind::File => change.file_mode,
        PlannedKind::Directory => change.directory_mode,
        PlannedKind::Other => change.file_mode,
    };
    let desired = explicit.or_else(|| {
        change.executable.and_then(|enabled| {
            matches!(entry.kind, PlannedKind::File).then(|| {
                if enabled {
                    entry.mode | 0o100
                } else {
                    entry.mode & !0o111
                }
            })
        })
    });
    if let Some(mode) = desired {
        let descriptor_path = PathBuf::from(format!("/proc/self/fd/{}", descriptor.as_raw_fd()));
        rustix::fs::chmod(&descriptor_path, Mode::from_raw_mode(mode)).map_err(|error| {
            PermissionError::Io {
                path: entry.path.clone(),
                message: error.to_string(),
            }
        })?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PermissionSubmission {
    operation_id: OperationId,
    job_id: JobId,
}
impl PermissionSubmission {
    pub const fn operation_id(self) -> OperationId {
        self.operation_id
    }
    pub const fn job_id(self) -> JobId {
        self.job_id
    }
}

#[derive(Debug, Error)]
pub enum PermissionExecutorSpawnError {
    #[error("permission queue capacity cannot be zero")]
    ZeroCapacity,
    #[error("could not spawn permission worker: {0}")]
    Thread(#[source] std::io::Error),
}
#[derive(Debug, Error)]
pub enum PermissionSubmitError {
    #[error(transparent)]
    JobManager(#[from] JobManagerError),
    #[error("permission queue is full for {0:?}")]
    QueueFull(JobId),
    #[error("permission executor stopped for {0:?}")]
    ExecutorStopped(JobId),
}
impl PermissionSubmitError {
    pub const fn job_id(&self) -> Option<JobId> {
        match self {
            Self::QueueFull(id) | Self::ExecutorStopped(id) => Some(*id),
            Self::JobManager(_) => None,
        }
    }
}
#[derive(Debug, Error, Eq, PartialEq)]
pub enum PermissionCancelError {
    #[error("permission job is not active: {0:?}")]
    NotActive(JobId),
}

struct Task {
    job_id: JobId,
    request: PermissionRequest,
    cancellation: Arc<AtomicBool>,
}
enum Command {
    Execute(Task),
    Shutdown,
}

#[derive(Debug)]
pub struct PermissionExecutor {
    sender: Option<SyncSender<Command>>,
    cancellations: Arc<Mutex<HashMap<JobId, Arc<AtomicBool>>>>,
    jobs: SharedJobManager,
    worker: Option<JoinHandle<()>>,
}

impl PermissionExecutor {
    pub fn spawn(jobs: SharedJobManager) -> Result<Self, PermissionExecutorSpawnError> {
        Self::spawn_with_capacity(jobs, PERMISSION_QUEUE_CAPACITY)
    }
    fn spawn_with_capacity(
        jobs: SharedJobManager,
        capacity: usize,
    ) -> Result<Self, PermissionExecutorSpawnError> {
        if capacity == 0 {
            return Err(PermissionExecutorSpawnError::ZeroCapacity);
        }
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let cancellations = Arc::new(Mutex::new(HashMap::new()));
        let worker_jobs = Arc::clone(&jobs);
        let worker_cancellations = Arc::clone(&cancellations);
        let worker = thread::Builder::new()
            .name("floe-permissions-worker".to_owned())
            .spawn(move || run_worker(receiver, worker_jobs, worker_cancellations))
            .map_err(PermissionExecutorSpawnError::Thread)?;
        Ok(Self {
            sender: Some(sender),
            cancellations,
            jobs,
            worker: Some(worker),
        })
    }
    pub fn submit(
        &self,
        request: PermissionRequest,
    ) -> Result<PermissionSubmission, PermissionSubmitError> {
        let queued = lock(&self.jobs).queue_operation()?;
        let operation_id = queued.operation_id();
        let job_id = queued.job_id();
        let cancellation = Arc::new(AtomicBool::new(false));
        lock(&self.cancellations).insert(job_id, Arc::clone(&cancellation));
        let Some(sender) = &self.sender else {
            return Err(PermissionSubmitError::ExecutorStopped(job_id));
        };
        match sender.try_send(Command::Execute(Task {
            job_id,
            request,
            cancellation,
        })) {
            Ok(()) => Ok(PermissionSubmission {
                operation_id,
                job_id,
            }),
            Err(TrySendError::Full(_)) => Err(PermissionSubmitError::QueueFull(job_id)),
            Err(TrySendError::Disconnected(_)) => {
                Err(PermissionSubmitError::ExecutorStopped(job_id))
            }
        }
    }
    pub fn cancel(&self, job_id: JobId) -> Result<(), PermissionCancelError> {
        lock(&self.cancellations)
            .get(&job_id)
            .ok_or(PermissionCancelError::NotActive(job_id))?
            .store(true, Ordering::Release);
        Ok(())
    }
}
impl Drop for PermissionExecutor {
    fn drop(&mut self) {
        for cancellation in lock(&self.cancellations).values() {
            cancellation.store(true, Ordering::Release);
        }
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(Command::Shutdown);
        }
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("permission worker panicked during shutdown");
        }
    }
}

fn run_worker(
    receiver: Receiver<Command>,
    jobs: SharedJobManager,
    cancellations: Arc<Mutex<HashMap<JobId, Arc<AtomicBool>>>>,
) {
    while let Ok(command) = receiver.recv() {
        match command {
            Command::Execute(task) => run_task(task, &jobs, &cancellations),
            Command::Shutdown => return,
        }
    }
}
fn run_task(
    task: Task,
    jobs: &SharedJobManager,
    cancellations: &Mutex<HashMap<JobId, Arc<AtomicBool>>>,
) {
    if transition(jobs, task.job_id, JobCommand::Start).is_err() {
        lock(cancellations).remove(&task.job_id);
        return;
    }
    let result = execute_permission_change(
        &task.request,
        || task.cancellation.load(Ordering::Acquire),
        |progress| {
            if let Ok(value) = JobProgress::items(progress.completed(), Some(progress.total())) {
                let _ = transition(jobs, task.job_id, JobCommand::SetProgress(value));
            }
        },
    );
    let command = match result {
        Ok(outcome) => {
            tracing::debug!(
                changed = outcome.changed(),
                skipped_symlinks = outcome.skipped_symlinks(),
                "permission job completed"
            );
            JobCommand::Complete
        }
        Err(PermissionError::Cancelled) => JobCommand::Cancel,
        Err(error) => JobCommand::Fail(permission_failure(&error)),
    };
    let _ = transition(jobs, task.job_id, command);
    lock(cancellations).remove(&task.job_id);
}
fn permission_failure(error: &PermissionError) -> JobFailure {
    let kind = if matches!(error, PermissionError::Partial { .. }) {
        JobFailureKind::Partial
    } else if matches!(
        error,
        PermissionError::MountBoundary(_)
            | PermissionError::LimitExceeded
            | PermissionError::SymbolicLink(_)
    ) {
        JobFailureKind::Unsupported
    } else {
        JobFailureKind::Io
    };
    JobFailure::new(kind, error.to_string())
}
fn transition(
    jobs: &SharedJobManager,
    job_id: JobId,
    command: JobCommand,
) -> Result<(), JobManagerError> {
    lock(jobs).transition(job_id, command).map(|_| ())
}
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_manager::ApplicationJobManager;
    use floe_core::{JobState, PermissionChange, PermissionRequest, PermissionScope};
    use std::{
        os::unix::fs::{MetadataExt, PermissionsExt, symlink},
        thread,
    };
    use tempfile::tempdir;

    #[test]
    fn phase_10d_permission_executor_applies_mode_and_rejects_symlink_without_following() {
        let root = tempdir().expect("root");
        let passwd = root.path().join("passwd");
        fs::write(&passwd, b"local-user:x:4242:100::/tmp:/bin/false\n").expect("identity database");
        assert_eq!(
            resolve_identity(
                &PermissionIdentity::local_name(OsString::from("local-user"))
                    .expect("local identity"),
                &passwd,
                2,
                "owner",
            )
            .expect("resolved local owner"),
            4242
        );
        assert_eq!(
            resolve_identity(
                &PermissionIdentity::local_name(OsString::from("missing")).expect("local identity"),
                &passwd,
                2,
                "owner",
            ),
            Err(PermissionError::UnknownIdentity { kind: "owner" })
        );
        let file = root.path().join("file");
        fs::write(&file, b"x").expect("file");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).expect("mode");
        let change = PermissionChange::new(Some(0o640), None, None, None, None).expect("change");
        let request =
            PermissionRequest::new(vec![file.clone()], PermissionScope::Direct, change.clone())
                .expect("request");
        let outcome = execute_permission_change(&request, || false, |_| {}).expect("execute");
        assert_eq!(outcome.changed(), 1);
        assert_eq!(
            fs::symlink_metadata(&file).expect("metadata").mode() & 0o7777,
            0o640
        );
        let target = root.path().join("target");
        fs::write(&target, b"target").expect("target");
        let link = root.path().join("link");
        symlink(&target, &link).expect("link");
        let request = PermissionRequest::new(vec![link.clone()], PermissionScope::Direct, change)
            .expect("request");
        assert_eq!(
            execute_permission_change(&request, || false, |_| {}),
            Err(PermissionError::SymbolicLink(link))
        );
        assert_eq!(
            fs::symlink_metadata(target)
                .expect("target metadata")
                .mode()
                & 0o7777,
            0o644
        );

        let first = root.path().join("partial-first");
        let second = root.path().join("partial-second");
        fs::write(&first, b"first").expect("first");
        fs::write(&second, b"second").expect("second");
        let request = PermissionRequest::new(
            vec![first.clone(), second.clone()],
            PermissionScope::Direct,
            PermissionChange::new(Some(0o600), None, None, None, None).expect("change"),
        )
        .expect("request");
        let cancellation_checks = std::cell::Cell::new(0usize);
        let error = execute_permission_change(
            &request,
            || {
                let check = cancellation_checks.get();
                cancellation_checks.set(check + 1);
                check >= 3
            },
            |_| {},
        )
        .expect_err("cancellation after the first commit must be partial");
        assert!(matches!(
            error,
            PermissionError::Partial {
                changed: 1,
                total: 2,
                path,
                ..
            } if path == second
        ));
        assert_eq!(
            fs::metadata(&first).expect("first mode").mode() & 0o7777,
            0o600
        );
        assert_eq!(
            fs::metadata(&second).expect("second mode").mode() & 0o7777,
            0o644
        );

        let stale = root.path().join("stale");
        let displaced = root.path().join("stale-displaced");
        fs::write(&stale, b"original").expect("stale original");
        let request = PermissionRequest::new(
            vec![stale.clone()],
            PermissionScope::Direct,
            PermissionChange::new(Some(0o600), None, None, None, None).expect("change"),
        )
        .expect("request");
        let stale_checks = std::cell::Cell::new(0usize);
        let error = execute_permission_change(
            &request,
            || {
                let check = stale_checks.get();
                stale_checks.set(check + 1);
                if check == 1 {
                    fs::rename(&stale, &displaced).expect("displace stale target");
                    fs::write(&stale, b"replacement").expect("replacement target");
                }
                false
            },
            |_| {},
        )
        .expect_err("replaced target must fail identity revalidation");
        assert_eq!(error, PermissionError::SourceChanged(stale.clone()));
        assert_eq!(
            fs::metadata(&stale).expect("replacement mode").mode() & 0o7777,
            0o644
        );
        assert_eq!(
            fs::metadata(&displaced).expect("original mode").mode() & 0o7777,
            0o644
        );
        assert_eq!(
            permission_failure(&PermissionError::MountBoundary(PathBuf::from("/tmp/mount"))).kind(),
            JobFailureKind::Unsupported
        );
    }

    #[test]
    fn phase_10d_recursive_permissions_are_no_follow_and_distinguish_file_directory_modes() {
        let root = tempdir().expect("root");
        let folder = root.path().join("folder");
        fs::create_dir(&folder).expect("folder");
        fs::create_dir(folder.join("nested")).expect("nested");
        fs::write(folder.join("nested/file"), b"x").expect("file");
        symlink(root.path(), folder.join("escape")).expect("link");
        let change =
            PermissionChange::new(Some(0o640), Some(0o750), None, None, None).expect("change");
        let request =
            PermissionRequest::new(vec![folder.clone()], PermissionScope::Recursive, change)
                .expect("request");
        let outcome = execute_permission_change(&request, || false, |_| {}).expect("execute");
        assert_eq!(outcome.changed(), 3);
        assert_eq!(outcome.skipped_symlinks(), 1);
        assert_eq!(
            fs::metadata(folder.join("nested/file"))
                .expect("file metadata")
                .mode()
                & 0o7777,
            0o640
        );
        assert_eq!(
            fs::metadata(folder.join("nested"))
                .expect("dir metadata")
                .mode()
                & 0o7777,
            0o750
        );
    }

    #[test]
    fn phase_10d_permission_executor_reports_job_completion_and_precancel() {
        let root = tempdir().expect("root");
        let file = root.path().join("file");
        fs::write(&file, b"x").expect("file");
        let jobs = Arc::new(Mutex::new(ApplicationJobManager::new()));
        let executor = PermissionExecutor::spawn(Arc::clone(&jobs)).expect("executor");
        let change = PermissionChange::new(Some(0o600), None, None, None, None).expect("change");
        let request =
            PermissionRequest::new(vec![file], PermissionScope::Direct, change).expect("request");
        let submission = executor.submit(request).expect("submit");
        loop {
            let state = lock(&jobs)
                .record(submission.job_id())
                .map(|record| record.state());
            if state == Some(JobState::Completed) {
                break;
            }
            thread::yield_now();
        }
    }
}
