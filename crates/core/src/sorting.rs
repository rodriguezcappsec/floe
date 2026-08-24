use std::cmp::Ordering;

use crate::{DirectoryEntry, EntryKind};

/// Metadata columns the directory view can order without loading more data.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SortColumn {
    #[default]
    Name,
    Type,
    Size,
    Modified,
}

impl SortColumn {
    pub const ALL: [Self; 4] = [Self::Name, Self::Type, Self::Size, Self::Modified];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Type => "Type",
            Self::Size => "Size",
            Self::Modified => "Modified",
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
}

impl DirectorySort {
    pub const fn new(column: SortColumn, direction: SortDirection) -> Self {
        Self { column, direction }
    }

    pub fn next_for(self, column: SortColumn) -> Self {
        if self.column == column {
            Self::new(column, self.direction.reversed())
        } else {
            Self::new(column, SortDirection::Ascending)
        }
    }

    pub fn sort_entries(self, entries: &mut [DirectoryEntry]) {
        entries.sort_by(|left, right| self.compare_entries(left, right));
    }

    pub fn compare_entries(self, left: &DirectoryEntry, right: &DirectoryEntry) -> Ordering {
        let directory_order = u8::from(!left.is_navigable_directory())
            .cmp(&u8::from(!right.is_navigable_directory()));
        if directory_order != Ordering::Equal {
            return directory_order;
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
        };

        primary
            .then_with(|| left.display_name().cmp(right.display_name()))
            .then_with(|| left.path().cmp(right.path()))
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
}
