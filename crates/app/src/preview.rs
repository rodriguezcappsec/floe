//! Bounded, GTK-independent Quick Preview provider orchestration.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    ffi::OsStr,
    fs::File,
    io::{Cursor, Read},
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime},
};

use floe_core::{DirectoryEntry, EntryKind};
use gtk::{gio, prelude::*};
use image::{ImageDecoder, ImageFormat, ImageReader, Limits, metadata::Orientation};
use rustix::fs::{Mode, OFlags};
use thiserror::Error;

use crate::system_thumbnailer::{SystemThumbnailerError, SystemThumbnailerRegistry};

pub const PREVIEW_QUEUE_CAPACITY: usize = 16;
pub const PREVIEW_PROVIDER_CAPACITY: usize = 32;
pub const PREVIEW_MEMORY_CACHE_CAPACITY: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreviewLimits {
    pub max_source_bytes: u64,
    pub max_output_bytes: u64,
    pub max_text_bytes: usize,
    pub max_archive_entries: usize,
    pub deadline: Duration,
}

impl Default for PreviewLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 256 * 1024 * 1024,
            max_output_bytes: 128 * 1024 * 1024,
            max_text_bytes: 2 * 1024 * 1024,
            max_archive_entries: 4_096,
            deadline: Duration::from_secs(15),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PreviewCachePolicy {
    Disabled,
    #[default]
    MemoryOnly,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PreviewKind {
    Image,
    Text,
    Document,
    Media,
    Font,
    Archive,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewTextFormat {
    Plain,
    Markdown,
    Code,
    Json,
    Xml,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewContent {
    None,
    Image {
        width: u32,
        height: u32,
        rowstride: usize,
        rgba: Arc<[u8]>,
        first_frame_only: bool,
    },
    Text {
        text: Arc<str>,
        format: PreviewTextFormat,
    },
    Document {
        width: u32,
        height: u32,
        rowstride: usize,
        rgba: Arc<[u8]>,
        content_type: Arc<str>,
        first_page_only: bool,
    },
    Media {
        path: PathBuf,
        content_type: Arc<str>,
        is_video: bool,
        poster: Option<PreviewPoster>,
    },
    Font {
        width: u32,
        height: u32,
        rowstride: usize,
        rgba: Arc<[u8]>,
        content_type: Arc<str>,
    },
    Archive {
        format: PreviewArchiveFormat,
        entries: Arc<[PreviewArchiveEntry]>,
        listing: Arc<str>,
        truncated: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewPoster {
    pub width: u32,
    pub height: u32,
    pub rowstride: usize,
    pub rgba: Arc<[u8]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewArchiveFormat {
    Zip,
    Tar,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewArchiveEntry {
    pub raw_name: Arc<[u8]>,
    pub display_name: Arc<str>,
    pub size: u64,
    pub is_directory: bool,
    pub unsafe_path: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PreviewSourceKey {
    path: PathBuf,
    size: Option<u64>,
    modified: Option<SystemTime>,
}

impl PreviewSourceKey {
    pub fn from_entry(entry: &DirectoryEntry) -> Option<Self> {
        matches!(
            entry.kind(),
            EntryKind::RegularFile
                | EntryKind::SymbolicLink {
                    target_is_directory: false
                }
        )
        .then(|| Self {
            path: entry.path().to_path_buf(),
            size: entry.size(),
            modified: entry.modified(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub const fn size(&self) -> Option<u64> {
        self.size
    }

    pub const fn modified(&self) -> Option<SystemTime> {
        self.modified
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewRequest {
    generation: u64,
    source: PreviewSourceKey,
    limits: PreviewLimits,
    cache_policy: PreviewCachePolicy,
}

impl PreviewRequest {
    pub fn new(
        generation: u64,
        source: PreviewSourceKey,
        limits: PreviewLimits,
        cache_policy: PreviewCachePolicy,
    ) -> Option<Self> {
        (generation != 0).then_some(Self {
            generation,
            source,
            limits,
            cache_policy,
        })
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn source(&self) -> &PreviewSourceKey {
        &self.source
    }

    pub const fn limits(&self) -> PreviewLimits {
        self.limits
    }

    pub const fn cache_policy(&self) -> PreviewCachePolicy {
        self.cache_policy
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewPayload {
    pub provider_id: &'static str,
    pub kind: PreviewKind,
    pub content: PreviewContent,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PreviewProviderError {
    #[error("no preview provider is available for this format")]
    Unsupported,
    #[error("preview was cancelled")]
    Cancelled,
    #[error("preview source exceeds configured limits")]
    LimitExceeded,
    #[error("preview source changed")]
    SourceChanged,
    #[error("preview provider failed: {0}")]
    Failed(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewOutcome {
    Ready(PreviewPayload),
    Unsupported,
    Cancelled,
    Failed(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewResponse {
    pub generation: u64,
    pub source: PreviewSourceKey,
    pub outcome: PreviewOutcome,
}

#[derive(Clone)]
pub struct PreviewCancellation {
    active_generation: Arc<AtomicU64>,
    generation: u64,
    started: Instant,
    deadline: Duration,
}

impl PreviewCancellation {
    pub fn is_cancelled(&self) -> bool {
        self.active_generation.load(Ordering::Acquire) != self.generation
            || self.started.elapsed() >= self.deadline
    }
}

pub trait PreviewProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn supports(&self, request: &PreviewRequest) -> bool;
    fn load(
        &self,
        request: &PreviewRequest,
        cancellation: &PreviewCancellation,
    ) -> Result<PreviewPayload, PreviewProviderError>;
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum PreviewRegistryError {
    #[error("preview provider registry is full")]
    Full,
    #[error("duplicate preview provider id: {0}")]
    Duplicate(&'static str),
}

#[derive(Default)]
pub struct PreviewProviderRegistry {
    providers: Vec<Arc<dyn PreviewProvider>>,
    ids: HashSet<&'static str>,
}

impl PreviewProviderRegistry {
    pub fn first_party() -> Self {
        let mut registry = Self::default();
        registry
            .register(Arc::new(RasterPreviewProvider))
            .expect("first-party raster provider registration is bounded and unique");
        registry
            .register(Arc::new(TextPreviewProvider))
            .expect("first-party text provider registration is bounded and unique");
        registry
            .register(Arc::new(DocumentPreviewProvider::discover()))
            .expect("first-party document provider registration is bounded and unique");
        registry
            .register(Arc::new(MediaPreviewProvider::discover()))
            .expect("first-party media provider registration is bounded and unique");
        registry
            .register(Arc::new(FontPreviewProvider::discover()))
            .expect("first-party font provider registration is bounded and unique");
        registry
            .register(Arc::new(ArchivePreviewProvider))
            .expect("first-party archive provider registration is bounded and unique");
        registry
    }

    pub fn register(
        &mut self,
        provider: Arc<dyn PreviewProvider>,
    ) -> Result<(), PreviewRegistryError> {
        if self.providers.len() == PREVIEW_PROVIDER_CAPACITY {
            return Err(PreviewRegistryError::Full);
        }
        let id = provider.id();
        if !self.ids.insert(id) {
            return Err(PreviewRegistryError::Duplicate(id));
        }
        self.providers.push(provider);
        Ok(())
    }

    fn load(&self, request: &PreviewRequest, cancellation: &PreviewCancellation) -> PreviewOutcome {
        if cancellation.is_cancelled() {
            return PreviewOutcome::Cancelled;
        }
        let mut selected = None;
        for provider in &self.providers {
            match catch_unwind(AssertUnwindSafe(|| provider.supports(request))) {
                Ok(true) => {
                    selected = Some(provider);
                    break;
                }
                Ok(false) => {}
                Err(_) => return PreviewOutcome::Failed("preview provider panicked".to_owned()),
            }
        }
        let Some(provider) = selected else {
            return PreviewOutcome::Unsupported;
        };
        match catch_unwind(AssertUnwindSafe(|| provider.load(request, cancellation))) {
            Ok(Ok(_payload)) if cancellation.is_cancelled() => PreviewOutcome::Cancelled,
            Ok(Ok(payload)) => PreviewOutcome::Ready(payload),
            Ok(Err(PreviewProviderError::Unsupported)) => PreviewOutcome::Unsupported,
            Ok(Err(PreviewProviderError::Cancelled)) => PreviewOutcome::Cancelled,
            Ok(Err(error)) => PreviewOutcome::Failed(error.to_string()),
            Err(_) => PreviewOutcome::Failed("preview provider panicked".to_owned()),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct RasterPreviewProvider;

impl PreviewProvider for RasterPreviewProvider {
    fn id(&self) -> &'static str {
        "floe.raster"
    }

    fn supports(&self, request: &PreviewRequest) -> bool {
        raster_format(request.source().path()).is_some()
    }

    fn load(
        &self,
        request: &PreviewRequest,
        cancellation: &PreviewCancellation,
    ) -> Result<PreviewPayload, PreviewProviderError> {
        let format = raster_format(request.source().path()).ok_or_else(|| {
            PreviewProviderError::Failed("unsupported raster image format".to_owned())
        })?;
        let mut source = open_verified_source(request)?;
        let encoded = read_bounded_source(&mut source, request, cancellation)?;
        revalidate_source(&source, request.source())?;
        if cancellation.is_cancelled() {
            return Err(PreviewProviderError::Cancelled);
        }

        let mut reader = ImageReader::with_format(Cursor::new(encoded), format);
        let mut limits = Limits::default();
        limits.max_image_width = Some(65_535);
        limits.max_image_height = Some(65_535);
        limits.max_alloc = Some(request.limits().max_output_bytes);
        reader.limits(limits);
        let mut decoder = reader.into_decoder().map_err(map_image_error)?;
        let (width, height) = decoder.dimensions();
        if width == 0 || height == 0 || decoder.total_bytes() > request.limits().max_output_bytes {
            return Err(PreviewProviderError::LimitExceeded);
        }
        let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
        let mut decoded = image::DynamicImage::from_decoder(decoder).map_err(map_image_error)?;
        decoded.apply_orientation(orientation);
        let rgba = decoded.into_rgba8();
        let width = rgba.width();
        let height = rgba.height();
        let rowstride = usize::try_from(width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or(PreviewProviderError::LimitExceeded)?;
        let pixels = rgba.into_raw();
        if pixels.len() as u64 > request.limits().max_output_bytes {
            return Err(PreviewProviderError::LimitExceeded);
        }

        Ok(PreviewPayload {
            provider_id: self.id(),
            kind: PreviewKind::Image,
            content: PreviewContent::Image {
                width,
                height,
                rowstride,
                rgba: pixels.into(),
                first_frame_only: matches!(format, ImageFormat::Gif | ImageFormat::WebP),
            },
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct TextPreviewProvider;

impl PreviewProvider for TextPreviewProvider {
    fn id(&self) -> &'static str {
        "floe.text"
    }

    fn supports(&self, request: &PreviewRequest) -> bool {
        text_format(request.source().path()).is_some()
    }

    fn load(
        &self,
        request: &PreviewRequest,
        cancellation: &PreviewCancellation,
    ) -> Result<PreviewPayload, PreviewProviderError> {
        let format = text_format(request.source().path()).ok_or_else(|| {
            PreviewProviderError::Failed("unsupported passive text format".to_owned())
        })?;
        let mut source = open_verified_source(request)?;
        let encoded = read_bounded_source(&mut source, request, cancellation)?;
        revalidate_source(&source, request.source())?;
        if encoded.len() > request.limits().max_text_bytes {
            return Err(PreviewProviderError::LimitExceeded);
        }
        let text = decode_passive_text(&encoded)?;
        if cancellation.is_cancelled() {
            return Err(PreviewProviderError::Cancelled);
        }
        Ok(PreviewPayload {
            provider_id: self.id(),
            kind: PreviewKind::Text,
            content: PreviewContent::Text {
                text: Arc::from(text),
                format,
            },
        })
    }
}

#[derive(Clone, Debug)]
struct DocumentPreviewProvider {
    providers: SystemThumbnailerRegistry,
}

impl DocumentPreviewProvider {
    fn discover() -> Self {
        Self {
            providers: SystemThumbnailerRegistry::discover(),
        }
    }

    #[cfg(test)]
    fn discover_from_data_dirs(data_dirs: &[PathBuf]) -> Self {
        Self {
            providers: SystemThumbnailerRegistry::discover_from_data_dirs(data_dirs),
        }
    }
}

impl PreviewProvider for DocumentPreviewProvider {
    fn id(&self) -> &'static str {
        "floe.document"
    }

    fn supports(&self, request: &PreviewRequest) -> bool {
        document_extension_allowed(request.source().path())
    }

    fn load(
        &self,
        request: &PreviewRequest,
        cancellation: &PreviewCancellation,
    ) -> Result<PreviewPayload, PreviewProviderError> {
        if !document_extension_allowed(request.source().path()) {
            return Err(PreviewProviderError::Failed(
                "unsupported document format".to_owned(),
            ));
        }
        let source = open_verified_source(request)?;
        drop(source);
        self.providers
            .supports_path(request.source().path())
            .map_err(map_thumbnailer_error)?;
        let output = self
            .providers
            .generate(request.source().path(), 1_024, || {
                cancellation.is_cancelled()
            })
            .map_err(map_thumbnailer_error)?;
        if output.bytes.len() as u64 > request.limits().max_output_bytes {
            return Err(PreviewProviderError::LimitExceeded);
        }
        let source = open_verified_source(request)?;
        revalidate_source(&source, request.source())?;
        if cancellation.is_cancelled() {
            return Err(PreviewProviderError::Cancelled);
        }
        let decoded = decode_rgba(
            output.bytes,
            ImageFormat::Png,
            request.limits().max_output_bytes,
        )?;
        Ok(PreviewPayload {
            provider_id: self.id(),
            kind: PreviewKind::Document,
            content: PreviewContent::Document {
                width: decoded.width,
                height: decoded.height,
                rowstride: decoded.rowstride,
                rgba: decoded.rgba,
                content_type: Arc::from(output.content_type),
                first_page_only: true,
            },
        })
    }
}

#[derive(Clone, Debug)]
struct MediaPreviewProvider {
    poster_providers: SystemThumbnailerRegistry,
}

impl MediaPreviewProvider {
    fn discover() -> Self {
        Self {
            poster_providers: SystemThumbnailerRegistry::discover(),
        }
    }

    #[cfg(test)]
    fn discover_from_data_dirs(data_dirs: &[PathBuf]) -> Self {
        Self {
            poster_providers: SystemThumbnailerRegistry::discover_from_data_dirs(data_dirs),
        }
    }
}

impl PreviewProvider for MediaPreviewProvider {
    fn id(&self) -> &'static str {
        "floe.media"
    }

    fn supports(&self, request: &PreviewRequest) -> bool {
        media_extension_kind(request.source().path()).is_some()
    }

    fn load(
        &self,
        request: &PreviewRequest,
        cancellation: &PreviewCancellation,
    ) -> Result<PreviewPayload, PreviewProviderError> {
        let extension_is_video = media_extension_kind(request.source().path())
            .ok_or(PreviewProviderError::Unsupported)?;
        let source = open_verified_source(request)?;
        let content_type = content_type_no_follow(request.source().path())?;
        let mime_is_video = content_type.starts_with("video/");
        let mime_is_audio = content_type.starts_with("audio/");
        if (!mime_is_video && !mime_is_audio) || mime_is_video != extension_is_video {
            return Err(PreviewProviderError::Unsupported);
        }
        drop(source);

        let poster = if mime_is_video {
            match self
                .poster_providers
                .generate(request.source().path(), 768, || cancellation.is_cancelled())
            {
                Ok(output) if output.bytes.len() as u64 <= request.limits().max_output_bytes => {
                    decode_rgba(
                        output.bytes,
                        ImageFormat::Png,
                        request.limits().max_output_bytes,
                    )
                    .ok()
                    .map(|decoded| PreviewPoster {
                        width: decoded.width,
                        height: decoded.height,
                        rowstride: decoded.rowstride,
                        rgba: decoded.rgba,
                    })
                }
                Err(SystemThumbnailerError::Cancelled) => {
                    return Err(PreviewProviderError::Cancelled);
                }
                Ok(_) | Err(_) => None,
            }
        } else {
            None
        };
        let source = open_verified_source(request)?;
        revalidate_source(&source, request.source())?;
        if cancellation.is_cancelled() {
            return Err(PreviewProviderError::Cancelled);
        }

        Ok(PreviewPayload {
            provider_id: self.id(),
            kind: PreviewKind::Media,
            content: PreviewContent::Media {
                path: request.source().path().to_path_buf(),
                content_type: Arc::from(content_type),
                is_video: mime_is_video,
                poster,
            },
        })
    }
}

#[derive(Clone, Debug)]
struct FontPreviewProvider {
    providers: SystemThumbnailerRegistry,
}

impl FontPreviewProvider {
    fn discover() -> Self {
        Self {
            providers: SystemThumbnailerRegistry::discover(),
        }
    }

    #[cfg(test)]
    fn discover_from_data_dirs(data_dirs: &[PathBuf]) -> Self {
        Self {
            providers: SystemThumbnailerRegistry::discover_from_data_dirs(data_dirs),
        }
    }
}

impl PreviewProvider for FontPreviewProvider {
    fn id(&self) -> &'static str {
        "floe.font"
    }

    fn supports(&self, request: &PreviewRequest) -> bool {
        font_extension_allowed(request.source().path())
    }

    fn load(
        &self,
        request: &PreviewRequest,
        cancellation: &PreviewCancellation,
    ) -> Result<PreviewPayload, PreviewProviderError> {
        if !font_extension_allowed(request.source().path()) {
            return Err(PreviewProviderError::Unsupported);
        }
        let source = open_verified_source(request)?;
        drop(source);
        self.providers
            .supports_path(request.source().path())
            .map_err(map_thumbnailer_error)?;
        let output = self
            .providers
            .generate(request.source().path(), 1_024, || {
                cancellation.is_cancelled()
            })
            .map_err(map_thumbnailer_error)?;
        if output.bytes.len() as u64 > request.limits().max_output_bytes {
            return Err(PreviewProviderError::LimitExceeded);
        }
        let source = open_verified_source(request)?;
        revalidate_source(&source, request.source())?;
        let decoded = decode_rgba(
            output.bytes,
            ImageFormat::Png,
            request.limits().max_output_bytes,
        )?;
        Ok(PreviewPayload {
            provider_id: self.id(),
            kind: PreviewKind::Font,
            content: PreviewContent::Font {
                width: decoded.width,
                height: decoded.height,
                rowstride: decoded.rowstride,
                rgba: decoded.rgba,
                content_type: Arc::from(output.content_type),
            },
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct ArchivePreviewProvider;

impl PreviewProvider for ArchivePreviewProvider {
    fn id(&self) -> &'static str {
        "floe.archive"
    }

    fn supports(&self, request: &PreviewRequest) -> bool {
        archive_format(request.source().path()).is_some()
    }

    fn load(
        &self,
        request: &PreviewRequest,
        cancellation: &PreviewCancellation,
    ) -> Result<PreviewPayload, PreviewProviderError> {
        let format =
            archive_format(request.source().path()).ok_or(PreviewProviderError::Unsupported)?;
        let mut source = open_verified_source(request)?;
        let encoded = read_bounded_source(&mut source, request, cancellation)?;
        revalidate_source(&source, request.source())?;
        let (entries, mut truncated) = match format {
            PreviewArchiveFormat::Zip => {
                parse_zip_listing(&encoded, request.limits().max_archive_entries, cancellation)?
            }
            PreviewArchiveFormat::Tar => {
                parse_tar_listing(&encoded, request.limits().max_archive_entries, cancellation)?
            }
        };
        let listing =
            archive_listing_text(&entries, request.limits().max_text_bytes, &mut truncated);
        Ok(PreviewPayload {
            provider_id: self.id(),
            kind: PreviewKind::Archive,
            content: PreviewContent::Archive {
                format,
                entries: entries.into(),
                listing: Arc::from(listing),
                truncated,
            },
        })
    }
}

fn raster_format(path: &Path) -> Option<ImageFormat> {
    let extension = path.extension()?.to_str()?;
    if extension.eq_ignore_ascii_case("png") {
        Some(ImageFormat::Png)
    } else if extension.eq_ignore_ascii_case("jpg") || extension.eq_ignore_ascii_case("jpeg") {
        Some(ImageFormat::Jpeg)
    } else if extension.eq_ignore_ascii_case("webp") {
        Some(ImageFormat::WebP)
    } else if extension.eq_ignore_ascii_case("gif") {
        Some(ImageFormat::Gif)
    } else if extension.eq_ignore_ascii_case("bmp") {
        Some(ImageFormat::Bmp)
    } else if extension.eq_ignore_ascii_case("tif") || extension.eq_ignore_ascii_case("tiff") {
        Some(ImageFormat::Tiff)
    } else if extension.eq_ignore_ascii_case("ico") {
        Some(ImageFormat::Ico)
    } else {
        None
    }
}

fn map_image_error(error: image::ImageError) -> PreviewProviderError {
    match error {
        image::ImageError::Limits(_) => PreviewProviderError::LimitExceeded,
        error => PreviewProviderError::Failed(error.to_string()),
    }
}

fn map_thumbnailer_error(error: SystemThumbnailerError) -> PreviewProviderError {
    match error {
        SystemThumbnailerError::Unsupported => PreviewProviderError::Unsupported,
        SystemThumbnailerError::Cancelled => PreviewProviderError::Cancelled,
        SystemThumbnailerError::OutputTooLarge => PreviewProviderError::LimitExceeded,
        error => PreviewProviderError::Failed(error.to_string()),
    }
}

fn document_extension_allowed(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(OsStr::to_str) else {
        return false;
    };
    [
        "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "odg", "odf",
        "rtf",
    ]
    .iter()
    .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

fn media_extension_kind(path: &Path) -> Option<bool> {
    let extension = path.extension()?.to_str()?;
    if [
        "mp4", "m4v", "mkv", "webm", "mov", "avi", "mpeg", "mpg", "ogv", "flv", "wmv",
    ]
    .iter()
    .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    {
        return Some(true);
    }
    [
        "mp3", "flac", "ogg", "oga", "opus", "wav", "m4a", "aac", "wma", "aiff", "aif",
    ]
    .iter()
    .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    .then_some(false)
}

fn font_extension_allowed(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(OsStr::to_str) else {
        return false;
    };
    ["ttf", "otf", "ttc", "otc", "woff", "woff2"]
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

fn archive_format(path: &Path) -> Option<PreviewArchiveFormat> {
    let extension = path.extension()?.to_str()?;
    if extension.eq_ignore_ascii_case("zip") {
        Some(PreviewArchiveFormat::Zip)
    } else if extension.eq_ignore_ascii_case("tar") {
        Some(PreviewArchiveFormat::Tar)
    } else {
        None
    }
}

fn parse_zip_listing(
    encoded: &[u8],
    maximum_entries: usize,
    cancellation: &PreviewCancellation,
) -> Result<(Vec<PreviewArchiveEntry>, bool), PreviewProviderError> {
    const HEADER: &[u8; 4] = b"PK\x01\x02";
    const FIXED: usize = 46;
    const MAX_NAME: usize = 4_096;
    let mut entries = Vec::new();
    let mut cursor = 0_usize;
    let mut found_header = false;
    let mut truncated = false;
    while cursor.saturating_add(FIXED) <= encoded.len() {
        if cancellation.is_cancelled() {
            return Err(PreviewProviderError::Cancelled);
        }
        let Some(relative) = encoded[cursor..]
            .windows(4)
            .position(|window| window == HEADER)
        else {
            break;
        };
        cursor = cursor.saturating_add(relative);
        found_header = true;
        let fixed = encoded
            .get(cursor..cursor + FIXED)
            .ok_or_else(|| PreviewProviderError::Failed("truncated ZIP directory".to_owned()))?;
        let name_len = usize::from(u16::from_le_bytes([fixed[28], fixed[29]]));
        let extra_len = usize::from(u16::from_le_bytes([fixed[30], fixed[31]]));
        let comment_len = usize::from(u16::from_le_bytes([fixed[32], fixed[33]]));
        if name_len == 0 || name_len > MAX_NAME {
            return Err(PreviewProviderError::LimitExceeded);
        }
        let record_len = FIXED
            .checked_add(name_len)
            .and_then(|length| length.checked_add(extra_len))
            .and_then(|length| length.checked_add(comment_len))
            .ok_or(PreviewProviderError::LimitExceeded)?;
        let record = encoded
            .get(cursor..cursor + record_len)
            .ok_or_else(|| PreviewProviderError::Failed("truncated ZIP directory".to_owned()))?;
        if entries.len() == maximum_entries {
            truncated = true;
            break;
        }
        let raw_name = &record[FIXED..FIXED + name_len];
        let size = u64::from(u32::from_le_bytes([
            fixed[24], fixed[25], fixed[26], fixed[27],
        ]));
        entries.push(archive_entry(raw_name, size, raw_name.ends_with(b"/")));
        cursor = cursor
            .checked_add(record_len)
            .ok_or(PreviewProviderError::LimitExceeded)?;
    }
    if !found_header {
        return Err(PreviewProviderError::Failed(
            "ZIP central directory is missing or unsupported".to_owned(),
        ));
    }
    Ok((entries, truncated))
}

fn parse_tar_listing(
    encoded: &[u8],
    maximum_entries: usize,
    cancellation: &PreviewCancellation,
) -> Result<(Vec<PreviewArchiveEntry>, bool), PreviewProviderError> {
    const BLOCK: usize = 512;
    let mut entries = Vec::new();
    let mut cursor = 0_usize;
    let mut truncated = false;
    while let Some(header) = encoded.get(cursor..cursor.saturating_add(BLOCK)) {
        if cancellation.is_cancelled() {
            return Err(PreviewProviderError::Cancelled);
        }
        if header.iter().all(|byte| *byte == 0) {
            return Ok((entries, truncated));
        }
        if &header[257..262] != b"ustar" {
            return Err(PreviewProviderError::Failed(
                "unsupported or malformed TAR header".to_owned(),
            ));
        }
        let stored_checksum = parse_tar_octal(&header[148..156])?;
        let actual_checksum = header
            .iter()
            .enumerate()
            .map(|(index, byte)| {
                if (148..156).contains(&index) {
                    u64::from(b' ')
                } else {
                    u64::from(*byte)
                }
            })
            .sum::<u64>();
        if stored_checksum != actual_checksum {
            return Err(PreviewProviderError::Failed(
                "TAR header checksum is invalid".to_owned(),
            ));
        }
        if entries.len() == maximum_entries {
            truncated = true;
            return Ok((entries, truncated));
        }
        let name = trim_nul(&header[0..100]);
        let prefix = trim_nul(&header[345..500]);
        let raw_name = if prefix.is_empty() {
            name.to_vec()
        } else {
            [prefix, b"/", name].concat()
        };
        if raw_name.is_empty() || raw_name.len() > 4_096 {
            return Err(PreviewProviderError::LimitExceeded);
        }
        let size = parse_tar_octal(&header[124..136])?;
        let is_directory = header[156] == b'5' || raw_name.ends_with(b"/");
        entries.push(archive_entry(&raw_name, size, is_directory));
        let data_blocks = usize::try_from(size)
            .map_err(|_| PreviewProviderError::LimitExceeded)?
            .checked_add(BLOCK - 1)
            .ok_or(PreviewProviderError::LimitExceeded)?
            / BLOCK;
        cursor = cursor
            .checked_add(BLOCK)
            .and_then(|value| value.checked_add(data_blocks.checked_mul(BLOCK)?))
            .ok_or(PreviewProviderError::LimitExceeded)?;
        if cursor > encoded.len() {
            return Err(PreviewProviderError::Failed(
                "truncated TAR entry".to_owned(),
            ));
        }
    }
    Err(PreviewProviderError::Failed(
        "TAR end marker is missing".to_owned(),
    ))
}

fn trim_nul(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    &bytes[..end]
}

fn parse_tar_octal(bytes: &[u8]) -> Result<u64, PreviewProviderError> {
    let text = std::str::from_utf8(trim_nul(bytes))
        .map_err(|_| PreviewProviderError::Failed("invalid TAR size".to_owned()))?
        .trim();
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(text, 8)
        .map_err(|_| PreviewProviderError::Failed("invalid TAR size".to_owned()))
}

fn archive_entry(raw_name: &[u8], size: u64, is_directory: bool) -> PreviewArchiveEntry {
    PreviewArchiveEntry {
        raw_name: Arc::from(raw_name),
        display_name: Arc::from(String::from_utf8_lossy(raw_name).into_owned()),
        size,
        is_directory,
        unsafe_path: archive_path_is_unsafe(raw_name),
    }
}

fn archive_listing_text(
    entries: &[PreviewArchiveEntry],
    maximum_bytes: usize,
    truncated: &mut bool,
) -> String {
    let mut listing = String::new();
    for entry in entries {
        let warning = if entry.unsafe_path {
            "[unsafe path] "
        } else {
            ""
        };
        let kind = if entry.is_directory {
            "directory"
        } else {
            "file"
        };
        let line = format!(
            "{warning}{}\t{kind}\t{} bytes\n",
            entry.display_name, entry.size
        );
        if listing.len().saturating_add(line.len()) > maximum_bytes {
            *truncated = true;
            break;
        }
        listing.push_str(&line);
    }
    listing
}

fn archive_path_is_unsafe(raw_name: &[u8]) -> bool {
    raw_name.starts_with(b"/")
        || raw_name.starts_with(b"\\")
        || raw_name.get(1) == Some(&b':')
        || raw_name
            .split(|byte| matches!(byte, b'/' | b'\\'))
            .any(|component| component == b"..")
}

fn content_type_no_follow(path: &Path) -> Result<String, PreviewProviderError> {
    let info = gio::File::for_path(path)
        .query_info(
            "standard::content-type",
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            None::<&gio::Cancellable>,
        )
        .map_err(|error| PreviewProviderError::Failed(error.to_string()))?;
    info.content_type()
        .map(|content_type| content_type.to_string())
        .ok_or(PreviewProviderError::Unsupported)
}

struct DecodedRgba {
    width: u32,
    height: u32,
    rowstride: usize,
    rgba: Arc<[u8]>,
}

fn decode_rgba(
    encoded: Vec<u8>,
    format: ImageFormat,
    max_output_bytes: u64,
) -> Result<DecodedRgba, PreviewProviderError> {
    let mut reader = ImageReader::with_format(Cursor::new(encoded), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(65_535);
    limits.max_image_height = Some(65_535);
    limits.max_alloc = Some(max_output_bytes);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(map_image_error)?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || decoder.total_bytes() > max_output_bytes {
        return Err(PreviewProviderError::LimitExceeded);
    }
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut decoded = image::DynamicImage::from_decoder(decoder).map_err(map_image_error)?;
    decoded.apply_orientation(orientation);
    let rgba = decoded.into_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    let rowstride = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or(PreviewProviderError::LimitExceeded)?;
    let pixels = rgba.into_raw();
    if pixels.len() as u64 > max_output_bytes {
        return Err(PreviewProviderError::LimitExceeded);
    }
    Ok(DecodedRgba {
        width,
        height,
        rowstride,
        rgba: pixels.into(),
    })
}

fn text_format(path: &Path) -> Option<PreviewTextFormat> {
    let extension = path.extension()?.to_str()?;
    if ["md", "markdown", "mdown", "mkd"]
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    {
        return Some(PreviewTextFormat::Markdown);
    }
    if extension.eq_ignore_ascii_case("json") {
        return Some(PreviewTextFormat::Json);
    }
    if ["xml", "xsd", "xsl", "xslt"]
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    {
        return Some(PreviewTextFormat::Xml);
    }
    if [
        "c", "cc", "cpp", "cxx", "h", "hpp", "rs", "go", "java", "kt", "kts", "py", "rb", "php",
        "swift", "js", "mjs", "cjs", "ts", "tsx", "jsx", "css", "scss", "sass", "less", "sh",
        "bash", "zsh", "fish", "nu", "lua", "pl", "r", "sql", "toml", "yaml", "yml", "ini", "conf",
        "cfg", "desktop", "service", "gradle", "cmake", "makefile",
    ]
    .iter()
    .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    {
        return Some(PreviewTextFormat::Code);
    }
    ["txt", "text", "log", "csv", "tsv", "nfo", "readme"]
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        .then_some(PreviewTextFormat::Plain)
}

fn open_verified_source(request: &PreviewRequest) -> Result<File, PreviewProviderError> {
    let descriptor = rustix::fs::open(
        request.source().path(),
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|error| PreviewProviderError::Failed(error.to_string()))?;
    let source = File::from(descriptor);
    revalidate_source(&source, request.source())?;
    let metadata = source
        .metadata()
        .map_err(|error| PreviewProviderError::Failed(error.to_string()))?;
    if !metadata.is_file() {
        return Err(PreviewProviderError::Failed(
            "preview source is not a regular file".to_owned(),
        ));
    }
    if metadata.len() > request.limits().max_source_bytes {
        return Err(PreviewProviderError::LimitExceeded);
    }
    Ok(source)
}

fn read_bounded_source(
    source: &mut File,
    request: &PreviewRequest,
    cancellation: &PreviewCancellation,
) -> Result<Vec<u8>, PreviewProviderError> {
    if cancellation.is_cancelled() {
        return Err(PreviewProviderError::Cancelled);
    }
    let maximum = request.limits().max_source_bytes;
    let mut encoded = Vec::with_capacity(
        source
            .metadata()
            .map(|metadata| metadata.len().min(maximum) as usize)
            .unwrap_or_default(),
    );
    source
        .take(maximum.saturating_add(1))
        .read_to_end(&mut encoded)
        .map_err(|error| PreviewProviderError::Failed(error.to_string()))?;
    if encoded.len() as u64 > maximum {
        return Err(PreviewProviderError::LimitExceeded);
    }
    Ok(encoded)
}

fn revalidate_source(source: &File, key: &PreviewSourceKey) -> Result<(), PreviewProviderError> {
    let metadata = source
        .metadata()
        .map_err(|error| PreviewProviderError::Failed(error.to_string()))?;
    if key.size() != Some(metadata.len()) || key.modified() != metadata.modified().ok() {
        return Err(PreviewProviderError::SourceChanged);
    }
    Ok(())
}

fn decode_passive_text(encoded: &[u8]) -> Result<String, PreviewProviderError> {
    if let Some(body) = encoded.strip_prefix(&[0xff, 0xfe]) {
        if body.len() % 2 != 0 {
            return Err(PreviewProviderError::Failed(
                "malformed UTF-16LE text".to_owned(),
            ));
        }
        let units = body
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16(&units)
            .map_err(|_| PreviewProviderError::Failed("malformed UTF-16LE text".to_owned()));
    }
    if let Some(body) = encoded.strip_prefix(&[0xfe, 0xff]) {
        if body.len() % 2 != 0 {
            return Err(PreviewProviderError::Failed(
                "malformed UTF-16BE text".to_owned(),
            ));
        }
        let units = body
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16(&units)
            .map_err(|_| PreviewProviderError::Failed("malformed UTF-16BE text".to_owned()));
    }
    let body = encoded.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(encoded);
    if body.contains(&0) {
        return Err(PreviewProviderError::Failed(
            "binary content is not shown as text".to_owned(),
        ));
    }
    std::str::from_utf8(body)
        .map(str::to_owned)
        .map_err(|_| PreviewProviderError::Failed("text is not valid UTF-8".to_owned()))
}

#[derive(Debug, Error)]
pub enum PreviewSubmitError {
    #[error("preview queue is full")]
    Full(PreviewRequest),
    #[error("preview worker disconnected")]
    Disconnected,
    #[error("preview request generation is stale")]
    Stale(PreviewRequest),
}

pub struct PreviewWorker {
    sender: Option<SyncSender<PreviewRequest>>,
    receiver: Receiver<PreviewResponse>,
    active_generation: Arc<AtomicU64>,
    cache_epoch: Arc<AtomicU64>,
    worker: Option<JoinHandle<()>>,
}

impl PreviewWorker {
    pub fn spawn(registry: PreviewProviderRegistry) -> std::io::Result<Self> {
        Self::spawn_internal(registry, PREVIEW_QUEUE_CAPACITY, None)
    }

    fn spawn_internal(
        registry: PreviewProviderRegistry,
        capacity: usize,
        start_gate: Option<Receiver<()>>,
    ) -> std::io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<PreviewRequest>(capacity);
        let (responses, receiver) = mpsc::channel();
        let active_generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&active_generation);
        let cache_epoch = Arc::new(AtomicU64::new(0));
        let worker_cache_epoch = Arc::clone(&cache_epoch);
        let worker = thread::Builder::new()
            .name("floe-preview".to_owned())
            .spawn(move || {
                if let Some(gate) = start_gate
                    && gate.recv().is_err()
                {
                    return;
                }
                let mut cache = PreviewMemoryCache::default();
                let mut observed_cache_epoch = 0;
                while let Ok(request) = requests.recv() {
                    let current_cache_epoch = worker_cache_epoch.load(Ordering::Acquire);
                    if current_cache_epoch != observed_cache_epoch {
                        cache = PreviewMemoryCache::default();
                        observed_cache_epoch = current_cache_epoch;
                    }
                    let cancellation = PreviewCancellation {
                        active_generation: Arc::clone(&worker_generation),
                        generation: request.generation,
                        started: Instant::now(),
                        deadline: request.limits.deadline,
                    };
                    let outcome = if cancellation.is_cancelled() {
                        PreviewOutcome::Cancelled
                    } else if request.cache_policy == PreviewCachePolicy::MemoryOnly {
                        cache.get(&request.source).map_or_else(
                            || {
                                let outcome = registry.load(&request, &cancellation);
                                if let PreviewOutcome::Ready(payload) = &outcome {
                                    cache.insert(request.source.clone(), payload.clone());
                                }
                                outcome
                            },
                            PreviewOutcome::Ready,
                        )
                    } else {
                        registry.load(&request, &cancellation)
                    };
                    if responses
                        .send(PreviewResponse {
                            generation: request.generation,
                            source: request.source,
                            outcome,
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            receiver,
            active_generation,
            cache_epoch,
            worker: Some(worker),
        })
    }

    pub fn begin_generation(&self) -> u64 {
        let mut current = self.active_generation.load(Ordering::Acquire);
        loop {
            let next = current.wrapping_add(1).max(1);
            match self.active_generation.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return next,
                Err(observed) => current = observed,
            }
        }
    }

    pub fn cancel(&self) {
        let _ = self.begin_generation();
    }

    pub fn clear_memory_cache(&self) {
        self.cancel();
        self.cache_epoch.fetch_add(1, Ordering::AcqRel);
    }

    pub fn submit(&self, request: PreviewRequest) -> Result<(), PreviewSubmitError> {
        if request.generation != self.active_generation.load(Ordering::Acquire) {
            return Err(PreviewSubmitError::Stale(request));
        }
        let Some(sender) = self.sender.as_ref() else {
            return Err(PreviewSubmitError::Disconnected);
        };
        match sender.try_send(request) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(request)) => Err(PreviewSubmitError::Full(request)),
            Err(TrySendError::Disconnected(_)) => Err(PreviewSubmitError::Disconnected),
        }
    }

    pub fn try_response(&self) -> Option<PreviewResponse> {
        self.receiver.try_recv().ok()
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.active_generation.load(Ordering::Acquire) == generation
    }
}

impl Drop for PreviewWorker {
    fn drop(&mut self) {
        self.cancel();
        self.sender.take();
        // Providers can be blocked in file or child-process I/O. Cooperative
        // cancellation plus detachment keeps window destruction nonblocking.
        self.worker.take();
    }
}

#[derive(Default)]
struct PreviewMemoryCache {
    values: HashMap<PreviewSourceKey, PreviewPayload>,
    order: VecDeque<PreviewSourceKey>,
}

impl PreviewMemoryCache {
    fn get(&self, key: &PreviewSourceKey) -> Option<PreviewPayload> {
        self.values.get(key).cloned()
    }

    fn insert(&mut self, key: PreviewSourceKey, payload: PreviewPayload) {
        self.order.retain(|candidate| candidate != &key);
        self.order.push_back(key.clone());
        self.values.insert(key, payload);
        while self.values.len() > PREVIEW_MEMORY_CACHE_CAPACITY {
            if let Some(expired) = self.order.pop_front() {
                self.values.remove(&expired);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::ffi::{OsStrExt, OsStringExt},
        sync::atomic::{AtomicUsize, Ordering},
    };

    use floe_core::enumerate_directory;
    use tempfile::tempdir;

    use super::*;

    struct TestProvider {
        id: &'static str,
        calls: Arc<AtomicUsize>,
        wait_for_cancel: bool,
        fail: bool,
    }

    impl PreviewProvider for TestProvider {
        fn id(&self) -> &'static str {
            self.id
        }

        fn supports(&self, _: &PreviewRequest) -> bool {
            true
        }

        fn load(
            &self,
            _: &PreviewRequest,
            cancellation: &PreviewCancellation,
        ) -> Result<PreviewPayload, PreviewProviderError> {
            self.calls.fetch_add(1, Ordering::AcqRel);
            while self.wait_for_cancel && !cancellation.is_cancelled() {
                thread::yield_now();
            }
            if cancellation.is_cancelled() {
                return Err(PreviewProviderError::Cancelled);
            }
            if self.fail {
                return Err(PreviewProviderError::Failed("fixture".to_owned()));
            }
            Ok(PreviewPayload {
                provider_id: self.id,
                kind: PreviewKind::Unknown,
                content: PreviewContent::None,
            })
        }
    }

    fn source_fixture() -> (tempfile::TempDir, PreviewSourceKey) {
        let root = tempdir().expect("root");
        let path = root
            .path()
            .join(std::ffi::OsString::from_vec(b"raw-\xff".to_vec()));
        fs::write(&path, b"preview").expect("fixture");
        let entry = enumerate_directory(root.path())
            .expect("listing")
            .into_entries()
            .remove(0);
        (root, PreviewSourceKey::from_entry(&entry).expect("source"))
    }

    fn request(generation: u64, source: PreviewSourceKey) -> PreviewRequest {
        PreviewRequest::new(
            generation,
            source,
            PreviewLimits::default(),
            PreviewCachePolicy::MemoryOnly,
        )
        .expect("request")
    }

    #[test]
    fn phase_9a_contract_preserves_raw_identity_limits_order_and_fallback() {
        let (_root, source) = source_fixture();
        assert_eq!(source.path().as_os_str().as_bytes().last(), Some(&0xff));
        assert_eq!(
            PreviewCachePolicy::default(),
            PreviewCachePolicy::MemoryOnly
        );
        assert_eq!(PreviewLimits::default().max_archive_entries, 4_096);
        assert!(
            PreviewRequest::new(
                0,
                source.clone(),
                PreviewLimits::default(),
                PreviewCachePolicy::Disabled
            )
            .is_none()
        );
        let registry = PreviewProviderRegistry::default();
        let active = Arc::new(AtomicU64::new(1));
        let outcome = registry.load(
            &request(1, source),
            &PreviewCancellation {
                active_generation: active,
                generation: 1,
                started: Instant::now(),
                deadline: Duration::from_secs(1),
            },
        );
        assert_eq!(outcome, PreviewOutcome::Unsupported);

        let calls = Arc::new(AtomicUsize::new(0));
        let mut ordered = PreviewProviderRegistry::default();
        ordered
            .register(Arc::new(TestProvider {
                id: "first",
                calls: Arc::clone(&calls),
                wait_for_cancel: false,
                fail: false,
            }))
            .expect("first provider");
        ordered
            .register(Arc::new(TestProvider {
                id: "second",
                calls: Arc::clone(&calls),
                wait_for_cancel: false,
                fail: false,
            }))
            .expect("second provider");
        assert!(matches!(
            ordered.register(Arc::new(TestProvider {
                id: "first",
                calls: Arc::clone(&calls),
                wait_for_cancel: false,
                fail: false,
            })),
            Err(PreviewRegistryError::Duplicate("first"))
        ));
        let active = Arc::new(AtomicU64::new(2));
        let outcome = ordered.load(
            &request(2, source_fixture().1),
            &PreviewCancellation {
                active_generation: active,
                generation: 2,
                started: Instant::now(),
                deadline: Duration::from_secs(1),
            },
        );
        assert!(matches!(
            outcome,
            PreviewOutcome::Ready(PreviewPayload {
                provider_id: "first",
                ..
            })
        ));
        assert_eq!(calls.load(Ordering::Acquire), 1);
    }

    #[test]
    fn phase_9a_worker_cancels_stale_work_bounds_queue_and_caches_memory_only() {
        let (_root, source) = source_fixture();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = PreviewProviderRegistry::default();
        registry
            .register(Arc::new(TestProvider {
                id: "test",
                calls: Arc::clone(&calls),
                wait_for_cancel: false,
                fail: false,
            }))
            .expect("provider");
        let worker = PreviewWorker::spawn(registry).expect("worker");
        let generation = worker.begin_generation();
        worker
            .submit(request(generation, source.clone()))
            .expect("first");
        let first = loop {
            if let Some(response) = worker.try_response() {
                break response;
            }
            thread::yield_now();
        };
        assert!(matches!(first.outcome, PreviewOutcome::Ready(_)));
        let generation = worker.begin_generation();
        worker
            .submit(request(generation, source.clone()))
            .expect("cached");
        while worker.try_response().is_none() {
            thread::yield_now();
        }
        assert_eq!(calls.load(Ordering::Acquire), 1);

        let (gate_send, gate_receive) = mpsc::channel();
        let blocked = PreviewWorker::spawn_internal(
            PreviewProviderRegistry::default(),
            1,
            Some(gate_receive),
        )
        .expect("blocked worker");
        let generation = blocked.begin_generation();
        blocked
            .submit(request(generation, source.clone()))
            .expect("queued");
        assert!(matches!(
            blocked.submit(request(generation, source)),
            Err(PreviewSubmitError::Full(_))
        ));
        drop(gate_send);

        let (_root, source) = source_fixture();
        let cancel_calls = Arc::new(AtomicUsize::new(0));
        let mut registry = PreviewProviderRegistry::default();
        registry
            .register(Arc::new(TestProvider {
                id: "cancel",
                calls: Arc::clone(&cancel_calls),
                wait_for_cancel: true,
                fail: false,
            }))
            .expect("cancel provider");
        let cancelling = PreviewWorker::spawn(registry).expect("cancel worker");
        let generation = cancelling.begin_generation();
        cancelling
            .submit(request(generation, source))
            .expect("cancel request");
        while cancel_calls.load(Ordering::Acquire) == 0 {
            thread::yield_now();
        }
        cancelling.cancel();
        let cancelled = loop {
            if let Some(response) = cancelling.try_response() {
                break response;
            }
            thread::yield_now();
        };
        assert_eq!(cancelled.outcome, PreviewOutcome::Cancelled);

        let (_root, source) = source_fixture();
        let mut registry = PreviewProviderRegistry::default();
        registry
            .register(Arc::new(TestProvider {
                id: "failure",
                calls: Arc::new(AtomicUsize::new(0)),
                wait_for_cancel: false,
                fail: true,
            }))
            .expect("failure provider");
        let failing = PreviewWorker::spawn(registry).expect("failure worker");
        let generation = failing.begin_generation();
        failing
            .submit(request(generation, source.clone()))
            .expect("failure request");
        assert!(matches!(
            failing.submit(request(generation.wrapping_add(1), source)),
            Err(PreviewSubmitError::Stale(_))
        ));
        let failed = loop {
            if let Some(response) = failing.try_response() {
                break response;
            }
            thread::yield_now();
        };
        assert!(matches!(failed.outcome, PreviewOutcome::Failed(_)));
    }

    fn source_for(path: &Path) -> PreviewSourceKey {
        let entry = enumerate_directory(path.parent().expect("fixture parent"))
            .expect("fixture listing")
            .into_entries()
            .into_iter()
            .find(|entry| entry.path() == path)
            .expect("fixture entry");
        PreviewSourceKey::from_entry(&entry).expect("preview source")
    }

    fn load_first_party(source: PreviewSourceKey, limits: PreviewLimits) -> PreviewOutcome {
        let generation = 41;
        PreviewProviderRegistry::first_party().load(
            &PreviewRequest::new(generation, source, limits, PreviewCachePolicy::Disabled)
                .expect("request"),
            &PreviewCancellation {
                active_generation: Arc::new(AtomicU64::new(generation)),
                generation,
                started: Instant::now(),
                deadline: Duration::from_secs(2),
            },
        )
    }

    #[test]
    fn phase_9b_image_decodes_bounded_first_frames_and_rejects_unsafe_sources() {
        use std::os::unix::fs::symlink;

        let root = tempdir().expect("image root");
        let png = root
            .path()
            .join(std::ffi::OsString::from_vec(b"raw-\xff.png".to_vec()));
        image::RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 255])
            .expect("pixels")
            .save(&png)
            .expect("png");
        let payload = match load_first_party(source_for(&png), PreviewLimits::default()) {
            PreviewOutcome::Ready(payload) => payload,
            outcome => panic!("unexpected PNG outcome: {outcome:?}"),
        };
        assert_eq!(payload.kind, PreviewKind::Image);
        assert!(matches!(
            payload.content,
            PreviewContent::Image {
                width: 2,
                height: 1,
                rowstride: 8,
                first_frame_only: false,
                ..
            }
        ));
        assert!(png.as_os_str().as_bytes().contains(&0xff));

        let gif = root.path().join("animated.gif");
        let file = File::create(&gif).expect("gif file");
        let mut encoder = image::codecs::gif::GifEncoder::new(file);
        encoder
            .encode_frame(image::Frame::new(
                image::RgbaImage::from_raw(1, 1, vec![0, 0, 255, 255]).expect("frame one"),
            ))
            .expect("first frame");
        encoder
            .encode_frame(image::Frame::new(
                image::RgbaImage::from_raw(1, 1, vec![255, 255, 0, 255]).expect("frame two"),
            ))
            .expect("second frame");
        drop(encoder);
        assert!(matches!(
            load_first_party(source_for(&gif), PreviewLimits::default()),
            PreviewOutcome::Ready(PreviewPayload {
                content: PreviewContent::Image {
                    first_frame_only: true,
                    ..
                },
                ..
            })
        ));

        let tiny_output = PreviewLimits {
            max_output_bytes: 3,
            ..PreviewLimits::default()
        };
        assert!(matches!(
            load_first_party(source_for(&png), tiny_output),
            PreviewOutcome::Failed(message) if message.contains("limits")
        ));

        let link = root.path().join("link.png");
        symlink(&png, &link).expect("symlink");
        assert!(matches!(
            load_first_party(source_for(&link), PreviewLimits::default()),
            PreviewOutcome::Failed(_)
        ));

        let malformed = root.path().join("malformed.png");
        fs::write(&malformed, b"not a PNG").expect("malformed source");
        assert!(matches!(
            load_first_party(source_for(&malformed), PreviewLimits::default()),
            PreviewOutcome::Failed(_)
        ));

        let changed = root.path().join("changed.png");
        fs::copy(&png, &changed).expect("changed fixture");
        let stale_key = source_for(&changed);
        fs::write(&changed, b"changed after enumeration").expect("change source");
        assert!(matches!(
            load_first_party(stale_key, PreviewLimits::default()),
            PreviewOutcome::Failed(message) if message.contains("changed")
        ));
    }

    #[test]
    fn phase_9b_text_decodes_inert_utf8_utf16_and_rejects_active_or_binary_content() {
        let root = tempdir().expect("text root");
        let markdown = root.path().join("notes.md");
        fs::write(&markdown, b"# Heading\n<script>inert text</script>").expect("markdown source");
        let payload = match load_first_party(source_for(&markdown), PreviewLimits::default()) {
            PreviewOutcome::Ready(payload) => payload,
            outcome => panic!("unexpected Markdown outcome: {outcome:?}"),
        };
        assert!(matches!(
            payload.content,
            PreviewContent::Text {
                format: PreviewTextFormat::Markdown,
                ref text,
            } if text.contains("<script>")
        ));

        for (name, bytes) in [
            (
                "little.txt",
                [0xff, 0xfe]
                    .into_iter()
                    .chain("Hello".encode_utf16().flat_map(u16::to_le_bytes))
                    .collect::<Vec<_>>(),
            ),
            (
                "big.txt",
                [0xfe, 0xff]
                    .into_iter()
                    .chain("Hello".encode_utf16().flat_map(u16::to_be_bytes))
                    .collect::<Vec<_>>(),
            ),
        ] {
            let path = root.path().join(name);
            fs::write(&path, bytes).expect("UTF-16 fixture");
            assert!(matches!(
                load_first_party(source_for(&path), PreviewLimits::default()),
                PreviewOutcome::Ready(PreviewPayload {
                    content: PreviewContent::Text { ref text, .. },
                    ..
                }) if text.as_ref() == "Hello"
            ));
        }

        for (name, bytes) in [
            ("binary.txt", b"hello\0binary".as_slice()),
            ("invalid.txt", &[0xff, 0x00, 0x01][..]),
        ] {
            let path = root.path().join(name);
            fs::write(&path, bytes).expect("rejected text fixture");
            assert!(matches!(
                load_first_party(source_for(&path), PreviewLimits::default()),
                PreviewOutcome::Failed(_)
            ));
        }

        let oversized = root.path().join("oversized.txt");
        fs::write(&oversized, b"too much text").expect("oversized fixture");
        let limits = PreviewLimits {
            max_text_bytes: 4,
            ..PreviewLimits::default()
        };
        assert!(matches!(
            load_first_party(source_for(&oversized), limits),
            PreviewOutcome::Failed(message) if message.contains("limits")
        ));

        for name in ["active.html", "vector.svg"] {
            let path = root.path().join(name);
            fs::write(&path, b"<active/>").expect("active fixture");
            assert_eq!(
                load_first_party(source_for(&path), PreviewLimits::default()),
                PreviewOutcome::Unsupported
            );
        }

        for (name, format) in [
            ("data.json", PreviewTextFormat::Json),
            ("data.xml", PreviewTextFormat::Xml),
            ("code.rs", PreviewTextFormat::Code),
        ] {
            let path = root.path().join(name);
            fs::write(&path, b"<not interpreted>").expect("passive fixture");
            assert!(matches!(
                load_first_party(source_for(&path), PreviewLimits::default()),
                PreviewOutcome::Ready(PreviewPayload {
                    content: PreviewContent::Text { format: actual, .. },
                    ..
                }) if actual == format
            ));
        }
    }

    fn controlled_document_provider(root: &Path, script_body: &str) -> DocumentPreviewProvider {
        use std::os::unix::fs::PermissionsExt;

        let data = root.join("data");
        let definitions = data.join("thumbnailers");
        fs::create_dir_all(&definitions).expect("thumbnailer definitions");
        let script = root.join("document-provider");
        fs::write(&script, format!("#!/bin/sh\nset -eu\n{script_body}\n"))
            .expect("provider script");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700))
            .expect("provider executable");
        fs::write(
            definitions.join("document.thumbnailer"),
            format!(
                "[Thumbnailer Entry]\nExec={} %i %o %s\nMimeType=application/pdf;application/vnd.openxmlformats-officedocument.wordprocessingml.document;\n",
                script.display()
            ),
        )
        .expect("provider definition");
        DocumentPreviewProvider::discover_from_data_dirs(&[data])
    }

    fn load_document_provider(
        provider: DocumentPreviewProvider,
        source: PreviewSourceKey,
        limits: PreviewLimits,
    ) -> PreviewOutcome {
        let generation = 52;
        let active_generation = Arc::new(AtomicU64::new(generation));
        let request = PreviewRequest::new(generation, source, limits, PreviewCachePolicy::Disabled)
            .expect("document request");
        PreviewProviderRegistry {
            providers: vec![Arc::new(provider)],
            ids: HashSet::from(["floe.document"]),
        }
        .load(
            &request,
            &PreviewCancellation {
                active_generation,
                generation,
                started: Instant::now(),
                deadline: Duration::from_secs(2),
            },
        )
    }

    #[test]
    fn phase_9c_document_contract_limits_formats_and_keeps_fallback_truthful() {
        let root = tempdir().expect("document root");
        for name in [
            "report.pdf",
            "report.docx",
            "sheet.xlsx",
            "slides.pptx",
            "open.odt",
            "legacy.rtf",
        ] {
            assert!(
                document_extension_allowed(&root.path().join(name)),
                "{name}"
            );
        }
        for name in [
            "macro.docm",
            "macro.xlsm",
            "active.html",
            "vector.svg",
            "archive.zip",
        ] {
            assert!(
                !document_extension_allowed(&root.path().join(name)),
                "{name}"
            );
        }

        let source = root.path().join("unsupported.pdf");
        fs::write(&source, b"%PDF fixture").expect("document fixture");
        let provider = DocumentPreviewProvider {
            providers: SystemThumbnailerRegistry::default(),
        };
        assert_eq!(
            load_document_provider(provider, source_for(&source), PreviewLimits::default()),
            PreviewOutcome::Unsupported
        );
    }

    #[test]
    fn phase_9c_document_provider_bounds_png_and_revalidates_exact_source() {
        use std::os::unix::fs::symlink;

        let root = tempdir().expect("document provider root");
        let rendition = root.path().join("rendition.png");
        image::RgbaImage::from_raw(2, 2, [20, 40, 60, 255].repeat(4))
            .expect("rendition pixels")
            .save(&rendition)
            .expect("rendition PNG");
        let provider = controlled_document_provider(
            root.path(),
            &format!("cp '{}' \"$2\"", rendition.display()),
        );
        let source = root
            .path()
            .join(std::ffi::OsString::from_vec(b"report-\xff.pdf".to_vec()));
        fs::write(&source, b"%PDF passive fixture").expect("PDF fixture");
        let payload = match load_document_provider(
            provider.clone(),
            source_for(&source),
            PreviewLimits::default(),
        ) {
            PreviewOutcome::Ready(payload) => payload,
            outcome => panic!("unexpected document outcome: {outcome:?}"),
        };
        assert!(matches!(
            payload,
            PreviewPayload {
                kind: PreviewKind::Document,
                content: PreviewContent::Document {
                    width: 2,
                    height: 2,
                    rowstride: 8,
                    first_page_only: true,
                    ref content_type,
                    ..
                },
                ..
            } if content_type.as_ref() == "application/pdf"
        ));
        assert!(source.as_os_str().as_bytes().contains(&0xff));

        let small_output = PreviewLimits {
            max_output_bytes: 8,
            ..PreviewLimits::default()
        };
        assert!(matches!(
            load_document_provider(provider.clone(), source_for(&source), small_output),
            PreviewOutcome::Failed(message) if message.contains("limits")
        ));

        let malformed_provider = controlled_document_provider(
            &root.path().join("malformed-provider"),
            "printf 'not png' > \"$2\"",
        );
        assert!(matches!(
            load_document_provider(
                malformed_provider,
                source_for(&source),
                PreviewLimits::default()
            ),
            PreviewOutcome::Failed(_)
        ));

        let changing_provider = controlled_document_provider(
            &root.path().join("changing-provider"),
            &format!(
                "printf 'changed by provider' > \"$1\"\ncp '{}' \"$2\"",
                rendition.display()
            ),
        );
        let changing_source = root.path().join("changing.pdf");
        fs::write(&changing_source, b"%PDF original").expect("changing source");
        assert!(matches!(
            load_document_provider(
                changing_provider,
                source_for(&changing_source),
                PreviewLimits::default()
            ),
            PreviewOutcome::Failed(message) if message.contains("changed")
        ));

        let link = root.path().join("linked.pdf");
        symlink(&source, &link).expect("document symlink");
        assert!(matches!(
            load_document_provider(provider, source_for(&link), PreviewLimits::default()),
            PreviewOutcome::Failed(_)
        ));
    }

    fn controlled_media_provider(root: &Path, script_body: &str) -> MediaPreviewProvider {
        use std::os::unix::fs::PermissionsExt;

        let data = root.join("data");
        let definitions = data.join("thumbnailers");
        fs::create_dir_all(&definitions).expect("media definitions");
        let script = root.join("media-provider");
        fs::write(&script, format!("#!/bin/sh\nset -eu\n{script_body}\n"))
            .expect("media provider script");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700))
            .expect("media provider executable");
        fs::write(
            definitions.join("media.thumbnailer"),
            format!(
                "[Thumbnailer Entry]\nExec={} %i %o %s\nMimeType=video/mp4;video/webm;\n",
                script.display()
            ),
        )
        .expect("media provider definition");
        MediaPreviewProvider::discover_from_data_dirs(&[data])
    }

    fn load_media_provider(
        provider: MediaPreviewProvider,
        source: PreviewSourceKey,
    ) -> PreviewOutcome {
        let generation = 63;
        let request = PreviewRequest::new(
            generation,
            source,
            PreviewLimits::default(),
            PreviewCachePolicy::Disabled,
        )
        .expect("media request");
        PreviewProviderRegistry {
            providers: vec![Arc::new(provider)],
            ids: HashSet::from(["floe.media"]),
        }
        .load(
            &request,
            &PreviewCancellation {
                active_generation: Arc::new(AtomicU64::new(generation)),
                generation,
                started: Instant::now(),
                deadline: Duration::from_secs(2),
            },
        )
    }

    #[test]
    fn phase_9d_media_contract_requires_reviewed_extension_and_mime_without_codec_install() {
        let root = tempdir().expect("media contract root");
        for name in [
            "clip.mp4",
            "clip.mkv",
            "clip.webm",
            "sound.mp3",
            "sound.flac",
            "sound.opus",
        ] {
            assert!(
                media_extension_kind(&root.path().join(name)).is_some(),
                "{name}"
            );
        }
        for name in ["playlist.m3u", "script.sh", "active.html", "unknown.bin"] {
            assert!(
                media_extension_kind(&root.path().join(name)).is_none(),
                "{name}"
            );
        }

        let audio = root.path().join("sound.mp3");
        fs::write(&audio, b"ID3 passive fixture").expect("audio fixture");
        let payload = match load_media_provider(
            MediaPreviewProvider {
                poster_providers: SystemThumbnailerRegistry::default(),
            },
            source_for(&audio),
        ) {
            PreviewOutcome::Ready(payload) => payload,
            outcome => panic!("unexpected audio outcome: {outcome:?}"),
        };
        assert!(matches!(
            payload.content,
            PreviewContent::Media {
                is_video: false,
                poster: None,
                ..
            }
        ));
    }

    #[test]
    fn phase_9d_media_provider_validates_identity_and_optional_bounded_poster() {
        use std::os::unix::fs::symlink;

        let root = tempdir().expect("media provider root");
        let poster = root.path().join("poster.png");
        image::RgbaImage::from_raw(2, 1, [90, 70, 50, 255].repeat(2))
            .expect("poster pixels")
            .save(&poster)
            .expect("poster PNG");
        let provider =
            controlled_media_provider(root.path(), &format!("cp '{}' \"$2\"", poster.display()));
        let video = root
            .path()
            .join(std::ffi::OsString::from_vec(b"clip-\xff.mp4".to_vec()));
        fs::write(&video, b"passive video fixture").expect("video fixture");
        let payload = match load_media_provider(provider.clone(), source_for(&video)) {
            PreviewOutcome::Ready(payload) => payload,
            outcome => panic!("unexpected video outcome: {outcome:?}"),
        };
        assert!(matches!(
            payload.content,
            PreviewContent::Media {
                is_video: true,
                poster: Some(PreviewPoster {
                    width: 2,
                    height: 1,
                    rowstride: 8,
                    ..
                }),
                ..
            }
        ));
        assert!(video.as_os_str().as_bytes().contains(&0xff));

        let changing_provider = controlled_media_provider(
            &root.path().join("changing-media"),
            &format!(
                "printf 'changed by provider' > \"$1\"\ncp '{}' \"$2\"",
                poster.display()
            ),
        );
        let changing = root.path().join("changing.mp4");
        fs::write(&changing, b"original video").expect("changing video");
        assert!(matches!(
            load_media_provider(changing_provider, source_for(&changing)),
            PreviewOutcome::Failed(message) if message.contains("changed")
        ));

        let link = root.path().join("linked.mp4");
        symlink(&video, &link).expect("media symlink");
        assert!(matches!(
            load_media_provider(provider, source_for(&link)),
            PreviewOutcome::Failed(_)
        ));
    }

    fn controlled_font_provider(root: &Path, specimen: &Path) -> FontPreviewProvider {
        use std::os::unix::fs::PermissionsExt;

        let data = root.join("data");
        let definitions = data.join("thumbnailers");
        fs::create_dir_all(&definitions).expect("font definitions");
        let script = root.join("font-provider");
        fs::write(
            &script,
            format!("#!/bin/sh\nset -eu\ncp '{}' \"$2\"\n", specimen.display()),
        )
        .expect("font provider script");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700))
            .expect("font provider executable");
        fs::write(
            definitions.join("font.thumbnailer"),
            format!(
                "[Thumbnailer Entry]\nExec={} %i %o %s\nMimeType=font/ttf;application/x-font-ttf;\n",
                script.display()
            ),
        )
        .expect("font provider definition");
        FontPreviewProvider::discover_from_data_dirs(&[data])
    }

    fn load_single_provider(
        provider: Arc<dyn PreviewProvider>,
        source: PreviewSourceKey,
        limits: PreviewLimits,
    ) -> PreviewOutcome {
        let generation = 74;
        let request = PreviewRequest::new(generation, source, limits, PreviewCachePolicy::Disabled)
            .expect("single-provider request");
        PreviewProviderRegistry {
            providers: vec![provider],
            ids: HashSet::new(),
        }
        .load(
            &request,
            &PreviewCancellation {
                active_generation: Arc::new(AtomicU64::new(generation)),
                generation,
                started: Instant::now(),
                deadline: Duration::from_secs(2),
            },
        )
    }

    #[test]
    fn phase_9e_font_provider_returns_passive_bounded_specimen_without_installing() {
        use std::os::unix::fs::symlink;

        let root = tempdir().expect("font root");
        let specimen = root.path().join("specimen.png");
        image::RgbaImage::from_raw(3, 1, [30, 60, 90, 255].repeat(3))
            .expect("font specimen pixels")
            .save(&specimen)
            .expect("font specimen PNG");
        let provider = controlled_font_provider(root.path(), &specimen);
        let font = root
            .path()
            .join(std::ffi::OsString::from_vec(b"font-\xff.ttf".to_vec()));
        fs::write(&font, b"\0\x01\0\0 passive font fixture").expect("font fixture");
        let payload = match load_single_provider(
            Arc::new(provider.clone()),
            source_for(&font),
            PreviewLimits::default(),
        ) {
            PreviewOutcome::Ready(payload) => payload,
            outcome => panic!("unexpected font outcome: {outcome:?}"),
        };
        assert!(matches!(
            payload.content,
            PreviewContent::Font {
                width: 3,
                height: 1,
                rowstride: 12,
                ..
            }
        ));
        let link = root.path().join("linked.ttf");
        symlink(&font, &link).expect("font symlink");
        assert!(matches!(
            load_single_provider(
                Arc::new(provider),
                source_for(&link),
                PreviewLimits::default()
            ),
            PreviewOutcome::Failed(_)
        ));
        assert!(!font_extension_allowed(&root.path().join("installer.exe")));
    }

    fn zip_directory_entry(name: &[u8], size: u32) -> Vec<u8> {
        let mut record = vec![0_u8; 46];
        record[0..4].copy_from_slice(b"PK\x01\x02");
        record[24..28].copy_from_slice(&size.to_le_bytes());
        record[28..30].copy_from_slice(&(name.len() as u16).to_le_bytes());
        record.extend_from_slice(name);
        record
    }

    fn tar_entry(name: &[u8], data: &[u8], type_flag: u8) -> Vec<u8> {
        let mut header = vec![0_u8; 512];
        header[..name.len()].copy_from_slice(name);
        header[100..108].copy_from_slice(b"0000644\0");
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        let size = format!("{:011o}\0", data.len());
        header[124..136].copy_from_slice(size.as_bytes());
        header[136..148].copy_from_slice(b"00000000000\0");
        header[148..156].fill(b' ');
        header[156] = type_flag;
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        let checksum = header.iter().map(|byte| u64::from(*byte)).sum::<u64>();
        let checksum = format!("{:06o}\0 ", checksum);
        header[148..156].copy_from_slice(checksum.as_bytes());
        let mut output = header;
        output.extend_from_slice(data);
        output.resize(output.len().div_ceil(512) * 512, 0);
        output
    }

    #[test]
    fn phase_9e_archive_provider_lists_bounded_raw_zip_and_tar_without_extraction() {
        let root = tempdir().expect("archive root");
        let zip = root
            .path()
            .join(std::ffi::OsString::from_vec(b"raw-\xff.zip".to_vec()));
        let mut zip_bytes = zip_directory_entry(b"safe/readme.txt", 12);
        zip_bytes.extend(zip_directory_entry(b"../escape-\xff", 7));
        fs::write(&zip, zip_bytes).expect("ZIP fixture");
        let zip_payload = match load_single_provider(
            Arc::new(ArchivePreviewProvider),
            source_for(&zip),
            PreviewLimits::default(),
        ) {
            PreviewOutcome::Ready(payload) => payload,
            outcome => panic!("unexpected ZIP outcome: {outcome:?}"),
        };
        assert!(matches!(
            zip_payload.content,
            PreviewContent::Archive {
                format: PreviewArchiveFormat::Zip,
                ref entries,
                ref listing,
                truncated: false,
            } if entries.len() == 2
                && entries[1].unsafe_path
                && entries[1].raw_name.ends_with(&[0xff])
                && listing.contains("[unsafe path]")
        ));

        let tar = root.path().join("contents.tar");
        let mut tar_bytes = tar_entry(b"folder/", &[], b'5');
        tar_bytes.extend(tar_entry(b"folder/file.txt", b"hello", b'0'));
        tar_bytes.extend(vec![0_u8; 1024]);
        fs::write(&tar, tar_bytes).expect("TAR fixture");
        assert!(matches!(
            load_single_provider(
                Arc::new(ArchivePreviewProvider),
                source_for(&tar),
                PreviewLimits::default()
            ),
            PreviewOutcome::Ready(PreviewPayload {
                content: PreviewContent::Archive {
                    format: PreviewArchiveFormat::Tar,
                    ref entries,
                    ..
                },
                ..
            }) if entries.len() == 2 && entries[0].is_directory && entries[1].size == 5
        ));

        let limited = PreviewLimits {
            max_archive_entries: 1,
            ..PreviewLimits::default()
        };
        assert!(matches!(
            load_single_provider(Arc::new(ArchivePreviewProvider), source_for(&zip), limited),
            PreviewOutcome::Ready(PreviewPayload {
                content: PreviewContent::Archive {
                    ref entries,
                    truncated: true,
                    ..
                },
                ..
            }) if entries.len() == 1
        ));

        let malformed = root.path().join("malformed.tar");
        fs::write(&malformed, vec![1_u8; 512]).expect("malformed TAR");
        assert!(matches!(
            load_single_provider(
                Arc::new(ArchivePreviewProvider),
                source_for(&malformed),
                PreviewLimits::default()
            ),
            PreviewOutcome::Failed(_)
        ));
        assert_eq!(archive_format(&root.path().join("compressed.tar.gz")), None);
    }

    #[test]
    fn phase_9f_cache_privacy_purge_cancels_generation_and_evicts_memory_only_payloads() {
        let (_root, source) = source_fixture();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = PreviewProviderRegistry::default();
        registry
            .register(Arc::new(TestProvider {
                id: "cache-purge",
                calls: Arc::clone(&calls),
                wait_for_cancel: false,
                fail: false,
            }))
            .expect("cache provider");
        let worker = PreviewWorker::spawn(registry).expect("preview worker");

        for _ in 0..2 {
            let generation = worker.begin_generation();
            worker
                .submit(request(generation, source.clone()))
                .expect("cached request");
            while worker.try_response().is_none() {
                thread::yield_now();
            }
        }
        assert_eq!(calls.load(Ordering::Acquire), 1);

        worker.clear_memory_cache();
        let generation = worker.begin_generation();
        worker
            .submit(request(generation, source))
            .expect("post-purge request");
        while worker.try_response().is_none() {
            thread::yield_now();
        }
        assert_eq!(calls.load(Ordering::Acquire), 2);
    }
}
