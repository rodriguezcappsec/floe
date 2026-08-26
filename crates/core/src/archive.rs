//! Bounded, no-overwrite archive operations for Floe.

use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use rustix::{
    fs::{CWD, FileType, Mode, OFlags, RenameFlags, open, renameat_with},
    io::Errno,
};
use sevenz_rust::{Password, SevenZArchiveEntry, SevenZReader, SevenZWriter};
use tar::{Archive as TarArchive, Builder as TarBuilder, EntryType, Header};
use thiserror::Error;
use xz2::{read::XzDecoder, write::XzEncoder};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

pub const ARCHIVE_SOURCE_CAPACITY: usize = 4_096;
pub const ARCHIVE_LIST_RESULT_CAPACITY: usize = 100_000;
pub const ARCHIVE_MEMBER_PATH_BYTES: usize = 4_096;
pub const ARCHIVE_MEMBER_DEPTH: usize = 256;
const ARCHIVE_COPY_BUFFER_BYTES: usize = 1024 * 1024;
const STAGING_ATTEMPTS: usize = 128;
static STAGING_NONCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ArchiveFormat {
    Zip,
    Tar,
    TarGz,
    TarXz,
    SevenZip,
}

impl ArchiveFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.as_bytes();
        if ascii_ends_with(name, b".tar.gz") || ascii_ends_with(name, b".tgz") {
            Some(Self::TarGz)
        } else if ascii_ends_with(name, b".tar.xz") || ascii_ends_with(name, b".txz") {
            Some(Self::TarXz)
        } else if ascii_ends_with(name, b".tar") {
            Some(Self::Tar)
        } else if ascii_ends_with(name, b".zip") {
            Some(Self::Zip)
        } else if ascii_ends_with(name, b".7z") {
            Some(Self::SevenZip)
        } else {
            None
        }
    }

    pub const fn extension(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::Tar => "tar",
            Self::TarGz => "tar.gz",
            Self::TarXz => "tar.xz",
            Self::SevenZip => "7z",
        }
    }

    const fn requires_utf8_names(self) -> bool {
        matches!(self, Self::Zip | Self::SevenZip)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArchiveLimits {
    pub max_archive_bytes: u64,
    pub max_entries: usize,
    pub max_member_bytes: u64,
    pub max_total_bytes: u64,
    pub max_expansion_ratio: u64,
    pub max_path_bytes: usize,
    pub max_path_depth: usize,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_archive_bytes: 32 * 1024 * 1024 * 1024,
            max_entries: ARCHIVE_LIST_RESULT_CAPACITY,
            max_member_bytes: 16 * 1024 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024 * 1024,
            max_expansion_ratio: 1_000,
            max_path_bytes: ARCHIVE_MEMBER_PATH_BYTES,
            max_path_depth: ARCHIVE_MEMBER_DEPTH,
        }
    }
}

impl ArchiveLimits {
    pub fn validate(self) -> Result<Self, ArchiveRequestError> {
        if self.max_archive_bytes == 0
            || self.max_entries == 0
            || self.max_entries > ARCHIVE_LIST_RESULT_CAPACITY
            || self.max_member_bytes == 0
            || self.max_total_bytes == 0
            || self.max_member_bytes > self.max_total_bytes
            || self.max_expansion_ratio == 0
            || self.max_path_bytes == 0
            || self.max_path_bytes > ARCHIVE_MEMBER_PATH_BYTES
            || self.max_path_depth == 0
            || self.max_path_depth > ARCHIVE_MEMBER_DEPTH
        {
            return Err(ArchiveRequestError::InvalidLimits);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveMemberKind {
    File,
    Directory,
    SymbolicLink,
    HardLink,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveMember {
    path: PathBuf,
    kind: ArchiveMemberKind,
    size: u64,
    compressed_size: Option<u64>,
    link_target: Option<PathBuf>,
}

impl ArchiveMember {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub const fn kind(&self) -> ArchiveMemberKind {
        self.kind
    }

    pub const fn size(&self) -> u64 {
        self.size
    }

    pub const fn compressed_size(&self) -> Option<u64> {
        self.compressed_size
    }

    pub fn link_target(&self) -> Option<&Path> {
        self.link_target.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArchiveRequest {
    List {
        source: PathBuf,
        format: ArchiveFormat,
        limits: ArchiveLimits,
    },
    Extract {
        source: PathBuf,
        destination: PathBuf,
        format: ArchiveFormat,
        limits: ArchiveLimits,
    },
    Compress {
        sources: Arc<[PathBuf]>,
        destination: PathBuf,
        format: ArchiveFormat,
        limits: ArchiveLimits,
    },
}

impl ArchiveRequest {
    pub fn list(source: impl Into<PathBuf>) -> Result<Self, ArchiveRequestError> {
        Self::list_with_limits(source, ArchiveLimits::default())
    }

    pub fn list_with_limits(
        source: impl Into<PathBuf>,
        limits: ArchiveLimits,
    ) -> Result<Self, ArchiveRequestError> {
        let source = source.into();
        validate_absolute_file_path(&source, "archive source")?;
        let format = ArchiveFormat::from_path(&source)
            .ok_or_else(|| ArchiveRequestError::UnsupportedFormat(source.clone()))?;
        Ok(Self::List {
            source,
            format,
            limits: limits.validate()?,
        })
    }

    pub fn extract(
        source: impl Into<PathBuf>,
        destination: impl Into<PathBuf>,
    ) -> Result<Self, ArchiveRequestError> {
        Self::extract_with_limits(source, destination, ArchiveLimits::default())
    }

    pub fn extract_with_limits(
        source: impl Into<PathBuf>,
        destination: impl Into<PathBuf>,
        limits: ArchiveLimits,
    ) -> Result<Self, ArchiveRequestError> {
        let source = source.into();
        let destination = destination.into();
        validate_absolute_file_path(&source, "archive source")?;
        validate_absolute_file_path(&destination, "extraction destination")?;
        if source == destination {
            return Err(ArchiveRequestError::SameSourceDestination(source));
        }
        let format = ArchiveFormat::from_path(&source)
            .ok_or_else(|| ArchiveRequestError::UnsupportedFormat(source.clone()))?;
        Ok(Self::Extract {
            source,
            destination,
            format,
            limits: limits.validate()?,
        })
    }

    pub fn compress(
        sources: Vec<PathBuf>,
        destination: impl Into<PathBuf>,
    ) -> Result<Self, ArchiveRequestError> {
        Self::compress_with_limits(sources, destination, ArchiveLimits::default())
    }

    pub fn compress_with_limits(
        sources: Vec<PathBuf>,
        destination: impl Into<PathBuf>,
        limits: ArchiveLimits,
    ) -> Result<Self, ArchiveRequestError> {
        if sources.is_empty() || sources.len() > ARCHIVE_SOURCE_CAPACITY {
            return Err(ArchiveRequestError::InvalidSourceCount(sources.len()));
        }
        let destination = destination.into();
        validate_absolute_file_path(&destination, "archive destination")?;
        let format = ArchiveFormat::from_path(&destination)
            .ok_or_else(|| ArchiveRequestError::UnsupportedFormat(destination.clone()))?;
        let mut seen = HashSet::with_capacity(sources.len());
        for source in &sources {
            validate_absolute_file_path(source, "archive input")?;
            if !seen.insert(source.clone()) {
                return Err(ArchiveRequestError::DuplicateSource(source.clone()));
            }
            if destination.starts_with(source) {
                return Err(ArchiveRequestError::DestinationInsideSource {
                    archive_source: source.clone(),
                    destination,
                });
            }
        }
        for (index, source) in sources.iter().enumerate() {
            for other in sources.iter().skip(index + 1) {
                if source.starts_with(other) || other.starts_with(source) {
                    return Err(ArchiveRequestError::OverlappingSources {
                        first: source.clone(),
                        second: other.clone(),
                    });
                }
            }
        }
        Ok(Self::Compress {
            sources: sources.into(),
            destination,
            format,
            limits: limits.validate()?,
        })
    }

    pub const fn format(&self) -> ArchiveFormat {
        match self {
            Self::List { format, .. }
            | Self::Extract { format, .. }
            | Self::Compress { format, .. } => *format,
        }
    }

    pub fn source(&self) -> Option<&Path> {
        match self {
            Self::List { source, .. } | Self::Extract { source, .. } => Some(source),
            Self::Compress { .. } => None,
        }
    }

    pub fn sources(&self) -> &[PathBuf] {
        match self {
            Self::Compress { sources, .. } => sources,
            Self::List { .. } | Self::Extract { .. } => &[],
        }
    }

    pub fn destination(&self) -> Option<&Path> {
        match self {
            Self::Extract { destination, .. } | Self::Compress { destination, .. } => {
                Some(destination)
            }
            Self::List { .. } => None,
        }
    }

    pub const fn limits(&self) -> ArchiveLimits {
        match self {
            Self::List { limits, .. }
            | Self::Extract { limits, .. }
            | Self::Compress { limits, .. } => *limits,
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ArchiveRequestError {
    #[error("{role} must be an absolute normalized non-root path: {}", path.display())]
    InvalidPath { role: &'static str, path: PathBuf },
    #[error("unsupported archive format: {}", .0.display())]
    UnsupportedFormat(PathBuf),
    #[error("archive source and destination are the same: {}", .0.display())]
    SameSourceDestination(PathBuf),
    #[error("select between one and {ARCHIVE_SOURCE_CAPACITY} archive inputs, received {0}")]
    InvalidSourceCount(usize),
    #[error("duplicate archive input: {}", .0.display())]
    DuplicateSource(PathBuf),
    #[error("archive inputs overlap: {} and {}", first.display(), second.display())]
    OverlappingSources { first: PathBuf, second: PathBuf },
    #[error(
        "archive destination {} is inside source {}",
        destination.display(),
        archive_source.display()
    )]
    DestinationInsideSource {
        archive_source: PathBuf,
        destination: PathBuf,
    },
    #[error("archive limits are invalid")]
    InvalidLimits,
}

#[derive(Clone, Debug, Default)]
pub struct ArchiveCancellation(Arc<AtomicBool>);

impl ArchiveCancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveProgress {
    Items { completed: u64, total: u64 },
    Bytes { completed: u64, total: u64 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArchiveOutcome {
    Listed {
        source: PathBuf,
        format: ArchiveFormat,
        members: Arc<[ArchiveMember]>,
        total_bytes: u64,
    },
    Extracted {
        source: PathBuf,
        destination: PathBuf,
        entries: u64,
        bytes: u64,
    },
    Compressed {
        sources: Arc<[PathBuf]>,
        destination: PathBuf,
        entries: u64,
        input_bytes: u64,
        archive_bytes: u64,
    },
}

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("archive operation was cancelled before publication")]
    Cancelled,
    #[error("archive source is not a regular non-symbolic file: {}", .0.display())]
    InvalidArchiveSource(PathBuf),
    #[error("archive source changed during operation: {}", .0.display())]
    SourceChanged(PathBuf),
    #[error("archive destination already exists: {}", .0.display())]
    DestinationExists(PathBuf),
    #[error("archive destination parent is unavailable: {}", .0.display())]
    InvalidDestinationParent(PathBuf),
    #[error("archive contains too many entries")]
    EntryLimit,
    #[error("archive member path is unsafe or exceeds limits")]
    UnsafeMemberPath,
    #[error("archive contains duplicate or conflicting member path: {}", .0.display())]
    MemberConflict(PathBuf),
    #[error("archive member exceeds the per-entry size limit: {}", .0.display())]
    MemberSizeLimit(PathBuf),
    #[error("archive expanded size exceeds the configured limit")]
    TotalSizeLimit,
    #[error("archive expansion ratio exceeds the configured limit")]
    ExpansionRatioLimit,
    #[error("archive entry kind is unsupported for safe extraction: {}", .0.display())]
    UnsupportedEntry(PathBuf),
    #[error("archive compression source is unsupported: {}", .0.display())]
    UnsupportedSource(PathBuf),
    #[error("{format:?} compression requires UTF-8 member names: {}", path.display())]
    NonUtf8MemberName {
        format: ArchiveFormat,
        path: PathBuf,
    },
    #[error("archive is encrypted or requires a password, which Phase 12A does not accept")]
    PasswordRequired,
    #[error("archive data is malformed or unsupported: {0}")]
    Malformed(String),
    #[error("could not {action} archive path {}: {source}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl ArchiveError {
    pub const fn is_conflict(&self) -> bool {
        matches!(self, Self::DestinationExists(_) | Self::MemberConflict(_))
    }

    pub const fn is_unsupported(&self) -> bool {
        matches!(
            self,
            Self::UnsupportedEntry(_)
                | Self::UnsupportedSource(_)
                | Self::NonUtf8MemberName { .. }
                | Self::PasswordRequired
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SourceIdentity {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

impl SourceIdentity {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
        }
    }
}

#[derive(Clone, Debug)]
struct PlannedInput {
    source: PathBuf,
    member: PathBuf,
    kind: ArchiveMemberKind,
    identity: SourceIdentity,
    mode: u32,
}

pub fn execute_archive(
    request: &ArchiveRequest,
    cancellation: &ArchiveCancellation,
    mut report_progress: impl FnMut(ArchiveProgress),
) -> Result<ArchiveOutcome, ArchiveError> {
    if cancellation.is_cancelled() {
        return Err(ArchiveError::Cancelled);
    }
    match request {
        ArchiveRequest::List {
            source,
            format,
            limits,
        } => {
            let members = list_members(source, *format, *limits, cancellation)?;
            let total_bytes = member_total_bytes(&members)?;
            report_progress(ArchiveProgress::Items {
                completed: members.len() as u64,
                total: members.len() as u64,
            });
            Ok(ArchiveOutcome::Listed {
                source: source.clone(),
                format: *format,
                members: members.into(),
                total_bytes,
            })
        }
        ArchiveRequest::Extract {
            source,
            destination,
            format,
            limits,
        } => extract_archive(
            source,
            destination,
            *format,
            *limits,
            cancellation,
            &mut report_progress,
        ),
        ArchiveRequest::Compress {
            sources,
            destination,
            format,
            limits,
        } => compress_archive(
            sources,
            destination,
            *format,
            *limits,
            cancellation,
            &mut report_progress,
        ),
    }
}

fn list_members(
    source: &Path,
    format: ArchiveFormat,
    limits: ArchiveLimits,
    cancellation: &ArchiveCancellation,
) -> Result<Vec<ArchiveMember>, ArchiveError> {
    let (file, identity) = open_archive_source(source, limits)?;
    let members = match format {
        ArchiveFormat::Zip => list_zip(file, limits, cancellation),
        ArchiveFormat::Tar => list_tar(Box::new(file), limits, cancellation),
        ArchiveFormat::TarGz => list_tar(Box::new(GzDecoder::new(file)), limits, cancellation),
        ArchiveFormat::TarXz => list_tar(Box::new(XzDecoder::new(file)), limits, cancellation),
        ArchiveFormat::SevenZip => list_7z(file, identity.size, limits, cancellation),
    }?;
    validate_member_plan(&members, identity.size, limits, false)?;
    revalidate_source(source, identity)?;
    Ok(members)
}

fn list_zip(
    file: File,
    limits: ArchiveLimits,
    cancellation: &ArchiveCancellation,
) -> Result<Vec<ArchiveMember>, ArchiveError> {
    let mut archive = ZipArchive::new(file).map_err(zip_error)?;
    if archive.len() > limits.max_entries {
        return Err(ArchiveError::EntryLimit);
    }
    let mut members = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        if cancellation.is_cancelled() {
            return Err(ArchiveError::Cancelled);
        }
        let entry = archive.by_index(index).map_err(zip_error)?;
        let path = validate_member_bytes(entry.name_raw(), limits)?;
        let kind = zip_member_kind(entry.is_dir(), entry.unix_mode());
        members.push(ArchiveMember {
            path,
            kind,
            size: entry.size(),
            compressed_size: Some(entry.compressed_size()),
            link_target: None,
        });
    }
    Ok(members)
}

fn list_tar(
    reader: Box<dyn Read>,
    limits: ArchiveLimits,
    cancellation: &ArchiveCancellation,
) -> Result<Vec<ArchiveMember>, ArchiveError> {
    let mut archive = TarArchive::new(reader);
    let entries = archive
        .entries()
        .map_err(|source| ArchiveError::Malformed(source.to_string()))?;
    let mut members = Vec::new();
    for result in entries {
        if cancellation.is_cancelled() {
            return Err(ArchiveError::Cancelled);
        }
        if members.len() == limits.max_entries {
            return Err(ArchiveError::EntryLimit);
        }
        let entry = result.map_err(|source| ArchiveError::Malformed(source.to_string()))?;
        let path = validate_member_bytes(&entry.path_bytes(), limits)?;
        let kind = tar_member_kind(entry.header().entry_type());
        let link_target = entry
            .link_name_bytes()
            .map(|target| validate_member_bytes(&target, limits))
            .transpose()?;
        members.push(ArchiveMember {
            path,
            kind,
            size: entry.size(),
            compressed_size: None,
            link_target,
        });
    }
    Ok(members)
}

fn list_7z(
    file: File,
    source_size: u64,
    limits: ArchiveLimits,
    cancellation: &ArchiveCancellation,
) -> Result<Vec<ArchiveMember>, ArchiveError> {
    let reader = SevenZReader::new(file, source_size, Password::empty()).map_err(sevenz_error)?;
    if reader.archive().files.len() > limits.max_entries {
        return Err(ArchiveError::EntryLimit);
    }
    let mut members = Vec::with_capacity(reader.archive().files.len());
    for entry in &reader.archive().files {
        if cancellation.is_cancelled() {
            return Err(ArchiveError::Cancelled);
        }
        members.push(ArchiveMember {
            path: validate_member_bytes(entry.name.as_bytes(), limits)?,
            kind: if entry.is_directory {
                ArchiveMemberKind::Directory
            } else {
                ArchiveMemberKind::File
            },
            size: entry.size,
            compressed_size: Some(entry.compressed_size),
            link_target: None,
        });
    }
    Ok(members)
}

fn validate_member_plan(
    members: &[ArchiveMember],
    archive_size: u64,
    limits: ArchiveLimits,
    extraction: bool,
) -> Result<(), ArchiveError> {
    if members.len() > limits.max_entries {
        return Err(ArchiveError::EntryLimit);
    }
    let mut seen = HashMap::with_capacity(members.len());
    let mut non_directories = HashSet::new();
    let mut total = 0u64;
    for member in members {
        if member.size > limits.max_member_bytes {
            return Err(ArchiveError::MemberSizeLimit(member.path.clone()));
        }
        total = total
            .checked_add(member.size)
            .ok_or(ArchiveError::TotalSizeLimit)?;
        if total > limits.max_total_bytes {
            return Err(ArchiveError::TotalSizeLimit);
        }
        if seen.insert(member.path.clone(), member.kind).is_some() {
            return Err(ArchiveError::MemberConflict(member.path.clone()));
        }
        if member.kind != ArchiveMemberKind::Directory {
            non_directories.insert(member.path.clone());
        }
        if extraction
            && !matches!(
                member.kind,
                ArchiveMemberKind::File | ArchiveMemberKind::Directory
            )
        {
            return Err(ArchiveError::UnsupportedEntry(member.path.clone()));
        }
    }
    for member in members {
        let mut ancestor = member.path.parent();
        while let Some(parent) = ancestor {
            if parent.as_os_str().is_empty() {
                break;
            }
            if non_directories.contains(parent) {
                return Err(ArchiveError::MemberConflict(member.path.clone()));
            }
            ancestor = parent.parent();
        }
    }
    if total > 0
        && (archive_size == 0 || total > archive_size.saturating_mul(limits.max_expansion_ratio))
    {
        return Err(ArchiveError::ExpansionRatioLimit);
    }
    Ok(())
}

fn extract_archive(
    source: &Path,
    destination: &Path,
    format: ArchiveFormat,
    limits: ArchiveLimits,
    cancellation: &ArchiveCancellation,
    report_progress: &mut dyn FnMut(ArchiveProgress),
) -> Result<ArchiveOutcome, ArchiveError> {
    ensure_destination_available(destination)?;
    let members = list_members(source, format, limits, cancellation)?;
    let (_, identity) = open_archive_source(source, limits)?;
    validate_member_plan(&members, identity.size, limits, true)?;
    let total_bytes = member_total_bytes(&members)?;
    let stage = create_stage_directory(destination)?;
    let result = (|| {
        create_member_directories(&stage, &members)?;
        let mut completed = 0u64;
        match format {
            ArchiveFormat::Zip => extract_zip(
                source,
                &stage,
                limits,
                cancellation,
                &mut completed,
                total_bytes,
                report_progress,
            )?,
            ArchiveFormat::Tar | ArchiveFormat::TarGz | ArchiveFormat::TarXz => extract_tar(
                source,
                format,
                &stage,
                limits,
                cancellation,
                &mut completed,
                total_bytes,
                report_progress,
            )?,
            ArchiveFormat::SevenZip => extract_7z(
                source,
                &stage,
                limits,
                cancellation,
                &mut completed,
                total_bytes,
                report_progress,
            )?,
        }
        if cancellation.is_cancelled() {
            return Err(ArchiveError::Cancelled);
        }
        revalidate_source(source, identity)?;
        publish_noreplace(&stage, destination)?;
        Ok(ArchiveOutcome::Extracted {
            source: source.to_path_buf(),
            destination: destination.to_path_buf(),
            entries: members.len() as u64,
            bytes: total_bytes,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}

fn extract_zip(
    source: &Path,
    stage: &Path,
    limits: ArchiveLimits,
    cancellation: &ArchiveCancellation,
    completed: &mut u64,
    total: u64,
    report_progress: &mut dyn FnMut(ArchiveProgress),
) -> Result<(), ArchiveError> {
    let (file, _) = open_archive_source(source, limits)?;
    let mut archive = ZipArchive::new(file).map_err(zip_error)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(zip_error)?;
        let member = validate_member_bytes(entry.name_raw(), limits)?;
        let entry_size = entry.size();
        match zip_member_kind(entry.is_dir(), entry.unix_mode()) {
            ArchiveMemberKind::Directory => {}
            ArchiveMemberKind::File => write_member(
                stage,
                &member,
                &mut entry,
                entry_size,
                cancellation,
                completed,
                total,
                report_progress,
            )?,
            _ => return Err(ArchiveError::UnsupportedEntry(member)),
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn extract_tar(
    source: &Path,
    format: ArchiveFormat,
    stage: &Path,
    limits: ArchiveLimits,
    cancellation: &ArchiveCancellation,
    completed: &mut u64,
    total: u64,
    report_progress: &mut dyn FnMut(ArchiveProgress),
) -> Result<(), ArchiveError> {
    let (file, _) = open_archive_source(source, limits)?;
    let reader: Box<dyn Read> = match format {
        ArchiveFormat::Tar => Box::new(file),
        ArchiveFormat::TarGz => Box::new(GzDecoder::new(file)),
        ArchiveFormat::TarXz => Box::new(XzDecoder::new(file)),
        _ => {
            return Err(ArchiveError::Malformed(
                "invalid TAR format dispatch".to_owned(),
            ));
        }
    };
    let mut archive = TarArchive::new(reader);
    let entries = archive
        .entries()
        .map_err(|source| ArchiveError::Malformed(source.to_string()))?;
    for result in entries {
        let mut entry = result.map_err(|source| ArchiveError::Malformed(source.to_string()))?;
        let member = validate_member_bytes(&entry.path_bytes(), limits)?;
        match tar_member_kind(entry.header().entry_type()) {
            ArchiveMemberKind::Directory => {}
            ArchiveMemberKind::File => {
                let size = entry.size();
                write_member(
                    stage,
                    &member,
                    &mut entry,
                    size,
                    cancellation,
                    completed,
                    total,
                    report_progress,
                )?;
            }
            _ => return Err(ArchiveError::UnsupportedEntry(member)),
        }
    }
    Ok(())
}

fn extract_7z(
    source: &Path,
    stage: &Path,
    limits: ArchiveLimits,
    cancellation: &ArchiveCancellation,
    completed: &mut u64,
    total: u64,
    report_progress: &mut dyn FnMut(ArchiveProgress),
) -> Result<(), ArchiveError> {
    let (file, identity) = open_archive_source(source, limits)?;
    let mut reader =
        SevenZReader::new(file, identity.size, Password::empty()).map_err(sevenz_error)?;
    let mut failure = None;
    reader
        .for_each_entries(|entry, input| {
            if cancellation.is_cancelled() {
                failure = Some(ArchiveError::Cancelled);
                return Ok(false);
            }
            let result = (|| {
                let member = validate_member_bytes(entry.name.as_bytes(), limits)?;
                if !entry.is_directory {
                    write_member(
                        stage,
                        &member,
                        input,
                        entry.size,
                        cancellation,
                        completed,
                        total,
                        report_progress,
                    )?;
                }
                Ok(())
            })();
            if let Err(error) = result {
                failure = Some(error);
                return Ok(false);
            }
            Ok(true)
        })
        .map_err(sevenz_error)?;
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_member(
    stage: &Path,
    member: &Path,
    input: &mut dyn Read,
    expected: u64,
    cancellation: &ArchiveCancellation,
    completed: &mut u64,
    total: u64,
    report_progress: &mut dyn FnMut(ArchiveProgress),
) -> Result<(), ArchiveError> {
    let output_path = stage.join(member);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|source| ArchiveError::Io {
            action: "create extraction parent",
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&output_path)
        .map_err(|source| ArchiveError::Io {
            action: "create extracted file",
            path: output_path.clone(),
            source,
        })?;
    let mut remaining = expected;
    let mut buffer = vec![0u8; ARCHIVE_COPY_BUFFER_BYTES];
    while remaining > 0 {
        if cancellation.is_cancelled() {
            return Err(ArchiveError::Cancelled);
        }
        let wanted = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let read = input
            .read(&mut buffer[..wanted])
            .map_err(|source| ArchiveError::Io {
                action: "read archive member",
                path: member.to_path_buf(),
                source,
            })?;
        if read == 0 {
            return Err(ArchiveError::Malformed(
                "archive member ended early".to_owned(),
            ));
        }
        output
            .write_all(&buffer[..read])
            .map_err(|source| ArchiveError::Io {
                action: "write extracted file",
                path: output_path.clone(),
                source,
            })?;
        let read = read as u64;
        remaining -= read;
        *completed = completed
            .checked_add(read)
            .ok_or(ArchiveError::TotalSizeLimit)?;
        report_progress(ArchiveProgress::Bytes {
            completed: *completed,
            total,
        });
    }
    output.sync_all().map_err(|source| ArchiveError::Io {
        action: "synchronize extracted file",
        path: output_path,
        source,
    })?;
    Ok(())
}

fn compress_archive(
    sources: &Arc<[PathBuf]>,
    destination: &Path,
    format: ArchiveFormat,
    limits: ArchiveLimits,
    cancellation: &ArchiveCancellation,
    report_progress: &mut dyn FnMut(ArchiveProgress),
) -> Result<ArchiveOutcome, ArchiveError> {
    ensure_destination_available(destination)?;
    let plan = plan_compression(sources, destination, format, limits, cancellation)?;
    let input_bytes = plan.iter().try_fold(0u64, |total, item| {
        total
            .checked_add(if item.kind == ArchiveMemberKind::File {
                item.identity.size
            } else {
                0
            })
            .ok_or(ArchiveError::TotalSizeLimit)
    })?;
    let (stage, file) = create_stage_file(destination)?;
    let result = (|| {
        let mut completed = 0u64;
        match format {
            ArchiveFormat::Zip => write_zip(
                file,
                &plan,
                cancellation,
                &mut completed,
                input_bytes,
                report_progress,
            )?,
            ArchiveFormat::Tar => {
                let file = write_tar(
                    file,
                    &plan,
                    cancellation,
                    &mut completed,
                    input_bytes,
                    report_progress,
                )?;
                file.sync_all().map_err(|source| ArchiveError::Io {
                    action: "synchronize TAR",
                    path: stage.clone(),
                    source,
                })?;
            }
            ArchiveFormat::TarGz => {
                let encoder = GzEncoder::new(file, Compression::default());
                let encoder = write_tar(
                    encoder,
                    &plan,
                    cancellation,
                    &mut completed,
                    input_bytes,
                    report_progress,
                )?;
                let file = encoder.finish().map_err(|source| ArchiveError::Io {
                    action: "finish gzip archive",
                    path: stage.clone(),
                    source,
                })?;
                file.sync_all().map_err(|source| ArchiveError::Io {
                    action: "synchronize gzip archive",
                    path: stage.clone(),
                    source,
                })?;
            }
            ArchiveFormat::TarXz => {
                let encoder = XzEncoder::new(file, 6);
                let encoder = write_tar(
                    encoder,
                    &plan,
                    cancellation,
                    &mut completed,
                    input_bytes,
                    report_progress,
                )?;
                let file = encoder.finish().map_err(|source| ArchiveError::Io {
                    action: "finish xz archive",
                    path: stage.clone(),
                    source,
                })?;
                file.sync_all().map_err(|source| ArchiveError::Io {
                    action: "synchronize xz archive",
                    path: stage.clone(),
                    source,
                })?;
            }
            ArchiveFormat::SevenZip => write_7z(
                file,
                &plan,
                cancellation,
                &mut completed,
                input_bytes,
                report_progress,
                &stage,
            )?,
        }
        if cancellation.is_cancelled() {
            return Err(ArchiveError::Cancelled);
        }
        for item in &plan {
            revalidate_source(&item.source, item.identity)?;
        }
        publish_noreplace(&stage, destination)?;
        let archive_bytes = fs::symlink_metadata(destination)
            .map_err(|source| ArchiveError::Io {
                action: "inspect published archive",
                path: destination.to_path_buf(),
                source,
            })?
            .len();
        Ok(ArchiveOutcome::Compressed {
            sources: Arc::clone(sources),
            destination: destination.to_path_buf(),
            entries: plan.len() as u64,
            input_bytes,
            archive_bytes,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&stage);
    }
    result
}

fn plan_compression(
    sources: &[PathBuf],
    destination: &Path,
    format: ArchiveFormat,
    limits: ArchiveLimits,
    cancellation: &ArchiveCancellation,
) -> Result<Vec<PlannedInput>, ArchiveError> {
    let mut plan = Vec::new();
    let mut total = 0u64;
    for source in sources {
        let member = PathBuf::from(
            source
                .file_name()
                .ok_or_else(|| ArchiveError::UnsupportedSource(source.clone()))?,
        );
        collect_source_plan(
            source,
            &member,
            destination,
            format,
            limits,
            cancellation,
            &mut total,
            &mut plan,
        )?;
    }
    plan.sort_by(|left, right| {
        left.member
            .as_os_str()
            .as_bytes()
            .cmp(right.member.as_os_str().as_bytes())
    });
    let members: Vec<_> = plan
        .iter()
        .map(|item| ArchiveMember {
            path: item.member.clone(),
            kind: item.kind,
            size: if item.kind == ArchiveMemberKind::File {
                item.identity.size
            } else {
                0
            },
            compressed_size: None,
            link_target: None,
        })
        .collect();
    validate_member_plan(&members, total.max(1), limits, true)?;
    Ok(plan)
}

#[allow(clippy::too_many_arguments)]
fn collect_source_plan(
    source: &Path,
    member: &Path,
    destination: &Path,
    format: ArchiveFormat,
    limits: ArchiveLimits,
    cancellation: &ArchiveCancellation,
    total: &mut u64,
    plan: &mut Vec<PlannedInput>,
) -> Result<(), ArchiveError> {
    if cancellation.is_cancelled() {
        return Err(ArchiveError::Cancelled);
    }
    if plan.len() == limits.max_entries {
        return Err(ArchiveError::EntryLimit);
    }
    validate_member_bytes(member.as_os_str().as_bytes(), limits)?;
    if format.requires_utf8_names() && member.to_str().is_none() {
        return Err(ArchiveError::NonUtf8MemberName {
            format,
            path: source.to_path_buf(),
        });
    }
    if source == destination {
        return Err(ArchiveError::UnsupportedSource(source.to_path_buf()));
    }
    let metadata = fs::symlink_metadata(source).map_err(|source_error| ArchiveError::Io {
        action: "inspect compression source",
        path: source.to_path_buf(),
        source: source_error,
    })?;
    if metadata.file_type().is_symlink() {
        return Err(ArchiveError::UnsupportedSource(source.to_path_buf()));
    }
    let kind = if metadata.is_dir() {
        ArchiveMemberKind::Directory
    } else if metadata.is_file() {
        ArchiveMemberKind::File
    } else {
        return Err(ArchiveError::UnsupportedSource(source.to_path_buf()));
    };
    let content_size = if kind == ArchiveMemberKind::File {
        metadata.len()
    } else {
        0
    };
    if content_size > limits.max_member_bytes {
        return Err(ArchiveError::MemberSizeLimit(member.to_path_buf()));
    }
    *total = total
        .checked_add(content_size)
        .ok_or(ArchiveError::TotalSizeLimit)?;
    if *total > limits.max_total_bytes {
        return Err(ArchiveError::TotalSizeLimit);
    }
    plan.push(PlannedInput {
        source: source.to_path_buf(),
        member: member.to_path_buf(),
        kind,
        identity: SourceIdentity::from_metadata(&metadata),
        mode: metadata.mode() & 0o777,
    });
    if kind == ArchiveMemberKind::Directory {
        let mut children = fs::read_dir(source)
            .map_err(|source_error| ArchiveError::Io {
                action: "enumerate compression source",
                path: source.to_path_buf(),
                source: source_error,
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source_error| ArchiveError::Io {
                action: "enumerate compression source",
                path: source.to_path_buf(),
                source: source_error,
            })?;
        children.sort_by(|left, right| {
            left.file_name()
                .as_bytes()
                .cmp(right.file_name().as_bytes())
        });
        for child in children {
            collect_source_plan(
                &child.path(),
                &member.join(child.file_name()),
                destination,
                format,
                limits,
                cancellation,
                total,
                plan,
            )?;
        }
    }
    Ok(())
}

fn write_zip(
    file: File,
    plan: &[PlannedInput],
    cancellation: &ArchiveCancellation,
    completed: &mut u64,
    total: u64,
    report_progress: &mut dyn FnMut(ArchiveProgress),
) -> Result<(), ArchiveError> {
    let mut writer = ZipWriter::new(file);
    for item in plan {
        if cancellation.is_cancelled() {
            return Err(ArchiveError::Cancelled);
        }
        let name = item
            .member
            .to_str()
            .ok_or_else(|| ArchiveError::NonUtf8MemberName {
                format: ArchiveFormat::Zip,
                path: item.source.clone(),
            })?;
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(item.mode);
        match item.kind {
            ArchiveMemberKind::Directory => writer
                .add_directory(format!("{}/", name.trim_end_matches('/')), options)
                .map_err(zip_error)?,
            ArchiveMemberKind::File => {
                writer.start_file(name, options).map_err(zip_error)?;
                copy_planned_input(
                    item,
                    &mut writer,
                    cancellation,
                    completed,
                    total,
                    report_progress,
                )?;
            }
            _ => return Err(ArchiveError::UnsupportedSource(item.source.clone())),
        }
    }
    let file = writer.finish().map_err(zip_error)?;
    file.sync_all().map_err(|source| ArchiveError::Io {
        action: "synchronize ZIP archive",
        path: PathBuf::from("<staging>"),
        source,
    })?;
    Ok(())
}

fn write_tar<W: Write>(
    writer: W,
    plan: &[PlannedInput],
    cancellation: &ArchiveCancellation,
    completed: &mut u64,
    total: u64,
    report_progress: &mut dyn FnMut(ArchiveProgress),
) -> Result<W, ArchiveError> {
    let mut builder = TarBuilder::new(writer);
    for item in plan {
        if cancellation.is_cancelled() {
            return Err(ArchiveError::Cancelled);
        }
        let mut header = Header::new_gnu();
        header.set_mode(item.mode);
        match item.kind {
            ArchiveMemberKind::Directory => {
                header.set_entry_type(EntryType::Directory);
                header.set_size(0);
                header.set_cksum();
                builder
                    .append_data(&mut header, &item.member, io::empty())
                    .map_err(|source| ArchiveError::Io {
                        action: "write TAR directory",
                        path: item.member.clone(),
                        source,
                    })?;
            }
            ArchiveMemberKind::File => {
                header.set_entry_type(EntryType::Regular);
                header.set_size(item.identity.size);
                header.set_cksum();
                let (file, identity) = open_planned_source(item)?;
                let mut reader = ProgressReader {
                    inner: file,
                    cancellation,
                    completed,
                    total,
                    report_progress,
                };
                builder
                    .append_data(&mut header, &item.member, &mut reader)
                    .map_err(|source| ArchiveError::Io {
                        action: "write TAR member",
                        path: item.member.clone(),
                        source,
                    })?;
                revalidate_source(&item.source, identity)?;
            }
            _ => return Err(ArchiveError::UnsupportedSource(item.source.clone())),
        }
    }
    builder.finish().map_err(|source| ArchiveError::Io {
        action: "finish TAR archive",
        path: PathBuf::from("<staging>"),
        source,
    })?;
    builder.into_inner().map_err(|source| ArchiveError::Io {
        action: "finalize TAR archive",
        path: PathBuf::from("<staging>"),
        source,
    })
}

#[allow(clippy::too_many_arguments)]
fn write_7z(
    file: File,
    plan: &[PlannedInput],
    cancellation: &ArchiveCancellation,
    completed: &mut u64,
    total: u64,
    report_progress: &mut dyn FnMut(ArchiveProgress),
    stage: &Path,
) -> Result<(), ArchiveError> {
    let mut writer = SevenZWriter::new(file).map_err(sevenz_error)?;
    writer.set_encrypt_header(false);
    for item in plan {
        if cancellation.is_cancelled() {
            return Err(ArchiveError::Cancelled);
        }
        let name = item
            .member
            .to_str()
            .ok_or_else(|| ArchiveError::NonUtf8MemberName {
                format: ArchiveFormat::SevenZip,
                path: item.source.clone(),
            })?;
        let mut entry = SevenZArchiveEntry::new();
        entry.name = name.to_owned();
        entry.is_directory = item.kind == ArchiveMemberKind::Directory;
        if entry.is_directory {
            writer
                .push_archive_entry::<io::Empty>(entry, None)
                .map_err(sevenz_error)?;
        } else {
            let (file, identity) = open_planned_source(item)?;
            let reader = ProgressReader {
                inner: file,
                cancellation,
                completed,
                total,
                report_progress,
            };
            writer
                .push_archive_entry(entry, Some(reader))
                .map_err(|error| {
                    if cancellation.is_cancelled() {
                        ArchiveError::Cancelled
                    } else {
                        sevenz_error(error)
                    }
                })?;
            revalidate_source(&item.source, identity)?;
        }
    }
    let mut file = writer.finish().map_err(|source| ArchiveError::Io {
        action: "finish 7z archive",
        path: stage.to_path_buf(),
        source,
    })?;
    file.flush().map_err(|source| ArchiveError::Io {
        action: "flush 7z archive",
        path: stage.to_path_buf(),
        source,
    })?;
    file.sync_all().map_err(|source| ArchiveError::Io {
        action: "synchronize 7z archive",
        path: stage.to_path_buf(),
        source,
    })?;
    Ok(())
}

fn copy_planned_input(
    item: &PlannedInput,
    output: &mut dyn Write,
    cancellation: &ArchiveCancellation,
    completed: &mut u64,
    total: u64,
    report_progress: &mut dyn FnMut(ArchiveProgress),
) -> Result<(), ArchiveError> {
    let (mut file, identity) = open_planned_source(item)?;
    let mut remaining = identity.size;
    let mut buffer = vec![0u8; ARCHIVE_COPY_BUFFER_BYTES];
    while remaining > 0 {
        if cancellation.is_cancelled() {
            return Err(ArchiveError::Cancelled);
        }
        let wanted = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let read = file
            .read(&mut buffer[..wanted])
            .map_err(|source| ArchiveError::Io {
                action: "read compression source",
                path: item.source.clone(),
                source,
            })?;
        if read == 0 {
            return Err(ArchiveError::SourceChanged(item.source.clone()));
        }
        output
            .write_all(&buffer[..read])
            .map_err(|source| ArchiveError::Io {
                action: "write archive member",
                path: item.member.clone(),
                source,
            })?;
        let read = read as u64;
        remaining -= read;
        *completed = completed
            .checked_add(read)
            .ok_or(ArchiveError::TotalSizeLimit)?;
        report_progress(ArchiveProgress::Bytes {
            completed: *completed,
            total,
        });
    }
    revalidate_source(&item.source, identity)
}

struct ProgressReader<'a, R> {
    inner: R,
    cancellation: &'a ArchiveCancellation,
    completed: &'a mut u64,
    total: u64,
    report_progress: &'a mut dyn FnMut(ArchiveProgress),
}

impl<R: Read> Read for ProgressReader<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.cancellation.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "archive cancelled",
            ));
        }
        let read = self.inner.read(buffer)?;
        *self.completed = self
            .completed
            .checked_add(read as u64)
            .ok_or_else(|| io::Error::other("archive progress overflow"))?;
        (self.report_progress)(ArchiveProgress::Bytes {
            completed: *self.completed,
            total: self.total,
        });
        Ok(read)
    }
}

fn open_archive_source(
    path: &Path,
    limits: ArchiveLimits,
) -> Result<(File, SourceIdentity), ArchiveError> {
    let descriptor = open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|error| ArchiveError::Io {
        action: "open archive source",
        path: path.to_path_buf(),
        source: io::Error::from_raw_os_error(error.raw_os_error()),
    })?;
    let stat = rustix::fs::fstat(&descriptor).map_err(|error| ArchiveError::Io {
        action: "inspect archive source",
        path: path.to_path_buf(),
        source: io::Error::from_raw_os_error(error.raw_os_error()),
    })?;
    if !FileType::from_raw_mode(stat.st_mode).is_file() {
        return Err(ArchiveError::InvalidArchiveSource(path.to_path_buf()));
    }
    let file = File::from(descriptor);
    let metadata = file.metadata().map_err(|source| ArchiveError::Io {
        action: "inspect archive source",
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.len() > limits.max_archive_bytes {
        return Err(ArchiveError::TotalSizeLimit);
    }
    let identity = SourceIdentity::from_metadata(&metadata);
    Ok((file, identity))
}

fn open_planned_source(item: &PlannedInput) -> Result<(File, SourceIdentity), ArchiveError> {
    let descriptor = open(
        &item.source,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|error| ArchiveError::Io {
        action: "open compression source",
        path: item.source.clone(),
        source: io::Error::from_raw_os_error(error.raw_os_error()),
    })?;
    let stat = rustix::fs::fstat(&descriptor).map_err(|error| ArchiveError::Io {
        action: "inspect compression source",
        path: item.source.clone(),
        source: io::Error::from_raw_os_error(error.raw_os_error()),
    })?;
    if !FileType::from_raw_mode(stat.st_mode).is_file()
        || stat.st_dev != item.identity.device
        || stat.st_ino != item.identity.inode
        || u64::try_from(stat.st_size).unwrap_or(u64::MAX) != item.identity.size
    {
        return Err(ArchiveError::SourceChanged(item.source.clone()));
    }
    Ok((File::from(descriptor), item.identity))
}

fn revalidate_source(path: &Path, identity: SourceIdentity) -> Result<(), ArchiveError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| ArchiveError::SourceChanged(path.to_path_buf()))?;
    if metadata.file_type().is_symlink() || SourceIdentity::from_metadata(&metadata) != identity {
        return Err(ArchiveError::SourceChanged(path.to_path_buf()));
    }
    Ok(())
}

fn ensure_destination_available(destination: &Path) -> Result<(), ArchiveError> {
    if fs::symlink_metadata(destination).is_ok() {
        return Err(ArchiveError::DestinationExists(destination.to_path_buf()));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| ArchiveError::InvalidDestinationParent(destination.to_path_buf()))?;
    let metadata = fs::symlink_metadata(parent)
        .map_err(|_| ArchiveError::InvalidDestinationParent(parent.to_path_buf()))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(ArchiveError::InvalidDestinationParent(parent.to_path_buf()));
    }
    Ok(())
}

fn create_stage_directory(destination: &Path) -> Result<PathBuf, ArchiveError> {
    let parent = destination
        .parent()
        .ok_or_else(|| ArchiveError::InvalidDestinationParent(destination.to_path_buf()))?;
    for _ in 0..STAGING_ATTEMPTS {
        let path = parent.join(staging_name("extract"));
        match fs::create_dir(&path) {
            Ok(()) => {
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).map_err(
                    |source| ArchiveError::Io {
                        action: "protect extraction staging directory",
                        path: path.clone(),
                        source,
                    },
                )?;
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(ArchiveError::Io {
                    action: "create extraction staging directory",
                    path,
                    source,
                });
            }
        }
    }
    Err(ArchiveError::InvalidDestinationParent(parent.to_path_buf()))
}

fn create_stage_file(destination: &Path) -> Result<(PathBuf, File), ArchiveError> {
    let parent = destination
        .parent()
        .ok_or_else(|| ArchiveError::InvalidDestinationParent(destination.to_path_buf()))?;
    for _ in 0..STAGING_ATTEMPTS {
        let path = parent.join(staging_name("compress"));
        match OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(ArchiveError::Io {
                    action: "create archive staging file",
                    path,
                    source,
                });
            }
        }
    }
    Err(ArchiveError::InvalidDestinationParent(parent.to_path_buf()))
}

fn staging_name(kind: &str) -> OsString {
    let nonce = STAGING_NONCE.fetch_add(1, Ordering::Relaxed);
    OsString::from(format!(
        ".floe-archive-{kind}-{}-{nonce}",
        std::process::id()
    ))
}

fn publish_noreplace(stage: &Path, destination: &Path) -> Result<(), ArchiveError> {
    renameat_with(CWD, stage, CWD, destination, RenameFlags::NOREPLACE).map_err(|error| {
        if error == Errno::EXIST {
            ArchiveError::DestinationExists(destination.to_path_buf())
        } else {
            ArchiveError::Io {
                action: "publish archive result",
                path: destination.to_path_buf(),
                source: io::Error::from_raw_os_error(error.raw_os_error()),
            }
        }
    })
}

fn create_member_directories(stage: &Path, members: &[ArchiveMember]) -> Result<(), ArchiveError> {
    let mut directories = Vec::new();
    for member in members {
        if member.kind == ArchiveMemberKind::Directory {
            directories.push(member.path.clone());
        }
        if let Some(parent) = member.path.parent()
            && !parent.as_os_str().is_empty()
        {
            directories.push(parent.to_path_buf());
        }
    }
    directories.sort_by_key(|path| path.components().count());
    directories.dedup();
    for directory in directories {
        let path = stage.join(directory);
        fs::create_dir_all(&path).map_err(|source| ArchiveError::Io {
            action: "create extraction directory",
            path,
            source,
        })?;
    }
    Ok(())
}

fn validate_member_bytes(raw: &[u8], limits: ArchiveLimits) -> Result<PathBuf, ArchiveError> {
    let raw = raw.strip_suffix(b"/").unwrap_or(raw);
    if raw.is_empty()
        || raw.len() > limits.max_path_bytes
        || raw.contains(&0)
        || raw.contains(&b'\\')
        || raw.starts_with(b"/")
    {
        return Err(ArchiveError::UnsafeMemberPath);
    }
    let mut depth = 0usize;
    for (index, component) in raw.split(|byte| *byte == b'/').enumerate() {
        depth += 1;
        if component.is_empty()
            || component == b"."
            || component == b".."
            || (index == 0
                && component.len() >= 2
                && component[0].is_ascii_alphabetic()
                && component[1] == b':')
        {
            return Err(ArchiveError::UnsafeMemberPath);
        }
    }
    if depth > limits.max_path_depth {
        return Err(ArchiveError::UnsafeMemberPath);
    }
    Ok(PathBuf::from(OsString::from_vec(raw.to_vec())))
}

fn validate_absolute_file_path(path: &Path, role: &'static str) -> Result<(), ArchiveRequestError> {
    if !path.is_absolute()
        || path.file_name().is_none()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(ArchiveRequestError::InvalidPath {
            role,
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

fn zip_member_kind(directory: bool, mode: Option<u32>) -> ArchiveMemberKind {
    if directory {
        ArchiveMemberKind::Directory
    } else if mode.is_some_and(|mode| mode & 0o170000 == 0o120000) {
        ArchiveMemberKind::SymbolicLink
    } else if mode.is_some_and(|mode| {
        let kind = mode & 0o170000;
        kind != 0 && kind != 0o100000
    }) {
        ArchiveMemberKind::Other
    } else {
        ArchiveMemberKind::File
    }
}

fn tar_member_kind(kind: EntryType) -> ArchiveMemberKind {
    if kind.is_dir() {
        ArchiveMemberKind::Directory
    } else if kind.is_file() || kind.is_contiguous() {
        ArchiveMemberKind::File
    } else if kind.is_symlink() {
        ArchiveMemberKind::SymbolicLink
    } else if kind.is_hard_link() {
        ArchiveMemberKind::HardLink
    } else {
        ArchiveMemberKind::Other
    }
}

fn member_total_bytes(members: &[ArchiveMember]) -> Result<u64, ArchiveError> {
    members.iter().try_fold(0u64, |total, member| {
        total
            .checked_add(member.size)
            .ok_or(ArchiveError::TotalSizeLimit)
    })
}

fn ascii_ends_with(value: &[u8], suffix: &[u8]) -> bool {
    value.len() >= suffix.len()
        && value[value.len() - suffix.len()..]
            .iter()
            .zip(suffix)
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
}

fn zip_error(error: zip::result::ZipError) -> ArchiveError {
    let message = error.to_string();
    if message.to_ascii_lowercase().contains("password")
        || message.to_ascii_lowercase().contains("encrypted")
    {
        ArchiveError::PasswordRequired
    } else {
        ArchiveError::Malformed(message)
    }
}

fn sevenz_error(error: sevenz_rust::Error) -> ArchiveError {
    let message = error.to_string();
    if message.to_ascii_lowercase().contains("password")
        || message.to_ascii_lowercase().contains("encrypted")
        || message.to_ascii_lowercase().contains("aes")
    {
        ArchiveError::PasswordRequired
    } else {
        ArchiveError::Malformed(message)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::Cursor,
        os::unix::{ffi::OsStringExt, fs::symlink},
    };

    use tempfile::tempdir;

    use super::*;

    fn write_zip_fixture(path: &Path, name: &str, data: &[u8]) {
        let file = File::create(path).expect("zip fixture");
        let mut writer = ZipWriter::new(file);
        writer
            .start_file(name, SimpleFileOptions::default())
            .expect("zip entry");
        writer.write_all(data).expect("zip data");
        writer.finish().expect("zip finish");
    }

    fn assert_round_trip(extension: &str) {
        let root = tempdir().expect("root");
        let input = root.path().join("input");
        fs::create_dir(&input).expect("input");
        fs::write(input.join("hello.txt"), b"hello archive").expect("content");
        let archive = root.path().join(format!("bundle.{extension}"));
        let compress =
            ArchiveRequest::compress(vec![input.clone()], archive.clone()).expect("request");
        let outcome =
            execute_archive(&compress, &ArchiveCancellation::new(), |_| {}).expect("compress");
        assert!(matches!(outcome, ArchiveOutcome::Compressed { .. }));
        let list = ArchiveRequest::list(archive.clone()).expect("list request");
        let listed = execute_archive(&list, &ArchiveCancellation::new(), |_| {}).expect("list");
        let ArchiveOutcome::Listed { members, .. } = listed else {
            panic!("listed outcome");
        };
        assert!(
            members
                .iter()
                .any(|entry| entry.path() == Path::new("input/hello.txt"))
        );
        let destination = root.path().join("restored");
        let extract =
            ArchiveRequest::extract(archive, destination.clone()).expect("extract request");
        execute_archive(&extract, &ArchiveCancellation::new(), |_| {}).expect("extract");
        assert_eq!(
            fs::read(destination.join("input/hello.txt")).expect("restored"),
            b"hello archive"
        );
    }

    #[test]
    fn phase_12a_archive_contract_validates_formats_requests_limits_and_raw_paths() {
        assert_eq!(
            ArchiveFormat::from_path(Path::new("A.TAR.GZ")),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            ArchiveFormat::from_path(Path::new("a.txz")),
            Some(ArchiveFormat::TarXz)
        );
        assert!(ArchiveRequest::list("relative.zip").is_err());
        assert!(ArchiveRequest::compress(Vec::new(), "/tmp/a.zip").is_err());
        assert!(ArchiveRequest::compress(vec![PathBuf::from("/tmp")], "/tmp/a.zip").is_err());
        assert!(validate_member_bytes(b"../escape", ArchiveLimits::default()).is_err());
        assert!(validate_member_bytes(b"/absolute", ArchiveLimits::default()).is_err());
        assert!(validate_member_bytes(b"C:/drive", ArchiveLimits::default()).is_err());
        assert!(validate_member_bytes(b"folder\\escape", ArchiveLimits::default()).is_err());
        let raw = validate_member_bytes(b"raw-\xff", ArchiveLimits::default()).expect("raw path");
        assert_eq!(raw.as_os_str().as_bytes(), b"raw-\xff");

        let members = vec![
            ArchiveMember {
                path: PathBuf::from("file"),
                kind: ArchiveMemberKind::File,
                size: 1,
                compressed_size: None,
                link_target: None,
            },
            ArchiveMember {
                path: PathBuf::from("file/child"),
                kind: ArchiveMemberKind::File,
                size: 1,
                compressed_size: None,
                link_target: None,
            },
        ];
        assert!(matches!(
            validate_member_plan(&members, 1, ArchiveLimits::default(), false),
            Err(ArchiveError::MemberConflict(_))
        ));
    }

    #[test]
    fn phase_12a_archive_zip_tar_round_trip_conflict_cancel_links_and_bombs() {
        for extension in ["zip", "tar", "tar.gz", "tar.xz"] {
            assert_round_trip(extension);
        }

        let root = tempdir().expect("root");
        let unsafe_zip = root.path().join("unsafe.zip");
        write_zip_fixture(&unsafe_zip, "../escape", b"bad");
        let request = ArchiveRequest::list(unsafe_zip).expect("request");
        assert!(matches!(
            execute_archive(&request, &ArchiveCancellation::new(), |_| {}),
            Err(ArchiveError::UnsafeMemberPath)
        ));

        let link = root.path().join("link");
        symlink("missing", &link).expect("link");
        let destination = root.path().join("link.tar");
        let request = ArchiveRequest::compress(vec![link], destination).expect("request");
        assert!(matches!(
            execute_archive(&request, &ArchiveCancellation::new(), |_| {}),
            Err(ArchiveError::UnsupportedSource(_))
        ));

        let source = root.path().join("cancel.txt");
        fs::write(&source, b"cancel").expect("source");
        let destination = root.path().join("cancel.zip");
        let request = ArchiveRequest::compress(vec![source], destination.clone()).expect("request");
        let cancellation = ArchiveCancellation::new();
        cancellation.cancel();
        assert!(matches!(
            execute_archive(&request, &cancellation, |_| {}),
            Err(ArchiveError::Cancelled)
        ));
        assert!(!destination.exists());

        let bomb = vec![ArchiveMember {
            path: PathBuf::from("bomb"),
            kind: ArchiveMemberKind::File,
            size: 10_001,
            compressed_size: Some(1),
            link_target: None,
        }];
        let limits = ArchiveLimits {
            max_total_bytes: 20_000,
            max_member_bytes: 20_000,
            max_expansion_ratio: 10,
            ..ArchiveLimits::default()
        };
        assert!(matches!(
            validate_member_plan(&bomb, 1, limits, false),
            Err(ArchiveError::ExpansionRatioLimit)
        ));
    }

    #[test]
    fn phase_12a_archive_7z_round_trip_and_non_utf8_policy() {
        assert_round_trip("7z");
        let root = tempdir().expect("root");
        let raw = root.path().join(OsString::from_vec(vec![b'r', 0xff]));
        fs::write(&raw, b"raw").expect("raw source");
        let request =
            ArchiveRequest::compress(vec![raw], root.path().join("raw.7z")).expect("request");
        assert!(matches!(
            execute_archive(&request, &ArchiveCancellation::new(), |_| {}),
            Err(ArchiveError::NonUtf8MemberName {
                format: ArchiveFormat::SevenZip,
                ..
            })
        ));

        let mut writer = SevenZWriter::new(Cursor::new(Vec::new())).expect("writer");
        writer.set_encrypt_header(false);
        let mut entry = SevenZArchiveEntry::new();
        entry.name = "../escape".to_owned();
        writer
            .push_archive_entry(entry, Some(Cursor::new(b"bad")))
            .expect("entry");
        let cursor = writer.finish().expect("finish");
        let unsafe_path = root.path().join("unsafe.7z");
        fs::write(&unsafe_path, cursor.into_inner()).expect("fixture");
        let request = ArchiveRequest::list(unsafe_path).expect("request");
        assert!(matches!(
            execute_archive(&request, &ArchiveCancellation::new(), |_| {}),
            Err(ArchiveError::UnsafeMemberPath)
        ));
    }
}
