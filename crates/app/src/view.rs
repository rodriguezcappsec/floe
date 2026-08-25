//! Application action mapping for the canonical core-owned view policy.

pub use floe_core::{
    FileViewDensity, FolderViewState, GRID_SIZES, GridSize, ListColumn, ListColumnLayout, ViewMode,
};

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
                ("zoom-in", ViewCommand::ZoomIn),
                ("zoom-out", ViewCommand::ZoomOut),
            ]
        );
    }
}
