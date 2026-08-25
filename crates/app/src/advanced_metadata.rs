//! Bounded passive EXIF and media/audio metadata parsing.

use std::{
    borrow::Cow,
    fs::{self, File, Metadata},
    io::{self, BufReader, Read, Seek, SeekFrom},
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime},
};

use exif::{In, Reader as ExifReader, Tag as ExifTag};
use lofty::{
    config::{ParseOptions, ParsingMode},
    prelude::{Accessor, AudioFile, TaggedFileExt},
    probe::Probe,
};
use rustix::fs::{Mode, OFlags};
use thiserror::Error;

pub const ADVANCED_METADATA_READ_CAPACITY: u64 = 16 * 1024 * 1024;
pub const ADVANCED_METADATA_STRING_CAPACITY: usize = 1_024;
pub const ADVANCED_METADATA_FIELD_CAPACITY: usize = 10;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataField {
    pub label: &'static str,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExifMetadata {
    pub fields: Arc<[MetadataField]>,
    pub values_truncated: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MediaMetadata {
    pub duration: Option<Duration>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track: Option<u32>,
    pub track_total: Option<u32>,
    pub disk: Option<u32>,
    pub disk_total: Option<u32>,
    pub genre: Option<String>,
    pub year: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub audio_bitrate: Option<u32>,
    pub values_truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdvancedMetadata {
    pub exif: Option<ExifMetadata>,
    pub media: Option<MediaMetadata>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdvancedMetadataState {
    Unsupported,
    NoMetadata,
    Present(AdvancedMetadata),
    LimitExceeded,
    Malformed(String),
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum AdvancedMetadataError {
    #[error("entry disappeared while advanced metadata was loading")]
    Missing,
    #[error("entry is not a regular file and was not followed")]
    NotRegular,
    #[error("entry changed while advanced metadata was loading")]
    Changed,
    #[error("advanced metadata is inaccessible: {0}")]
    Inaccessible(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParserKind {
    Exif,
    Media,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceIdentity {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

struct ReadBudget<R> {
    inner: R,
    remaining: u64,
    exceeded: Arc<AtomicBool>,
}

impl SourceIdentity {
    fn from_metadata(metadata: &Metadata) -> Self {
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

impl<R> ReadBudget<R> {
    fn new(inner: R, exceeded: Arc<AtomicBool>) -> Self {
        Self {
            inner,
            remaining: ADVANCED_METADATA_READ_CAPACITY,
            exceeded,
        }
    }
}

impl<R: Read> Read for ReadBudget<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            self.exceeded.store(true, Ordering::Release);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "advanced metadata read budget exceeded",
            ));
        }
        let allowed = usize::try_from(self.remaining)
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        let read = self.inner.read(&mut buffer[..allowed])?;
        self.remaining = self.remaining.saturating_sub(read as u64);
        Ok(read)
    }
}

impl<R: Seek> Seek for ReadBudget<R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.inner.seek(position)
    }
}

pub fn load_advanced_metadata(
    path: &Path,
    expected_size: Option<u64>,
    expected_modified: Option<SystemTime>,
) -> Result<AdvancedMetadataState, AdvancedMetadataError> {
    let Some(parser) = parser_for_path(path) else {
        return Ok(AdvancedMetadataState::Unsupported);
    };
    let before = fs::symlink_metadata(path).map_err(map_io_error)?;
    if !before.is_file() || before.file_type().is_symlink() {
        return Err(AdvancedMetadataError::NotRegular);
    }
    if expected_size.is_some_and(|expected| expected != before.len())
        || expected_modified.is_some_and(|expected| before.modified().ok() != Some(expected))
    {
        return Err(AdvancedMetadataError::Changed);
    }
    let identity = SourceIdentity::from_metadata(&before);
    let descriptor = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|error| AdvancedMetadataError::Inaccessible(error.to_string()))?;
    let file = File::from(descriptor);
    let opened = file.metadata().map_err(map_io_error)?;
    if SourceIdentity::from_metadata(&opened) != identity {
        return Err(AdvancedMetadataError::Changed);
    }

    let state = match parser {
        ParserKind::Exif => parse_exif(file),
        ParserKind::Media => parse_media(file),
    };

    let after = fs::symlink_metadata(path).map_err(map_io_error)?;
    if SourceIdentity::from_metadata(&after) != identity {
        return Err(AdvancedMetadataError::Changed);
    }
    Ok(state)
}

fn parse_exif(file: File) -> AdvancedMetadataState {
    let exceeded = Arc::new(AtomicBool::new(false));
    let mut reader = BufReader::new(ReadBudget::new(file, Arc::clone(&exceeded)));
    let parsed = ExifReader::new().read_from_container(&mut reader);
    let exif = match parsed {
        Ok(exif) => exif,
        Err(exif::Error::NotFound(_)) => return AdvancedMetadataState::NoMetadata,
        Err(_error) if exceeded.load(Ordering::Acquire) => {
            return AdvancedMetadataState::LimitExceeded;
        }
        Err(error) => return AdvancedMetadataState::Malformed(error.to_string()),
    };
    let reviewed = [
        ("Camera maker", ExifTag::Make),
        ("Camera model", ExifTag::Model),
        ("Lens maker", ExifTag::LensMake),
        ("Lens model", ExifTag::LensModel),
        ("Captured", ExifTag::DateTimeOriginal),
        ("Orientation", ExifTag::Orientation),
        ("Exposure", ExifTag::ExposureTime),
        ("Aperture", ExifTag::FNumber),
        ("ISO", ExifTag::ISOSpeed),
        ("Focal length", ExifTag::FocalLength),
    ];
    let mut truncated = false;
    let fields = reviewed
        .into_iter()
        .filter_map(|(label, tag)| {
            let field = exif.get_field(tag, In::PRIMARY)?;
            let value = field.display_value().with_unit(&exif).to_string();
            let value = bounded_text(Cow::Owned(value), &mut truncated)?;
            Some(MetadataField { label, value })
        })
        .take(ADVANCED_METADATA_FIELD_CAPACITY)
        .collect::<Vec<_>>();
    if fields.is_empty() {
        AdvancedMetadataState::NoMetadata
    } else {
        AdvancedMetadataState::Present(AdvancedMetadata {
            exif: Some(ExifMetadata {
                fields: fields.into(),
                values_truncated: truncated,
            }),
            media: None,
        })
    }
}

fn parse_media(file: File) -> AdvancedMetadataState {
    let exceeded = Arc::new(AtomicBool::new(false));
    let reader = BufReader::new(ReadBudget::new(file, Arc::clone(&exceeded)));
    let options = ParseOptions::new()
        .parsing_mode(ParsingMode::Strict)
        .max_junk_bytes(1_024)
        .read_cover_art(false)
        .implicit_conversions(true);
    let tagged = match Probe::new(reader)
        .options(options)
        .guess_file_type()
        .and_then(|probe| probe.read().map_err(io::Error::other))
    {
        Ok(tagged) => tagged,
        Err(_) if exceeded.load(Ordering::Acquire) => {
            return AdvancedMetadataState::LimitExceeded;
        }
        Err(error) => return AdvancedMetadataState::Malformed(error.to_string()),
    };
    let properties = tagged.properties();
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    let mut truncated = false;
    let mut media = MediaMetadata {
        duration: (properties.duration() > Duration::ZERO).then(|| properties.duration()),
        sample_rate: properties.sample_rate(),
        channels: properties.channels(),
        audio_bitrate: properties.audio_bitrate(),
        ..MediaMetadata::default()
    };
    if let Some(tag) = tag {
        media.title = tag
            .title()
            .and_then(|value| bounded_text(value, &mut truncated));
        media.artist = tag
            .artist()
            .and_then(|value| bounded_text(value, &mut truncated));
        media.album = tag
            .album()
            .and_then(|value| bounded_text(value, &mut truncated));
        media.genre = tag
            .genre()
            .and_then(|value| bounded_text(value, &mut truncated));
        media.track = tag.track();
        media.track_total = tag.track_total();
        media.disk = tag.disk();
        media.disk_total = tag.disk_total();
        media.year = tag.year();
    }
    media.values_truncated = truncated;
    let has_metadata = media.duration.is_some()
        || media.title.is_some()
        || media.artist.is_some()
        || media.album.is_some()
        || media.genre.is_some()
        || media.track.is_some()
        || media.sample_rate.is_some()
        || media.channels.is_some()
        || media.audio_bitrate.is_some();
    if has_metadata {
        AdvancedMetadataState::Present(AdvancedMetadata {
            exif: None,
            media: Some(media),
        })
    } else {
        AdvancedMetadataState::NoMetadata
    }
}

fn bounded_text(value: Cow<'_, str>, truncated: &mut bool) -> Option<String> {
    let trimmed =
        value.trim_matches(|character: char| character.is_whitespace() || character == '\0');
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().count() <= ADVANCED_METADATA_STRING_CAPACITY {
        return Some(trimmed.to_owned());
    }
    *truncated = true;
    Some(
        trimmed
            .chars()
            .take(ADVANCED_METADATA_STRING_CAPACITY)
            .collect(),
    )
}

fn parser_for_path(path: &Path) -> Option<ParserKind> {
    let extension = path.extension()?.as_bytes();
    if ascii_extension_matches(
        extension,
        &[
            b"jpg", b"jpeg", b"tif", b"tiff", b"png", b"webp", b"heif", b"heic", b"avif",
        ],
    ) {
        return Some(ParserKind::Exif);
    }
    if ascii_extension_matches(
        extension,
        &[
            b"mp3", b"flac", b"ogg", b"opus", b"wav", b"m4a", b"mp4", b"aac", b"aiff", b"aif",
            b"ape", b"wv", b"mpc", b"spx",
        ],
    ) {
        return Some(ParserKind::Media);
    }
    None
}

fn ascii_extension_matches(extension: &[u8], candidates: &[&[u8]]) -> bool {
    candidates.iter().any(|candidate| {
        extension.len() == candidate.len()
            && extension
                .iter()
                .zip(candidate.iter())
                .all(|(left, right)| left.to_ascii_lowercase() == *right)
    })
}

fn map_io_error(error: io::Error) -> AdvancedMetadataError {
    match error.kind() {
        io::ErrorKind::NotFound => AdvancedMetadataError::Missing,
        _ => AdvancedMetadataError::Inaccessible(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::{
        config::WriteOptions,
        prelude::TagExt,
        tag::{Tag, TagType},
    };
    use std::{fs, os::unix::fs::symlink};
    use tempfile::tempdir;

    fn minimal_tiff_with_make(make: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"II");
        bytes.extend_from_slice(&42u16.to_le_bytes());
        bytes.extend_from_slice(&8u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&0x010fu16.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&(make.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&26u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(make);
        bytes
    }

    fn minimal_wav(samples: usize) -> Vec<u8> {
        let data_len = samples * 2;
        let mut bytes = Vec::with_capacity(44 + data_len);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36u32 + data_len as u32).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&8_000u32.to_le_bytes());
        bytes.extend_from_slice(&16_000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(data_len as u32).to_le_bytes());
        bytes.resize(44 + data_len, 0);
        bytes
    }

    #[test]
    fn phase_10f_advanced_metadata_contract_is_exact_bounded_and_explicit() {
        assert_eq!(
            parser_for_path(Path::new("/tmp/PHOTO.JPEG")),
            Some(ParserKind::Exif)
        );
        assert_eq!(
            parser_for_path(Path::new("/tmp/track.FLAC")),
            Some(ParserKind::Media)
        );
        assert_eq!(parser_for_path(Path::new("/tmp/notes.txt")), None);
        let mut truncated = false;
        let long = "x".repeat(ADVANCED_METADATA_STRING_CAPACITY + 40);
        assert_eq!(
            bounded_text(Cow::Borrowed(&long), &mut truncated)
                .expect("bounded")
                .chars()
                .count(),
            ADVANCED_METADATA_STRING_CAPACITY
        );
        assert!(truncated);
    }

    #[test]
    fn phase_10f_exif_metadata_is_no_follow_revalidated_and_passive() {
        let root = tempdir().expect("temporary directory");
        let path = root.path().join("camera.tiff");
        fs::write(&path, minimal_tiff_with_make(b"FloeCam\0")).expect("TIFF fixture");
        let metadata = fs::metadata(&path).expect("metadata");
        let state = load_advanced_metadata(&path, Some(metadata.len()), metadata.modified().ok())
            .expect("advanced metadata");
        let AdvancedMetadataState::Present(metadata) = state else {
            panic!("expected EXIF metadata");
        };
        assert_eq!(metadata.exif.expect("EXIF").fields[0].label, "Camera maker");

        let link = root.path().join("camera-link.tiff");
        symlink(&path, &link).expect("symlink");
        assert_eq!(
            load_advanced_metadata(&link, None, None),
            Err(AdvancedMetadataError::NotRegular)
        );

        let oversized = root.path().join("oversized.tiff");
        fs::write(&oversized, minimal_tiff_with_make(b"FloeCam\0")).expect("oversized TIFF header");
        fs::OpenOptions::new()
            .write(true)
            .open(&oversized)
            .expect("open oversized fixture")
            .set_len(ADVANCED_METADATA_READ_CAPACITY + 1)
            .expect("resize sparse oversized fixture");
        assert_eq!(
            load_advanced_metadata(&oversized, None, None).expect("bounded state"),
            AdvancedMetadataState::LimitExceeded
        );
    }

    #[test]
    fn phase_10f_media_metadata_handles_duration_malformed_and_changed_inputs() {
        let root = tempdir().expect("temporary directory");
        let path = root.path().join("tone.wav");
        fs::write(&path, minimal_wav(8_000)).expect("WAV fixture");
        let mut tag = Tag::new(TagType::RiffInfo);
        tag.set_artist("Floe Artist".to_owned());
        tag.set_album("Floe Album".to_owned());
        tag.set_track(3);
        tag.set_track_total(12);
        tag.save_to_path(&path, WriteOptions::default())
            .expect("write temporary tag fixture");
        let metadata = fs::metadata(&path).expect("metadata");
        let state = load_advanced_metadata(&path, Some(metadata.len()), metadata.modified().ok())
            .expect("advanced metadata");
        let AdvancedMetadataState::Present(metadata) = state else {
            panic!("expected media metadata");
        };
        let media = metadata.media.expect("media");
        assert_eq!(media.duration, Some(Duration::from_secs(1)));
        assert_eq!(media.artist.as_deref(), Some("Floe Artist"));
        assert_eq!(media.album.as_deref(), Some("Floe Album"));
        assert_eq!((media.track, media.track_total), (Some(3), Some(12)));

        let malformed = root.path().join("bad.mp3");
        fs::write(&malformed, b"not an audio file").expect("malformed fixture");
        assert!(matches!(
            load_advanced_metadata(&malformed, None, None).expect("state"),
            AdvancedMetadataState::Malformed(_)
        ));
        assert_eq!(
            load_advanced_metadata(&path, Some(metadata_size(&path) + 1), None),
            Err(AdvancedMetadataError::Changed)
        );

        let link = root.path().join("tone-link.wav");
        symlink(&path, &link).expect("media symlink");
        assert_eq!(
            load_advanced_metadata(&link, None, None),
            Err(AdvancedMetadataError::NotRegular)
        );

        let oversized = root.path().join("oversized.wav");
        fs::write(&oversized, minimal_wav(8_000)).expect("oversized WAV header");
        fs::OpenOptions::new()
            .write(true)
            .open(&oversized)
            .expect("open oversized WAV fixture")
            .set_len(ADVANCED_METADATA_READ_CAPACITY + 1)
            .expect("resize sparse oversized WAV fixture");
        assert!(matches!(
            load_advanced_metadata(&oversized, None, None).expect("bounded media state"),
            AdvancedMetadataState::Present(_) | AdvancedMetadataState::LimitExceeded
        ));
    }

    fn metadata_size(path: &Path) -> u64 {
        fs::metadata(path).expect("metadata").len()
    }
}
