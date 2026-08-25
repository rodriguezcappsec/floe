use std::{cmp::Ordering, ffi::OsStr};

use crate::{DirectoryEntry, EntryKind};

/// Metadata columns the directory view can order without loading more data.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SortColumn {
    #[default]
    Name,
    Type,
    Size,
    Modified,
    Extension,
}

impl SortColumn {
    pub const ALL: [Self; 5] = [
        Self::Name,
        Self::Type,
        Self::Size,
        Self::Modified,
        Self::Extension,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Type => "Type",
            Self::Size => "Size",
            Self::Modified => "Modified",
            Self::Extension => "Extension",
        }
    }

    pub const fn persisted(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Type => "type",
            Self::Size => "size",
            Self::Modified => "modified",
            Self::Extension => "extension",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|column| column.persisted() == value)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DirectoryPlacement {
    #[default]
    First,
    Last,
}

impl DirectoryPlacement {
    pub const fn persisted(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::Last => "last",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        match value {
            "first" => Some(Self::First),
            "last" => Some(Self::Last),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DirectoryGrouping {
    #[default]
    None,
    Type,
    Extension,
}

impl DirectoryGrouping {
    pub const fn persisted(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Type => "type",
            Self::Extension => "extension",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "type" => Some(Self::Type),
            "extension" => Some(Self::Extension),
            _ => None,
        }
    }

    pub fn starts_group(self, entry: &DirectoryEntry, previous: Option<&DirectoryEntry>) -> bool {
        self != Self::None
            && previous
                .is_none_or(|previous| self.compare_groups(previous, entry) != Ordering::Equal)
    }

    pub fn label(self, entry: &DirectoryEntry) -> Option<String> {
        match self {
            Self::None => None,
            Self::Type => Some(kind_label(entry.kind()).to_owned()),
            Self::Extension if entry.is_navigable_directory() => Some("Folders".to_owned()),
            Self::Extension => Some(
                entry_extension(entry)
                    .map(|extension| extension.to_string_lossy().into_owned())
                    .filter(|extension| !extension.is_empty())
                    .map(|extension| format!(".{extension}"))
                    .unwrap_or_else(|| "No extension".to_owned()),
            ),
        }
    }

    fn compare_groups(self, left: &DirectoryEntry, right: &DirectoryEntry) -> Ordering {
        match self {
            Self::None => Ordering::Equal,
            Self::Type => kind_rank(left.kind()).cmp(&kind_rank(right.kind())),
            Self::Extension if left.is_navigable_directory() && right.is_navigable_directory() => {
                Ordering::Equal
            }
            Self::Extension => optional_os_str(
                entry_extension(left),
                entry_extension(right),
                SortDirection::Ascending,
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

impl SortDirection {
    pub const fn reversed(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Ascending => "ascending",
            Self::Descending => "descending",
        }
    }

    pub const fn persisted(self) -> &'static str {
        match self {
            Self::Ascending => "ascending",
            Self::Descending => "descending",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        match value {
            "ascending" => Some(Self::Ascending),
            "descending" => Some(Self::Descending),
            _ => None,
        }
    }
}

/// Application-selected directory order using metadata already in each entry.
///
/// Navigable directories always remain before other entries. Missing optional
/// metadata remains last in both directions so blank cells never displace known
/// values. Original `OsStr`/`Path` values provide deterministic tie-breakers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DirectorySort {
    pub column: SortColumn,
    pub direction: SortDirection,
    pub directories: DirectoryPlacement,
    pub grouping: DirectoryGrouping,
}

impl DirectorySort {
    pub const fn new(column: SortColumn, direction: SortDirection) -> Self {
        Self {
            column,
            direction,
            directories: DirectoryPlacement::First,
            grouping: DirectoryGrouping::None,
        }
    }

    pub const fn with_directories(mut self, directories: DirectoryPlacement) -> Self {
        self.directories = directories;
        self
    }

    pub const fn with_grouping(mut self, grouping: DirectoryGrouping) -> Self {
        self.grouping = grouping;
        self
    }

    pub fn next_for(self, column: SortColumn) -> Self {
        let next = if self.column == column {
            Self::new(column, self.direction.reversed())
        } else {
            Self::new(column, SortDirection::Ascending)
        };
        next.with_directories(self.directories)
            .with_grouping(self.grouping)
    }

    pub fn sort_entries(self, entries: &mut [DirectoryEntry]) {
        entries.sort_by(|left, right| self.compare_entries(left, right));
    }

    pub fn compare_entries(self, left: &DirectoryEntry, right: &DirectoryEntry) -> Ordering {
        let directory_order = match self.directories {
            DirectoryPlacement::First => u8::from(!left.is_navigable_directory())
                .cmp(&u8::from(!right.is_navigable_directory())),
            DirectoryPlacement::Last => u8::from(left.is_navigable_directory())
                .cmp(&u8::from(right.is_navigable_directory())),
        };
        if directory_order != Ordering::Equal {
            return directory_order;
        }

        let group_order = self.grouping.compare_groups(left, right);
        if group_order != Ordering::Equal {
            return group_order;
        }

        let primary = match self.column {
            SortColumn::Name => directed(
                left.display_name().cmp(right.display_name()),
                self.direction,
            ),
            SortColumn::Type => directed(
                kind_rank(left.kind()).cmp(&kind_rank(right.kind())),
                self.direction,
            ),
            SortColumn::Size => optional(left.size(), right.size(), self.direction),
            SortColumn::Modified => optional(left.modified(), right.modified(), self.direction),
            SortColumn::Extension => optional_os_str(
                entry_extension(left),
                entry_extension(right),
                self.direction,
            ),
        };

        primary
            .then_with(|| left.display_name().cmp(right.display_name()))
            .then_with(|| left.path().cmp(right.path()))
    }
}

fn entry_extension(entry: &DirectoryEntry) -> Option<&OsStr> {
    (!entry.is_navigable_directory())
        .then(|| entry.display_name().extension())
        .flatten()
}

trait OsStrExtension {
    fn extension(&self) -> Option<&OsStr>;
}

impl OsStrExtension for OsStr {
    fn extension(&self) -> Option<&OsStr> {
        std::path::Path::new(self).extension()
    }
}

fn directed(ordering: Ordering, direction: SortDirection) -> Ordering {
    match direction {
        SortDirection::Ascending => ordering,
        SortDirection::Descending => ordering.reverse(),
    }
}

fn optional<T: Ord>(left: Option<T>, right: Option<T>, direction: SortDirection) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => directed(left.cmp(&right), direction),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn optional_os_str(
    left: Option<&OsStr>,
    right: Option<&OsStr>,
    direction: SortDirection,
) -> Ordering {
    optional(left, right, direction)
}

fn kind_rank(kind: EntryKind) -> u8 {
    match kind {
        EntryKind::Directory => 0,
        EntryKind::SymbolicLink {
            target_is_directory: true,
        } => 1,
        EntryKind::RegularFile => 2,
        EntryKind::SymbolicLink {
            target_is_directory: false,
        } => 3,
        EntryKind::Other => 4,
    }
}

fn kind_label(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Directory => "Folders",
        EntryKind::SymbolicLink {
            target_is_directory: true,
        } => "Folder links",
        EntryKind::RegularFile => "Files",
        EntryKind::SymbolicLink {
            target_is_directory: false,
        } => "File links",
        EntryKind::Other => "Special files",
    }
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        path::PathBuf,
        time::{Duration, UNIX_EPOCH},
    };

    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt;

    use crate::ThumbnailState;

    use super::*;

    fn entry(
        name: OsString,
        kind: EntryKind,
        size: Option<u64>,
        modified_seconds: Option<u64>,
    ) -> DirectoryEntry {
        DirectoryEntry::new(
            PathBuf::from("/tmp").join(&name),
            name,
            kind,
            size,
            modified_seconds.map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds)),
            None,
            false,
            false,
            ThumbnailState::NotRequested,
        )
    }

    fn names(entries: &[DirectoryEntry]) -> Vec<OsString> {
        entries
            .iter()
            .map(|entry| entry.display_name().to_os_string())
            .collect()
    }

    #[test]
    fn phase_6b_direction_cycles_current_column_and_resets_new_column() {
        let name_ascending = DirectorySort::default();
        assert_eq!(
            name_ascending.next_for(SortColumn::Name),
            DirectorySort::new(SortColumn::Name, SortDirection::Descending)
        );
        assert_eq!(
            name_ascending
                .next_for(SortColumn::Name)
                .next_for(SortColumn::Size),
            DirectorySort::new(SortColumn::Size, SortDirection::Ascending)
        );
    }

    #[test]
    fn phase_6b_directories_stay_first_in_both_name_directions() {
        let mut entries = vec![
            entry("alpha.txt".into(), EntryKind::RegularFile, Some(1), Some(1)),
            entry("zeta".into(), EntryKind::Directory, None, Some(2)),
            entry("beta".into(), EntryKind::Directory, None, Some(3)),
        ];

        DirectorySort::new(SortColumn::Name, SortDirection::Ascending).sort_entries(&mut entries);
        assert_eq!(names(&entries), ["beta", "zeta", "alpha.txt"]);

        DirectorySort::new(SortColumn::Name, SortDirection::Descending).sort_entries(&mut entries);
        assert_eq!(names(&entries), ["zeta", "beta", "alpha.txt"]);
    }

    #[test]
    fn phase_6b_type_sort_uses_textual_kind_order_within_directory_groups() {
        let mut entries = vec![
            entry("special".into(), EntryKind::Other, None, None),
            entry(
                "file-link".into(),
                EntryKind::SymbolicLink {
                    target_is_directory: false,
                },
                None,
                None,
            ),
            entry("file".into(), EntryKind::RegularFile, Some(1), None),
            entry(
                "folder-link".into(),
                EntryKind::SymbolicLink {
                    target_is_directory: true,
                },
                None,
                None,
            ),
            entry("folder".into(), EntryKind::Directory, None, None),
        ];

        DirectorySort::new(SortColumn::Type, SortDirection::Ascending).sort_entries(&mut entries);
        assert_eq!(
            names(&entries),
            ["folder", "folder-link", "file", "file-link", "special"]
        );
    }

    #[test]
    fn phase_6b_size_and_modified_sorts_keep_unknown_values_last() {
        let mut entries = vec![
            entry("unknown".into(), EntryKind::Other, None, None),
            entry("small".into(), EntryKind::RegularFile, Some(10), Some(10)),
            entry("large".into(), EntryKind::RegularFile, Some(20), Some(20)),
        ];

        DirectorySort::new(SortColumn::Size, SortDirection::Descending).sort_entries(&mut entries);
        assert_eq!(names(&entries), ["large", "small", "unknown"]);

        DirectorySort::new(SortColumn::Modified, SortDirection::Ascending)
            .sort_entries(&mut entries);
        assert_eq!(names(&entries), ["small", "large", "unknown"]);
    }

    #[cfg(unix)]
    #[test]
    fn phase_6b_name_sort_preserves_raw_non_utf8_identity() {
        let raw_low = OsString::from_vec(vec![b'f', b'o', 0x80]);
        let raw_high = OsString::from_vec(vec![b'f', b'o', 0x81]);
        let mut entries = vec![
            entry(raw_high.clone(), EntryKind::RegularFile, Some(1), None),
            entry(raw_low.clone(), EntryKind::RegularFile, Some(1), None),
        ];

        DirectorySort::default().sort_entries(&mut entries);

        assert_eq!(entries[0].display_name(), raw_low);
        assert_eq!(entries[1].display_name(), raw_high);
        assert_eq!(entries[0].path(), PathBuf::from("/tmp").join(raw_low));
    }

    #[test]
    fn phase_6t_sort_group_directory_placement_and_extension_are_independent() {
        let mut entries = vec![
            entry("folder".into(), EntryKind::Directory, None, None),
            entry("zeta.rs".into(), EntryKind::RegularFile, Some(1), None),
            entry("alpha.txt".into(), EntryKind::RegularFile, Some(1), None),
            entry("readme".into(), EntryKind::RegularFile, Some(1), None),
        ];

        DirectorySort::new(SortColumn::Extension, SortDirection::Ascending)
            .with_directories(DirectoryPlacement::Last)
            .sort_entries(&mut entries);

        assert_eq!(
            names(&entries),
            ["zeta.rs", "alpha.txt", "readme", "folder"]
        );
    }

    #[test]
    fn phase_6t_sort_group_type_and_extension_groups_are_stable_and_labelled() {
        let folder = entry("folder".into(), EntryKind::Directory, None, None);
        let rust = entry("main.rs".into(), EntryKind::RegularFile, Some(1), None);
        let text = entry("notes.txt".into(), EntryKind::RegularFile, Some(1), None);
        let plain = entry("README".into(), EntryKind::RegularFile, Some(1), None);

        assert!(DirectoryGrouping::Type.starts_group(&folder, None));
        assert_eq!(
            DirectoryGrouping::Type.label(&folder).as_deref(),
            Some("Folders")
        );
        assert_eq!(
            DirectoryGrouping::Extension.label(&rust).as_deref(),
            Some(".rs")
        );
        assert_eq!(
            DirectoryGrouping::Extension.label(&plain).as_deref(),
            Some("No extension")
        );

        let mut entries = vec![text, plain, rust, folder];
        DirectorySort::new(SortColumn::Name, SortDirection::Ascending)
            .with_grouping(DirectoryGrouping::Extension)
            .sort_entries(&mut entries);
        assert_eq!(
            names(&entries),
            ["folder", "main.rs", "notes.txt", "README"]
        );
    }

    #[test]
    fn grid_grouping_keeps_dotted_directories_in_one_folders_section() {
        let mut entries = vec![
            entry("archive.2024".into(), EntryKind::Directory, None, None),
            entry("projects".into(), EntryKind::Directory, None, None),
            entry("main.rs".into(), EntryKind::RegularFile, Some(1), None),
        ];
        let grouping = DirectoryGrouping::Extension;
        DirectorySort::new(SortColumn::Name, SortDirection::Ascending)
            .with_grouping(grouping)
            .sort_entries(&mut entries);

        assert_eq!(names(&entries), ["archive.2024", "projects", "main.rs"]);
        assert!(grouping.starts_group(&entries[0], None));
        assert!(!grouping.starts_group(&entries[1], Some(&entries[0])));
        assert!(grouping.starts_group(&entries[2], Some(&entries[1])));
        assert_eq!(grouping.label(&entries[0]).as_deref(), Some("Folders"));
        assert_eq!(grouping.label(&entries[2]).as_deref(), Some(".rs"));
    }

    #[cfg(unix)]
    #[test]
    fn phase_6t_sort_group_raw_non_utf8_extensions_remain_distinct() {
        let low = OsString::from_vec(vec![b'a', b'.', 0x80]);
        let high = OsString::from_vec(vec![b'a', b'.', 0x81]);
        let low_entry = entry(low.clone(), EntryKind::RegularFile, Some(1), None);
        let high_entry = entry(high.clone(), EntryKind::RegularFile, Some(1), None);
        let mut entries = vec![high_entry, low_entry];

        DirectorySort::new(SortColumn::Extension, SortDirection::Ascending)
            .sort_entries(&mut entries);

        assert_eq!(names(&entries), [low, high]);
        assert!(DirectoryGrouping::Extension.starts_group(&entries[1], Some(&entries[0])));
    }
}
