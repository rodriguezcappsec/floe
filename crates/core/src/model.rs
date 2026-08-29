use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    time::SystemTime,
};

/// The inexpensive file kind information loaded during initial enumeration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryKind {
    Directory,
    RegularFile,
    SymbolicLink { target_is_directory: bool },
    Other,
}

/// Thumbnail work is deliberately lazy and belongs to a later phase.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThumbnailState {
    #[default]
    NotRequested,
}

/// Standards metadata attached to an item enumerated from a local Trash root.
///
/// Every path remains an exact platform path. Display code may render it
/// lossily, but must never recreate a restore target from that rendering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrashMetadata {
    original_path: Option<PathBuf>,
    deletion_date: Option<String>,
    info_path: Option<PathBuf>,
}

impl TrashMetadata {
    pub fn new(
        original_path: Option<PathBuf>,
        deletion_date: Option<String>,
        info_path: Option<PathBuf>,
    ) -> Self {
        Self {
            original_path,
            deletion_date,
            info_path,
        }
    }

    pub fn original_path(&self) -> Option<&Path> {
        self.original_path.as_deref()
    }

    pub fn deletion_date(&self) -> Option<&str> {
        self.deletion_date.as_deref()
    }

    pub fn info_path(&self) -> Option<&Path> {
        self.info_path.as_deref()
    }
}

/// A local directory entry that always retains its original platform path.
#[derive(Clone, Debug)]
pub struct DirectoryEntry {
    path: PathBuf,
    display_name: OsString,
    kind: EntryKind,
    size: Option<u64>,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    accessed: Option<SystemTime>,
    mime_type: Option<String>,
    hidden: bool,
    executable: bool,
    thumbnail: ThumbnailState,
    trash: Option<TrashMetadata>,
    rating_loaded: bool,
    tags_loaded: bool,
    comment_loaded: bool,
    rating: Option<u8>,
    tags: Option<Box<[u8]>>,
    comment: Option<Box<[u8]>>,
}

impl DirectoryEntry {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        path: PathBuf,
        display_name: OsString,
        kind: EntryKind,
        size: Option<u64>,
        modified: Option<SystemTime>,
        mime_type: Option<String>,
        hidden: bool,
        executable: bool,
        thumbnail: ThumbnailState,
    ) -> Self {
        Self {
            path,
            display_name,
            kind,
            size,
            modified,
            created: None,
            accessed: None,
            mime_type,
            hidden,
            executable,
            thumbnail,
            trash: None,
            rating_loaded: false,
            tags_loaded: false,
            comment_loaded: false,
            rating: None,
            tags: None,
            comment: None,
        }
    }

    pub(crate) fn with_additional_timestamps(
        mut self,
        created: Option<SystemTime>,
        accessed: Option<SystemTime>,
    ) -> Self {
        self.created = created;
        self.accessed = accessed;
        self
    }

    pub(crate) fn set_rating_sort_metadata(&mut self, rating: Option<u8>) {
        self.rating_loaded = true;
        self.rating = rating;
    }

    pub(crate) fn set_tags_sort_metadata(&mut self, tags: Option<Box<[u8]>>) {
        self.tags_loaded = true;
        self.tags = tags;
    }

    pub(crate) fn set_comment_sort_metadata(&mut self, comment: Option<Box<[u8]>>) {
        self.comment_loaded = true;
        self.comment = comment;
    }

    pub(crate) fn with_trash_metadata(mut self, metadata: TrashMetadata) -> Self {
        self.trash = Some(metadata);
        self
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn display_name(&self) -> &OsStr {
        &self.display_name
    }

    pub fn display_name_lossy(&self) -> String {
        self.display_name.to_string_lossy().into_owned()
    }

    pub fn kind(&self) -> EntryKind {
        self.kind
    }

    pub fn size(&self) -> Option<u64> {
        self.size
    }

    pub fn modified(&self) -> Option<SystemTime> {
        self.modified
    }

    pub fn created(&self) -> Option<SystemTime> {
        self.created
    }

    pub fn accessed(&self) -> Option<SystemTime> {
        self.accessed
    }

    pub(crate) fn rating_sort_metadata_loaded(&self) -> bool {
        self.rating_loaded
    }

    pub(crate) fn tags_sort_metadata_loaded(&self) -> bool {
        self.tags_loaded
    }

    pub(crate) fn comment_sort_metadata_loaded(&self) -> bool {
        self.comment_loaded
    }

    pub fn rating(&self) -> Option<u8> {
        self.rating
    }

    pub fn tags(&self) -> Option<&[u8]> {
        self.tags.as_deref()
    }

    pub fn comment(&self) -> Option<&[u8]> {
        self.comment.as_deref()
    }

    pub fn mime_type(&self) -> Option<&str> {
        self.mime_type.as_deref()
    }

    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    pub fn is_executable(&self) -> bool {
        self.executable
    }

    pub fn thumbnail_state(&self) -> ThumbnailState {
        self.thumbnail
    }

    pub fn trash_metadata(&self) -> Option<&TrashMetadata> {
        self.trash.as_ref()
    }

    pub fn is_navigable_directory(&self) -> bool {
        matches!(
            self.kind,
            EntryKind::Directory
                | EntryKind::SymbolicLink {
                    target_is_directory: true
                }
        )
    }
}

/// An immutable result from one non-recursive directory enumeration.
#[derive(Clone, Debug)]
pub struct DirectoryListing {
    directory: PathBuf,
    entries: Vec<DirectoryEntry>,
}

impl DirectoryListing {
    pub(crate) fn new(directory: PathBuf, entries: Vec<DirectoryEntry>) -> Self {
        Self { directory, entries }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn entries(&self) -> &[DirectoryEntry] {
        &self.entries
    }

    pub fn into_entries(self) -> Vec<DirectoryEntry> {
        self.entries
    }
}
