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
    mime_type: Option<String>,
    hidden: bool,
    executable: bool,
    thumbnail: ThumbnailState,
    trash: Option<TrashMetadata>,
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
            mime_type,
            hidden,
            executable,
            thumbnail,
            trash: None,
        }
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
