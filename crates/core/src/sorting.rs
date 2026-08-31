use std::{cmp::Ordering, ffi::OsStr, os::unix::ffi::OsStrExt};

use crate::{DirectoryEntry, EntryKind};

/// Metadata columns the directory view can order without loading more data.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SortColumn {
    #[default]
    Name,
    NaturalName,
    Type,
    Size,
    Modified,
    Created,
    Accessed,
    Extension,
    Rating,
    Tags,
    Comment,
    DocumentWordCount,
    DocumentLineCount,
    ImageDimensions,
    ImageOrientation,
    ImageWidth,
    ImageHeight,
    AudioArtist,
    AudioAlbum,
    AudioDuration,
    AudioTrack,
    AudioGenre,
    AudioBitrate,
    VideoDuration,
    VideoDimensions,
    VideoWidth,
    VideoHeight,
    VideoFrameRate,
    VideoBitrate,
    Path,
    LinkDestination,
    Permissions,
    Owner,
    Group,
}

impl SortColumn {
    pub const ALL: [Self; 34] = [
        Self::Name,
        Self::NaturalName,
        Self::Type,
        Self::Size,
        Self::Modified,
        Self::Created,
        Self::Accessed,
        Self::Extension,
        Self::Rating,
        Self::Tags,
        Self::Comment,
        Self::DocumentWordCount,
        Self::DocumentLineCount,
        Self::ImageDimensions,
        Self::ImageOrientation,
        Self::ImageWidth,
        Self::ImageHeight,
        Self::AudioArtist,
        Self::AudioAlbum,
        Self::AudioDuration,
        Self::AudioTrack,
        Self::AudioGenre,
        Self::AudioBitrate,
        Self::VideoDuration,
        Self::VideoDimensions,
        Self::VideoWidth,
        Self::VideoHeight,
        Self::VideoFrameRate,
        Self::VideoBitrate,
        Self::Path,
        Self::LinkDestination,
        Self::Permissions,
        Self::Owner,
        Self::Group,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::NaturalName => "Natural Name",
            Self::Type => "Type",
            Self::Size => "Size",
            Self::Modified => "Modified",
            Self::Created => "Created",
            Self::Accessed => "Accessed",
            Self::Extension => "Extension",
            Self::Rating => "Rating",
            Self::Tags => "Tags",
            Self::Comment => "Comment",
            Self::DocumentWordCount => "Word Count",
            Self::DocumentLineCount => "Line Count",
            Self::ImageDimensions | Self::VideoDimensions => "Dimensions",
            Self::ImageOrientation => "Orientation",
            Self::ImageWidth | Self::VideoWidth => "Width",
            Self::ImageHeight | Self::VideoHeight => "Height",
            Self::AudioArtist => "Artist",
            Self::AudioAlbum => "Album",
            Self::AudioDuration | Self::VideoDuration => "Duration",
            Self::AudioTrack => "Track",
            Self::AudioGenre => "Genre",
            Self::AudioBitrate | Self::VideoBitrate => "Bitrate",
            Self::VideoFrameRate => "Frame Rate",
            Self::Path => "Path",
            Self::LinkDestination => "Link Destination",
            Self::Permissions => "Permissions",
            Self::Owner => "Owner",
            Self::Group => "Group",
        }
    }

    pub const fn persisted(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::NaturalName => "natural-name",
            Self::Type => "type",
            Self::Size => "size",
            Self::Modified => "modified",
            Self::Created => "created",
            Self::Accessed => "accessed",
            Self::Extension => "extension",
            Self::Rating => "rating",
            Self::Tags => "tags",
            Self::Comment => "comment",
            Self::DocumentWordCount => "document-word-count",
            Self::DocumentLineCount => "document-line-count",
            Self::ImageDimensions => "image-dimensions",
            Self::ImageOrientation => "image-orientation",
            Self::ImageWidth => "image-width",
            Self::ImageHeight => "image-height",
            Self::AudioArtist => "audio-artist",
            Self::AudioAlbum => "audio-album",
            Self::AudioDuration => "audio-duration",
            Self::AudioTrack => "audio-track",
            Self::AudioGenre => "audio-genre",
            Self::AudioBitrate => "audio-bitrate",
            Self::VideoDuration => "video-duration",
            Self::VideoDimensions => "video-dimensions",
            Self::VideoWidth => "video-width",
            Self::VideoHeight => "video-height",
            Self::VideoFrameRate => "video-frame-rate",
            Self::VideoBitrate => "video-bitrate",
            Self::Path => "path",
            Self::LinkDestination => "link-destination",
            Self::Permissions => "permissions",
            Self::Owner => "owner",
            Self::Group => "group",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|column| column.persisted() == value)
    }

    pub const fn needs_user_metadata(self) -> bool {
        matches!(self, Self::Rating | Self::Tags | Self::Comment)
    }

    pub const fn needs_indexed_metadata(self) -> bool {
        !matches!(
            self,
            Self::Name
                | Self::NaturalName
                | Self::Type
                | Self::Size
                | Self::Modified
                | Self::Created
                | Self::Accessed
                | Self::Extension
                | Self::Rating
                | Self::Tags
                | Self::Comment
                | Self::Path
        )
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
    Date,
    Size,
}

impl DirectoryGrouping {
    pub const fn persisted(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Type => "type",
            Self::Extension => "extension",
            Self::Date => "date",
            Self::Size => "size",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "type" => Some(Self::Type),
            "extension" => Some(Self::Extension),
            "date" => Some(Self::Date),
            "size" => Some(Self::Size),
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
            Self::Date if entry.is_navigable_directory() => Some("Folders".to_owned()),
            Self::Date => Some(date_group_label(entry.modified())),
            Self::Size if entry.is_navigable_directory() => Some("Folders".to_owned()),
            Self::Size => Some(size_group(entry.size()).label().to_owned()),
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
            Self::Date => optional(
                date_group(left.modified()),
                date_group(right.modified()),
                SortDirection::Ascending,
            ),
            Self::Size => size_group(left.size()).cmp(&size_group(right.size())),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SizeGroup {
    Empty,
    Tiny,
    Small,
    Medium,
    Large,
    VeryLarge,
    Unknown,
}

impl SizeGroup {
    const fn label(self) -> &'static str {
        match self {
            Self::Empty => "Empty (0 B)",
            Self::Tiny => "Tiny (under 1 MB)",
            Self::Small => "Small (1–100 MB)",
            Self::Medium => "Medium (100 MB–1 GB)",
            Self::Large => "Large (1–10 GB)",
            Self::VeryLarge => "Very large (10 GB or more)",
            Self::Unknown => "Size unknown",
        }
    }
}

fn size_group(size: Option<u64>) -> SizeGroup {
    const MB: u64 = 1_000_000;
    const GB: u64 = 1_000_000_000;
    match size {
        Some(0) => SizeGroup::Empty,
        Some(value) if value < MB => SizeGroup::Tiny,
        Some(value) if value < 100 * MB => SizeGroup::Small,
        Some(value) if value < GB => SizeGroup::Medium,
        Some(value) if value < 10 * GB => SizeGroup::Large,
        Some(_) => SizeGroup::VeryLarge,
        None => SizeGroup::Unknown,
    }
}

fn date_group(modified: Option<std::time::SystemTime>) -> Option<i64> {
    use std::time::UNIX_EPOCH;

    modified.map(|value| match value.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs() / 86_400).unwrap_or(i64::MAX),
        Err(error) => {
            -i64::try_from(error.duration().as_secs().div_ceil(86_400)).unwrap_or(i64::MAX)
        }
    })
}

fn date_group_label(modified: Option<std::time::SystemTime>) -> String {
    let Some(days) = date_group(modified) else {
        return "Date unknown".to_owned();
    };
    let (year, month, day) = civil_date_from_unix_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

// Howard Hinnant's public-domain civil-from-days algorithm. Keeping the
// conversion here avoids adding a date crate merely for stable group labels.
fn civil_date_from_unix_days(days: i64) -> (i64, u32, u32) {
    let days = days.saturating_add(719_468);
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month as u32, day as u32)
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
    pub hidden_last: bool,
}

impl DirectorySort {
    pub const fn new(column: SortColumn, direction: SortDirection) -> Self {
        Self {
            column,
            direction,
            directories: DirectoryPlacement::First,
            grouping: DirectoryGrouping::None,
            hidden_last: false,
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

    pub const fn with_hidden_last(mut self, hidden_last: bool) -> Self {
        self.hidden_last = hidden_last;
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
            .with_hidden_last(self.hidden_last)
    }

    pub fn sort_entries(self, entries: &mut [DirectoryEntry]) {
        entries.sort_by(|left, right| self.compare_entries(left, right));
    }

    pub fn compare_entries(self, left: &DirectoryEntry, right: &DirectoryEntry) -> Ordering {
        if self.hidden_last {
            let hidden_order = left.is_hidden().cmp(&right.is_hidden());
            if hidden_order != Ordering::Equal {
                return hidden_order;
            }
        }

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
            SortColumn::NaturalName => directed(
                natural_os_cmp(left.display_name(), right.display_name()),
                self.direction,
            ),
            SortColumn::Type => directed(
                kind_rank(left.kind()).cmp(&kind_rank(right.kind())),
                self.direction,
            ),
            SortColumn::Size => optional(left.size(), right.size(), self.direction),
            SortColumn::Modified => optional(left.modified(), right.modified(), self.direction),
            SortColumn::Created => optional(left.created(), right.created(), self.direction),
            SortColumn::Accessed => optional(left.accessed(), right.accessed(), self.direction),
            SortColumn::Extension => optional_os_str(
                entry_extension(left),
                entry_extension(right),
                self.direction,
            ),
            SortColumn::Rating => optional(left.rating(), right.rating(), self.direction),
            SortColumn::Tags => optional(left.tags(), right.tags(), self.direction),
            SortColumn::Comment => optional(left.comment(), right.comment(), self.direction),
            SortColumn::DocumentWordCount => optional(
                left.indexed_sort_metadata()
                    .and_then(|value| value.word_count),
                right
                    .indexed_sort_metadata()
                    .and_then(|value| value.word_count),
                self.direction,
            ),
            SortColumn::DocumentLineCount => optional(
                left.indexed_sort_metadata()
                    .and_then(|value| value.line_count),
                right
                    .indexed_sort_metadata()
                    .and_then(|value| value.line_count),
                self.direction,
            ),
            SortColumn::ImageDimensions => optional(
                left.indexed_sort_metadata()
                    .and_then(|value| value.image_dimensions()),
                right
                    .indexed_sort_metadata()
                    .and_then(|value| value.image_dimensions()),
                self.direction,
            ),
            SortColumn::ImageOrientation => optional(
                left.indexed_sort_metadata()
                    .and_then(|value| value.image_orientation.as_deref()),
                right
                    .indexed_sort_metadata()
                    .and_then(|value| value.image_orientation.as_deref()),
                self.direction,
            ),
            SortColumn::ImageWidth => optional(
                left.indexed_sort_metadata()
                    .and_then(|value| value.image_width),
                right
                    .indexed_sort_metadata()
                    .and_then(|value| value.image_width),
                self.direction,
            ),
            SortColumn::ImageHeight => optional(
                left.indexed_sort_metadata()
                    .and_then(|value| value.image_height),
                right
                    .indexed_sort_metadata()
                    .and_then(|value| value.image_height),
                self.direction,
            ),
            SortColumn::AudioArtist => optional_bytes(left, right, self.direction, |value| {
                value.audio_artist.as_deref()
            }),
            SortColumn::AudioAlbum => optional_bytes(left, right, self.direction, |value| {
                value.audio_album.as_deref()
            }),
            SortColumn::AudioDuration => optional(
                left.indexed_sort_metadata()
                    .and_then(|value| value.audio_duration_millis),
                right
                    .indexed_sort_metadata()
                    .and_then(|value| value.audio_duration_millis),
                self.direction,
            ),
            SortColumn::AudioTrack => optional(
                left.indexed_sort_metadata()
                    .and_then(|value| value.audio_track),
                right
                    .indexed_sort_metadata()
                    .and_then(|value| value.audio_track),
                self.direction,
            ),
            SortColumn::AudioGenre => optional_bytes(left, right, self.direction, |value| {
                value.audio_genre.as_deref()
            }),
            SortColumn::AudioBitrate => optional(
                left.indexed_sort_metadata()
                    .and_then(|value| value.audio_bitrate),
                right
                    .indexed_sort_metadata()
                    .and_then(|value| value.audio_bitrate),
                self.direction,
            ),
            SortColumn::VideoDuration => optional(
                left.indexed_sort_metadata()
                    .and_then(|value| value.video_duration_millis),
                right
                    .indexed_sort_metadata()
                    .and_then(|value| value.video_duration_millis),
                self.direction,
            ),
            SortColumn::VideoDimensions => optional(
                left.indexed_sort_metadata()
                    .and_then(|value| value.video_dimensions()),
                right
                    .indexed_sort_metadata()
                    .and_then(|value| value.video_dimensions()),
                self.direction,
            ),
            SortColumn::VideoWidth => optional(
                left.indexed_sort_metadata()
                    .and_then(|value| value.video_width),
                right
                    .indexed_sort_metadata()
                    .and_then(|value| value.video_width),
                self.direction,
            ),
            SortColumn::VideoHeight => optional(
                left.indexed_sort_metadata()
                    .and_then(|value| value.video_height),
                right
                    .indexed_sort_metadata()
                    .and_then(|value| value.video_height),
                self.direction,
            ),
            SortColumn::VideoFrameRate => optional(
                left.indexed_sort_metadata()
                    .and_then(|value| value.video_frame_rate_milli),
                right
                    .indexed_sort_metadata()
                    .and_then(|value| value.video_frame_rate_milli),
                self.direction,
            ),
            SortColumn::VideoBitrate => optional(
                left.indexed_sort_metadata()
                    .and_then(|value| value.video_bitrate),
                right
                    .indexed_sort_metadata()
                    .and_then(|value| value.video_bitrate),
                self.direction,
            ),
            SortColumn::Path => directed(left.path().cmp(right.path()), self.direction),
            SortColumn::LinkDestination => optional_os_str(
                left.indexed_sort_metadata()
                    .and_then(|value| value.link_destination.as_deref()),
                right
                    .indexed_sort_metadata()
                    .and_then(|value| value.link_destination.as_deref()),
                self.direction,
            ),
            SortColumn::Permissions => optional(
                left.indexed_sort_metadata()
                    .and_then(|value| value.permissions),
                right
                    .indexed_sort_metadata()
                    .and_then(|value| value.permissions),
                self.direction,
            ),
            SortColumn::Owner => optional(
                left.indexed_sort_metadata().and_then(|value| value.owner),
                right.indexed_sort_metadata().and_then(|value| value.owner),
                self.direction,
            ),
            SortColumn::Group => optional(
                left.indexed_sort_metadata().and_then(|value| value.group),
                right.indexed_sort_metadata().and_then(|value| value.group),
                self.direction,
            ),
        };

        primary
            .then_with(|| left.display_name().cmp(right.display_name()))
            .then_with(|| left.path().cmp(right.path()))
    }
}

fn optional_bytes(
    left: &DirectoryEntry,
    right: &DirectoryEntry,
    direction: SortDirection,
    value: impl Fn(&crate::IndexedSortMetadata) -> Option<&[u8]>,
) -> Ordering {
    optional(
        left.indexed_sort_metadata().and_then(&value),
        right.indexed_sort_metadata().and_then(value),
        direction,
    )
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

/// Compare Linux filenames the way people usually expect numbered files to
/// appear, without ever parsing a digit run into a fixed-width integer.
///
/// ASCII letters are folded for the natural comparison. The original bytes
/// remain the final tie-break, so distinct names (including non-UTF-8 names)
/// never collapse into one ordering identity.
pub fn natural_os_cmp(left: &OsStr, right: &OsStr) -> Ordering {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let mut left_index = 0;
    let mut right_index = 0;

    while left_index < left.len() && right_index < right.len() {
        let left_digit = left[left_index].is_ascii_digit();
        let right_digit = right[right_index].is_ascii_digit();
        if left_digit && right_digit {
            let left_end = digit_run_end(left, left_index);
            let right_end = digit_run_end(right, right_index);
            let left_significant = skip_leading_zeroes(left, left_index, left_end);
            let right_significant = skip_leading_zeroes(right, right_index, right_end);
            let left_length = left_end - left_significant;
            let right_length = right_end - right_significant;

            let order = left_length.cmp(&right_length).then_with(|| {
                left[left_significant..left_end].cmp(&right[right_significant..right_end])
            });
            if order != Ordering::Equal {
                return order;
            }

            // Equal numeric values sort the shortest spelling first: 1, 01,
            // 001. This makes the ordering total without integer conversion.
            let order = (left_end - left_index).cmp(&(right_end - right_index));
            if order != Ordering::Equal {
                return order;
            }
            left_index = left_end;
            right_index = right_end;
            continue;
        }

        let order = left[left_index]
            .to_ascii_lowercase()
            .cmp(&right[right_index].to_ascii_lowercase());
        if order != Ordering::Equal {
            return order;
        }
        left_index += 1;
        right_index += 1;
    }

    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

fn digit_run_end(value: &[u8], mut index: usize) -> usize {
    while index < value.len() && value[index].is_ascii_digit() {
        index += 1;
    }
    index
}

fn skip_leading_zeroes(value: &[u8], mut index: usize, end: usize) -> usize {
    while index + 1 < end && value[index] == b'0' {
        index += 1;
    }
    index
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

    #[test]
    fn phase_23c_natural_sort_handles_numbers_case_and_raw_bytes() {
        let mut names = vec![
            OsStr::new("file10"),
            OsStr::new("file2"),
            OsStr::new("file001"),
            OsStr::new("file01"),
            OsStr::new("file1"),
            OsStr::from_bytes(b"FILE3"),
            OsStr::from_bytes(b"file\xff"),
            OsStr::from_bytes(b"file\xfe"),
        ];
        names.sort_by(|left, right| natural_os_cmp(left, right));
        assert_eq!(
            names,
            vec![
                OsStr::new("file1"),
                OsStr::new("file01"),
                OsStr::new("file001"),
                OsStr::new("file2"),
                OsStr::from_bytes(b"FILE3"),
                OsStr::new("file10"),
                OsStr::from_bytes(b"file\xfe"),
                OsStr::from_bytes(b"file\xff"),
            ]
        );

        let huge_a = OsStr::new("item99999999999999999999999999999999999999");
        let huge_b = OsStr::new("item100000000000000000000000000000000000000");
        assert_eq!(natural_os_cmp(huge_a, huge_b), Ordering::Less);
        assert_eq!(natural_os_cmp(huge_b, huge_a), Ordering::Greater);
    }

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

    #[test]
    fn phase_20b1_sort_created_and_accessed_keep_unknown_values_last() {
        let older = entry("older".into(), EntryKind::RegularFile, Some(1), None)
            .with_additional_timestamps(
                Some(UNIX_EPOCH + Duration::from_secs(10)),
                Some(UNIX_EPOCH + Duration::from_secs(30)),
            );
        let newer = entry("newer".into(), EntryKind::RegularFile, Some(1), None)
            .with_additional_timestamps(
                Some(UNIX_EPOCH + Duration::from_secs(20)),
                Some(UNIX_EPOCH + Duration::from_secs(40)),
            );
        let unknown = entry("unknown".into(), EntryKind::RegularFile, Some(1), None);

        let mut entries = vec![unknown.clone(), newer.clone(), older.clone()];
        DirectorySort::new(SortColumn::Created, SortDirection::Ascending)
            .sort_entries(&mut entries);
        assert_eq!(names(&entries), ["older", "newer", "unknown"]);

        DirectorySort::new(SortColumn::Created, SortDirection::Descending)
            .sort_entries(&mut entries);
        assert_eq!(names(&entries), ["newer", "older", "unknown"]);

        DirectorySort::new(SortColumn::Accessed, SortDirection::Ascending)
            .sort_entries(&mut entries);
        assert_eq!(names(&entries), ["older", "newer", "unknown"]);
    }

    #[test]
    fn phase_20b1_sort_user_metadata_is_real_and_unknown_last() {
        let mut alpha = entry("alpha".into(), EntryKind::RegularFile, Some(1), None);
        alpha.set_rating_sort_metadata(Some(2));
        alpha.set_tags_sort_metadata(Some(b"work".to_vec().into_boxed_slice()));
        alpha.set_comment_sort_metadata(Some(b"zeta".to_vec().into_boxed_slice()));
        let mut beta = entry("beta".into(), EntryKind::RegularFile, Some(1), None);
        beta.set_rating_sort_metadata(Some(8));
        beta.set_tags_sort_metadata(Some(b"archive".to_vec().into_boxed_slice()));
        beta.set_comment_sort_metadata(Some(b"alpha".to_vec().into_boxed_slice()));
        let unknown = entry("unknown".into(), EntryKind::RegularFile, Some(1), None);
        let mut entries = vec![unknown, beta, alpha];

        DirectorySort::new(SortColumn::Rating, SortDirection::Descending)
            .sort_entries(&mut entries);
        assert_eq!(names(&entries), ["beta", "alpha", "unknown"]);
        DirectorySort::new(SortColumn::Tags, SortDirection::Ascending).sort_entries(&mut entries);
        assert_eq!(names(&entries), ["beta", "alpha", "unknown"]);
        DirectorySort::new(SortColumn::Comment, SortDirection::Ascending)
            .sort_entries(&mut entries);
        assert_eq!(names(&entries), ["beta", "alpha", "unknown"]);
    }

    #[test]
    fn phase_20b1_hidden_last_partitions_before_folder_placement() {
        let visible_folder = entry("visible-folder".into(), EntryKind::Directory, None, None);
        let visible_file = entry("visible-file".into(), EntryKind::RegularFile, Some(1), None);
        let hidden_folder = DirectoryEntry::new(
            PathBuf::from("/tmp/.hidden-folder"),
            ".hidden-folder".into(),
            EntryKind::Directory,
            None,
            None,
            None,
            true,
            false,
            ThumbnailState::NotRequested,
        );
        let hidden_file = DirectoryEntry::new(
            PathBuf::from("/tmp/.hidden-file"),
            ".hidden-file".into(),
            EntryKind::RegularFile,
            Some(1),
            None,
            None,
            true,
            false,
            ThumbnailState::NotRequested,
        );
        let mut entries = vec![hidden_file, visible_file, hidden_folder, visible_folder];

        DirectorySort::default()
            .with_hidden_last(true)
            .sort_entries(&mut entries);
        assert_eq!(
            names(&entries),
            [
                "visible-folder",
                "visible-file",
                ".hidden-folder",
                ".hidden-file"
            ]
        );
    }

    #[test]
    fn phase_20b1a_sort_advanced_metadata_is_deterministic_and_unknown_last() {
        let mut low = entry("low".into(), EntryKind::RegularFile, Some(1), Some(1));
        low.set_indexed_sort_metadata(crate::IndexedSortMetadata {
            word_count: Some(1),
            line_count: Some(1),
            image_width: Some(10),
            image_height: Some(20),
            image_orientation: Some(b"a".to_vec().into_boxed_slice()),
            audio_artist: Some(b"a".to_vec().into_boxed_slice()),
            audio_album: Some(b"a".to_vec().into_boxed_slice()),
            audio_duration_millis: Some(1),
            audio_track: Some(1),
            audio_genre: Some(b"a".to_vec().into_boxed_slice()),
            audio_bitrate: Some(1),
            video_duration_millis: Some(1),
            video_width: Some(10),
            video_height: Some(20),
            video_frame_rate_milli: Some(1),
            video_bitrate: Some(1),
            link_destination: Some("a".into()),
            permissions: Some(0o600),
            owner: Some(1),
            group: Some(1),
        });
        let mut high = entry("high".into(), EntryKind::RegularFile, Some(1), Some(1));
        high.set_indexed_sort_metadata(crate::IndexedSortMetadata {
            word_count: Some(2),
            line_count: Some(2),
            image_width: Some(20),
            image_height: Some(30),
            image_orientation: Some(b"b".to_vec().into_boxed_slice()),
            audio_artist: Some(b"b".to_vec().into_boxed_slice()),
            audio_album: Some(b"b".to_vec().into_boxed_slice()),
            audio_duration_millis: Some(2),
            audio_track: Some(2),
            audio_genre: Some(b"b".to_vec().into_boxed_slice()),
            audio_bitrate: Some(2),
            video_duration_millis: Some(2),
            video_width: Some(20),
            video_height: Some(30),
            video_frame_rate_milli: Some(2),
            video_bitrate: Some(2),
            link_destination: Some("b".into()),
            permissions: Some(0o700),
            owner: Some(2),
            group: Some(2),
        });
        let unknown = entry("unknown".into(), EntryKind::RegularFile, Some(1), Some(1));
        let columns = [
            SortColumn::DocumentWordCount,
            SortColumn::DocumentLineCount,
            SortColumn::ImageDimensions,
            SortColumn::ImageOrientation,
            SortColumn::ImageWidth,
            SortColumn::ImageHeight,
            SortColumn::AudioArtist,
            SortColumn::AudioAlbum,
            SortColumn::AudioDuration,
            SortColumn::AudioTrack,
            SortColumn::AudioGenre,
            SortColumn::AudioBitrate,
            SortColumn::VideoDuration,
            SortColumn::VideoDimensions,
            SortColumn::VideoWidth,
            SortColumn::VideoHeight,
            SortColumn::VideoFrameRate,
            SortColumn::VideoBitrate,
            SortColumn::LinkDestination,
            SortColumn::Permissions,
            SortColumn::Owner,
            SortColumn::Group,
        ];
        for column in columns {
            assert_eq!(SortColumn::from_persisted(column.persisted()), Some(column));
            let mut ascending = vec![unknown.clone(), high.clone(), low.clone()];
            DirectorySort::new(column, SortDirection::Ascending).sort_entries(&mut ascending);
            assert_eq!(names(&ascending), ["low", "high", "unknown"], "{column:?}");
            let mut descending = vec![unknown.clone(), low.clone(), high.clone()];
            DirectorySort::new(column, SortDirection::Descending).sort_entries(&mut descending);
            assert_eq!(names(&descending), ["high", "low", "unknown"], "{column:?}");
        }
        let mut path_entries = vec![low, high];
        DirectorySort::new(SortColumn::Path, SortDirection::Ascending)
            .sort_entries(&mut path_entries);
        assert_eq!(names(&path_entries), ["high", "low"]);
        assert_eq!(SortColumn::ALL.len(), 34);
    }

    #[test]
    fn phase_20b2_grouping_date_uses_stable_calendar_days_and_unknown_last() {
        let folder = entry("folder".into(), EntryKind::Directory, None, None);
        let first = entry(
            "first".into(),
            EntryKind::RegularFile,
            Some(1),
            Some(86_400),
        );
        let second = entry(
            "second".into(),
            EntryKind::RegularFile,
            Some(1),
            Some(2 * 86_400),
        );
        let unknown = entry("unknown".into(), EntryKind::RegularFile, Some(1), None);
        let grouping = DirectoryGrouping::Date;
        let mut entries = vec![unknown, second, first, folder];
        DirectorySort::default()
            .with_grouping(grouping)
            .sort_entries(&mut entries);
        assert_eq!(names(&entries), ["folder", "first", "second", "unknown"]);
        assert_eq!(grouping.label(&entries[0]).as_deref(), Some("Folders"));
        assert_eq!(grouping.label(&entries[1]).as_deref(), Some("1970-01-02"));
        assert_eq!(grouping.label(&entries[2]).as_deref(), Some("1970-01-03"));
        assert_eq!(grouping.label(&entries[3]).as_deref(), Some("Date unknown"));
        assert_eq!(DirectoryGrouping::from_persisted("date"), Some(grouping));
    }

    #[test]
    fn phase_20b2_grouping_size_uses_human_buckets_and_boundaries() {
        let grouping = DirectoryGrouping::Size;
        let mut entries = vec![
            entry("unknown".into(), EntryKind::RegularFile, None, None),
            entry(
                "huge".into(),
                EntryKind::RegularFile,
                Some(11_000_000_000),
                None,
            ),
            entry(
                "medium".into(),
                EntryKind::RegularFile,
                Some(500_000_000),
                None,
            ),
            entry(
                "small".into(),
                EntryKind::RegularFile,
                Some(2_000_000),
                None,
            ),
            entry("tiny".into(), EntryKind::RegularFile, Some(1), None),
            entry("empty".into(), EntryKind::RegularFile, Some(0), None),
        ];
        DirectorySort::default()
            .with_grouping(grouping)
            .sort_entries(&mut entries);
        assert_eq!(
            names(&entries),
            ["empty", "tiny", "small", "medium", "huge", "unknown"]
        );
        assert_eq!(grouping.label(&entries[0]).as_deref(), Some("Empty (0 B)"));
        assert_eq!(grouping.label(&entries[5]).as_deref(), Some("Size unknown"));
        assert_eq!(DirectoryGrouping::from_persisted("size"), Some(grouping));
    }
}
