//! Application action mapping for the canonical core-owned view policy.

pub use floe_core::{
    FileViewDensity, FolderViewState, GRID_SIZES, GridSize, ListColumn, ListColumnLayout, ViewMode,
};

pub const MILLER_COLUMN_WIDTH_MIN: u16 = 180;
pub const MILLER_COLUMN_WIDTH_DEFAULT: u16 = 280;
pub const MILLER_COLUMN_WIDTH_MAX: u16 = 520;
pub const MILLER_COLUMN_WIDTH_STEP: u16 = 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MillerColumnWidth(u16);

impl Default for MillerColumnWidth {
    fn default() -> Self {
        Self(MILLER_COLUMN_WIDTH_DEFAULT)
    }
}

impl MillerColumnWidth {
    pub const fn new(value: u16) -> Self {
        if value < MILLER_COLUMN_WIDTH_MIN {
            Self(MILLER_COLUMN_WIDTH_MIN)
        } else if value > MILLER_COLUMN_WIDTH_MAX {
            Self(MILLER_COLUMN_WIDTH_MAX)
        } else {
            Self(value)
        }
    }

    pub const fn get(self) -> u16 {
        self.0
    }

    pub const fn narrower(self) -> Self {
        Self::new(self.0.saturating_sub(MILLER_COLUMN_WIDTH_STEP))
    }

    pub const fn wider(self) -> Self {
        Self::new(self.0.saturating_add(MILLER_COLUMN_WIDTH_STEP))
    }
}

pub const VIEW_ACTIONS: [(&str, ViewCommand); 5] = [
    ("view-list", ViewCommand::List),
    ("view-grid", ViewCommand::Grid),
    ("view-miller", ViewCommand::Miller),
    ("zoom-in", ViewCommand::ZoomIn),
    ("zoom-out", ViewCommand::ZoomOut),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewCommand {
    List,
    Miller,
    Grid,
    ZoomIn,
    ZoomOut,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_6d_view_actions_have_stable_keyboard_facing_names() {
        assert_eq!(
            VIEW_ACTIONS,
            [
                ("view-list", ViewCommand::List),
                ("view-grid", ViewCommand::Grid),
                ("view-miller", ViewCommand::Miller),
                ("zoom-in", ViewCommand::ZoomIn),
                ("zoom-out", ViewCommand::ZoomOut),
            ]
        );
    }
}
