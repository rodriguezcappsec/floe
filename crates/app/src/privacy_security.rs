//! Bounded, local-only privacy inspection and verified sanitized-copy creation.
//!
//! This module deliberately supports a small reviewed set of image containers.
//! An empty finding list is not an exhaustive privacy guarantee.

use std::{
    collections::VecDeque,
    fs::{self, OpenOptions},
    io::{self, Cursor, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

use exif::{Reader as ExifReader, Tag as ExifTag};
use floe_core::{SuspiciousAnalysis, analyze_suspicious_file};
use rustix::fs::{CWD, OFlags, RenameFlags, renameat_with};
use thiserror::Error;

const REQUEST_CAPACITY: usize = 4;
const RESULT_CAPACITY: usize = 8;
const SELECTION_CAPACITY: usize = 128;
const SOURCE_CAPACITY: u64 = 64 * 1024 * 1024;
const FINDING_CAPACITY: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewedFormat {
    Jpeg,
    Png,
    WebP,
    Tiff,
}

impl ReviewedFormat {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Jpeg => "JPEG",
            Self::Png => "PNG",
            Self::WebP => "WebP",
            Self::Tiff => "TIFF",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivacyFinding {
    pub category: &'static str,
    pub explanation: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivacyInspectionState {
    Reviewed {
        format: ReviewedFormat,
        findings: Vec<PrivacyFinding>,
    },
    Unsupported,
    TooLarge,
    NotRegular,
    Changed,
    Inaccessible(String),
    Malformed(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectionEntry {
    pub path: PathBuf,
    pub suspicious: SuspiciousAnalysis,
    pub privacy: PrivacyInspectionState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectionOutcome {
    pub generation: u64,
    pub entries: Vec<InspectionEntry>,
    pub cancelled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SanitizedCopyOutcome {
    pub generation: u64,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub format: ReviewedFormat,
    pub removed_categories: Vec<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SanitizedItemOutcome {
    pub source: PathBuf,
    pub result: Result<SanitizedCopyOutcome, PrivacySecurityError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SanitizationOutcome {
    pub generation: u64,
    pub items: Vec<SanitizedItemOutcome>,
    pub cancelled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivacySecurityResult {
    Inspection(InspectionOutcome),
    Sanitized(SanitizationOutcome),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivacySecurityRequest {
    Inspect {
        generation: u64,
        paths: Vec<PathBuf>,
    },
    Sanitize {
        generation: u64,
        sources: Vec<PathBuf>,
    },
}

impl PrivacySecurityRequest {
    pub fn inspect(generation: u64, paths: Vec<PathBuf>) -> Result<Self, PrivacySecurityError> {
        if generation == 0 || paths.is_empty() || paths.len() > SELECTION_CAPACITY {
            return Err(PrivacySecurityError::InvalidRequest);
        }
        if paths.iter().any(|path| !path.is_absolute()) {
            return Err(PrivacySecurityError::InvalidRequest);
        }
        Ok(Self::Inspect { generation, paths })
    }

    pub fn sanitize(generation: u64, sources: Vec<PathBuf>) -> Result<Self, PrivacySecurityError> {
        if generation == 0
            || sources.is_empty()
            || sources.len() > SELECTION_CAPACITY
            || sources.iter().any(|source| !source.is_absolute())
        {
            return Err(PrivacySecurityError::InvalidRequest);
        }
        Ok(Self::Sanitize {
            generation,
            sources,
        })
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PrivacySecurityError {
    #[error("privacy and safety request is invalid")]
    InvalidRequest,
    #[error("privacy and safety worker queue is busy")]
    QueueFull,
    #[error("privacy and safety worker stopped")]
    Stopped,
    #[error("source is not a regular file or is a symbolic link")]
    NotRegular,
    #[error("source format is not supported for verified sanitization")]
    Unsupported,
    #[error("source exceeds the 64 MiB inspection limit")]
    TooLarge,
    #[error("source changed during processing")]
    Changed,
    #[error("sanitized copy name could not be reserved")]
    DestinationExhausted,
    #[error("sanitized output failed verification: {0}")]
    Verification(String),
    #[error("file data is malformed: {0}")]
    Malformed(String),
    #[error("file operation failed: {0}")]
    Io(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Identity {
    dev: u64,
    ino: u64,
    len: u64,
    mtime: i64,
    mtime_ns: i64,
    ctime: i64,
    ctime_ns: i64,
}

impl Identity {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            dev: metadata.dev(),
            ino: metadata.ino(),
            len: metadata.len(),
            mtime: metadata.mtime(),
            mtime_ns: metadata.mtime_nsec(),
            ctime: metadata.ctime(),
            ctime_ns: metadata.ctime_nsec(),
        }
    }
}

fn read_regular(path: &Path) -> Result<(Vec<u8>, Identity), PrivacySecurityError> {
    let before = fs::symlink_metadata(path).map_err(map_io)?;
    if !before.is_file() || before.file_type().is_symlink() {
        return Err(PrivacySecurityError::NotRegular);
    }
    if before.len() > SOURCE_CAPACITY {
        return Err(PrivacySecurityError::TooLarge);
    }
    let expected = Identity::from_metadata(&before);
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(OFlags::NOFOLLOW.bits() as i32);
    let mut file = options.open(path).map_err(map_io)?;
    if Identity::from_metadata(&file.metadata().map_err(map_io)?) != expected {
        return Err(PrivacySecurityError::Changed);
    }
    let mut bytes = Vec::with_capacity(usize::try_from(before.len()).unwrap_or_default());
    Read::by_ref(&mut file)
        .take(SOURCE_CAPACITY + 1)
        .read_to_end(&mut bytes)
        .map_err(map_io)?;
    if bytes.len() as u64 > SOURCE_CAPACITY {
        return Err(PrivacySecurityError::TooLarge);
    }
    if Identity::from_metadata(&file.metadata().map_err(map_io)?) != expected
        || Identity::from_metadata(&fs::symlink_metadata(path).map_err(map_io)?) != expected
    {
        return Err(PrivacySecurityError::Changed);
    }
    Ok((bytes, expected))
}

fn detect_format(bytes: &[u8]) -> Option<ReviewedFormat> {
    if bytes.starts_with(&[0xff, 0xd8]) {
        Some(ReviewedFormat::Jpeg)
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(ReviewedFormat::Png)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(ReviewedFormat::WebP)
    } else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        Some(ReviewedFormat::Tiff)
    } else {
        None
    }
}

fn inspect_path(path: PathBuf) -> InspectionEntry {
    let metadata = fs::symlink_metadata(&path).ok();
    let executable = metadata
        .as_ref()
        .is_some_and(|metadata| metadata.permissions().mode() & 0o111 != 0);
    let content_type = gio::content_type_guess(Some(&path), None::<&[u8]>).0;
    let suspicious = analyze_suspicious_file(&path, Some(content_type.as_str()), executable);
    let privacy = match read_regular(&path) {
        Ok((bytes, _)) => inspect_bytes(&bytes),
        Err(PrivacySecurityError::NotRegular) => PrivacyInspectionState::NotRegular,
        Err(PrivacySecurityError::TooLarge) => PrivacyInspectionState::TooLarge,
        Err(PrivacySecurityError::Changed) => PrivacyInspectionState::Changed,
        Err(error) => PrivacyInspectionState::Inaccessible(error.to_string()),
    };
    InspectionEntry {
        path,
        suspicious,
        privacy,
    }
}

fn inspect_bytes(bytes: &[u8]) -> PrivacyInspectionState {
    let Some(format) = detect_format(bytes) else {
        return PrivacyInspectionState::Unsupported;
    };
    match inspect_container(format, bytes) {
        Ok(findings) => PrivacyInspectionState::Reviewed { format, findings },
        Err(error) => PrivacyInspectionState::Malformed(error),
    }
}

fn inspect_container(format: ReviewedFormat, bytes: &[u8]) -> Result<Vec<PrivacyFinding>, String> {
    let mut findings = Vec::new();
    match format {
        ReviewedFormat::Jpeg => {
            for segment in jpeg_segments(bytes)? {
                match segment.marker {
                    0xe1 if segment.payload.starts_with(b"Exif\0\0") => push_finding(
                        &mut findings,
                        "EXIF metadata",
                        "May include capture time, camera or device details, software, author fields, an embedded thumbnail, and location tags.",
                    ),
                    0xe1 => push_finding(
                        &mut findings,
                        "XMP or application metadata",
                        "An APP1 metadata block is present and may contain descriptive or editing history.",
                    ),
                    0xed => push_finding(
                        &mut findings,
                        "IPTC or Photoshop metadata",
                        "An APP13 block is present and may contain author, caption, location, or editing metadata.",
                    ),
                    0xfe => push_finding(
                        &mut findings,
                        "JPEG comment",
                        "A free-form comment block is embedded in the image.",
                    ),
                    _ => {}
                }
            }
            inspect_exif_evidence(bytes, &mut findings);
        }
        ReviewedFormat::Png => {
            for chunk in png_chunks(bytes)? {
                match chunk.kind {
                    b"eXIf" => push_finding(
                        &mut findings,
                        "EXIF metadata",
                        "An EXIF chunk may contain location, capture time, camera, software, author, or thumbnail details.",
                    ),
                    b"iTXt" | b"tEXt" | b"zTXt" => push_finding(
                        &mut findings,
                        "PNG text metadata",
                        "A textual metadata chunk may contain comments, software, author, copyright, or XMP data.",
                    ),
                    b"tIME" => push_finding(
                        &mut findings,
                        "PNG modification time",
                        "An embedded modification-time chunk is present.",
                    ),
                    _ => {}
                }
            }
        }
        ReviewedFormat::WebP => {
            for chunk in webp_chunks(bytes)? {
                match chunk.kind {
                    b"EXIF" => push_finding(
                        &mut findings,
                        "EXIF metadata",
                        "An EXIF chunk may contain location, capture time, camera, software, author, or thumbnail details.",
                    ),
                    b"XMP " => push_finding(
                        &mut findings,
                        "XMP metadata",
                        "An XMP chunk may contain descriptive fields, author data, or editing history.",
                    ),
                    _ => {}
                }
            }
        }
        ReviewedFormat::Tiff => inspect_exif_evidence(bytes, &mut findings),
    }
    Ok(findings)
}

fn inspect_exif_evidence(bytes: &[u8], findings: &mut Vec<PrivacyFinding>) {
    let mut cursor = Cursor::new(bytes);
    let Ok(exif) = ExifReader::new().read_from_container(&mut cursor) else {
        return;
    };
    let mut gps = false;
    let mut camera = false;
    let mut software = false;
    let mut creator = false;
    let mut timestamps = false;
    let mut thumbnail = false;
    for field in exif.fields() {
        match field.tag {
            ExifTag::GPSLatitude
            | ExifTag::GPSLongitude
            | ExifTag::GPSAltitude
            | ExifTag::GPSDateStamp => gps = true,
            ExifTag::Make
            | ExifTag::Model
            | ExifTag::LensMake
            | ExifTag::LensModel
            | ExifTag::BodySerialNumber => camera = true,
            ExifTag::Software => software = true,
            ExifTag::Artist | ExifTag::Copyright => creator = true,
            ExifTag::DateTime | ExifTag::DateTimeOriginal | ExifTag::DateTimeDigitized => {
                timestamps = true;
            }
            ExifTag::JPEGInterchangeFormat | ExifTag::JPEGInterchangeFormatLength => {
                thumbnail = true;
            }
            _ => {}
        }
    }
    if gps {
        push_finding(
            findings,
            "Location metadata",
            "Parsed EXIF GPS fields are present and may reveal where the image was captured.",
        );
    }
    if camera {
        push_finding(
            findings,
            "Camera or device metadata",
            "Parsed EXIF maker, model, lens, or serial fields are present.",
        );
    }
    if software {
        push_finding(
            findings,
            "Software metadata",
            "A parsed EXIF software field may identify the creating or editing application.",
        );
    }
    if creator {
        push_finding(
            findings,
            "Creator or copyright metadata",
            "Parsed EXIF artist or copyright fields are present.",
        );
    }
    if timestamps {
        push_finding(
            findings,
            "Capture or modification timestamps",
            "Parsed EXIF date/time fields are present.",
        );
    }
    if thumbnail {
        push_finding(
            findings,
            "Embedded thumbnail",
            "Parsed EXIF thumbnail offset or length fields indicate an embedded thumbnail.",
        );
    }
}

fn push_finding(findings: &mut Vec<PrivacyFinding>, category: &'static str, explanation: &str) {
    if findings.len() < FINDING_CAPACITY && !findings.iter().any(|item| item.category == category) {
        findings.push(PrivacyFinding {
            category,
            explanation: explanation.to_owned(),
        });
    }
}

struct JpegSegment<'a> {
    marker: u8,
    payload: &'a [u8],
}

fn jpeg_segments(bytes: &[u8]) -> Result<Vec<JpegSegment<'_>>, String> {
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return Err("missing JPEG start marker".to_owned());
    }
    let mut segments = Vec::new();
    let mut offset = 2usize;
    while offset < bytes.len() {
        while offset < bytes.len() && bytes[offset] == 0xff {
            offset += 1;
        }
        if offset >= bytes.len() {
            return Err("truncated JPEG marker".to_owned());
        }
        let marker = bytes[offset];
        offset += 1;
        if marker == 0xd9 || marker == 0xda {
            break;
        }
        if matches!(marker, 0x01 | 0xd0..=0xd7) {
            continue;
        }
        if offset + 2 > bytes.len() {
            return Err("truncated JPEG segment length".to_owned());
        }
        let length = usize::from(u16::from_be_bytes([bytes[offset], bytes[offset + 1]]));
        if length < 2 || offset + length > bytes.len() {
            return Err("invalid JPEG segment length".to_owned());
        }
        segments.push(JpegSegment {
            marker,
            payload: &bytes[offset + 2..offset + length],
        });
        offset += length;
    }
    Ok(segments)
}

struct Chunk<'a> {
    kind: &'a [u8; 4],
    full: &'a [u8],
}

fn png_chunks(bytes: &[u8]) -> Result<Vec<Chunk<'_>>, String> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("missing PNG signature".to_owned());
    }
    let mut chunks = Vec::new();
    let mut offset = 8usize;
    let mut saw_end = false;
    while offset < bytes.len() {
        if offset + 12 > bytes.len() {
            return Err("truncated PNG chunk".to_owned());
        }
        let length = u32::from_be_bytes(
            bytes[offset..offset + 4]
                .try_into()
                .map_err(|_| "invalid PNG length")?,
        ) as usize;
        let end = offset
            .checked_add(12)
            .and_then(|base| base.checked_add(length))
            .ok_or("PNG chunk length overflow")?;
        if end > bytes.len() {
            return Err("PNG chunk exceeds file".to_owned());
        }
        let kind: &[u8; 4] = bytes[offset + 4..offset + 8]
            .try_into()
            .map_err(|_| "invalid PNG kind")?;
        chunks.push(Chunk {
            kind,
            full: &bytes[offset..end],
        });
        offset = end;
        if kind == b"IEND" {
            saw_end = true;
            break;
        }
    }
    if !saw_end {
        return Err("PNG has no IEND chunk".to_owned());
    }
    Ok(chunks)
}

fn webp_chunks(bytes: &[u8]) -> Result<Vec<Chunk<'_>>, String> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return Err("missing WebP RIFF signature".to_owned());
    }
    let declared =
        u32::from_le_bytes(bytes[4..8].try_into().map_err(|_| "invalid WebP size")?) as usize;
    if declared
        .checked_add(8)
        .is_none_or(|length| length > bytes.len())
    {
        return Err("WebP RIFF size exceeds file".to_owned());
    }
    let mut chunks = Vec::new();
    let mut offset = 12usize;
    while offset < declared + 8 {
        if offset + 8 > bytes.len() {
            return Err("truncated WebP chunk".to_owned());
        }
        let length = u32::from_le_bytes(
            bytes[offset + 4..offset + 8]
                .try_into()
                .map_err(|_| "invalid WebP chunk size")?,
        ) as usize;
        let padded = length
            .checked_add(length & 1)
            .ok_or("WebP chunk overflow")?;
        let end = offset
            .checked_add(8)
            .and_then(|base| base.checked_add(padded))
            .ok_or("WebP chunk overflow")?;
        if end > bytes.len() || end > declared + 8 {
            return Err("WebP chunk exceeds RIFF".to_owned());
        }
        let kind: &[u8; 4] = bytes[offset..offset + 4]
            .try_into()
            .map_err(|_| "invalid WebP kind")?;
        chunks.push(Chunk {
            kind,
            full: &bytes[offset..end],
        });
        offset = end;
    }
    Ok(chunks)
}

fn sanitize_bytes(
    format: ReviewedFormat,
    bytes: &[u8],
) -> Result<(Vec<u8>, Vec<&'static str>), PrivacySecurityError> {
    match format {
        ReviewedFormat::Jpeg => sanitize_jpeg(bytes),
        ReviewedFormat::Png => sanitize_png(bytes),
        ReviewedFormat::WebP => sanitize_webp(bytes),
        ReviewedFormat::Tiff => Err(PrivacySecurityError::Unsupported),
    }
}

fn sanitize_jpeg(bytes: &[u8]) -> Result<(Vec<u8>, Vec<&'static str>), PrivacySecurityError> {
    jpeg_segments(bytes).map_err(PrivacySecurityError::Malformed)?;
    let mut output = vec![0xff, 0xd8];
    let mut removed = Vec::new();
    let mut offset = 2usize;
    while offset < bytes.len() {
        let marker_start = offset;
        while offset < bytes.len() && bytes[offset] == 0xff {
            offset += 1;
        }
        if offset >= bytes.len() {
            return Err(PrivacySecurityError::Malformed(
                "truncated JPEG marker".to_owned(),
            ));
        }
        let marker = bytes[offset];
        offset += 1;
        if marker == 0xda {
            output.extend_from_slice(&bytes[marker_start..]);
            break;
        }
        if marker == 0xd9 {
            output.extend_from_slice(&bytes[marker_start..offset]);
            break;
        }
        if matches!(marker, 0x01 | 0xd0..=0xd7) {
            output.extend_from_slice(&bytes[marker_start..offset]);
            continue;
        }
        if offset + 2 > bytes.len() {
            return Err(PrivacySecurityError::Malformed(
                "truncated JPEG segment".to_owned(),
            ));
        }
        let length = usize::from(u16::from_be_bytes([bytes[offset], bytes[offset + 1]]));
        let end = offset
            .checked_add(length)
            .ok_or_else(|| PrivacySecurityError::Malformed("JPEG segment overflow".to_owned()))?;
        if length < 2 || end > bytes.len() {
            return Err(PrivacySecurityError::Malformed(
                "invalid JPEG segment".to_owned(),
            ));
        }
        if matches!(marker, 0xe1 | 0xed | 0xfe) {
            let category = if marker == 0xed {
                "IPTC/Photoshop metadata"
            } else if marker == 0xfe {
                "JPEG comments"
            } else {
                "EXIF/XMP metadata"
            };
            if !removed.contains(&category) {
                removed.push(category);
            }
        } else {
            output.extend_from_slice(&bytes[marker_start..end]);
        }
        offset = end;
    }
    Ok((output, removed))
}

fn sanitize_png(bytes: &[u8]) -> Result<(Vec<u8>, Vec<&'static str>), PrivacySecurityError> {
    let chunks = png_chunks(bytes).map_err(PrivacySecurityError::Malformed)?;
    let mut output = bytes[..8].to_vec();
    let mut removed = Vec::new();
    for chunk in chunks {
        if matches!(chunk.kind, b"eXIf" | b"iTXt" | b"tEXt" | b"zTXt" | b"tIME") {
            if !removed.contains(&"EXIF/text/time metadata") {
                removed.push("EXIF/text/time metadata");
            }
        } else {
            output.extend_from_slice(chunk.full);
        }
    }
    Ok((output, removed))
}

fn sanitize_webp(bytes: &[u8]) -> Result<(Vec<u8>, Vec<&'static str>), PrivacySecurityError> {
    let chunks = webp_chunks(bytes).map_err(PrivacySecurityError::Malformed)?;
    let mut output = b"RIFF\0\0\0\0WEBP".to_vec();
    let mut removed = Vec::new();
    for chunk in chunks {
        if matches!(chunk.kind, b"EXIF" | b"XMP ") {
            if !removed.contains(&"EXIF/XMP metadata") {
                removed.push("EXIF/XMP metadata");
            }
        } else if chunk.kind == b"VP8X" {
            if chunk.full.len() != 18 {
                return Err(PrivacySecurityError::Malformed(
                    "invalid WebP VP8X chunk length".to_owned(),
                ));
            }
            let mut feature_chunk = chunk.full.to_vec();
            feature_chunk[8] &= !(0x08 | 0x04);
            output.extend_from_slice(&feature_chunk);
        } else {
            output.extend_from_slice(chunk.full);
        }
    }
    let riff_size = u32::try_from(output.len().saturating_sub(8))
        .map_err(|_| PrivacySecurityError::TooLarge)?;
    output[4..8].copy_from_slice(&riff_size.to_le_bytes());
    Ok((output, removed))
}

fn sanitized_destination(source: &Path, attempt: usize) -> Option<PathBuf> {
    let parent = source.parent()?;
    let stem = source
        .file_stem()
        .unwrap_or_else(|| source.file_name().unwrap_or_default());
    let extension = source.extension();
    let mut name = stem.to_os_string();
    if attempt == 0 {
        name.push(" (sanitized)");
    } else {
        name.push(format!(" (sanitized {attempt})"));
    }
    if let Some(extension) = extension {
        name.push(".");
        name.push(extension);
    }
    Some(parent.join(name))
}

fn revalidate_source_after_staging(
    source: &Path,
    expected: Identity,
    stage: &Path,
) -> Result<(), PrivacySecurityError> {
    let current = match fs::symlink_metadata(source) {
        Ok(metadata) => Identity::from_metadata(&metadata),
        Err(error) => {
            let _ = fs::remove_file(stage);
            return Err(map_io(error));
        }
    };
    if current != expected {
        let _ = fs::remove_file(stage);
        return Err(PrivacySecurityError::Changed);
    }
    Ok(())
}

fn sanitize_copy(
    generation: u64,
    source: PathBuf,
) -> Result<SanitizedCopyOutcome, PrivacySecurityError> {
    let (bytes, identity) = read_regular(&source)?;
    let format = detect_format(&bytes).ok_or(PrivacySecurityError::Unsupported)?;
    let (sanitized, removed_categories) = sanitize_bytes(format, &bytes)?;
    let verified =
        inspect_container(format, &sanitized).map_err(PrivacySecurityError::Verification)?;
    if !verified.is_empty() {
        return Err(PrivacySecurityError::Verification(
            "reviewed metadata remains".to_owned(),
        ));
    }
    if Identity::from_metadata(&fs::symlink_metadata(&source).map_err(map_io)?) != identity {
        return Err(PrivacySecurityError::Changed);
    }
    let parent = source
        .parent()
        .ok_or(PrivacySecurityError::InvalidRequest)?;
    for attempt in 0..100usize {
        let destination =
            sanitized_destination(&source, attempt).ok_or(PrivacySecurityError::InvalidRequest)?;
        if destination.exists() {
            continue;
        }
        let stage = parent.join(format!(
            ".floe-sanitize-{}-{generation}-{attempt}",
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(OFlags::NOFOLLOW.bits() as i32);
        match options.open(&stage) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(&sanitized).and_then(|()| file.sync_all()) {
                    let _ = fs::remove_file(&stage);
                    return Err(map_io(error));
                }
                drop(file);
                revalidate_source_after_staging(&source, identity, &stage)?;
                match renameat_with(CWD, &stage, CWD, &destination, RenameFlags::NOREPLACE) {
                    Ok(()) => {}
                    Err(error) if error == rustix::io::Errno::EXIST => {
                        let _ = fs::remove_file(&stage);
                        continue;
                    }
                    Err(error) => {
                        let _ = fs::remove_file(&stage);
                        return Err(PrivacySecurityError::Io(error.to_string()));
                    }
                }
                return Ok(SanitizedCopyOutcome {
                    generation,
                    source,
                    destination,
                    format,
                    removed_categories,
                });
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(map_io(error)),
        }
    }
    Err(PrivacySecurityError::DestinationExhausted)
}

pub struct PrivacySecurityWorker {
    sender: Option<SyncSender<PrivacySecurityRequest>>,
    results: Arc<Mutex<VecDeque<PrivacySecurityResult>>>,
    latest_generation: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

fn cancelled_result(request: PrivacySecurityRequest) -> PrivacySecurityResult {
    match request {
        PrivacySecurityRequest::Inspect { generation, .. } => {
            PrivacySecurityResult::Inspection(InspectionOutcome {
                generation,
                entries: Vec::new(),
                cancelled: true,
            })
        }
        PrivacySecurityRequest::Sanitize { generation, .. } => {
            PrivacySecurityResult::Sanitized(SanitizationOutcome {
                generation,
                items: Vec::new(),
                cancelled: true,
            })
        }
    }
}

impl std::fmt::Debug for PrivacySecurityWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PrivacySecurityWorker")
            .finish_non_exhaustive()
    }
}

impl PrivacySecurityWorker {
    pub fn spawn() -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(REQUEST_CAPACITY);
        let results = Arc::new(Mutex::new(VecDeque::with_capacity(RESULT_CAPACITY)));
        let latest_generation = Arc::new(AtomicU64::new(0));
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_results = Arc::clone(&results);
        let worker_generation = Arc::clone(&latest_generation);
        let worker_shutdown = Arc::clone(&shutdown);
        let join = thread::Builder::new()
            .name("floe-privacy-security".to_owned())
            .spawn(move || {
                while let Ok(request) = receiver.recv() {
                    if worker_shutdown.load(Ordering::Acquire) {
                        break;
                    }
                    let generation = match &request {
                        PrivacySecurityRequest::Inspect { generation, .. }
                        | PrivacySecurityRequest::Sanitize { generation, .. } => *generation,
                    };
                    if worker_generation.load(Ordering::Acquire) != generation {
                        let cancelled = cancelled_result(request);
                        if let Ok(mut queue) = worker_results.lock() {
                            if queue.len() == RESULT_CAPACITY {
                                queue.pop_front();
                            }
                            queue.push_back(cancelled);
                        }
                        continue;
                    }
                    let result = match request {
                        PrivacySecurityRequest::Inspect { generation, paths } => {
                            let mut entries = Vec::with_capacity(paths.len());
                            let mut cancelled = false;
                            for path in paths {
                                if worker_shutdown.load(Ordering::Acquire)
                                    || worker_generation.load(Ordering::Acquire) != generation
                                {
                                    cancelled = true;
                                    break;
                                }
                                entries.push(inspect_path(path));
                            }
                            cancelled |= worker_shutdown.load(Ordering::Acquire)
                                || worker_generation.load(Ordering::Acquire) != generation;
                            PrivacySecurityResult::Inspection(InspectionOutcome {
                                generation,
                                entries,
                                cancelled,
                            })
                        }
                        PrivacySecurityRequest::Sanitize {
                            generation,
                            sources,
                        } => {
                            let mut items = Vec::with_capacity(sources.len());
                            let mut cancelled = false;
                            for source in sources {
                                if worker_shutdown.load(Ordering::Acquire)
                                    || worker_generation.load(Ordering::Acquire) != generation
                                {
                                    cancelled = true;
                                    break;
                                }
                                let result = sanitize_copy(generation, source.clone());
                                items.push(SanitizedItemOutcome { source, result });
                            }
                            PrivacySecurityResult::Sanitized(SanitizationOutcome {
                                generation,
                                items,
                                cancelled,
                            })
                        }
                    };
                    if let Ok(mut queue) = worker_results.lock() {
                        if queue.len() == RESULT_CAPACITY {
                            queue.pop_front();
                        }
                        queue.push_back(result);
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            results,
            latest_generation,
            shutdown,
            join: Some(join),
        })
    }

    pub fn submit(&self, request: PrivacySecurityRequest) -> Result<(), PrivacySecurityError> {
        let generation = match &request {
            PrivacySecurityRequest::Inspect { generation, .. }
            | PrivacySecurityRequest::Sanitize { generation, .. } => *generation,
        };
        self.latest_generation.store(generation, Ordering::Release);
        self.sender
            .as_ref()
            .ok_or(PrivacySecurityError::Stopped)?
            .try_send(request)
            .map_err(|error| match error {
                TrySendError::Full(_) => PrivacySecurityError::QueueFull,
                TrySendError::Disconnected(_) => PrivacySecurityError::Stopped,
            })
    }

    pub fn try_result(&self) -> Option<PrivacySecurityResult> {
        self.results.lock().ok()?.pop_front()
    }

    pub fn cancel(&self) {
        self.latest_generation.fetch_add(1, Ordering::AcqRel);
    }
}

impl Drop for PrivacySecurityWorker {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        self.latest_generation.fetch_add(1, Ordering::AcqRel);
        self.sender.take();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn map_io(error: io::Error) -> PrivacySecurityError {
    PrivacySecurityError::Io(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    fn wait_result(worker: &PrivacySecurityWorker) -> PrivacySecurityResult {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(result) = worker.try_result() {
                return result;
            }
            assert!(Instant::now() < deadline, "worker result timed out");
            thread::yield_now();
        }
    }

    fn png_with_text() -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&[0, 0, 0, 3]);
        bytes.extend_from_slice(b"tEXt");
        bytes.extend_from_slice(b"x=y");
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.extend_from_slice(b"IEND");
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes
    }

    fn jpeg_with_exif() -> Vec<u8> {
        vec![
            0xff, 0xd8, 0xff, 0xe1, 0x00, 0x08, b'E', b'x', b'i', b'f', 0, 0, 0xff, 0xda, 0x00,
            0x02, 0xff, 0xd9,
        ]
    }

    fn webp_with_exif() -> Vec<u8> {
        let mut bytes = b"RIFF\0\0\0\0WEBP".to_vec();
        bytes.extend_from_slice(b"EXIF");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(b"test");
        let size = u32::try_from(bytes.len() - 8).expect("fixture size");
        bytes[4..8].copy_from_slice(&size.to_le_bytes());
        bytes
    }

    fn webp_with_vp8x_metadata_flags() -> Vec<u8> {
        let mut bytes = b"RIFF\0\0\0\0WEBP".to_vec();
        bytes.extend_from_slice(b"VP8X");
        bytes.extend_from_slice(&10_u32.to_le_bytes());
        bytes.extend_from_slice(&[0x0c, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        bytes.extend_from_slice(b"EXIF");
        bytes.extend_from_slice(&4_u32.to_le_bytes());
        bytes.extend_from_slice(b"test");
        bytes.extend_from_slice(b"XMP ");
        bytes.extend_from_slice(&4_u32.to_le_bytes());
        bytes.extend_from_slice(b"test");
        let size = u32::try_from(bytes.len() - 8).expect("fixture size");
        bytes[4..8].copy_from_slice(&size.to_le_bytes());
        bytes
    }

    fn tiff_with_location_camera_and_time() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"II*\0");
        bytes.extend_from_slice(&8_u32.to_le_bytes());
        bytes.extend_from_slice(&3_u16.to_le_bytes());
        for (tag, kind, count, value) in [
            (0x010f_u16, 2_u16, 5_u32, 50_u32),
            (0x0132_u16, 2_u16, 20_u32, 55_u32),
            (0x8825_u16, 4_u16, 1_u32, 75_u32),
        ] {
            bytes.extend_from_slice(&tag.to_le_bytes());
            bytes.extend_from_slice(&kind.to_le_bytes());
            bytes.extend_from_slice(&count.to_le_bytes());
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(b"Floe\0");
        bytes.extend_from_slice(b"2026:08:31 12:00:00\0");
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&5_u16.to_le_bytes());
        bytes.extend_from_slice(&3_u32.to_le_bytes());
        bytes.extend_from_slice(&93_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        for (numerator, denominator) in [(41_u32, 1_u32), (52, 1), (0, 1)] {
            bytes.extend_from_slice(&numerator.to_le_bytes());
            bytes.extend_from_slice(&denominator.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn phase_18o_provider_reports_supported_exif_and_container_evidence() {
        assert!(matches!(
            inspect_bytes(b"plain"),
            PrivacyInspectionState::Unsupported
        ));
        assert!(matches!(
            inspect_bytes(b"\x89PNG\r\n\x1a\n"),
            PrivacyInspectionState::Malformed(_)
        ));
        let PrivacyInspectionState::Reviewed { findings, .. } = inspect_bytes(&png_with_text())
        else {
            panic!("reviewed PNG")
        };
        assert_eq!(findings[0].category, "PNG text metadata");
    }

    #[test]
    fn phase_18o_provider_parses_tiff_location_camera_and_time_categories() {
        let PrivacyInspectionState::Reviewed { findings, .. } =
            inspect_bytes(&tiff_with_location_camera_and_time())
        else {
            panic!("reviewed TIFF")
        };
        for category in [
            "Location metadata",
            "Camera or device metadata",
            "Capture or modification timestamps",
        ] {
            assert!(
                findings.iter().any(|finding| finding.category == category),
                "missing {category}: {findings:?}"
            );
        }
    }

    #[test]
    fn phase_18o_failures_are_explicit_and_identity_changes_are_detected() {
        let fixture = tempdir().expect("fixture");
        let missing = inspect_path(fixture.path().join("missing.png"));
        assert!(matches!(
            missing.privacy,
            PrivacyInspectionState::Inaccessible(_)
        ));

        let target = fixture.path().join("target.png");
        let link = fixture.path().join("link.png");
        fs::write(&target, png_with_text()).expect("target");
        std::os::unix::fs::symlink(&target, &link).expect("link");
        assert!(matches!(
            inspect_path(link).privacy,
            PrivacyInspectionState::NotRegular
        ));

        let oversized = fixture.path().join("oversized.png");
        fs::File::create(&oversized)
            .expect("oversized fixture")
            .set_len(SOURCE_CAPACITY + 1)
            .expect("oversized length");
        assert!(matches!(
            inspect_path(oversized).privacy,
            PrivacyInspectionState::TooLarge
        ));

        let before = Identity::from_metadata(&fs::symlink_metadata(&target).expect("before"));
        fs::write(&target, b"changed content with a different length").expect("change target");
        let after = Identity::from_metadata(&fs::symlink_metadata(&target).expect("after"));
        assert_ne!(before, after, "source identity must expose a changed file");
    }

    #[test]
    fn phase_18p_sanitized_copy_is_no_overwrite_and_source_preserving() {
        let fixture = tempdir().expect("temporary directory");
        let source = fixture.path().join("image.png");
        let original = png_with_text();
        fs::write(&source, &original).expect("source");
        fs::write(fixture.path().join("image (sanitized).png"), b"existing").expect("conflict");
        let outcome = sanitize_copy(7, source.clone()).expect("sanitize");
        assert_eq!(
            outcome
                .destination
                .file_name()
                .and_then(|name| name.to_str()),
            Some("image (sanitized 1).png")
        );
        assert_eq!(fs::read(&source).expect("source remains"), original);
        assert!(
            matches!(inspect_bytes(&fs::read(&outcome.destination).expect("copy")), PrivacyInspectionState::Reviewed { findings, .. } if findings.is_empty())
        );
        assert_eq!(
            fs::read(fixture.path().join("image (sanitized).png")).expect("conflict remains"),
            b"existing"
        );
    }

    #[test]
    fn phase_18p_formats_remove_reviewed_metadata_from_jpeg_png_and_webp() {
        for (format, bytes) in [
            (ReviewedFormat::Jpeg, jpeg_with_exif()),
            (ReviewedFormat::Png, png_with_text()),
            (ReviewedFormat::WebP, webp_with_exif()),
        ] {
            let findings = inspect_container(format, &bytes).expect("valid container");
            assert!(!findings.is_empty(), "fixture must contain metadata");
            let (sanitized, removed) = sanitize_bytes(format, &bytes).expect("sanitize");
            assert!(!removed.is_empty());
            assert!(
                inspect_container(format, &sanitized)
                    .expect("sanitized valid container")
                    .is_empty()
            );
        }
    }

    #[test]
    fn phase_18p_formats_webp_clears_removed_metadata_feature_flags() {
        let (sanitized, removed) =
            sanitize_webp(&webp_with_vp8x_metadata_flags()).expect("sanitize WebP");
        assert_eq!(removed, vec!["EXIF/XMP metadata"]);
        let chunks = webp_chunks(&sanitized).expect("valid sanitized WebP");
        assert!(
            chunks
                .iter()
                .all(|chunk| !matches!(chunk.kind, b"EXIF" | b"XMP "))
        );
        let feature = chunks
            .iter()
            .find(|chunk| chunk.kind == b"VP8X")
            .expect("VP8X retained");
        assert_eq!(feature.full[8] & 0x0c, 0);
    }

    #[cfg(unix)]
    #[test]
    fn phase_18p_sanitizer_rejects_symlink_and_unsupported_without_output() {
        use std::os::unix::fs::symlink;
        let fixture = tempdir().expect("temporary directory");
        let source = fixture.path().join("source.txt");
        fs::write(&source, b"text").expect("source");
        let link = fixture.path().join("image.png");
        symlink(&source, &link).expect("link");
        assert_eq!(
            sanitize_copy(1, link).expect_err("symlink rejected"),
            PrivacySecurityError::NotRegular
        );
        assert_eq!(
            sanitize_copy(1, source).expect_err("unsupported"),
            PrivacySecurityError::Unsupported
        );
    }

    #[test]
    fn phase_18p_worker_source_revalidation_error_removes_private_stage() {
        let fixture = tempdir().expect("fixture");
        let original = fixture.path().join("original.png");
        let stage = fixture.path().join(".floe-sanitize-stage");
        fs::write(&original, png_with_text()).expect("source");
        fs::write(&stage, b"private staged bytes").expect("stage");
        let expected =
            Identity::from_metadata(&fs::symlink_metadata(&original).expect("source identity"));
        fs::remove_file(&original).expect("remove source for race");

        assert!(matches!(
            revalidate_source_after_staging(&original, expected, &stage),
            Err(PrivacySecurityError::Io(_))
        ));
        assert!(!stage.exists(), "failed revalidation must clean the stage");
    }

    #[test]
    fn phase_18p_worker_batches_partial_results_and_reports_cancellation() {
        let fixture = tempdir().expect("temporary directory");
        let supported = fixture.path().join("image.png");
        let unsupported = fixture.path().join("notes.txt");
        fs::write(&supported, png_with_text()).expect("image");
        fs::write(&unsupported, b"notes").expect("notes");

        let worker = PrivacySecurityWorker::spawn().expect("worker");
        worker
            .submit(
                PrivacySecurityRequest::sanitize(4, vec![supported.clone(), unsupported.clone()])
                    .expect("request"),
            )
            .expect("submit");
        let PrivacySecurityResult::Sanitized(outcome) = wait_result(&worker) else {
            panic!("sanitization result")
        };
        assert!(!outcome.cancelled);
        assert_eq!(outcome.items.len(), 2);
        assert!(outcome.items[0].result.is_ok());
        assert_eq!(
            outcome.items[1].result,
            Err(PrivacySecurityError::Unsupported)
        );

        worker
            .submit(PrivacySecurityRequest::sanitize(5, vec![supported]).expect("request"))
            .expect("submit");
        worker.cancel();
        let PrivacySecurityResult::Sanitized(outcome) = wait_result(&worker) else {
            panic!("cancelled result")
        };
        assert!(outcome.cancelled);
    }

    #[test]
    fn background_feedback_lifecycle_privacy_cancel_always_has_a_terminal_result() {
        let request = PrivacySecurityRequest::inspect(77, vec![PathBuf::from("/tmp/item")])
            .expect("inspection request");
        let PrivacySecurityResult::Inspection(outcome) = cancelled_result(request) else {
            panic!("inspection cancellation result");
        };
        assert_eq!(outcome.generation, 77);
        assert!(outcome.entries.is_empty());
        assert!(outcome.cancelled);
    }
}
