//! Bounded duplicate discovery with injected reviewed hashing.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::{self, File},
    hash::{DefaultHasher, Hash, Hasher},
    io::{Read, Seek, SeekFrom},
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
    sync::{Condvar, Mutex},
    thread,
};

use rustix::fs::{FileType, Mode, OFlags};
use thiserror::Error;

pub const DUPLICATE_ROOT_CAPACITY: usize = 4_096;
pub const DUPLICATE_FILE_CAPACITY: usize = 1_000_000;
pub const DUPLICATE_DIRECTORY_CAPACITY: usize = 100_000;
pub const DUPLICATE_DEPTH_CAPACITY: usize = 128;
pub const DUPLICATE_GROUP_CAPACITY: usize = 10_000;
pub const DUPLICATE_RESULT_PATH_CAPACITY: usize = 100_000;
pub const DUPLICATE_FILE_BYTES: u64 = 256 * 1024 * 1024 * 1024;
pub const DUPLICATE_TOTAL_HASH_BYTES: u64 = 1024 * 1024 * 1024 * 1024;
const COMPARE_CHUNK_BYTES: usize = 1024 * 1024;
const QUICK_SAMPLE_BYTES: usize = 64 * 1024;
pub const DUPLICATE_HASH_WORKERS: usize = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateScanRequest {
    roots: Vec<PathBuf>,
    reference: Option<PathBuf>,
}

impl DuplicateScanRequest {
    pub fn new(roots: Vec<PathBuf>) -> Result<Self, DuplicateScanError> {
        if roots.is_empty() || roots.len() > DUPLICATE_ROOT_CAPACITY {
            return Err(DuplicateScanError::InvalidRootCount);
        }
        let mut seen = HashSet::with_capacity(roots.len());
        for root in &roots {
            if !root.is_absolute() {
                return Err(DuplicateScanError::RelativeRoot(root.clone()));
            }
            if root.file_name().is_none()
                || root
                    .components()
                    .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
            {
                return Err(DuplicateScanError::UnsafeRoot(root.clone()));
            }
            if !seen.insert(root.clone()) {
                return Err(DuplicateScanError::DuplicateRoot(root.clone()));
            }
        }
        Ok(Self {
            roots,
            reference: None,
        })
    }

    pub fn for_folder(root: PathBuf) -> Result<Self, DuplicateScanError> {
        Self::new(vec![root])
    }

    pub fn for_reference(reference: PathBuf, folder: PathBuf) -> Result<Self, DuplicateScanError> {
        let mut request = Self::new(vec![reference.clone(), folder])?;
        request.reference = Some(reference);
        Ok(request)
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub fn reference(&self) -> Option<&Path> {
        self.reference.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileFingerprint {
    identity: FileIdentity,
    size: u64,
    modified_seconds: i64,
    modified_nanos: i64,
    changed_seconds: i64,
    changed_nanos: i64,
}

impl FileFingerprint {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            identity: FileIdentity {
                device: metadata.dev(),
                inode: metadata.ino(),
            },
            size: metadata.len(),
            modified_seconds: metadata.mtime(),
            modified_nanos: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanos: metadata.ctime_nsec(),
        }
    }

    fn matches(self, metadata: &fs::Metadata) -> bool {
        self == Self::from_metadata(metadata)
    }
}

#[derive(Clone, Debug)]
struct Candidate {
    path: PathBuf,
    fingerprint: FileFingerprint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateItem {
    path: PathBuf,
    hard_link_alias: bool,
}

impl DuplicateItem {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub const fn is_hard_link_alias(&self) -> bool {
        self.hard_link_alias
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateGroup {
    size: u64,
    items: Vec<DuplicateItem>,
    independent_copies: usize,
}

impl DuplicateGroup {
    pub const fn size(&self) -> u64 {
        self.size
    }

    pub fn items(&self) -> &[DuplicateItem] {
        &self.items
    }

    pub const fn independent_copies(&self) -> usize {
        self.independent_copies
    }

    pub fn reclaimable_bytes(&self) -> u64 {
        self.size.saturating_mul(
            u64::try_from(self.independent_copies.saturating_sub(1)).unwrap_or(u64::MAX),
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DuplicateScanPhase {
    #[default]
    Discovering,
    QuickFiltering,
    Hashing,
    Confirming,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DuplicateScanSummary {
    pub phase: DuplicateScanPhase,
    pub examined_files: usize,
    pub examined_directories: usize,
    pub candidate_files: usize,
    pub quick_checked_files: usize,
    pub quick_read_bytes: u64,
    pub hashed_files: usize,
    pub hashed_bytes: u64,
    pub reused_hashes: usize,
    pub reused_hash_bytes: u64,
    pub compared_bytes: u64,
    pub skipped_entries: usize,
    pub skipped_directories: usize,
    pub skipped_links: usize,
    pub skipped_mounts: usize,
    pub skipped_over_limit: usize,
    pub changed_files: usize,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateScanOutcome {
    groups: Vec<DuplicateGroup>,
    summary: DuplicateScanSummary,
}

impl DuplicateScanOutcome {
    pub fn groups(&self) -> &[DuplicateGroup] {
        &self.groups
    }

    pub const fn summary(&self) -> DuplicateScanSummary {
        self.summary
    }

    pub fn reclaimable_bytes(&self) -> u64 {
        self.groups
            .iter()
            .map(DuplicateGroup::reclaimable_bytes)
            .fold(0, u64::saturating_add)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DuplicateScanLimits {
    pub files: usize,
    pub directories: usize,
    pub depth: usize,
    pub groups: usize,
    pub result_paths: usize,
    pub file_bytes: u64,
    pub total_hash_bytes: u64,
    pub hash_workers: usize,
}

impl Default for DuplicateScanLimits {
    fn default() -> Self {
        Self {
            files: DUPLICATE_FILE_CAPACITY,
            directories: DUPLICATE_DIRECTORY_CAPACITY,
            depth: DUPLICATE_DEPTH_CAPACITY,
            groups: DUPLICATE_GROUP_CAPACITY,
            result_paths: DUPLICATE_RESULT_PATH_CAPACITY,
            file_bytes: DUPLICATE_FILE_BYTES,
            total_hash_bytes: DUPLICATE_TOTAL_HASH_BYTES,
            hash_workers: DUPLICATE_HASH_WORKERS,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DuplicateHashResult {
    digest: [u8; 32],
    reused: bool,
}

impl DuplicateHashResult {
    pub const fn computed(digest: [u8; 32]) -> Self {
        Self {
            digest,
            reused: false,
        }
    }

    pub const fn reused(digest: [u8; 32]) -> Self {
        Self {
            digest,
            reused: true,
        }
    }

    pub const fn digest(self) -> [u8; 32] {
        self.digest
    }

    pub const fn was_reused(self) -> bool {
        self.reused
    }
}

#[derive(Debug, Error)]
pub enum DuplicateHashError {
    #[error("duplicate scan cancelled")]
    Cancelled,
    #[error("hashing failed: {0}")]
    Failed(String),
}

#[derive(Debug, Error)]
pub enum DuplicateScanError {
    #[error("select between one and {DUPLICATE_ROOT_CAPACITY} files or folders")]
    InvalidRootCount,
    #[error("duplicate scan root must be absolute: {}", .0.display())]
    RelativeRoot(PathBuf),
    #[error("duplicate scan root is unsafe: {}", .0.display())]
    UnsafeRoot(PathBuf),
    #[error("duplicate scan root was repeated: {}", .0.display())]
    DuplicateRoot(PathBuf),
    #[error("reference file is unavailable or is not a regular local file: {}", .0.display())]
    ReferenceUnavailable(PathBuf),
    #[error("duplicate scan limits must be non-zero")]
    InvalidLimits,
    #[error("duplicate scan cancelled")]
    Cancelled,
    #[error("could not inspect duplicate root {}: {message}", path.display())]
    Root { path: PathBuf, message: String },
    #[error("hashing {} failed: {message}", path.display())]
    Hash { path: PathBuf, message: String },
    #[error("duplicate comparison I/O failed at {}: {message}", path.display())]
    Compare { path: PathBuf, message: String },
}

pub fn find_duplicates(
    request: &DuplicateScanRequest,
    limits: DuplicateScanLimits,
    is_cancelled: impl Fn() -> bool + Sync,
    hash_file: impl Fn(&Path) -> Result<DuplicateHashResult, DuplicateHashError> + Sync,
    mut on_progress: impl FnMut(DuplicateScanSummary),
) -> Result<DuplicateScanOutcome, DuplicateScanError> {
    if limits.files == 0
        || limits.directories == 0
        || limits.groups == 0
        || limits.result_paths == 0
        || limits.file_bytes == 0
        || limits.total_hash_bytes == 0
        || limits.hash_workers == 0
    {
        return Err(DuplicateScanError::InvalidLimits);
    }
    let mut summary = DuplicateScanSummary::default();
    let mut candidates = Vec::new();
    let mut seen_paths = HashSet::new();
    for root in request.roots() {
        if is_cancelled() {
            return Err(DuplicateScanError::Cancelled);
        }
        let metadata = fs::symlink_metadata(root).map_err(|error| DuplicateScanError::Root {
            path: root.clone(),
            message: error.to_string(),
        })?;
        if metadata.file_type().is_symlink() {
            summary.skipped_links = summary.skipped_links.saturating_add(1);
            continue;
        }
        if metadata.is_file() {
            add_candidate(
                root.clone(),
                &metadata,
                limits,
                &mut summary,
                &mut candidates,
                &mut seen_paths,
            );
            continue;
        }
        if !metadata.is_dir() {
            summary.skipped_entries = summary.skipped_entries.saturating_add(1);
            continue;
        }
        let root_device = metadata.dev();
        let mut queue = VecDeque::from([(root.clone(), 0usize)]);
        while let Some((directory, depth)) = queue.pop_front() {
            if is_cancelled() {
                return Err(DuplicateScanError::Cancelled);
            }
            if summary.examined_directories >= limits.directories {
                summary.truncated = true;
                break;
            }
            summary.examined_directories = summary.examined_directories.saturating_add(1);
            let reader = match fs::read_dir(&directory) {
                Ok(reader) => reader,
                Err(_) => {
                    summary.skipped_directories = summary.skipped_directories.saturating_add(1);
                    continue;
                }
            };
            for child in reader {
                if is_cancelled() {
                    return Err(DuplicateScanError::Cancelled);
                }
                let child = match child {
                    Ok(child) => child,
                    Err(_) => {
                        summary.skipped_entries = summary.skipped_entries.saturating_add(1);
                        continue;
                    }
                };
                let path = child.path();
                let metadata = match fs::symlink_metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(_) => {
                        summary.skipped_entries = summary.skipped_entries.saturating_add(1);
                        continue;
                    }
                };
                if metadata.file_type().is_symlink() {
                    summary.skipped_links = summary.skipped_links.saturating_add(1);
                } else if metadata.is_file() {
                    add_candidate(
                        path,
                        &metadata,
                        limits,
                        &mut summary,
                        &mut candidates,
                        &mut seen_paths,
                    );
                } else if metadata.is_dir() {
                    if metadata.dev() != root_device {
                        summary.skipped_mounts = summary.skipped_mounts.saturating_add(1);
                    } else if depth >= limits.depth {
                        summary.truncated = true;
                    } else {
                        queue.push_back((path, depth.saturating_add(1)));
                    }
                } else {
                    summary.skipped_entries = summary.skipped_entries.saturating_add(1);
                }
                if summary.examined_files >= limits.files {
                    summary.truncated = true;
                    break;
                }
            }
            on_progress(summary);
            if summary.truncated && summary.examined_files >= limits.files {
                break;
            }
        }
    }

    let reference_size = request
        .reference()
        .map(|reference| {
            candidates
                .iter()
                .find(|candidate| candidate.path == reference)
                .map(|candidate| candidate.fingerprint.size)
                .ok_or_else(|| DuplicateScanError::ReferenceUnavailable(reference.to_path_buf()))
        })
        .transpose()?;
    if let Some(reference_size) = reference_size {
        candidates.retain(|candidate| candidate.fingerprint.size == reference_size);
    }

    let mut by_size: HashMap<u64, Vec<Candidate>> = HashMap::new();
    for candidate in candidates {
        by_size
            .entry(candidate.fingerprint.size)
            .or_default()
            .push(candidate);
    }
    let mut groups = Vec::new();
    let mut result_paths = 0usize;
    let mut sizes = by_size.into_iter().collect::<Vec<_>>();
    sizes.sort_by_key(|(size, _)| *size);
    for (size, candidates) in sizes {
        if candidates.len() < 2 {
            continue;
        }
        summary.candidate_files = summary.candidate_files.saturating_add(candidates.len());
        let mut by_identity: HashMap<FileIdentity, Vec<Candidate>> = HashMap::new();
        for candidate in candidates {
            by_identity
                .entry(candidate.fingerprint.identity)
                .or_default()
                .push(candidate);
        }
        if by_identity.len() == 1 {
            let aliases = by_identity.into_values().next().unwrap_or_default();
            if aliases.len() > 1 && aliases_current(&aliases) {
                push_group(
                    &mut groups,
                    &mut result_paths,
                    limits,
                    size,
                    vec![aliases],
                    &mut summary,
                );
            } else if aliases.len() > 1 {
                summary.changed_files = summary.changed_files.saturating_add(aliases.len());
            }
            continue;
        }
        summary.phase = DuplicateScanPhase::QuickFiltering;
        on_progress(summary);
        let mut by_quick: HashMap<[u64; 2], Vec<Vec<Candidate>>> = HashMap::new();
        let mut identities = by_identity.into_values().collect::<Vec<_>>();
        identities.sort_by(|left, right| {
            left.first()
                .map(|candidate| &candidate.path)
                .cmp(&right.first().map(|candidate| &candidate.path))
        });
        for aliases in identities {
            let Some(first) = aliases.first() else {
                continue;
            };
            if size > limits.file_bytes {
                summary.skipped_over_limit =
                    summary.skipped_over_limit.saturating_add(aliases.len());
                continue;
            }
            match quick_signature(first, &is_cancelled, &mut summary) {
                Ok(signature) if aliases_current(&aliases) => {
                    by_quick.entry(signature).or_default().push(aliases);
                }
                Ok(_) => {
                    summary.changed_files = summary.changed_files.saturating_add(aliases.len());
                }
                Err(DuplicateScanError::Cancelled) => {
                    return Err(DuplicateScanError::Cancelled);
                }
                Err(_) => {
                    summary.skipped_entries = summary.skipped_entries.saturating_add(aliases.len());
                }
            }
        }
        on_progress(summary);

        let mut planned_bytes = 0_u64;
        let mut hash_jobs = Vec::new();
        let mut quick_groups = by_quick
            .into_values()
            .filter(|items| items.len() >= 2)
            .collect::<Vec<_>>();
        for identities in &mut quick_groups {
            identities.sort_by(|left, right| {
                left.first()
                    .map(|candidate| &candidate.path)
                    .cmp(&right.first().map(|candidate| &candidate.path))
            });
        }
        quick_groups.sort_by(|left, right| {
            left.first()
                .and_then(|aliases| aliases.first())
                .map(|candidate| &candidate.path)
                .cmp(
                    &right
                        .first()
                        .and_then(|aliases| aliases.first())
                        .map(|candidate| &candidate.path),
                )
        });
        for identities in quick_groups {
            for aliases in identities {
                if summary
                    .hashed_bytes
                    .saturating_add(planned_bytes)
                    .saturating_add(size)
                    > limits.total_hash_bytes
                {
                    summary.skipped_over_limit =
                        summary.skipped_over_limit.saturating_add(aliases.len());
                    continue;
                }
                planned_bytes = planned_bytes.saturating_add(size);
                hash_jobs.push(aliases);
            }
        }
        summary.phase = DuplicateScanPhase::Hashing;
        on_progress(summary);
        let devices = hash_jobs
            .iter()
            .filter_map(|aliases| aliases.first())
            .map(|candidate| candidate.fingerprint.identity.device)
            .collect::<HashSet<_>>()
            .len();
        let workers = limits
            .hash_workers
            .min(devices.saturating_mul(2).max(1))
            .max(1);
        let hash_results = hash_in_parallel(&hash_jobs, workers, &is_cancelled, &hash_file);
        let mut prehashed = hash_jobs
            .iter()
            .filter_map(|aliases| aliases.first())
            .map(|candidate| candidate.fingerprint.identity)
            .zip(hash_results)
            .collect::<HashMap<_, _>>();
        let by_identity = hash_jobs
            .into_iter()
            .filter_map(|aliases| {
                aliases
                    .first()
                    .map(|candidate| (candidate.fingerprint.identity, aliases.clone()))
            })
            .collect::<HashMap<_, _>>();

        let mut by_digest: HashMap<[u8; 32], Vec<Vec<Candidate>>> = HashMap::new();
        for aliases in by_identity.into_values() {
            let Some(first) = aliases.first() else {
                continue;
            };
            if size > limits.file_bytes
                || summary.hashed_bytes.saturating_add(size) > limits.total_hash_bytes
            {
                summary.skipped_over_limit =
                    summary.skipped_over_limit.saturating_add(aliases.len());
                continue;
            }
            let value = match prehashed
                .remove(&first.fingerprint.identity)
                .unwrap_or_else(|| {
                    Err(DuplicateHashError::Failed(
                        "parallel hash result was unavailable".to_owned(),
                    ))
                }) {
                Ok(value) => value,
                Err(DuplicateHashError::Cancelled) => return Err(DuplicateScanError::Cancelled),
                Err(DuplicateHashError::Failed(_)) => {
                    summary.skipped_entries = summary.skipped_entries.saturating_add(aliases.len());
                    continue;
                }
            };
            if !aliases_current(&aliases) {
                summary.changed_files = summary.changed_files.saturating_add(aliases.len());
                continue;
            }
            if value.was_reused() {
                summary.reused_hashes = summary.reused_hashes.saturating_add(1);
                summary.reused_hash_bytes = summary.reused_hash_bytes.saturating_add(size);
            } else {
                summary.hashed_files = summary.hashed_files.saturating_add(1);
                summary.hashed_bytes = summary.hashed_bytes.saturating_add(size);
            }
            by_digest.entry(value.digest()).or_default().push(aliases);
            on_progress(summary);
        }
        summary.phase = DuplicateScanPhase::Confirming;
        on_progress(summary);
        for identities in by_digest.into_values().filter(|items| items.len() >= 2) {
            let mut confirmed: Vec<Vec<Vec<Candidate>>> = Vec::new();
            for aliases in identities {
                let Some(first) = aliases.first() else {
                    continue;
                };
                let mut placed = false;
                for equal_set in &mut confirmed {
                    let representative = &equal_set[0][0];
                    if compare_files(representative, first, &is_cancelled, &mut summary)? {
                        equal_set.push(aliases.clone());
                        placed = true;
                        break;
                    }
                }
                if !placed {
                    confirmed.push(vec![aliases]);
                }
            }
            for equal_set in confirmed.into_iter().filter(|set| set.len() >= 2) {
                if !equal_set.iter().all(|aliases| aliases_current(aliases)) {
                    summary.changed_files = summary
                        .changed_files
                        .saturating_add(equal_set.iter().map(Vec::len).sum::<usize>());
                    continue;
                }
                push_group(
                    &mut groups,
                    &mut result_paths,
                    limits,
                    size,
                    equal_set,
                    &mut summary,
                );
            }
        }
        if groups.len() >= limits.groups || result_paths >= limits.result_paths {
            summary.truncated = true;
            break;
        }
    }
    groups.sort_by(|left, right| {
        right
            .reclaimable_bytes()
            .cmp(&left.reclaimable_bytes())
            .then_with(|| left.items[0].path.cmp(&right.items[0].path))
    });
    if let Some(reference) = request.reference() {
        groups.retain(|group| group.items.iter().any(|item| item.path == reference));
    }
    on_progress(summary);
    Ok(DuplicateScanOutcome { groups, summary })
}

fn aliases_current(aliases: &[Candidate]) -> bool {
    aliases.iter().all(|candidate| {
        fs::symlink_metadata(&candidate.path).is_ok_and(|metadata| {
            !metadata.file_type().is_symlink() && candidate.fingerprint.matches(&metadata)
        })
    })
}

fn add_candidate(
    path: PathBuf,
    metadata: &fs::Metadata,
    limits: DuplicateScanLimits,
    summary: &mut DuplicateScanSummary,
    candidates: &mut Vec<Candidate>,
    seen_paths: &mut HashSet<PathBuf>,
) {
    if summary.examined_files >= limits.files || !seen_paths.insert(path.clone()) {
        summary.truncated |= summary.examined_files >= limits.files;
        return;
    }
    summary.examined_files = summary.examined_files.saturating_add(1);
    candidates.push(Candidate {
        path,
        fingerprint: FileFingerprint::from_metadata(metadata),
    });
}

fn quick_signature(
    candidate: &Candidate,
    is_cancelled: &(impl Fn() -> bool + Sync),
    summary: &mut DuplicateScanSummary,
) -> Result<[u64; 2], DuplicateScanError> {
    if is_cancelled() {
        return Err(DuplicateScanError::Cancelled);
    }
    let mut file = open_validated(candidate)?;
    let sample_capacity =
        QUICK_SAMPLE_BYTES.min(usize::try_from(candidate.fingerprint.size).unwrap_or(usize::MAX));
    let mut first = vec![0_u8; sample_capacity];
    file.read_exact(&mut first)
        .map_err(|error| DuplicateScanError::Compare {
            path: candidate.path.clone(),
            message: error.to_string(),
        })?;

    let mut last = Vec::new();
    if candidate.fingerprint.size > u64::try_from(QUICK_SAMPLE_BYTES).unwrap_or(u64::MAX) {
        if is_cancelled() {
            return Err(DuplicateScanError::Cancelled);
        }
        let tail_bytes = QUICK_SAMPLE_BYTES
            .min(usize::try_from(candidate.fingerprint.size).unwrap_or(usize::MAX));
        let tail_start = candidate
            .fingerprint
            .size
            .saturating_sub(u64::try_from(tail_bytes).unwrap_or(u64::MAX));
        file.seek(SeekFrom::Start(tail_start))
            .map_err(|error| DuplicateScanError::Compare {
                path: candidate.path.clone(),
                message: error.to_string(),
            })?;
        last.resize(tail_bytes, 0);
        file.read_exact(&mut last)
            .map_err(|error| DuplicateScanError::Compare {
                path: candidate.path.clone(),
                message: error.to_string(),
            })?;
    }
    revalidate(candidate)?;

    let mut left = DefaultHasher::new();
    0x464c_4f45_4455_5031_u64.hash(&mut left);
    candidate.fingerprint.size.hash(&mut left);
    first.hash(&mut left);
    last.hash(&mut left);
    let mut right = DefaultHasher::new();
    0x464c_4f45_4455_5032_u64.hash(&mut right);
    last.hash(&mut right);
    first.hash(&mut right);
    candidate.fingerprint.size.hash(&mut right);

    let read_bytes = first.len().saturating_add(last.len());
    summary.quick_checked_files = summary.quick_checked_files.saturating_add(1);
    summary.quick_read_bytes = summary
        .quick_read_bytes
        .saturating_add(u64::try_from(read_bytes).unwrap_or(u64::MAX));
    Ok([left.finish(), right.finish()])
}

fn hash_in_parallel(
    jobs: &[Vec<Candidate>],
    workers: usize,
    is_cancelled: &(impl Fn() -> bool + Sync),
    hash_file: &(impl Fn(&Path) -> Result<DuplicateHashResult, DuplicateHashError> + Sync),
) -> Vec<Result<DuplicateHashResult, DuplicateHashError>> {
    struct DeviceJobs {
        pending: VecDeque<usize>,
        active: usize,
        ready: bool,
    }

    struct Schedule {
        devices: HashMap<u64, DeviceJobs>,
        ready: VecDeque<u64>,
        pending_jobs: usize,
        cancelled: bool,
    }

    impl Schedule {
        fn new(jobs: &[Vec<Candidate>]) -> Self {
            let mut devices = HashMap::<u64, DeviceJobs>::new();
            let mut ready = VecDeque::new();
            for (index, aliases) in jobs.iter().enumerate() {
                let device = aliases
                    .first()
                    .map_or(u64::MAX, |candidate| candidate.fingerprint.identity.device);
                let queue = devices.entry(device).or_insert_with(|| {
                    ready.push_back(device);
                    DeviceJobs {
                        pending: VecDeque::new(),
                        active: 0,
                        ready: true,
                    }
                });
                queue.pending.push_back(index);
            }
            Self {
                devices,
                ready,
                pending_jobs: jobs.len(),
                cancelled: false,
            }
        }

        fn take(&mut self) -> Option<(usize, u64)> {
            while let Some(device) = self.ready.pop_front() {
                let queue = self.devices.get_mut(&device)?;
                queue.ready = false;
                let Some(index) = queue.pending.pop_front() else {
                    continue;
                };
                queue.active = queue.active.saturating_add(1);
                self.pending_jobs = self.pending_jobs.saturating_sub(1);
                if !queue.pending.is_empty() && queue.active < 2 {
                    queue.ready = true;
                    self.ready.push_back(device);
                }
                return Some((index, device));
            }
            None
        }

        fn finish(&mut self, device: u64) {
            let Some(queue) = self.devices.get_mut(&device) else {
                return;
            };
            queue.active = queue.active.saturating_sub(1);
            if !queue.pending.is_empty() && queue.active < 2 && !queue.ready {
                queue.ready = true;
                self.ready.push_back(device);
            }
        }
    }

    let results = Mutex::new(
        std::iter::repeat_with(|| None)
            .take(jobs.len())
            .collect::<Vec<Option<Result<DuplicateHashResult, DuplicateHashError>>>>(),
    );
    let schedule = (Mutex::new(Schedule::new(jobs)), Condvar::new());
    thread::scope(|scope| {
        for _ in 0..workers.min(jobs.len()) {
            scope.spawn(|| {
                loop {
                    if is_cancelled() {
                        let (state, wake) = &schedule;
                        let mut state = state
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        state.cancelled = true;
                        wake.notify_all();
                        break;
                    }
                    let (index, device) = {
                        let (state, wake) = &schedule;
                        let mut state = state
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        loop {
                            if state.cancelled || state.pending_jobs == 0 {
                                return;
                            }
                            if let Some(job) = state.take() {
                                break job;
                            }
                            state = wake
                                .wait(state)
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                        }
                    };
                    let Some(aliases) = jobs.get(index) else {
                        let (state, wake) = &schedule;
                        state
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .finish(device);
                        wake.notify_all();
                        continue;
                    };
                    let result = aliases.first().map_or_else(
                        || {
                            Err(DuplicateHashError::Failed(
                                "empty candidate identity".to_owned(),
                            ))
                        },
                        |candidate| hash_file(&candidate.path),
                    );
                    results
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)[index] = Some(result);
                    let (state, wake) = &schedule;
                    state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .finish(device);
                    wake.notify_all();
                }
            });
        }
    });
    results
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .into_iter()
        .map(|result| result.unwrap_or(Err(DuplicateHashError::Cancelled)))
        .collect()
}

fn push_group(
    groups: &mut Vec<DuplicateGroup>,
    result_paths: &mut usize,
    limits: DuplicateScanLimits,
    size: u64,
    identities: Vec<Vec<Candidate>>,
    summary: &mut DuplicateScanSummary,
) {
    if groups.len() >= limits.groups {
        summary.truncated = true;
        return;
    }
    let item_count = identities.iter().map(Vec::len).sum::<usize>();
    if result_paths.saturating_add(item_count) > limits.result_paths {
        summary.truncated = true;
        return;
    }
    let independent_copies = identities.len();
    let mut items = Vec::with_capacity(item_count);
    for aliases in identities {
        for (index, candidate) in aliases.into_iter().enumerate() {
            items.push(DuplicateItem {
                path: candidate.path,
                hard_link_alias: index > 0,
            });
        }
    }
    items.sort_by(|left, right| left.path.cmp(&right.path));
    *result_paths = result_paths.saturating_add(items.len());
    groups.push(DuplicateGroup {
        size,
        items,
        independent_copies,
    });
}

fn compare_files(
    left: &Candidate,
    right: &Candidate,
    is_cancelled: &(impl Fn() -> bool + Sync),
    summary: &mut DuplicateScanSummary,
) -> Result<bool, DuplicateScanError> {
    let mut left_file = open_validated(left)?;
    let mut right_file = open_validated(right)?;
    let mut left_buffer = vec![0u8; COMPARE_CHUNK_BYTES];
    let mut right_buffer = vec![0u8; COMPARE_CHUNK_BYTES];
    loop {
        if is_cancelled() {
            return Err(DuplicateScanError::Cancelled);
        }
        let left_read =
            left_file
                .read(&mut left_buffer)
                .map_err(|error| DuplicateScanError::Compare {
                    path: left.path.clone(),
                    message: error.to_string(),
                })?;
        let right_read =
            right_file
                .read(&mut right_buffer)
                .map_err(|error| DuplicateScanError::Compare {
                    path: right.path.clone(),
                    message: error.to_string(),
                })?;
        let compared = u64::try_from(left_read.max(right_read)).unwrap_or(u64::MAX);
        summary.compared_bytes = summary.compared_bytes.saturating_add(compared);
        if left_read != right_read || left_buffer[..left_read] != right_buffer[..right_read] {
            revalidate(left)?;
            revalidate(right)?;
            return Ok(false);
        }
        if left_read == 0 {
            break;
        }
    }
    revalidate(left)?;
    revalidate(right)?;
    Ok(true)
}

fn open_validated(candidate: &Candidate) -> Result<File, DuplicateScanError> {
    let descriptor = rustix::fs::open(
        &candidate.path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|error| DuplicateScanError::Compare {
        path: candidate.path.clone(),
        message: error.to_string(),
    })?;
    let stat = rustix::fs::fstat(&descriptor).map_err(|error| DuplicateScanError::Compare {
        path: candidate.path.clone(),
        message: error.to_string(),
    })?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
        || stat.st_dev != candidate.fingerprint.identity.device
        || stat.st_ino != candidate.fingerprint.identity.inode
        || u64::try_from(stat.st_size).unwrap_or(u64::MAX) != candidate.fingerprint.size
    {
        return Err(DuplicateScanError::Compare {
            path: candidate.path.clone(),
            message: "file changed before comparison".to_owned(),
        });
    }
    Ok(File::from(descriptor))
}

fn revalidate(candidate: &Candidate) -> Result<(), DuplicateScanError> {
    let metadata =
        fs::symlink_metadata(&candidate.path).map_err(|error| DuplicateScanError::Compare {
            path: candidate.path.clone(),
            message: error.to_string(),
        })?;
    if metadata.file_type().is_symlink() || !candidate.fingerprint.matches(&metadata) {
        return Err(DuplicateScanError::Compare {
            path: candidate.path.clone(),
            message: "file changed during duplicate scan".to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        fs,
        os::unix::{
            ffi::{OsStrExt, OsStringExt},
            fs::symlink,
        },
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use tempfile::tempdir;

    use super::*;

    fn digest(path: &Path) -> Result<DuplicateHashResult, DuplicateHashError> {
        let bytes =
            fs::read(path).map_err(|error| DuplicateHashError::Failed(error.to_string()))?;
        let mut digest = [0u8; 32];
        for (index, byte) in bytes.iter().enumerate() {
            digest[index % 32] ^= *byte;
        }
        Ok(DuplicateHashResult::computed(digest))
    }

    #[test]
    fn phase_13g3_parallel_hash_caps_each_device_at_two_active_reads() {
        let candidate = |name: &str, device: u64, inode: u64| {
            vec![Candidate {
                path: PathBuf::from(format!("/{name}")),
                fingerprint: FileFingerprint {
                    identity: FileIdentity { device, inode },
                    size: 1,
                    modified_seconds: 0,
                    modified_nanos: 0,
                    changed_seconds: 0,
                    changed_nanos: 0,
                },
            }]
        };
        let jobs = vec![
            candidate("a-0", 1, 1),
            candidate("b-0", 2, 2),
            candidate("a-1", 1, 3),
            candidate("a-2", 1, 4),
            candidate("a-3", 1, 5),
            candidate("b-1", 2, 6),
        ];
        let active_a = Arc::new(AtomicUsize::new(0));
        let maximum_a = Arc::new(AtomicUsize::new(0));
        let active_a_for_hash = Arc::clone(&active_a);
        let maximum_a_for_hash = Arc::clone(&maximum_a);

        let results = hash_in_parallel(&jobs, 4, &|| false, &move |path| {
            let is_a = path
                .file_name()
                .is_some_and(|name| name.as_bytes().starts_with(b"a-"));
            if is_a {
                let active = active_a_for_hash.fetch_add(1, Ordering::SeqCst) + 1;
                maximum_a_for_hash.fetch_max(active, Ordering::SeqCst);
            }
            thread::sleep(Duration::from_millis(10));
            if is_a {
                active_a_for_hash.fetch_sub(1, Ordering::SeqCst);
            }
            Ok(DuplicateHashResult::computed([0; 32]))
        });

        assert!(results.iter().all(Result::is_ok));
        assert_eq!(maximum_a.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn phase_13g_size_hash_and_bytes_confirm_duplicates_without_following_links() {
        let fixture = tempdir().expect("fixture");
        fs::write(fixture.path().join("a"), b"same bytes").expect("a");
        fs::write(fixture.path().join("b"), b"same bytes").expect("b");
        fs::write(fixture.path().join("different"), b"different!").expect("different");
        symlink(fixture.path().join("a"), fixture.path().join("linked")).expect("link");
        let raw = OsString::from_vec(vec![b'c', 0xff]);
        fs::write(fixture.path().join(&raw), b"same bytes").expect("raw");
        let outcome = find_duplicates(
            &DuplicateScanRequest::new(vec![fixture.path().to_path_buf()]).expect("request"),
            DuplicateScanLimits::default(),
            || false,
            digest,
            |_| {},
        )
        .expect("scan");
        assert_eq!(outcome.groups().len(), 1);
        assert_eq!(outcome.groups()[0].independent_copies(), 3);
        assert!(
            outcome.groups()[0]
                .items()
                .iter()
                .any(|item| item.path().ends_with(&raw))
        );
        assert_eq!(outcome.summary().skipped_links, 1);
        assert_eq!(outcome.reclaimable_bytes(), 20);
    }

    #[test]
    fn phase_13g_duplicate_identity_distinguishes_hard_links_and_hash_collisions() {
        let fixture = tempdir().expect("fixture");
        fs::write(fixture.path().join("original"), b"AAAA").expect("original");
        fs::hard_link(
            fixture.path().join("original"),
            fixture.path().join("alias"),
        )
        .expect("alias");
        fs::write(fixture.path().join("copy"), b"AAAA").expect("copy");
        fs::write(fixture.path().join("collision"), b"BBBB").expect("collision");
        let outcome = find_duplicates(
            &DuplicateScanRequest::new(vec![fixture.path().to_path_buf()]).expect("request"),
            DuplicateScanLimits::default(),
            || false,
            |_| Ok(DuplicateHashResult::computed([0; 32])),
            |_| {},
        )
        .expect("scan");
        assert_eq!(outcome.groups().len(), 1);
        let group = &outcome.groups()[0];
        assert_eq!(group.independent_copies(), 2);
        assert_eq!(group.items().len(), 3);
        assert_eq!(
            group
                .items()
                .iter()
                .filter(|item| item.is_hard_link_alias())
                .count(),
            1
        );
        assert_eq!(group.reclaimable_bytes(), 4);
        assert!(
            !group
                .items()
                .iter()
                .any(|item| item.path().ends_with("collision"))
        );
    }

    #[test]
    fn phase_13g_bounds_and_cancellation_are_truthful() {
        let fixture = tempdir().expect("fixture");
        fs::write(fixture.path().join("a"), vec![0u8; 32]).expect("a");
        fs::write(fixture.path().join("b"), vec![0u8; 32]).expect("b");
        let limited = find_duplicates(
            &DuplicateScanRequest::new(vec![fixture.path().to_path_buf()]).expect("request"),
            DuplicateScanLimits {
                file_bytes: 16,
                ..DuplicateScanLimits::default()
            },
            || false,
            digest,
            |_| {},
        )
        .expect("limited scan");
        assert!(limited.groups().is_empty());
        assert_eq!(limited.summary().skipped_over_limit, 2);
        assert!(matches!(
            find_duplicates(
                &DuplicateScanRequest::new(vec![fixture.path().to_path_buf()]).expect("request"),
                DuplicateScanLimits::default(),
                || true,
                digest,
                |_| {},
            ),
            Err(DuplicateScanError::Cancelled)
        ));
    }
}
