//! Bounded, cancellable local text-content search without GTK, helpers, or indexing.

use std::{
    collections::VecDeque,
    ffi::OsStr,
    fs::{self, File},
    io::{self, Read},
    os::unix::{ffi::OsStrExt, fs::MetadataExt, fs::PermissionsExt},
    path::{Path, PathBuf},
};

use rustix::fs::{Mode, OFlags, open};
use thiserror::Error;

use crate::{
    AdvancedFilter, AdvancedFilterDecision, AdvancedFilterError, AdvancedMetadata, DirectoryEntry,
    EntryKind, FilenameSearchScope, FolderFilterError, FolderFilterMode, FolderFilterPattern,
    HiddenFilter, ThumbnailState,
};

pub const CONTENT_SEARCH_BATCH_CAPACITY: usize = 64;
pub const CONTENT_SEARCH_RESULT_CAPACITY: usize = 50_000;
pub const CONTENT_SEARCH_FILE_CAPACITY: usize = 100_000;
pub const CONTENT_SEARCH_DIRECTORY_CAPACITY: usize = 100_000;
pub const CONTENT_SEARCH_DEPTH_CAPACITY: usize = 128;
pub const CONTENT_SEARCH_FILE_BYTES: u64 = 16 * 1024 * 1024;
pub const CONTENT_SEARCH_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
pub const CONTENT_SEARCH_LINE_BYTES: usize = 256 * 1024;
pub const CONTENT_SEARCH_SNIPPET_CHARS: usize = 240;

#[derive(Clone, Debug)]
pub struct ContentSearchRequest {
    root: PathBuf,
    query: String,
    scope: FilenameSearchScope,
    include_hidden: bool,
    mode: FolderFilterMode,
    advanced: AdvancedFilter,
}

impl ContentSearchRequest {
    pub fn new(
        root: PathBuf,
        query: String,
        scope: FilenameSearchScope,
        show_hidden: bool,
        mode: FolderFilterMode,
        advanced: AdvancedFilter,
    ) -> Result<Self, ContentSearchError> {
        if !root.is_absolute() {
            return Err(ContentSearchError::RelativeRoot);
        }
        if query.is_empty() {
            return Err(ContentSearchError::EmptyQuery);
        }
        advanced.validate()?;
        FolderFilterPattern::compile_with_case(mode, &query, advanced.match_case)?;
        let include_hidden =
            show_hidden || matches!(advanced.hidden, HiddenFilter::Include | HiddenFilter::Only);
        Ok(Self {
            root,
            query,
            scope,
            include_hidden,
            mode,
            advanced,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub const fn scope(&self) -> FilenameSearchScope {
        self.scope
    }

    pub const fn include_hidden(&self) -> bool {
        self.include_hidden
    }

    pub const fn mode(&self) -> FolderFilterMode {
        self.mode
    }

    pub const fn advanced(&self) -> &AdvancedFilter {
        &self.advanced
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentSearchLimits {
    pub results: usize,
    pub files: usize,
    pub directories: usize,
    pub depth: usize,
    pub file_bytes: u64,
    pub total_bytes: u64,
    pub line_bytes: usize,
    pub snippet_chars: usize,
}

impl Default for ContentSearchLimits {
    fn default() -> Self {
        Self {
            results: CONTENT_SEARCH_RESULT_CAPACITY,
            files: CONTENT_SEARCH_FILE_CAPACITY,
            directories: CONTENT_SEARCH_DIRECTORY_CAPACITY,
            depth: CONTENT_SEARCH_DEPTH_CAPACITY,
            file_bytes: CONTENT_SEARCH_FILE_BYTES,
            total_bytes: CONTENT_SEARCH_TOTAL_BYTES,
            line_bytes: CONTENT_SEARCH_LINE_BYTES,
            snippet_chars: CONTENT_SEARCH_SNIPPET_CHARS,
        }
    }
}

impl ContentSearchLimits {
    fn validate(self) -> Result<Self, ContentSearchError> {
        if self.results == 0
            || self.files == 0
            || self.directories == 0
            || self.file_bytes == 0
            || self.total_bytes == 0
            || self.line_bytes == 0
            || self.snippet_chars == 0
        {
            return Err(ContentSearchError::InvalidLimits);
        }
        Ok(self)
    }
}

#[derive(Clone, Debug)]
pub struct ContentSearchMatch {
    entry: DirectoryEntry,
    line_number: u64,
    snippet: String,
}

impl ContentSearchMatch {
    pub fn entry(&self) -> &DirectoryEntry {
        &self.entry
    }

    pub const fn line_number(&self) -> u64 {
        self.line_number
    }

    pub fn snippet(&self) -> &str {
        &self.snippet
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ContentSearchSummary {
    pub matched: usize,
    pub examined_files: usize,
    pub examined_directories: usize,
    pub bytes_read: u64,
    pub skipped_entries: usize,
    pub skipped_directories: usize,
    pub skipped_mounts: usize,
    pub depth_limited: usize,
    pub metadata_unavailable: usize,
    pub binary_skipped: usize,
    pub encoding_skipped: usize,
    pub too_large: usize,
    pub changed_files: usize,
    pub long_lines_skipped: usize,
    pub truncated: bool,
}

#[derive(Debug, Error)]
pub enum ContentSearchError {
    #[error("search root must be an absolute local path")]
    RelativeRoot,
    #[error("enter text to search inside files")]
    EmptyQuery,
    #[error(transparent)]
    Pattern(#[from] FolderFilterError),
    #[error(transparent)]
    Advanced(#[from] AdvancedFilterError),
    #[error("content-search limits must all be positive")]
    InvalidLimits,
    #[error("could not inspect search root: {0}")]
    RootMetadata(#[source] io::Error),
    #[error("search root is a symbolic link; choose its real directory instead")]
    SymbolicRoot,
    #[error("search root is not a directory")]
    RootNotDirectory,
    #[error("could not open search root: {0}")]
    OpenRoot(#[source] io::Error),
    #[error("content search was cancelled")]
    Cancelled,
    #[error("content-search result consumer stopped")]
    ConsumerStopped,
}

/// Searches bounded local text content. MIME resolution is injected so the
/// GTK-independent core does not depend on GIO and never needs content sniffing.
pub fn search_contents_with_mime(
    request: &ContentSearchRequest,
    limits: ContentSearchLimits,
    mut is_cancelled: impl FnMut() -> bool,
    mut resolve_mime: impl FnMut(&Path) -> Option<String>,
    mut on_batch: impl FnMut(Vec<ContentSearchMatch>, ContentSearchSummary) -> bool,
) -> Result<ContentSearchSummary, ContentSearchError> {
    let limits = limits.validate()?;
    let matcher = FolderFilterPattern::compile_with_case(
        request.mode(),
        request.query(),
        request.advanced().match_case,
    )?;
    let root_metadata =
        fs::symlink_metadata(request.root()).map_err(ContentSearchError::RootMetadata)?;
    if root_metadata.file_type().is_symlink() {
        return Err(ContentSearchError::SymbolicRoot);
    }
    if !root_metadata.is_dir() {
        return Err(ContentSearchError::RootNotDirectory);
    }
    fs::read_dir(request.root()).map_err(ContentSearchError::OpenRoot)?;

    let root_device = root_metadata.dev();
    let mut queue = VecDeque::from([(request.root().to_path_buf(), 0_usize)]);
    let mut scheduled_directories = 1_usize;
    let mut summary = ContentSearchSummary::default();
    let mut batch = Vec::with_capacity(CONTENT_SEARCH_BATCH_CAPACITY.min(limits.results));

    'walk: while let Some((directory, depth)) = queue.pop_front() {
        if is_cancelled() {
            return Err(ContentSearchError::Cancelled);
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
                return Err(ContentSearchError::Cancelled);
            }
            let child = match child {
                Ok(child) => child,
                Err(_) => {
                    summary.skipped_entries = summary.skipped_entries.saturating_add(1);
                    continue;
                }
            };
            let name = child.file_name();
            if !request.include_hidden() && is_hidden(&name) {
                continue;
            }
            let path = child.path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    summary.skipped_entries = summary.skipped_entries.saturating_add(1);
                    continue;
                }
            };
            let file_type = metadata.file_type();

            if file_type.is_dir() {
                if request.scope() != FilenameSearchScope::Subtree {
                    continue;
                }
                if metadata.dev() != root_device {
                    summary.skipped_mounts = summary.skipped_mounts.saturating_add(1);
                    continue;
                }
                if depth >= limits.depth {
                    summary.depth_limited = summary.depth_limited.saturating_add(1);
                    summary.truncated = true;
                    continue;
                }
                if scheduled_directories >= limits.directories {
                    summary.truncated = true;
                    continue;
                }
                queue.push_back((path, depth.saturating_add(1)));
                scheduled_directories = scheduled_directories.saturating_add(1);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            if summary.examined_files >= limits.files {
                summary.truncated = true;
                break 'walk;
            }
            summary.examined_files = summary.examined_files.saturating_add(1);

            let entry = directory_entry(path.clone(), name, &metadata);
            if !advanced_matches(
                &entry,
                request.advanced(),
                &path,
                &mut resolve_mime,
                &mut summary,
            ) {
                continue;
            }
            if metadata.len() > limits.file_bytes
                || summary.bytes_read.saturating_add(metadata.len()) > limits.total_bytes
            {
                summary.too_large = summary.too_large.saturating_add(1);
                if summary.bytes_read >= limits.total_bytes {
                    summary.truncated = true;
                    break 'walk;
                }
                continue;
            }

            let bytes = match read_revalidated(&path, &metadata, limits.file_bytes) {
                Ok(bytes) => bytes,
                Err(ReadCandidateError::TooLarge) => {
                    summary.too_large = summary.too_large.saturating_add(1);
                    continue;
                }
                Err(ReadCandidateError::Changed) => {
                    summary.changed_files = summary.changed_files.saturating_add(1);
                    continue;
                }
                Err(ReadCandidateError::Io) => {
                    summary.skipped_entries = summary.skipped_entries.saturating_add(1);
                    continue;
                }
            };
            summary.bytes_read = summary.bytes_read.saturating_add(bytes.len() as u64);
            let text = match decode_text(&bytes) {
                Ok(text) => text,
                Err(TextKind::Binary) => {
                    summary.binary_skipped = summary.binary_skipped.saturating_add(1);
                    continue;
                }
                Err(TextKind::UnsupportedEncoding) => {
                    summary.encoding_skipped = summary.encoding_skipped.saturating_add(1);
                    continue;
                }
            };

            for (line_index, line) in text.lines().enumerate() {
                if is_cancelled() {
                    return Err(ContentSearchError::Cancelled);
                }
                if line.len() > limits.line_bytes {
                    summary.long_lines_skipped = summary.long_lines_skipped.saturating_add(1);
                    continue;
                }
                if !matcher.matches(OsStr::new(line)) {
                    continue;
                }
                if summary.matched >= limits.results {
                    summary.truncated = true;
                    break 'walk;
                }
                batch.push(ContentSearchMatch {
                    entry: entry.clone(),
                    line_number: u64::try_from(line_index)
                        .unwrap_or(u64::MAX)
                        .saturating_add(1),
                    snippet: normalized_snippet(line, limits.snippet_chars),
                });
                summary.matched = summary.matched.saturating_add(1);
                if batch.len() == CONTENT_SEARCH_BATCH_CAPACITY {
                    if !on_batch(batch, summary) {
                        return Err(ContentSearchError::ConsumerStopped);
                    }
                    batch = Vec::with_capacity(CONTENT_SEARCH_BATCH_CAPACITY);
                }
            }
        }
    }

    if !batch.is_empty() && !on_batch(batch, summary) {
        return Err(ContentSearchError::ConsumerStopped);
    }
    Ok(summary)
}

fn advanced_matches(
    entry: &DirectoryEntry,
    filter: &AdvancedFilter,
    path: &Path,
    resolve_mime: &mut impl FnMut(&Path) -> Option<String>,
    summary: &mut ContentSearchSummary,
) -> bool {
    match filter.evaluate(entry, None) {
        AdvancedFilterDecision::Match => true,
        AdvancedFilterDecision::NoMatch => false,
        AdvancedFilterDecision::NeedsMetadata(needs) => {
            let facts = AdvancedMetadata {
                mime: needs.mime.then(|| resolve_mime(path)).flatten(),
                owner_uid: needs
                    .owner
                    .then(|| {
                        fs::symlink_metadata(path)
                            .ok()
                            .map(|metadata| metadata.uid())
                    })
                    .flatten(),
            };
            if (needs.mime && facts.mime.is_none()) || (needs.owner && facts.owner_uid.is_none()) {
                summary.metadata_unavailable = summary.metadata_unavailable.saturating_add(1);
            }
            filter.evaluate(entry, Some(&facts)) == AdvancedFilterDecision::Match
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReadCandidateError {
    Io,
    TooLarge,
    Changed,
}

fn read_revalidated(
    path: &Path,
    before: &fs::Metadata,
    file_limit: u64,
) -> Result<Vec<u8>, ReadCandidateError> {
    let descriptor = open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| ReadCandidateError::Io)?;
    let mut file = File::from(descriptor);
    let opened = file.metadata().map_err(|_| ReadCandidateError::Io)?;
    if !opened.is_file() || !same_identity(before, &opened) {
        return Err(ReadCandidateError::Changed);
    }
    if opened.len() > file_limit {
        return Err(ReadCandidateError::TooLarge);
    }
    let mut bytes = Vec::with_capacity(usize::try_from(opened.len()).unwrap_or(0));
    file.by_ref()
        .take(file_limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| ReadCandidateError::Io)?;
    if bytes.len() as u64 > file_limit {
        return Err(ReadCandidateError::TooLarge);
    }
    let after = file.metadata().map_err(|_| ReadCandidateError::Io)?;
    if !same_identity(&opened, &after) {
        return Err(ReadCandidateError::Changed);
    }
    Ok(bytes)
}

fn same_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.len() == right.len()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TextKind {
    Binary,
    UnsupportedEncoding,
}

fn decode_text(bytes: &[u8]) -> Result<String, TextKind> {
    if let Some(body) = bytes.strip_prefix(&[0xff, 0xfe]) {
        return decode_utf16(body, u16::from_le_bytes);
    }
    if let Some(body) = bytes.strip_prefix(&[0xfe, 0xff]) {
        return decode_utf16(body, u16::from_be_bytes);
    }
    if bytes.contains(&0) {
        return Err(TextKind::Binary);
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| TextKind::UnsupportedEncoding)
}

fn decode_utf16(body: &[u8], decode: fn([u8; 2]) -> u16) -> Result<String, TextKind> {
    if body.len() % 2 != 0 {
        return Err(TextKind::UnsupportedEncoding);
    }
    let units = body
        .chunks_exact(2)
        .map(|pair| decode([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&units).map_err(|_| TextKind::UnsupportedEncoding)
}

fn normalized_snippet(line: &str, capacity: usize) -> String {
    let mut snippet = String::new();
    let mut chars = 0_usize;
    let mut truncated = false;
    for word in line.split_whitespace() {
        let separator = usize::from(!snippet.is_empty());
        let word_chars = word.chars().count();
        if chars.saturating_add(separator).saturating_add(word_chars) > capacity {
            truncated = true;
            break;
        }
        if separator == 1 {
            snippet.push(' ');
            chars = chars.saturating_add(1);
        }
        snippet.push_str(word);
        chars = chars.saturating_add(word_chars);
    }
    if truncated {
        snippet.push('…');
    }
    snippet
}

fn directory_entry(
    path: PathBuf,
    name: std::ffi::OsString,
    metadata: &fs::Metadata,
) -> DirectoryEntry {
    DirectoryEntry::new(
        path,
        name.clone(),
        EntryKind::RegularFile,
        Some(metadata.len()),
        metadata.modified().ok(),
        None,
        is_hidden(&name),
        metadata.permissions().mode() & 0o111 != 0,
        ThumbnailState::NotRequested,
    )
}

fn is_hidden(name: &OsStr) -> bool {
    name.as_bytes().first() == Some(&b'.')
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::symlink};

    use tempfile::tempdir;

    use super::*;

    fn request(root: &Path, query: &str, mode: FolderFilterMode) -> ContentSearchRequest {
        ContentSearchRequest::new(
            root.to_path_buf(),
            query.to_owned(),
            FilenameSearchScope::Subtree,
            false,
            mode,
            AdvancedFilter::default(),
        )
        .expect("valid content-search request")
    }

    fn collect(
        request: &ContentSearchRequest,
        limits: ContentSearchLimits,
    ) -> (Vec<ContentSearchMatch>, ContentSearchSummary) {
        let mut matches = Vec::new();
        let summary = search_contents_with_mime(
            request,
            limits,
            || false,
            |_| None,
            |mut batch, _| {
                matches.append(&mut batch);
                true
            },
        )
        .expect("content search");
        (matches, summary)
    }

    #[test]
    fn phase_13d_searches_utf8_utf16_lines_and_normalizes_snippets() {
        let root = tempdir().expect("content fixture");
        fs::write(root.path().join("utf8.txt"), b"first\n  needle   here  \n")
            .expect("UTF-8 fixture");
        let mut utf16 = vec![0xff, 0xfe];
        for unit in "Needle in UTF-16".encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        fs::write(root.path().join("utf16.txt"), utf16).expect("UTF-16 fixture");
        let (matches, summary) = collect(
            &request(root.path(), "needle", FolderFilterMode::Text),
            ContentSearchLimits::default(),
        );
        assert_eq!(matches.len(), 2);
        let utf8_match = matches
            .iter()
            .find(|item| item.entry().display_name() == OsStr::new("utf8.txt"))
            .expect("UTF-8 match");
        assert_eq!(utf8_match.line_number(), 2);
        assert_eq!(utf8_match.snippet(), "needle here");
        assert_eq!(summary.encoding_skipped, 0);
    }

    #[test]
    fn phase_13d_honors_glob_regex_and_match_case() {
        let root = tempdir().expect("pattern fixture");
        fs::write(root.path().join("data.txt"), b"Ticket-42\nticket-7\n").expect("pattern fixture");
        let advanced = AdvancedFilter {
            match_case: true,
            ..AdvancedFilter::default()
        };
        let regex = ContentSearchRequest::new(
            root.path().to_path_buf(),
            r"^Ticket-[0-9]+$".to_owned(),
            FilenameSearchScope::CurrentFolder,
            false,
            FolderFilterMode::Regex,
            advanced,
        )
        .expect("regex request");
        let (matches, _) = collect(&regex, ContentSearchLimits::default());
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line_number(), 1);
    }

    #[test]
    fn phase_13d_skips_binary_unsupported_symlink_and_over_limit_files() {
        let root = tempdir().expect("skip fixture");
        fs::write(root.path().join("binary.bin"), b"needle\0binary").expect("binary");
        fs::write(root.path().join("legacy.txt"), b"needle\xff").expect("legacy");
        fs::write(root.path().join("large.txt"), vec![b'n'; 64]).expect("large");
        fs::write(root.path().join("target.txt"), b"needle").expect("target");
        symlink(
            root.path().join("target.txt"),
            root.path().join("alias.txt"),
        )
        .expect("symlink");
        let limits = ContentSearchLimits {
            file_bytes: 32,
            ..ContentSearchLimits::default()
        };
        let (matches, summary) = collect(
            &request(root.path(), "needle", FolderFilterMode::Text),
            limits,
        );
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].entry().display_name(), OsStr::new("target.txt"));
        assert_eq!(summary.binary_skipped, 1);
        assert_eq!(summary.encoding_skipped, 1);
        assert_eq!(summary.too_large, 1);
    }

    #[test]
    fn phase_13d_applies_advanced_predicates_before_content_reads() {
        let root = tempdir().expect("advanced fixture");
        fs::write(root.path().join("keep.md"), b"needle").expect("keep");
        fs::write(root.path().join("skip.txt"), b"needle").expect("skip");
        let request = ContentSearchRequest::new(
            root.path().to_path_buf(),
            "needle".to_owned(),
            FilenameSearchScope::CurrentFolder,
            false,
            FolderFilterMode::Text,
            AdvancedFilter {
                extension: Some("md".to_owned()),
                ..AdvancedFilter::default()
            },
        )
        .expect("advanced request");
        let (matches, summary) = collect(&request, ContentSearchLimits::default());
        assert_eq!(matches.len(), 1);
        assert_eq!(summary.bytes_read, 6);
    }

    #[test]
    fn phase_13d_reports_caps_cancellation_and_invalid_requests() {
        let root = tempdir().expect("bounds fixture");
        fs::write(root.path().join("many.txt"), b"needle\nneedle\n").expect("many");
        let limits = ContentSearchLimits {
            results: 1,
            ..ContentSearchLimits::default()
        };
        let (matches, summary) = collect(
            &request(root.path(), "needle", FolderFilterMode::Text),
            limits,
        );
        assert_eq!(matches.len(), 1);
        assert!(summary.truncated);
        let cancelled = search_contents_with_mime(
            &request(root.path(), "needle", FolderFilterMode::Text),
            ContentSearchLimits::default(),
            || true,
            |_| None,
            |_, _| true,
        );
        assert!(matches!(cancelled, Err(ContentSearchError::Cancelled)));
        assert!(matches!(
            ContentSearchRequest::new(
                root.path().to_path_buf(),
                String::new(),
                FilenameSearchScope::CurrentFolder,
                false,
                FolderFilterMode::Text,
                AdvancedFilter::default(),
            ),
            Err(ContentSearchError::EmptyQuery)
        ));
    }
}
