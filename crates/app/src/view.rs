pub const GRID_SIZES: [u16; 7] = [64, 80, 96, 112, 128, 160, 192];
const DEFAULT_GRID_SIZE_INDEX: usize = 3;

pub const VIEW_ACTIONS: [(&str, ViewCommand); 4] = [
    ("view-list", ViewCommand::List),
    ("view-grid", ViewCommand::Grid),
    ("zoom-in", ViewCommand::ZoomIn),
    ("zoom-out", ViewCommand::ZoomOut),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewCommand {
    List,
    Grid,
    ZoomIn,
    ZoomOut,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ViewMode {
    #[default]
    List,
    Grid,
}

impl ViewMode {
    pub const fn stack_name(self) -> &'static str {
        self.persisted()
    }

    pub const fn persisted(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Grid => "grid",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        match value {
            "list" => Some(Self::List),
            "grid" => Some(Self::Grid),
            _ => None,
        }
    }
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
    fn phase_6d_view_modes_have_strict_persisted_values() {
        assert_eq!(ViewMode::from_persisted("list"), Some(ViewMode::List));
        assert_eq!(ViewMode::from_persisted("grid"), Some(ViewMode::Grid));
        assert_eq!(ViewMode::from_persisted("GRID"), None);
        assert_eq!(ViewMode::Grid.persisted(), "grid");
    }

    #[test]
    fn phase_6d_grid_size_zoom_is_bounded_and_uses_discrete_steps() {
        let smallest = GridSize::from_index(0).expect("smallest size should exist");
        let largest =
            GridSize::from_index(GRID_SIZES.len() - 1).expect("largest size should exist");
        assert_eq!(smallest.zoom_out(), smallest);
        assert_eq!(largest.zoom_in(), largest);
        assert_eq!(GridSize::default().edge(), 112);
        assert_eq!(GridSize::default().zoom_in().edge(), 128);
        assert_eq!(GridSize::default().zoom_out().edge(), 96);
        assert!(GridSize::from_index(GRID_SIZES.len()).is_none());
    }

    #[test]
    fn phase_6d_persisted_grid_size_rejects_arbitrary_values() {
        for (index, edge) in GRID_SIZES.into_iter().enumerate() {
            let size = GridSize::from_persisted(edge).expect("declared size should parse");
            assert_eq!(size.index(), index);
            assert_eq!(size.edge(), edge);
        }
        assert_eq!(GridSize::from_persisted(0), None);
        assert_eq!(GridSize::from_persisted(100), None);
        assert_eq!(GridSize::from_persisted(193), None);
    }

    #[test]
    fn phase_6d_view_actions_have_stable_keyboard_facing_names() {
        assert_eq!(
            VIEW_ACTIONS,
            [
                ("view-list", ViewCommand::List),
                ("view-grid", ViewCommand::Grid),
                ("zoom-in", ViewCommand::ZoomIn),
                ("zoom-out", ViewCommand::ZoomOut),
            ]
        );
    }
}
