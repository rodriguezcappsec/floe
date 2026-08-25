//! Toolkit-independent browser view policy shared by preferences and sessions.

use crate::{DirectorySort, SortColumn};

pub const GRID_SIZES: [u16; 7] = [64, 80, 96, 112, 128, 160, 192];
const DEFAULT_GRID_SIZE_INDEX: usize = 3;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ViewMode {
    #[default]
    List,
    Grid,
    Miller,
}

impl ViewMode {
    pub const fn stack_name(self) -> &'static str {
        self.persisted()
    }

    pub const fn persisted(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Grid => "grid",
            Self::Miller => "miller",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        match value {
            "list" => Some(Self::List),
            "grid" => Some(Self::Grid),
            "miller" => Some(Self::Miller),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FileViewDensity {
    Compact,
    #[default]
    Comfortable,
    Spacious,
}

impl FileViewDensity {
    pub const ALL: [Self; 3] = [Self::Compact, Self::Comfortable, Self::Spacious];

    pub const fn persisted(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Comfortable => "comfortable",
            Self::Spacious => "spacious",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        match value {
            "compact" => Some(Self::Compact),
            "comfortable" => Some(Self::Comfortable),
            "spacious" => Some(Self::Spacious),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum ListColumn {
    Name,
    Type,
    Size,
    Modified,
    Extension,
    Mime,
    Created,
    Accessed,
    Permissions,
    Dimensions,
    Duration,
    Artist,
    Album,
    Track,
}

impl ListColumn {
    pub const ALL: [Self; 14] = [
        Self::Name,
        Self::Type,
        Self::Size,
        Self::Modified,
        Self::Extension,
        Self::Mime,
        Self::Created,
        Self::Accessed,
        Self::Permissions,
        Self::Dimensions,
        Self::Duration,
        Self::Artist,
        Self::Album,
        Self::Track,
    ];

    pub const OPTIONAL: [Self; 13] = [
        Self::Type,
        Self::Size,
        Self::Modified,
        Self::Extension,
        Self::Mime,
        Self::Created,
        Self::Accessed,
        Self::Permissions,
        Self::Dimensions,
        Self::Duration,
        Self::Artist,
        Self::Album,
        Self::Track,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Type => "Type",
            Self::Size => "Size",
            Self::Modified => "Modified",
            Self::Extension => "Extension",
            Self::Mime => "MIME Type",
            Self::Created => "Created",
            Self::Accessed => "Accessed",
            Self::Permissions => "Permissions",
            Self::Dimensions => "Dimensions",
            Self::Duration => "Duration",
            Self::Artist => "Artist",
            Self::Album => "Album",
            Self::Track => "Track",
        }
    }

    pub const fn persisted(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Type => "type",
            Self::Size => "size",
            Self::Modified => "modified",
            Self::Extension => "extension",
            Self::Mime => "mime",
            Self::Created => "created",
            Self::Accessed => "accessed",
            Self::Permissions => "permissions",
            Self::Dimensions => "dimensions",
            Self::Duration => "duration",
            Self::Artist => "artist",
            Self::Album => "album",
            Self::Track => "track",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|column| column.persisted() == value)
    }

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn default_width(self) -> u16 {
        match self {
            Self::Name => 260,
            Self::Type => 112,
            Self::Size => 96,
            Self::Modified | Self::Created | Self::Accessed => 168,
            Self::Extension => 112,
            Self::Mime => 176,
            Self::Permissions => 128,
            Self::Dimensions => 128,
            Self::Duration => 104,
            Self::Artist | Self::Album => 176,
            Self::Track => 96,
        }
    }

    pub const fn width_bounds(self) -> (u16, u16) {
        match self {
            Self::Name => (140, 720),
            Self::Size => (72, 220),
            _ => (88, 360),
        }
    }

    pub const fn requires_lazy_metadata(self) -> bool {
        matches!(
            self,
            Self::Mime
                | Self::Created
                | Self::Accessed
                | Self::Permissions
                | Self::Dimensions
                | Self::Duration
                | Self::Artist
                | Self::Album
                | Self::Track
        )
    }

    pub const fn sort_column(self) -> Option<SortColumn> {
        match self {
            Self::Name => Some(SortColumn::Name),
            Self::Type => Some(SortColumn::Type),
            Self::Size => Some(SortColumn::Size),
            Self::Modified => Some(SortColumn::Modified),
            Self::Extension => Some(SortColumn::Extension),
            Self::Mime
            | Self::Created
            | Self::Accessed
            | Self::Permissions
            | Self::Dimensions
            | Self::Duration
            | Self::Artist
            | Self::Album
            | Self::Track => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListColumnLayout {
    visible: u16,
    widths: [u16; ListColumn::ALL.len()],
}

impl Default for ListColumnLayout {
    fn default() -> Self {
        let mut layout = Self {
            visible: 0,
            widths: [0; ListColumn::ALL.len()],
        };
        for column in ListColumn::ALL {
            layout.widths[column.index()] = column.default_width();
        }
        for column in [
            ListColumn::Name,
            ListColumn::Type,
            ListColumn::Size,
            ListColumn::Modified,
        ] {
            layout.visible |= 1 << column.index();
        }
        layout
    }
}

impl ListColumnLayout {
    pub fn is_visible(self, column: ListColumn) -> bool {
        self.visible & (1 << column.index()) != 0
    }

    pub fn set_visible(&mut self, column: ListColumn, visible: bool) {
        if column == ListColumn::Name {
            return;
        }
        if visible {
            self.visible |= 1 << column.index();
        } else {
            self.visible &= !(1 << column.index());
        }
    }

    pub fn width(self, column: ListColumn) -> u16 {
        self.widths[column.index()]
    }

    pub fn set_width(&mut self, column: ListColumn, width: u16) {
        let (minimum, maximum) = column.width_bounds();
        self.widths[column.index()] = width.clamp(minimum, maximum);
    }

    pub fn needs_lazy_metadata(self) -> bool {
        ListColumn::ALL
            .into_iter()
            .any(|column| self.is_visible(column) && column.requires_lazy_metadata())
    }

    pub fn needs_advanced_metadata(self) -> bool {
        [
            ListColumn::Dimensions,
            ListColumn::Duration,
            ListColumn::Artist,
            ListColumn::Album,
            ListColumn::Track,
        ]
        .into_iter()
        .any(|column| self.is_visible(column))
    }

    pub fn visible_names(self) -> String {
        ListColumn::ALL
            .into_iter()
            .filter(|column| self.is_visible(*column))
            .map(ListColumn::persisted)
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn parse_visible(value: &str) -> Self {
        let mut layout = Self {
            visible: 1 << ListColumn::Name.index(),
            ..Self::default()
        };
        for name in value.split(',') {
            if let Some(column) = ListColumn::from_persisted(name.trim()) {
                layout.visible |= 1 << column.index();
            }
        }
        layout
    }

    pub fn widths_text(self) -> String {
        ListColumn::ALL
            .into_iter()
            .map(|column| format!("{}:{}", column.persisted(), self.width(column)))
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn apply_widths_text(&mut self, value: &str) {
        for item in value.split(',') {
            let Some((name, width)) = item.split_once(':') else {
                continue;
            };
            let (Some(column), Ok(width)) = (
                ListColumn::from_persisted(name.trim()),
                width.trim().parse::<u16>(),
            ) else {
                continue;
            };
            self.set_width(column, width);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FolderViewState {
    pub mode: ViewMode,
    pub grid_size: GridSize,
    pub density: FileViewDensity,
    pub sort: DirectorySort,
    pub columns: ListColumnLayout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GridSize {
    index: usize,
}

impl Default for GridSize {
    fn default() -> Self {
        Self {
            index: DEFAULT_GRID_SIZE_INDEX,
        }
    }
}

impl GridSize {
    pub const fn from_index(index: usize) -> Option<Self> {
        if index < GRID_SIZES.len() {
            Some(Self { index })
        } else {
            None
        }
    }

    pub fn from_persisted(edge: u16) -> Option<Self> {
        GRID_SIZES
            .iter()
            .position(|candidate| *candidate == edge)
            .map(|index| Self { index })
    }

    pub const fn index(self) -> usize {
        self.index
    }

    pub const fn edge(self) -> u16 {
        GRID_SIZES[self.index]
    }

    pub fn tile_width(self) -> i32 {
        i32::from(self.edge()) + 40
    }

    pub const fn zoom_in(self) -> Self {
        if self.index + 1 < GRID_SIZES.len() {
            Self {
                index: self.index + 1,
            }
        } else {
            self
        }
    }

    pub const fn zoom_out(self) -> Self {
        if self.index > 0 {
            Self {
                index: self.index - 1,
            }
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_7a_view_preserves_strict_modes_and_bounded_grid_steps() {
        assert_eq!(ViewMode::from_persisted("list"), Some(ViewMode::List));
        assert_eq!(ViewMode::from_persisted("grid"), Some(ViewMode::Grid));
        assert_eq!(ViewMode::from_persisted("miller"), Some(ViewMode::Miller));
        assert_eq!(ViewMode::from_persisted("GRID"), None);
        let smallest = GridSize::from_index(0).expect("smallest");
        let largest = GridSize::from_index(GRID_SIZES.len() - 1).expect("largest");
        assert_eq!(smallest.zoom_out(), smallest);
        assert_eq!(largest.zoom_in(), largest);
        assert_eq!(GridSize::default().edge(), 112);
    }

    #[test]
    fn phase_7a_view_preserves_density_and_column_policy() {
        for density in FileViewDensity::ALL {
            assert_eq!(
                FileViewDensity::from_persisted(density.persisted()),
                Some(density)
            );
        }
        let mut layout = ListColumnLayout::parse_visible("name,size,mime,unknown");
        layout.apply_widths_text("name:320,size:1,mime:240,unknown:999,bad");
        assert_eq!(layout.visible_names(), "name,size,mime");
        assert_eq!(layout.width(ListColumn::Name), 320);
        assert_eq!(layout.width(ListColumn::Size), 72);
        assert!(layout.needs_lazy_metadata());
        assert!(!layout.needs_advanced_metadata());
        layout.set_visible(ListColumn::Name, false);
        assert!(layout.is_visible(ListColumn::Name));
        assert_eq!(
            ListColumn::Extension.sort_column(),
            Some(SortColumn::Extension)
        );
    }

    #[test]
    fn phase_10f_advanced_columns_are_optional_lazy_and_persisted() {
        let advanced = [
            ListColumn::Dimensions,
            ListColumn::Duration,
            ListColumn::Artist,
            ListColumn::Album,
            ListColumn::Track,
        ];
        for column in advanced {
            assert!(ListColumn::OPTIONAL.contains(&column));
            assert!(column.requires_lazy_metadata());
            assert_eq!(column.sort_column(), None);
        }
        let mut layout =
            ListColumnLayout::parse_visible("name,dimensions,duration,artist,album,track");
        layout.apply_widths_text("dimensions:150,duration:100,artist:220,album:230,track:95");
        assert_eq!(
            layout.visible_names(),
            "name,dimensions,duration,artist,album,track"
        );
        assert_eq!(layout.width(ListColumn::Artist), 220);
        assert!(layout.needs_lazy_metadata());

        let mut legacy = ListColumnLayout::parse_visible(
            "name,type,size,modified,extension,mime,created,accessed,permissions",
        );
        legacy.apply_widths_text(
            "name:300,type:110,size:90,modified:170,extension:120,mime:190,created:130,accessed:140,permissions:150",
        );
        assert_eq!(
            legacy.visible_names(),
            "name,type,size,modified,extension,mime,created,accessed,permissions"
        );
        assert!(!legacy.needs_advanced_metadata());
        assert_eq!(
            legacy.width(ListColumn::Artist),
            ListColumn::Artist.default_width()
        );
    }
}
