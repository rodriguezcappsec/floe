//! Bounded, cancellable local filename traversal without GTK or content reads.

use std::{
    collections::VecDeque,
    fs,
    os::unix::{ffi::OsStrExt, fs::MetadataExt, fs::PermissionsExt},
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::{
    AdvancedFilter, AdvancedFilterDecision, AdvancedFilterError, AdvancedMetadata, DirectoryEntry,
    EntryKind, FolderFilterError, FolderFilterMode, FolderFilterPattern, HiddenFilter,
    ThumbnailState,
};

pub const FILENAME_SEARCH_BATCH_CAPACITY: usize = 128;
pub const FILENAME_SEARCH_RESULT_CAPACITY: usize = 100_000;
pub const FILENAME_SEARCH_ENTRY_CAPACITY: usize = 1_000_000;
pub const FILENAME_SEARCH_DIRECTORY_CAPACITY: usize = 100_000;
pub const FILENAME_SEARCH_DEPTH_CAPACITY: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilenameSearchScope {
    CurrentFolder,
    Subtree,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilenameSearchRequest {
    root: PathBuf,
    query: String,
    scope: FilenameSearchScope,
    include_hidden: bool,
    mode: FolderFilterMode,
    advanced: AdvancedFilter,
}

impl FilenameSearchRequest {
    pub fn new(
        root: PathBuf,
        query: String,
        scope: FilenameSearchScope,
        include_hidden: bool,
    ) -> Result<Self, FilenameSearchError> {
        Self::new_with_filter(
            root,
            query,
            scope,
            include_hidden,
            FolderFilterMode::Text,
            AdvancedFilter::default(),
        )
    }

    pub fn new_with_filter(
        root: PathBuf,
        query: String,
        scope: FilenameSearchScope,
        show_hidden: bool,
        mode: FolderFilterMode,
        advanced: AdvancedFilter,
    ) -> Result<Self, FilenameSearchError> {
        if !root.is_absolute() {
            return Err(FilenameSearchError::RelativeRoot);
        }
        if query.is_empty() && !advanced.is_active() {
            return Err(FilenameSearchError::EmptyQuery);
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
pub struct FilenameSearchLimits {
    pub results: usize,
    pub entries: usize,
    pub directories: usize,
    pub depth: usize,
}

impl Default for FilenameSearchLimits {
    fn default() -> Self {
        Self {
            results: FILENAME_SEARCH_RESULT_CAPACITY,
            entries: FILENAME_SEARCH_ENTRY_CAPACITY,
            directories: FILENAME_SEARCH_DIRECTORY_CAPACITY,
            depth: FILENAME_SEARCH_DEPTH_CAPACITY,
        }
    }
}

impl FilenameSearchLimits {
    fn validate(self) -> Result<Self, FilenameSearchError> {
        if self.results == 0 || self.entries == 0 || self.directories == 0 {
            return Err(FilenameSearchError::InvalidLimits);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FilenameSearchSummary {
    pub matched: usize,
    pub examined_entries: usize,
    pub examined_directories: usize,
    pub skipped_entries: usize,
    pub skipped_directories: usize,
    pub skipped_mounts: usize,
    pub depth_limited: usize,
    pub metadata_unavailable: usize,
    pub truncated: bool,
}

#[derive(Debug, Error)]
pub enum FilenameSearchError {
    #[error("search root must be an absolute local path")]
    RelativeRoot,
    #[error("enter a filename to search for")]
    EmptyQuery,
    #[error(transparent)]
    Pattern(#[from] FolderFilterError),
    #[error(transparent)]
    Advanced(#[from] AdvancedFilterError),
    #[error("search limits must retain at least one result, entry, and directory")]
    InvalidLimits,
    #[error("could not inspect search root: {0}")]
    RootMetadata(#[source] std::io::Error),
    #[error("search root is a symbolic link; choose its real directory instead")]
    SymbolicRoot,
    #[error("search root is not a directory")]
    RootNotDirectory,
    #[error("could not open search root: {0}")]
    OpenRoot(#[source] std::io::Error),
    #[error("filename search was cancelled")]
    Cancelled,
    #[error("filename search result consumer stopped")]
    ConsumerStopped,
}

/// Searches names only and streams batches no larger than 128 entries.
///
/// Child lookup failures are counted in the summary. The root is authoritative:
/// an inaccessible, symbolic, or non-directory root returns an explicit error.
pub fn search_filenames(
    request: &FilenameSearchRequest,
    limits: FilenameSearchLimits,
    is_cancelled: impl FnMut() -> bool,
    on_batch: impl FnMut(Vec<DirectoryEntry>, FilenameSearchSummary) -> bool,
) -> Result<FilenameSearchSummary, FilenameSearchError> {
    search_filenames_with_mime(request, limits, is_cancelled, |_| None, on_batch)
}

/// Application-worker variant that resolves MIME lazily without making core
/// depend on GIO. The resolver runs only after cheap predicates and name match.
pub fn search_filenames_with_mime(
    request: &FilenameSearchRequest,
    limits: FilenameSearchLimits,
    mut is_cancelled: impl FnMut() -> bool,
    mut resolve_mime: impl FnMut(&Path) -> Option<String>,
    mut on_batch: impl FnMut(Vec<DirectoryEntry>, FilenameSearchSummary) -> bool,
) -> Result<FilenameSearchSummary, FilenameSearchError> {
    let limits = limits.validate()?;
    let matcher = FolderFilterPattern::compile_with_case(
        request.mode(),
        request.query(),
        request.advanced().match_case,
    )?;
    let root_metadata =
        fs::symlink_metadata(request.root()).map_err(FilenameSearchError::RootMetadata)?;
    if root_metadata.file_type().is_symlink() {
        return Err(FilenameSearchError::SymbolicRoot);
    }
    if !root_metadata.is_dir() {
        return Err(FilenameSearchError::RootNotDirectory);
    }
    fs::read_dir(request.root()).map_err(FilenameSearchError::OpenRoot)?;

    let root_device = root_metadata.dev();
    let mut queue = VecDeque::from([(request.root().to_path_buf(), 0_usize)]);
    let mut scheduled_directories = 1_usize;
    let mut summary = FilenameSearchSummary::default();
    let mut batch = Vec::with_capacity(FILENAME_SEARCH_BATCH_CAPACITY.min(limits.results));

    'walk: while let Some((directory, depth)) = queue.pop_front() {
        if is_cancelled() {
            return Err(FilenameSearchError::Cancelled);
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
                return Err(FilenameSearchError::Cancelled);
            }
            if summary.examined_entries >= limits.entries {
                summary.truncated = true;
                break 'walk;
            }
            summary.examined_entries = summary.examined_entries.saturating_add(1);
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

            let entry = directory_entry(path.clone(), name.clone(), kind, &metadata);
            let advanced_match = if matcher.matches(&name) {
                match request.advanced().evaluate(&entry, None) {
                    AdvancedFilterDecision::Match => true,
                    AdvancedFilterDecision::NoMatch => false,
                    AdvancedFilterDecision::NeedsMetadata(needs) => {
                        let facts = AdvancedMetadata {
                            mime: needs.mime.then(|| resolve_mime(&path)).flatten(),
                            owner_uid: needs.owner.then(|| metadata.uid()),
                        };
                        if (needs.mime && facts.mime.is_none())
                            || (needs.owner && facts.owner_uid.is_none())
                        {
                            summary.metadata_unavailable =
                                summary.metadata_unavailable.saturating_add(1);
                        }
                        request.advanced().evaluate(&entry, Some(&facts))
                            == AdvancedFilterDecision::Match
                    }
                }
            } else {
                false
            };
            if advanced_match {
                if summary.matched >= limits.results {
                    summary.truncated = true;
                    break 'walk;
                }
                batch.push(entry);
                summary.matched = summary.matched.saturating_add(1);
                if batch.len() == FILENAME_SEARCH_BATCH_CAPACITY {
                    if !on_batch(batch, summary) {
                        return Err(FilenameSearchError::ConsumerStopped);
                    }
                    batch = Vec::with_capacity(FILENAME_SEARCH_BATCH_CAPACITY);
                }
            }

            if request.scope() != FilenameSearchScope::Subtree || !file_type.is_dir() {
                continue;
            }
            if crosses_filesystem(root_device, metadata.dev()) {
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
        }
    }

    if !batch.is_empty() && !on_batch(batch, summary) {
        return Err(FilenameSearchError::ConsumerStopped);
    }
    Ok(summary)
}

fn crosses_filesystem(root_device: u64, candidate_device: u64) -> bool {
    root_device != candidate_device
}

fn directory_entry(
    path: PathBuf,
    name: std::ffi::OsString,
    kind: EntryKind,
    metadata: &fs::Metadata,
) -> DirectoryEntry {
    let size = matches!(kind, EntryKind::RegularFile).then_some(metadata.len());
    let modified = metadata.modified().ok();
    let executable =
        matches!(kind, EntryKind::RegularFile) && metadata.permissions().mode() & 0o111 != 0;
    DirectoryEntry::new(
        path,
        name.clone(),
        kind,
        size,
        modified,
        None,
        is_hidden(&name),
        executable,
        ThumbnailState::NotRequested,
    )
}

fn is_hidden(name: &std::ffi::OsStr) -> bool {
    name.as_bytes().first() == Some(&b'.')
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::{OsStr, OsString},
        fs,
        os::unix::ffi::OsStringExt,
        os::unix::fs::symlink,
    };

    use tempfile::tempdir;

    use super::*;

    fn collect(
        request: &FilenameSearchRequest,
        limits: FilenameSearchLimits,
    ) -> Result<(Vec<DirectoryEntry>, FilenameSearchSummary), FilenameSearchError> {
        let mut entries = Vec::new();
        let summary = search_filenames(
            request,
            limits,
            || false,
            |mut batch, _| {
                entries.append(&mut batch);
                true
            },
        )?;
        Ok((entries, summary))
    }

    #[test]
    fn phase_13b_filename_search_separates_current_folder_and_subtree() {
        let root = tempdir().expect("search fixture");
        fs::write(root.path().join("Report.txt"), b"root").expect("root file");
        fs::create_dir(root.path().join("nested")).expect("nested folder");
        fs::write(root.path().join("nested/report-final.txt"), b"nested").expect("nested file");

        let current = FilenameSearchRequest::new(
            root.path().to_path_buf(),
            "report".to_owned(),
            FilenameSearchScope::CurrentFolder,
            false,
        )
        .expect("current request");
        let (current_entries, current_summary) =
            collect(&current, FilenameSearchLimits::default()).expect("current search");
        assert_eq!(current_entries.len(), 1);
        assert_eq!(current_summary.matched, 1);

        let subtree = FilenameSearchRequest::new(
            root.path().to_path_buf(),
            "REPORT".to_owned(),
            FilenameSearchScope::Subtree,
            false,
        )
        .expect("subtree request");
        let (subtree_entries, subtree_summary) =
            collect(&subtree, FilenameSearchLimits::default()).expect("subtree search");
        assert_eq!(subtree_entries.len(), 2);
        assert_eq!(subtree_summary.examined_directories, 2);
    }

    #[test]
    fn phase_13b_filename_search_preserves_raw_names_and_never_descends_symlinks() {
        let root = tempdir().expect("search fixture");
        let raw_name = OsString::from_vec(b"raw-\xff-report.txt".to_vec());
        let raw_path = root.path().join(&raw_name);
        fs::write(&raw_path, b"raw").expect("raw file");
        fs::create_dir(root.path().join("outside")).expect("outside folder");
        fs::write(root.path().join("outside/secret-report.txt"), b"secret").expect("outside file");
        symlink(
            root.path().join("outside"),
            root.path().join("linked-report"),
        )
        .expect("directory symlink");

        let request = FilenameSearchRequest::new(
            root.path().to_path_buf(),
            "report".to_owned(),
            FilenameSearchScope::Subtree,
            false,
        )
        .expect("request");
        let (entries, _) = collect(&request, FilenameSearchLimits::default()).expect("search");
        assert!(entries.iter().any(|entry| entry.path() == raw_path));
        assert!(entries.iter().any(|entry| {
            entry.display_name() == OsStr::new("linked-report")
                && matches!(entry.kind(), EntryKind::SymbolicLink { .. })
        }));
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.display_name() == OsStr::new("secret-report.txt"))
                .count(),
            1
        );

        let linked_root = FilenameSearchRequest::new(
            root.path().join("linked-report"),
            "secret".to_owned(),
            FilenameSearchScope::Subtree,
            false,
        )
        .expect("typed linked-root request");
        assert!(matches!(
            collect(&linked_root, FilenameSearchLimits::default()),
            Err(FilenameSearchError::SymbolicRoot)
        ));
    }

    #[test]
    fn phase_13b_filename_search_cancels_streams_and_reports_explicit_caps() {
        let root = tempdir().expect("search fixture");
        for index in 0..300 {
            fs::write(root.path().join(format!("match-{index}.txt")), b"x").expect("fixture file");
        }
        let request = FilenameSearchRequest::new(
            root.path().to_path_buf(),
            "match".to_owned(),
            FilenameSearchScope::Subtree,
            false,
        )
        .expect("request");
        let mut checks = 0;
        assert!(matches!(
            search_filenames(
                &request,
                FilenameSearchLimits::default(),
                || {
                    checks += 1;
                    checks > 2
                },
                |_, _| true,
            ),
            Err(FilenameSearchError::Cancelled)
        ));

        let mut batch_lengths = Vec::new();
        let summary = search_filenames(
            &request,
            FilenameSearchLimits::default(),
            || false,
            |batch, _| {
                batch_lengths.push(batch.len());
                true
            },
        )
        .expect("streaming search");
        assert_eq!(batch_lengths, [128, 128, 44]);
        assert_eq!(summary.matched, 300);

        let (entries, summary) = collect(
            &request,
            FilenameSearchLimits {
                results: 3,
                ..FilenameSearchLimits::default()
            },
        )
        .expect("capped search");
        assert_eq!(entries.len(), 3);
        assert!(summary.truncated);
    }

    #[test]
    fn phase_13b_filename_search_validates_roots_queries_limits_and_boundaries() {
        assert!(matches!(
            FilenameSearchRequest::new(
                PathBuf::from("relative"),
                "name".to_owned(),
                FilenameSearchScope::CurrentFolder,
                false,
            ),
            Err(FilenameSearchError::RelativeRoot)
        ));
        let root = tempdir().expect("search fixture");
        assert!(matches!(
            FilenameSearchRequest::new(
                root.path().to_path_buf(),
                String::new(),
                FilenameSearchScope::CurrentFolder,
                false,
            ),
            Err(FilenameSearchError::EmptyQuery)
        ));
        let over_capacity = "x".repeat(crate::FOLDER_FILTER_QUERY_CAPACITY + 1);
        assert!(matches!(
            FilenameSearchRequest::new(
                root.path().to_path_buf(),
                over_capacity,
                FilenameSearchScope::CurrentFolder,
                false,
            ),
            Err(FilenameSearchError::Pattern(
                FolderFilterError::QueryTooLong
            ))
        ));
        let request = FilenameSearchRequest::new(
            root.path().to_path_buf(),
            "name".to_owned(),
            FilenameSearchScope::Subtree,
            false,
        )
        .expect("request");
        assert!(matches!(
            collect(
                &request,
                FilenameSearchLimits {
                    results: 0,
                    ..FilenameSearchLimits::default()
                }
            ),
            Err(FilenameSearchError::InvalidLimits)
        ));
        assert!(!crosses_filesystem(42, 42));
        assert!(crosses_filesystem(42, 43));
    }

    #[test]
    fn phase_13c_filename_search_combines_case_glob_hidden_and_extension() {
        let root = tempdir().expect("advanced search fixture");
        fs::write(root.path().join("Report.TXT"), b"large enough").expect("report");
        fs::write(root.path().join("report.txt"), b"large enough").expect("report lower");
        fs::write(root.path().join(".secret.TXT"), b"large enough").expect("hidden");

        let request = FilenameSearchRequest::new_with_filter(
            root.path().to_path_buf(),
            "*.TXT".to_owned(),
            FilenameSearchScope::CurrentFolder,
            false,
            FolderFilterMode::Glob,
            AdvancedFilter {
                extension: Some("TXT".to_owned()),
                minimum_size: Some(5),
                match_case: true,
                ..AdvancedFilter::default()
            },
        )
        .expect("advanced request");
        let (entries, _) = collect(&request, FilenameSearchLimits::default()).expect("search");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].display_name(), OsStr::new("Report.TXT"));

        let hidden_only = FilenameSearchRequest::new_with_filter(
            root.path().to_path_buf(),
            String::new(),
            FilenameSearchScope::CurrentFolder,
            false,
            FolderFilterMode::Text,
            AdvancedFilter {
                hidden: HiddenFilter::Only,
                ..AdvancedFilter::default()
            },
        )
        .expect("predicate-only request");
        let (entries, _) = collect(&hidden_only, FilenameSearchLimits::default()).expect("search");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].display_name(), OsStr::new(".secret.TXT"));
    }

    #[test]
    fn phase_13c_filename_search_resolves_mime_only_after_cheap_predicates() {
        let root = tempdir().expect("MIME search fixture");
        fs::write(root.path().join("photo.png"), b"png").expect("photo");
        fs::write(root.path().join("notes.txt"), b"text").expect("notes");
        let request = FilenameSearchRequest::new_with_filter(
            root.path().to_path_buf(),
            String::new(),
            FilenameSearchScope::CurrentFolder,
            false,
            FolderFilterMode::Text,
            AdvancedFilter {
                extension: Some("png".to_owned()),
                mime: Some("image/*".to_owned()),
                ..AdvancedFilter::default()
            },
        )
        .expect("MIME request");
        let mut resolver_calls = 0;
        let mut entries = Vec::new();
        let summary = search_filenames_with_mime(
            &request,
            FilenameSearchLimits::default(),
            || false,
            |_| {
                resolver_calls += 1;
                Some("image/png".to_owned())
            },
            |mut batch, _| {
                entries.append(&mut batch);
                true
            },
        )
        .expect("MIME search");
        assert_eq!(resolver_calls, 1);
        assert_eq!(entries.len(), 1);
        assert_eq!(summary.metadata_unavailable, 0);
    }
}
