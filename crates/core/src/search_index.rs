//! Bounded, GTK-independent filename/metadata search index.
//!
//! The index is deliberately conservative: it contains no file contents,
//! excludes hidden names, never follows symbolic links, and is useful only
//! while every recorded directory fingerprint remains current.

use std::{
    collections::{HashSet, VecDeque},
    ffi::{OsStr, OsString},
    fs,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{MetadataExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use thiserror::Error;

use crate::{
    AdvancedFilterDecision, AdvancedMetadata, DirectoryEntry, EntryKind,
    FILENAME_SEARCH_DEPTH_CAPACITY, FILENAME_SEARCH_DIRECTORY_CAPACITY,
    FILENAME_SEARCH_ENTRY_CAPACITY, FILENAME_SEARCH_RESULT_CAPACITY, FilenameSearchRequest,
    FilenameSearchScope, FilenameSearchSummary, FolderFilterPattern, HiddenFilter, ThumbnailState,
};

const INDEX_MAGIC: &str = "FLOE-SEARCH-INDEX-1";
pub const SEARCH_INDEX_SERIALIZED_CAPACITY: usize = 64 * 1024 * 1024;
pub const SEARCH_INDEX_PATH_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchIndexLimits {
    pub entries: usize,
    pub directories: usize,
    pub depth: usize,
    pub serialized_bytes: usize,
}

impl Default for SearchIndexLimits {
    fn default() -> Self {
        Self {
            entries: FILENAME_SEARCH_ENTRY_CAPACITY,
            directories: FILENAME_SEARCH_DIRECTORY_CAPACITY,
            depth: FILENAME_SEARCH_DEPTH_CAPACITY,
            serialized_bytes: SEARCH_INDEX_SERIALIZED_CAPACITY,
        }
    }
}

impl SearchIndexLimits {
    fn validate(self) -> Result<Self, SearchIndexError> {
        if self.entries == 0
            || self.directories == 0
            || self.serialized_bytes < INDEX_MAGIC.len()
            || self.serialized_bytes > SEARCH_INDEX_SERIALIZED_CAPACITY
        {
            return Err(SearchIndexError::InvalidLimits);
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchIndexBuildRequest {
    root: PathBuf,
}

impl SearchIndexBuildRequest {
    pub fn new(root: PathBuf) -> Result<Self, SearchIndexError> {
        if !root.is_absolute() {
            return Err(SearchIndexError::RelativeRoot);
        }
        if path_bytes(&root).len() > SEARCH_INDEX_PATH_BYTES {
            return Err(SearchIndexError::PathTooLong);
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SearchIndexBuildSummary {
    pub indexed_entries: usize,
    pub indexed_directories: usize,
    pub excluded_hidden: usize,
    pub skipped_entries: usize,
    pub skipped_directories: usize,
    pub skipped_mounts: usize,
    pub depth_limited: usize,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Fingerprint {
    device: u64,
    inode: u64,
    mode: u32,
    size: u64,
    modified_seconds: i64,
    modified_nanos: i64,
    changed_seconds: i64,
    changed_nanos: i64,
}

impl Fingerprint {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            mode: metadata.mode(),
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct IndexedDirectory {
    path: PathBuf,
    fingerprint: Fingerprint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct IndexedEntry {
    path: PathBuf,
    kind: EntryKind,
    fingerprint: Fingerprint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchIndex {
    root: PathBuf,
    directories: Vec<IndexedDirectory>,
    entries: Vec<IndexedEntry>,
    summary: SearchIndexBuildSummary,
}

impl SearchIndex {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn summary(&self) -> SearchIndexBuildSummary {
        self.summary
    }

    pub fn serialize(&self) -> Result<Vec<u8>, SearchIndexError> {
        let mut output = String::from(INDEX_MAGIC);
        output.push('\n');
        output.push_str("root\t");
        output.push_str(&hex_encode(path_bytes(&self.root)));
        output.push('\n');
        for directory in &self.directories {
            append_record(&mut output, 'd', &directory.path, directory.fingerprint);
        }
        for entry in &self.entries {
            let tag = match entry.kind {
                EntryKind::Directory => 'D',
                EntryKind::RegularFile => 'F',
                EntryKind::SymbolicLink { .. } => 'L',
                EntryKind::Other => 'O',
            };
            append_record(&mut output, tag, &entry.path, entry.fingerprint);
        }
        if output.len() > SEARCH_INDEX_SERIALIZED_CAPACITY {
            return Err(SearchIndexError::SerializedTooLarge);
        }
        Ok(output.into_bytes())
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, SearchIndexError> {
        if bytes.len() > SEARCH_INDEX_SERIALIZED_CAPACITY {
            return Err(SearchIndexError::SerializedTooLarge);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| SearchIndexError::Malformed)?;
        let mut lines = text.lines();
        if lines.next() != Some(INDEX_MAGIC) {
            return Err(SearchIndexError::UnsupportedVersion);
        }
        let Some(root_line) = lines.next() else {
            return Err(SearchIndexError::Malformed);
        };
        let Some(root_hex) = root_line.strip_prefix("root\t") else {
            return Err(SearchIndexError::Malformed);
        };
        let root = PathBuf::from(OsString::from_vec(hex_decode(root_hex)?));
        SearchIndexBuildRequest::new(root.clone())?;

        let mut directories = Vec::new();
        let mut entries = Vec::new();
        let mut directory_paths = HashSet::new();
        let mut entry_paths = HashSet::new();
        for line in lines {
            let (tag, path, fingerprint) = parse_record(line)?;
            if !path.starts_with(&root)
                || path_bytes(&path).len() > SEARCH_INDEX_PATH_BYTES
                || path
                    .strip_prefix(&root)
                    .map_err(|_| SearchIndexError::Malformed)?
                    .components()
                    .any(|component| is_hidden(component.as_os_str()))
            {
                return Err(SearchIndexError::Malformed);
            }
            match tag {
                'd' => {
                    if directories.len() >= FILENAME_SEARCH_DIRECTORY_CAPACITY {
                        return Err(SearchIndexError::TooManyDirectories);
                    }
                    if !directory_paths.insert(path.clone()) {
                        return Err(SearchIndexError::Malformed);
                    }
                    directories.push(IndexedDirectory { path, fingerprint });
                }
                'D' | 'F' | 'L' | 'O' => {
                    if entries.len() >= FILENAME_SEARCH_ENTRY_CAPACITY {
                        return Err(SearchIndexError::TooManyEntries);
                    }
                    let kind = match tag {
                        'D' => EntryKind::Directory,
                        'F' => EntryKind::RegularFile,
                        'L' => EntryKind::SymbolicLink {
                            target_is_directory: false,
                        },
                        _ => EntryKind::Other,
                    };
                    if path == root || !entry_paths.insert(path.clone()) {
                        return Err(SearchIndexError::Malformed);
                    }
                    entries.push(IndexedEntry {
                        path,
                        kind,
                        fingerprint,
                    });
                }
                _ => return Err(SearchIndexError::Malformed),
            }
        }
        if directories.is_empty() || directories[0].path != root {
            return Err(SearchIndexError::Malformed);
        }
        if entries.iter().any(|entry| {
            !entry
                .path
                .parent()
                .is_some_and(|parent| directory_paths.contains(parent))
        }) {
            return Err(SearchIndexError::Malformed);
        }
        if directories.iter().skip(1).any(|directory| {
            !entries
                .iter()
                .any(|entry| entry.path == directory.path && entry.kind == EntryKind::Directory)
        }) {
            return Err(SearchIndexError::Malformed);
        }
        let summary = SearchIndexBuildSummary {
            indexed_entries: entries.len(),
            indexed_directories: directories.len(),
            ..SearchIndexBuildSummary::default()
        };
        Ok(Self {
            root,
            directories,
            entries,
            summary,
        })
    }

    pub fn search_with_mime(
        &self,
        request: &FilenameSearchRequest,
        mut is_cancelled: impl FnMut() -> bool,
        mut resolve_mime: impl FnMut(&Path) -> Option<String>,
    ) -> Result<(Vec<DirectoryEntry>, FilenameSearchSummary), SearchIndexError> {
        if request.root() != self.root || request.include_hidden() {
            return Err(SearchIndexError::Ineligible);
        }
        if matches!(
            request.advanced().hidden,
            HiddenFilter::Include | HiddenFilter::Only
        ) {
            return Err(SearchIndexError::Ineligible);
        }
        let matcher = FolderFilterPattern::compile_with_case(
            request.mode(),
            request.query(),
            request.advanced().match_case,
        )
        .map_err(|_| SearchIndexError::Ineligible)?;

        for directory in &self.directories {
            if is_cancelled() {
                return Err(SearchIndexError::Cancelled);
            }
            let metadata =
                fs::symlink_metadata(&directory.path).map_err(|_| SearchIndexError::Stale)?;
            if metadata.file_type().is_symlink()
                || !metadata.is_dir()
                || !directory.fingerprint.matches(&metadata)
            {
                return Err(SearchIndexError::Stale);
            }
        }

        let validate_all_entries = request.advanced().is_active();
        let mut results = Vec::new();
        let mut summary = FilenameSearchSummary {
            examined_directories: self.directories.len(),
            ..FilenameSearchSummary::default()
        };
        for indexed in &self.entries {
            if is_cancelled() {
                return Err(SearchIndexError::Cancelled);
            }
            summary.examined_entries = summary.examined_entries.saturating_add(1);
            let in_scope = request.scope() == FilenameSearchScope::Subtree
                || indexed.path.parent() == Some(self.root.as_path());
            let name = indexed
                .path
                .file_name()
                .ok_or(SearchIndexError::Malformed)?;
            let name_matches = in_scope && matcher.matches(name);
            if !name_matches && !validate_all_entries {
                continue;
            }
            let metadata =
                fs::symlink_metadata(&indexed.path).map_err(|_| SearchIndexError::Stale)?;
            if !indexed.fingerprint.matches(&metadata) {
                return Err(SearchIndexError::Stale);
            }
            if !name_matches {
                continue;
            }
            let entry = directory_entry(&indexed.path, name, indexed.kind, &metadata);
            let mut decision = request.advanced().evaluate(&entry, None);
            if let AdvancedFilterDecision::NeedsMetadata(needs) = decision {
                let facts = AdvancedMetadata {
                    mime: needs.mime.then(|| resolve_mime(&indexed.path)).flatten(),
                    owner_uid: needs.owner.then_some(metadata.uid()),
                };
                if needs.mime && facts.mime.is_none() {
                    summary.metadata_unavailable = summary.metadata_unavailable.saturating_add(1);
                }
                decision = request.advanced().evaluate(&entry, Some(&facts));
            }
            if decision != AdvancedFilterDecision::Match {
                continue;
            }
            if results.len() >= FILENAME_SEARCH_RESULT_CAPACITY {
                summary.truncated = true;
                break;
            }
            results.push(entry);
            summary.matched = summary.matched.saturating_add(1);
        }
        Ok((results, summary))
    }
}

pub fn build_search_index(
    request: &SearchIndexBuildRequest,
    limits: SearchIndexLimits,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<SearchIndex, SearchIndexError> {
    let limits = limits.validate()?;
    let root_metadata =
        fs::symlink_metadata(request.root()).map_err(SearchIndexError::RootMetadata)?;
    if root_metadata.file_type().is_symlink() {
        return Err(SearchIndexError::SymbolicRoot);
    }
    if !root_metadata.is_dir() {
        return Err(SearchIndexError::RootNotDirectory);
    }
    let root_device = root_metadata.dev();
    let mut directories = vec![IndexedDirectory {
        path: request.root().to_path_buf(),
        fingerprint: Fingerprint::from_metadata(&root_metadata),
    }];
    let mut entries = Vec::new();
    let mut summary = SearchIndexBuildSummary {
        indexed_directories: 1,
        ..SearchIndexBuildSummary::default()
    };
    let mut estimated_bytes = INDEX_MAGIC.len() + path_bytes(request.root()).len() * 2 + 128;
    let mut queue = VecDeque::from([(request.root().to_path_buf(), 0usize)]);

    'walk: while let Some((directory, depth)) = queue.pop_front() {
        if is_cancelled() {
            return Err(SearchIndexError::Cancelled);
        }
        let reader = match fs::read_dir(&directory) {
            Ok(reader) => reader,
            Err(_) => {
                summary.skipped_directories = summary.skipped_directories.saturating_add(1);
                continue;
            }
        };
        for child in reader {
            if is_cancelled() {
                return Err(SearchIndexError::Cancelled);
            }
            if entries.len() >= limits.entries {
                summary.truncated = true;
                break 'walk;
            }
            let child = match child {
                Ok(child) => child,
                Err(_) => {
                    summary.skipped_entries = summary.skipped_entries.saturating_add(1);
                    continue;
                }
            };
            let name = child.file_name();
            if is_hidden(&name) {
                summary.excluded_hidden = summary.excluded_hidden.saturating_add(1);
                continue;
            }
            let path = child.path();
            let raw_path = path_bytes(&path);
            if raw_path.len() > SEARCH_INDEX_PATH_BYTES {
                summary.skipped_entries = summary.skipped_entries.saturating_add(1);
                continue;
            }
            estimated_bytes = estimated_bytes.saturating_add(raw_path.len() * 2 + 160);
            if estimated_bytes > limits.serialized_bytes {
                summary.truncated = true;
                break 'walk;
            }
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    summary.skipped_entries = summary.skipped_entries.saturating_add(1);
                    continue;
                }
            };
            let file_type = metadata.file_type();
            let kind = if file_type.is_dir() {
                EntryKind::Directory
            } else if file_type.is_file() {
                EntryKind::RegularFile
            } else if file_type.is_symlink() {
                EntryKind::SymbolicLink {
                    target_is_directory: false,
                }
            } else {
                EntryKind::Other
            };
            entries.push(IndexedEntry {
                path: path.clone(),
                kind,
                fingerprint: Fingerprint::from_metadata(&metadata),
            });
            summary.indexed_entries = summary.indexed_entries.saturating_add(1);

            if file_type.is_dir() {
                if metadata.dev() != root_device {
                    summary.skipped_mounts = summary.skipped_mounts.saturating_add(1);
                } else if depth >= limits.depth {
                    summary.depth_limited = summary.depth_limited.saturating_add(1);
                    summary.truncated = true;
                } else if directories.len() >= limits.directories {
                    summary.truncated = true;
                } else {
                    directories.push(IndexedDirectory {
                        path: path.clone(),
                        fingerprint: Fingerprint::from_metadata(&metadata),
                    });
                    summary.indexed_directories = summary.indexed_directories.saturating_add(1);
                    queue.push_back((path, depth.saturating_add(1)));
                }
            }
        }
    }
    Ok(SearchIndex {
        root: request.root().to_path_buf(),
        directories,
        entries,
        summary,
    })
}

#[derive(Debug, Error)]
pub enum SearchIndexError {
    #[error("search index root must be an absolute local path")]
    RelativeRoot,
    #[error("search index path exceeds the reviewed byte limit")]
    PathTooLong,
    #[error("search index limits are invalid")]
    InvalidLimits,
    #[error("could not inspect search index root: {0}")]
    RootMetadata(#[source] std::io::Error),
    #[error("search index root cannot be a symbolic link")]
    SymbolicRoot,
    #[error("search index root is not a directory")]
    RootNotDirectory,
    #[error("search index operation was cancelled")]
    Cancelled,
    #[error("search index is not eligible for this query")]
    Ineligible,
    #[error("search index is stale")]
    Stale,
    #[error("search index has an unsupported version")]
    UnsupportedVersion,
    #[error("search index data is malformed")]
    Malformed,
    #[error("search index contains too many entries")]
    TooManyEntries,
    #[error("search index contains too many directories")]
    TooManyDirectories,
    #[error("search index exceeds the reviewed storage limit")]
    SerializedTooLarge,
}

fn directory_entry(
    path: &Path,
    name: &OsStr,
    kind: EntryKind,
    metadata: &fs::Metadata,
) -> DirectoryEntry {
    let size = matches!(kind, EntryKind::RegularFile).then_some(metadata.len());
    let modified = system_time(metadata.mtime(), metadata.mtime_nsec());
    let executable =
        matches!(kind, EntryKind::RegularFile) && metadata.permissions().mode() & 0o111 != 0;
    DirectoryEntry::new(
        path.to_path_buf(),
        name.to_os_string(),
        kind,
        size,
        modified,
        None,
        false,
        executable,
        ThumbnailState::NotRequested,
    )
}

fn system_time(seconds: i64, nanos: i64) -> Option<SystemTime> {
    let seconds = u64::try_from(seconds).ok()?;
    let nanos = u32::try_from(nanos).ok()?;
    if nanos >= 1_000_000_000 {
        return None;
    }
    Some(SystemTime::UNIX_EPOCH + Duration::new(seconds, nanos))
}

fn append_record(output: &mut String, tag: char, path: &Path, fingerprint: Fingerprint) {
    output.push(tag);
    output.push('\t');
    output.push_str(&hex_encode(path_bytes(path)));
    output.push('\t');
    output.push_str(&format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
        fingerprint.device,
        fingerprint.inode,
        fingerprint.mode,
        fingerprint.size,
        fingerprint.modified_seconds,
        fingerprint.modified_nanos,
        fingerprint.changed_seconds,
        fingerprint.changed_nanos
    ));
}

fn parse_record(line: &str) -> Result<(char, PathBuf, Fingerprint), SearchIndexError> {
    let mut fields = line.split('\t');
    let tag = fields
        .next()
        .and_then(|value| value.chars().next())
        .ok_or(SearchIndexError::Malformed)?;
    let path = PathBuf::from(OsString::from_vec(hex_decode(
        fields.next().ok_or(SearchIndexError::Malformed)?,
    )?));
    let mut number = || {
        fields
            .next()
            .ok_or(SearchIndexError::Malformed)?
            .parse::<i128>()
            .map_err(|_| SearchIndexError::Malformed)
    };
    let device = u64::try_from(number()?).map_err(|_| SearchIndexError::Malformed)?;
    let inode = u64::try_from(number()?).map_err(|_| SearchIndexError::Malformed)?;
    let mode = u32::try_from(number()?).map_err(|_| SearchIndexError::Malformed)?;
    let size = u64::try_from(number()?).map_err(|_| SearchIndexError::Malformed)?;
    let modified_seconds = i64::try_from(number()?).map_err(|_| SearchIndexError::Malformed)?;
    let modified_nanos = i64::try_from(number()?).map_err(|_| SearchIndexError::Malformed)?;
    let changed_seconds = i64::try_from(number()?).map_err(|_| SearchIndexError::Malformed)?;
    let changed_nanos = i64::try_from(number()?).map_err(|_| SearchIndexError::Malformed)?;
    if fields.next().is_some() {
        return Err(SearchIndexError::Malformed);
    }
    Ok((
        tag,
        path,
        Fingerprint {
            device,
            inode,
            mode,
            size,
            modified_seconds,
            modified_nanos,
            changed_seconds,
            changed_nanos,
        },
    ))
}

fn is_hidden(name: &OsStr) -> bool {
    name.as_bytes().first() == Some(&b'.')
}

fn path_bytes(path: &Path) -> &[u8] {
    path.as_os_str().as_bytes()
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn hex_decode(value: &str) -> Result<Vec<u8>, SearchIndexError> {
    if value.len() % 2 != 0 {
        return Err(SearchIndexError::Malformed);
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_nibble(pair[0])?;
            let low = hex_nibble(pair[1])?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn hex_nibble(value: u8) -> Result<u8, SearchIndexError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(SearchIndexError::Malformed),
    }
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, fs, os::unix::ffi::OsStringExt, os::unix::fs::symlink};

    use crate::{AdvancedFilter, FilenameSearchScope, FolderFilterMode};
    use tempfile::tempdir;

    use super::*;

    fn request(root: &Path, query: &str) -> FilenameSearchRequest {
        FilenameSearchRequest::new_with_filter(
            root.to_path_buf(),
            query.to_owned(),
            FilenameSearchScope::Subtree,
            false,
            FolderFilterMode::Text,
            AdvancedFilter::default(),
        )
        .expect("valid request")
    }

    #[test]
    fn phase_13f_build_is_bounded_exact_and_conservative() {
        let fixture = tempdir().expect("fixture");
        fs::create_dir(fixture.path().join("visible")).expect("visible directory");
        fs::write(fixture.path().join("visible/report.txt"), b"public").expect("file");
        fs::create_dir(fixture.path().join(".private")).expect("hidden directory");
        fs::write(fixture.path().join(".private/secret.txt"), b"secret").expect("hidden file");
        symlink(
            fixture.path().join("visible"),
            fixture.path().join("shortcut"),
        )
        .expect("symlink");
        let raw = OsString::from_vec(vec![b'r', 0x80, b'w']);
        fs::write(fixture.path().join(&raw), b"raw").expect("raw file");

        let index = build_search_index(
            &SearchIndexBuildRequest::new(fixture.path().to_path_buf()).expect("request"),
            SearchIndexLimits::default(),
            || false,
        )
        .expect("index");
        assert_eq!(index.summary().excluded_hidden, 1);
        assert!(index.entries.iter().any(|entry| entry.path.ends_with(&raw)));
        assert!(
            !index
                .entries
                .iter()
                .any(|entry| entry.path.ends_with("secret.txt"))
        );
        assert_eq!(
            index
                .entries
                .iter()
                .filter(|entry| entry.path.ends_with("report.txt"))
                .count(),
            1
        );
        assert!(
            index
                .entries
                .iter()
                .any(|entry| matches!(entry.kind, EntryKind::SymbolicLink { .. }))
        );

        let tiny = SearchIndexLimits {
            entries: 1,
            ..SearchIndexLimits::default()
        };
        let limited = build_search_index(
            &SearchIndexBuildRequest::new(fixture.path().to_path_buf()).expect("request"),
            tiny,
            || false,
        )
        .expect("limited index");
        assert!(limited.summary().truncated);
    }

    #[test]
    fn phase_13f_codec_preserves_raw_paths_and_rejects_corruption() {
        let fixture = tempdir().expect("fixture");
        let raw = OsString::from_vec(vec![b'n', 0xff]);
        fs::write(fixture.path().join(&raw), b"raw").expect("raw file");
        let index = build_search_index(
            &SearchIndexBuildRequest::new(fixture.path().to_path_buf()).expect("request"),
            SearchIndexLimits::default(),
            || false,
        )
        .expect("index");
        let encoded = index.serialize().expect("serialize");
        let decoded = SearchIndex::parse(&encoded).expect("parse");
        assert_eq!(decoded, index);
        assert!(
            decoded
                .entries
                .iter()
                .any(|entry| entry.path.ends_with(&raw))
        );
        assert!(matches!(
            SearchIndex::parse(b"FLOE-SEARCH-INDEX-99\n"),
            Err(SearchIndexError::UnsupportedVersion)
        ));
        assert!(SearchIndex::parse(b"FLOE-SEARCH-INDEX-1\nroot\tnot-hex\n").is_err());
        let hidden = format!(
            "FLOE-SEARCH-INDEX-1\nroot\t{}\nd\t{}\t1\t1\t0\t0\t0\t0\t0\t0\nF\t{}\t1\t2\t0\t0\t0\t0\t0\t0\n",
            hex_encode(path_bytes(fixture.path())),
            hex_encode(path_bytes(fixture.path())),
            hex_encode(path_bytes(&fixture.path().join(".secret")))
        );
        assert!(matches!(
            SearchIndex::parse(hidden.as_bytes()),
            Err(SearchIndexError::Malformed)
        ));
    }

    #[test]
    fn phase_13f_index_query_detects_stale_state_and_ineligible_hidden_policy() {
        let fixture = tempdir().expect("fixture");
        fs::write(fixture.path().join("alpha.txt"), b"one").expect("file");
        let index = build_search_index(
            &SearchIndexBuildRequest::new(fixture.path().to_path_buf()).expect("request"),
            SearchIndexLimits::default(),
            || false,
        )
        .expect("index");
        let (results, summary) = index
            .search_with_mime(&request(fixture.path(), "alpha"), || false, |_| None)
            .expect("indexed query");
        assert_eq!(results.len(), 1);
        assert_eq!(summary.matched, 1);

        let hidden_request = FilenameSearchRequest::new(
            fixture.path().to_path_buf(),
            "alpha".to_owned(),
            FilenameSearchScope::Subtree,
            true,
        )
        .expect("hidden request");
        assert!(matches!(
            index.search_with_mime(&hidden_request, || false, |_| None),
            Err(SearchIndexError::Ineligible)
        ));

        fs::write(fixture.path().join("beta.txt"), b"two").expect("new file");
        assert!(matches!(
            index.search_with_mime(&request(fixture.path(), "alpha"), || false, |_| None),
            Err(SearchIndexError::Stale)
        ));
    }
}
