//! Validated saved-search definitions and bounded session search history.

use std::{
    cmp::Ordering,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

#[cfg(unix)]
use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
};

use thiserror::Error;

use crate::{
    AdvancedFilter, ContentSearchError, ContentSearchRequest, DirectoryEntry, DirectorySort,
    FilenameSearchError, FilenameSearchRequest, FilenameSearchScope, FolderFilterMode, SortColumn,
    SortDirection,
};

pub const SAVED_SEARCH_CAPACITY: usize = 64;
pub const RECENT_SEARCH_CAPACITY: usize = 32;
pub const SAVED_SEARCH_NAME_CAPACITY: usize = 80;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchKind {
    Files,
    Contents,
}

impl SearchKind {
    pub const fn persisted(self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Contents => "contents",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        match value {
            "files" => Some(Self::Files),
            "contents" => Some(Self::Contents),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchQuery {
    root: PathBuf,
    kind: SearchKind,
    query: String,
    scope: FilenameSearchScope,
    include_hidden: bool,
    mode: FolderFilterMode,
    advanced: AdvancedFilter,
}

impl SearchQuery {
    pub fn new(
        root: PathBuf,
        kind: SearchKind,
        query: String,
        scope: FilenameSearchScope,
        include_hidden: bool,
        mode: FolderFilterMode,
        advanced: AdvancedFilter,
    ) -> Result<Self, SavedSearchError> {
        match kind {
            SearchKind::Files => {
                FilenameSearchRequest::new_with_filter(
                    root.clone(),
                    query.clone(),
                    scope,
                    include_hidden,
                    mode,
                    advanced.clone(),
                )?;
            }
            SearchKind::Contents => {
                ContentSearchRequest::new(
                    root.clone(),
                    query.clone(),
                    scope,
                    include_hidden,
                    mode,
                    advanced.clone(),
                )?;
            }
        }
        Ok(Self {
            root,
            kind,
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

    pub const fn kind(&self) -> SearchKind {
        self.kind
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

    pub fn filename_request(&self) -> Result<FilenameSearchRequest, SavedSearchError> {
        if self.kind != SearchKind::Files {
            return Err(SavedSearchError::WrongKind);
        }
        Ok(FilenameSearchRequest::new_with_filter(
            self.root.clone(),
            self.query.clone(),
            self.scope,
            self.include_hidden,
            self.mode,
            self.advanced.clone(),
        )?)
    }

    pub fn content_request(&self) -> Result<ContentSearchRequest, SavedSearchError> {
        if self.kind != SearchKind::Contents {
            return Err(SavedSearchError::WrongKind);
        }
        Ok(ContentSearchRequest::new(
            self.root.clone(),
            self.query.clone(),
            self.scope,
            self.include_hidden,
            self.mode,
            self.advanced.clone(),
        )?)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedSearch {
    id: u64,
    name: String,
    query: SearchQuery,
}

impl SavedSearch {
    pub fn new(id: u64, name: String, query: SearchQuery) -> Result<Self, SavedSearchError> {
        let name = name.trim().to_owned();
        if id == 0 {
            return Err(SavedSearchError::InvalidId);
        }
        if name.is_empty() {
            return Err(SavedSearchError::EmptyName);
        }
        if name.chars().count() > SAVED_SEARCH_NAME_CAPACITY {
            return Err(SavedSearchError::NameTooLong);
        }
        if name.chars().any(char::is_control) {
            return Err(SavedSearchError::InvalidName);
        }
        Ok(Self { id, name, query })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn query_definition(&self) -> &SearchQuery {
        &self.query
    }

    #[cfg(unix)]
    pub fn serialize_record(&self) -> String {
        let query = &self.query;
        let advanced = &query.advanced;
        [
            self.id.to_string(),
            hex_encode(self.name.as_bytes()),
            hex_encode(query.root.as_os_str().as_bytes()),
            query.kind.persisted().to_owned(),
            scope_id(query.scope).to_owned(),
            bool_id(query.include_hidden).to_owned(),
            mode_id(query.mode).to_owned(),
            hex_encode(query.query.as_bytes()),
            entry_type_id(advanced.entry_type).to_owned(),
            encode_text(advanced.extension.as_deref()),
            encode_text(advanced.mime.as_deref()),
            encode_number(advanced.minimum_size),
            encode_number(advanced.maximum_size),
            encode_time(advanced.modified_after),
            encode_time(advanced.modified_before),
            encode_number(
                advanced
                    .owner
                    .map(|crate::OwnerFilter::Uid(uid)| u64::from(uid)),
            ),
            hidden_id(advanced.hidden).to_owned(),
            bool_id(advanced.match_case).to_owned(),
        ]
        .join("\t")
    }

    #[cfg(unix)]
    pub fn parse_record(record: &str) -> Option<Self> {
        let mut fields = record.split('\t');
        let id = fields.next()?.parse().ok()?;
        let name = String::from_utf8(hex_decode(fields.next()?)?).ok()?;
        let root = PathBuf::from(OsString::from_vec(hex_decode(fields.next()?)?));
        let kind = SearchKind::from_persisted(fields.next()?)?;
        let scope = parse_scope(fields.next()?)?;
        let include_hidden = parse_bool(fields.next()?)?;
        let mode = parse_mode(fields.next()?)?;
        let text = String::from_utf8(hex_decode(fields.next()?)?).ok()?;
        let advanced = AdvancedFilter {
            entry_type: parse_entry_type(fields.next()?)?,
            extension: decode_text(fields.next()?)?,
            mime: decode_text(fields.next()?)?,
            minimum_size: decode_number(fields.next()?)?,
            maximum_size: decode_number(fields.next()?)?,
            modified_after: decode_time(fields.next()?)?,
            modified_before: decode_time(fields.next()?)?,
            owner: decode_number(fields.next()?)?
                .and_then(|uid| u32::try_from(uid).ok())
                .map(crate::OwnerFilter::Uid),
            hidden: parse_hidden(fields.next()?)?,
            match_case: parse_bool(fields.next()?)?,
        };
        if fields.next().is_some() {
            return None;
        }
        let query =
            SearchQuery::new(root, kind, text, scope, include_hidden, mode, advanced).ok()?;
        Self::new(id, name, query).ok()
    }
}

const fn scope_id(value: FilenameSearchScope) -> &'static str {
    match value {
        FilenameSearchScope::CurrentFolder => "folder",
        FilenameSearchScope::Subtree => "subtree",
    }
}

fn parse_scope(value: &str) -> Option<FilenameSearchScope> {
    match value {
        "folder" => Some(FilenameSearchScope::CurrentFolder),
        "subtree" => Some(FilenameSearchScope::Subtree),
        _ => None,
    }
}

const fn mode_id(value: FolderFilterMode) -> &'static str {
    match value {
        FolderFilterMode::Text => "text",
        FolderFilterMode::Glob => "glob",
        FolderFilterMode::Regex => "regex",
    }
}

fn parse_mode(value: &str) -> Option<FolderFilterMode> {
    match value {
        "text" => Some(FolderFilterMode::Text),
        "glob" => Some(FolderFilterMode::Glob),
        "regex" => Some(FolderFilterMode::Regex),
        _ => None,
    }
}

const fn entry_type_id(value: crate::EntryTypeFilter) -> &'static str {
    match value {
        crate::EntryTypeFilter::Any => "any",
        crate::EntryTypeFilter::File => "file",
        crate::EntryTypeFilter::Folder => "folder",
        crate::EntryTypeFilter::SymbolicLink => "link",
        crate::EntryTypeFilter::Other => "other",
    }
}

fn parse_entry_type(value: &str) -> Option<crate::EntryTypeFilter> {
    match value {
        "any" => Some(crate::EntryTypeFilter::Any),
        "file" => Some(crate::EntryTypeFilter::File),
        "folder" => Some(crate::EntryTypeFilter::Folder),
        "link" => Some(crate::EntryTypeFilter::SymbolicLink),
        "other" => Some(crate::EntryTypeFilter::Other),
        _ => None,
    }
}

const fn hidden_id(value: crate::HiddenFilter) -> &'static str {
    match value {
        crate::HiddenFilter::CurrentSetting => "current",
        crate::HiddenFilter::Include => "include",
        crate::HiddenFilter::Only => "only",
    }
}

fn parse_hidden(value: &str) -> Option<crate::HiddenFilter> {
    match value {
        "current" => Some(crate::HiddenFilter::CurrentSetting),
        "include" => Some(crate::HiddenFilter::Include),
        "only" => Some(crate::HiddenFilter::Only),
        _ => None,
    }
}

const fn bool_id(value: bool) -> &'static str {
    if value { "1" } else { "0" }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

#[cfg(unix)]
fn encode_text(value: Option<&str>) -> String {
    value
        .map(|text| hex_encode(text.as_bytes()))
        .unwrap_or_else(|| "-".to_owned())
}

#[cfg(unix)]
fn decode_text(value: &str) -> Option<Option<String>> {
    if value == "-" {
        Some(None)
    } else {
        String::from_utf8(hex_decode(value)?).ok().map(Some)
    }
}

fn encode_number(value: Option<u64>) -> String {
    value.map_or_else(|| "-".to_owned(), |number| number.to_string())
}

fn decode_number(value: &str) -> Option<Option<u64>> {
    if value == "-" {
        Some(None)
    } else {
        value.parse().ok().map(Some)
    }
}

fn encode_time(value: Option<SystemTime>) -> String {
    value.map_or_else(
        || "-".to_owned(),
        |time| match time.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(duration) => format!("+{}", duration.as_secs()),
            Err(error) => format!("-{}", error.duration().as_secs()),
        },
    )
}

fn decode_time(value: &str) -> Option<Option<SystemTime>> {
    if value == "-" {
        return Some(None);
    }
    let (sign, seconds) = value.split_at(1);
    let duration = Duration::from_secs(seconds.parse().ok()?);
    match sign {
        "+" => SystemTime::UNIX_EPOCH.checked_add(duration).map(Some),
        "-" => SystemTime::UNIX_EPOCH.checked_sub(duration).map(Some),
        _ => None,
    }
}

#[cfg(unix)]
fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(unix)]
fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Some((hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?))
        .collect()
}

#[cfg(unix)]
const fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SavedSearchCatalog {
    entries: Vec<SavedSearch>,
}

impl SavedSearchCatalog {
    pub fn from_entries(entries: impl IntoIterator<Item = SavedSearch>) -> Self {
        let mut catalog = Self::default();
        for entry in entries {
            let _ = catalog.add(entry);
        }
        catalog
    }

    pub fn entries(&self) -> &[SavedSearch] {
        &self.entries
    }

    pub fn add(&mut self, entry: SavedSearch) -> Result<(), SavedSearchError> {
        if self.entries.len() >= SAVED_SEARCH_CAPACITY {
            return Err(SavedSearchError::CatalogFull);
        }
        if self.entries.iter().any(|existing| existing.id == entry.id) {
            return Err(SavedSearchError::DuplicateId);
        }
        if self
            .entries
            .iter()
            .any(|existing| existing.name.eq_ignore_ascii_case(&entry.name))
        {
            return Err(SavedSearchError::DuplicateName);
        }
        self.entries.push(entry);
        self.entries.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(())
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        before != self.entries.len()
    }

    pub fn get(&self, id: u64) -> Option<&SavedSearch> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn next_id(&self) -> Result<u64, SavedSearchError> {
        (1..=u64::MAX)
            .find(|candidate| self.entries.iter().all(|entry| entry.id != *candidate))
            .ok_or(SavedSearchError::NoAvailableId)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchHistoryPolicy {
    Record,
    Suppress,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecentSearches {
    entries: Vec<SearchQuery>,
}

impl RecentSearches {
    pub fn entries(&self) -> &[SearchQuery] {
        &self.entries
    }

    pub fn record(&mut self, query: SearchQuery, policy: SearchHistoryPolicy) {
        if policy == SearchHistoryPolicy::Suppress {
            return;
        }
        self.entries.retain(|entry| entry != &query);
        self.entries.insert(0, query);
        self.entries.truncate(RECENT_SEARCH_CAPACITY);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SearchResultOrder {
    #[default]
    Name,
    ModifiedNewest,
    SizeLargest,
}

impl SearchResultOrder {
    pub const ALL: [Self; 3] = [Self::Name, Self::ModifiedNewest, Self::SizeLargest];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::ModifiedNewest => "Modified (newest)",
            Self::SizeLargest => "Size (largest)",
        }
    }

    pub fn compare(self, left: &DirectoryEntry, right: &DirectoryEntry) -> Ordering {
        let sort = match self {
            Self::Name => DirectorySort::new(SortColumn::Name, SortDirection::Ascending),
            Self::ModifiedNewest => {
                DirectorySort::new(SortColumn::Modified, SortDirection::Descending)
            }
            Self::SizeLargest => DirectorySort::new(SortColumn::Size, SortDirection::Descending),
        };
        sort.compare_entries(left, right)
    }
}

#[derive(Debug, Error)]
pub enum SavedSearchError {
    #[error("saved search ID must be non-zero")]
    InvalidId,
    #[error("saved search name cannot be empty")]
    EmptyName,
    #[error("saved search name exceeds {SAVED_SEARCH_NAME_CAPACITY} characters")]
    NameTooLong,
    #[error("saved search name cannot contain control characters")]
    InvalidName,
    #[error("saved search catalog already contains this ID")]
    DuplicateId,
    #[error("saved search catalog already contains this name")]
    DuplicateName,
    #[error("saved search catalog is full")]
    CatalogFull,
    #[error("no saved search ID is available")]
    NoAvailableId,
    #[error("saved search has the wrong search kind")]
    WrongKind,
    #[error(transparent)]
    Filename(#[from] FilenameSearchError),
    #[error(transparent)]
    Content(#[from] ContentSearchError),
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        sync::Arc,
        time::{Duration, SystemTime},
    };

    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt;

    use crate::{EntryKind, ThumbnailState};

    use super::*;

    fn query(root: PathBuf, kind: SearchKind, text: &str) -> SearchQuery {
        SearchQuery::new(
            root,
            kind,
            text.to_owned(),
            FilenameSearchScope::Subtree,
            true,
            FolderFilterMode::Text,
            AdvancedFilter::default(),
        )
        .expect("valid saved query")
    }

    #[test]
    fn phase_13e_validates_catalog_names_ids_queries_and_capacity() {
        let root = PathBuf::from("/tmp/saved-search");
        let mut catalog = SavedSearchCatalog::default();
        catalog
            .add(
                SavedSearch::new(
                    2,
                    "Work".to_owned(),
                    query(root.clone(), SearchKind::Files, "rs"),
                )
                .expect("saved search"),
            )
            .expect("catalog add");
        catalog
            .add(
                SavedSearch::new(
                    1,
                    "Archive".to_owned(),
                    query(root.clone(), SearchKind::Contents, "needle"),
                )
                .expect("saved search"),
            )
            .expect("catalog add");
        assert_eq!(
            catalog
                .entries()
                .iter()
                .map(SavedSearch::name)
                .collect::<Vec<_>>(),
            ["Archive", "Work"]
        );
        assert!(matches!(
            catalog.add(
                SavedSearch::new(
                    3,
                    "work".to_owned(),
                    query(root.clone(), SearchKind::Files, "x")
                )
                .expect("saved search")
            ),
            Err(SavedSearchError::DuplicateName)
        ));
        assert!(
            SavedSearch::new(
                0,
                "No ID".to_owned(),
                query(root.clone(), SearchKind::Files, "x")
            )
            .is_err()
        );
        assert!(
            SearchQuery::new(
                PathBuf::from("relative"),
                SearchKind::Files,
                "x".to_owned(),
                FilenameSearchScope::CurrentFolder,
                false,
                FolderFilterMode::Text,
                AdvancedFilter::default()
            )
            .is_err()
        );

        for id in 3..=SAVED_SEARCH_CAPACITY as u64 {
            catalog
                .add(
                    SavedSearch::new(
                        id,
                        format!("Saved {id}"),
                        query(root.clone(), SearchKind::Files, &format!("q{id}")),
                    )
                    .expect("saved search"),
                )
                .expect("catalog add");
        }
        assert!(matches!(
            catalog.add(
                SavedSearch::new(
                    1000,
                    "Overflow".to_owned(),
                    query(root, SearchKind::Files, "overflow")
                )
                .expect("saved search")
            ),
            Err(SavedSearchError::CatalogFull)
        ));
    }

    #[test]
    fn phase_13e_recent_searches_are_memory_model_bounded_deduplicated_and_suppressible() {
        let root = PathBuf::from("/tmp/recent");
        let mut recent = RecentSearches::default();
        let first = query(root.clone(), SearchKind::Files, "first");
        recent.record(first.clone(), SearchHistoryPolicy::Suppress);
        assert!(recent.entries().is_empty());
        recent.record(first.clone(), SearchHistoryPolicy::Record);
        recent.record(first, SearchHistoryPolicy::Record);
        assert_eq!(recent.entries().len(), 1);
        for index in 0..=RECENT_SEARCH_CAPACITY {
            recent.record(
                query(root.clone(), SearchKind::Files, &format!("query-{index}")),
                SearchHistoryPolicy::Record,
            );
        }
        assert_eq!(recent.entries().len(), RECENT_SEARCH_CAPACITY);
        assert_eq!(
            recent.entries()[0].query(),
            format!("query-{RECENT_SEARCH_CAPACITY}")
        );
        recent.clear();
        assert!(recent.entries().is_empty());
    }

    #[test]
    fn phase_13e_preserves_exact_raw_root_and_all_query_fields() {
        #[cfg(unix)]
        let root = PathBuf::from("/tmp").join(OsString::from_vec(vec![b'r', 0x80]));
        #[cfg(not(unix))]
        let root = PathBuf::from("/tmp/raw-root");
        let advanced = AdvancedFilter {
            entry_type: crate::EntryTypeFilter::File,
            extension: Some("TXT".to_owned()),
            mime: Some("text/*".to_owned()),
            match_case: true,
            minimum_size: Some(7),
            maximum_size: Some(99),
            modified_after: Some(SystemTime::UNIX_EPOCH - Duration::from_secs(5)),
            modified_before: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(500)),
            owner: Some(crate::OwnerFilter::Uid(1000)),
            hidden: crate::HiddenFilter::Include,
        };
        let query = SearchQuery::new(
            root.clone(),
            SearchKind::Contents,
            "Needle".to_owned(),
            FilenameSearchScope::CurrentFolder,
            true,
            FolderFilterMode::Regex,
            advanced.clone(),
        )
        .expect("saved query");
        assert_eq!(query.root(), root);
        assert_eq!(query.kind(), SearchKind::Contents);
        assert_eq!(query.scope(), FilenameSearchScope::CurrentFolder);
        assert_eq!(query.mode(), FolderFilterMode::Regex);
        assert!(query.include_hidden());
        assert_eq!(query.advanced(), &advanced);
        assert!(query.filename_request().is_err());
        assert!(query.content_request().is_ok());
        let saved = SavedSearch::new(9, "Raw search".to_owned(), query).expect("saved search");
        #[cfg(unix)]
        assert_eq!(
            SavedSearch::parse_record(&saved.serialize_record()),
            Some(saved)
        );
    }

    #[test]
    fn phase_13e_result_order_is_deterministic_for_name_modified_and_size() {
        let make = |name: &str, size: u64, modified: u64| {
            Arc::new(DirectoryEntry::new(
                PathBuf::from("/tmp").join(name),
                OsString::from(name),
                EntryKind::RegularFile,
                Some(size),
                Some(SystemTime::UNIX_EPOCH + Duration::from_secs(modified)),
                None,
                false,
                false,
                ThumbnailState::NotRequested,
            ))
        };
        let alpha = make("alpha", 2, 10);
        let beta = make("beta", 9, 20);
        assert_eq!(
            SearchResultOrder::Name.compare(&alpha, &beta),
            Ordering::Less
        );
        assert_eq!(
            SearchResultOrder::ModifiedNewest.compare(&alpha, &beta),
            Ordering::Greater
        );
        assert_eq!(
            SearchResultOrder::SizeLargest.compare(&alpha, &beta),
            Ordering::Greater
        );
    }
}
