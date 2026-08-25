//! GTK-independent per-tab split-view state.

use thiserror::Error;

use crate::{BrowserSession, BrowserSessionId, SessionLocation};

pub const SPLIT_RATIO_MIN: u16 = 2_000;
pub const SPLIT_RATIO_MAX: u16 = 8_000;
pub const SPLIT_RATIO_DEFAULT: u16 = 5_000;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SplitSide {
    Primary,
    Secondary,
}

impl SplitSide {
    pub const fn opposite(self) -> Self {
        match self {
            Self::Primary => Self::Secondary,
            Self::Secondary => Self::Primary,
        }
    }

    pub(crate) const fn encoded(self) -> u8 {
        match self {
            Self::Primary => 0,
            Self::Secondary => 1,
        }
    }

    pub(crate) fn from_encoded(value: u8) -> Result<Self, SplitStateError> {
        match value {
            0 => Ok(Self::Primary),
            1 => Ok(Self::Secondary),
            _ => Err(SplitStateError::InvalidSide(value)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SplitRatio(u16);

impl SplitRatio {
    pub fn new(basis_points: u16) -> Result<Self, SplitStateError> {
        if !(SPLIT_RATIO_MIN..=SPLIT_RATIO_MAX).contains(&basis_points) {
            return Err(SplitStateError::RatioOutOfRange {
                value: basis_points,
                minimum: SPLIT_RATIO_MIN,
                maximum: SPLIT_RATIO_MAX,
            });
        }
        Ok(Self(basis_points))
    }

    pub const fn basis_points(self) -> u16 {
        self.0
    }
}

impl Default for SplitRatio {
    fn default() -> Self {
        Self(SPLIT_RATIO_DEFAULT)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserSplit {
    primary: BrowserSession,
    secondary: Option<BrowserSession>,
    active: SplitSide,
    ratio: SplitRatio,
}

impl BrowserSplit {
    pub const fn new(primary: BrowserSession) -> Self {
        Self {
            primary,
            secondary: None,
            active: SplitSide::Primary,
            ratio: SplitRatio(SPLIT_RATIO_DEFAULT),
        }
    }

    pub(crate) fn from_parts(
        primary: BrowserSession,
        secondary: Option<BrowserSession>,
        active: SplitSide,
        ratio: SplitRatio,
    ) -> Result<Self, SplitStateError> {
        if secondary
            .as_ref()
            .is_some_and(|session| session.id() == primary.id())
        {
            return Err(SplitStateError::DuplicatePaneId(primary.id().get()));
        }
        if active == SplitSide::Secondary && secondary.is_none() {
            return Err(SplitStateError::MissingSecondary);
        }
        Ok(Self {
            primary,
            secondary,
            active,
            ratio,
        })
    }

    pub const fn id(&self) -> BrowserSessionId {
        self.primary.id()
    }

    pub const fn is_split(&self) -> bool {
        self.secondary.is_some()
    }

    pub const fn active_side(&self) -> SplitSide {
        self.active
    }

    pub const fn ratio(&self) -> SplitRatio {
        self.ratio
    }

    pub fn primary(&self) -> &BrowserSession {
        &self.primary
    }

    pub fn secondary(&self) -> Option<&BrowserSession> {
        self.secondary.as_ref()
    }

    pub fn pane(&self, side: SplitSide) -> Option<&BrowserSession> {
        match side {
            SplitSide::Primary => Some(&self.primary),
            SplitSide::Secondary => self.secondary.as_ref(),
        }
    }

    pub fn pane_mut(&mut self, side: SplitSide) -> Option<&mut BrowserSession> {
        match side {
            SplitSide::Primary => Some(&mut self.primary),
            SplitSide::Secondary => self.secondary.as_mut(),
        }
    }

    pub fn active(&self) -> &BrowserSession {
        self.pane(self.active)
            .expect("split invariant keeps the active pane present")
    }

    pub fn current(&self) -> &SessionLocation {
        self.active().current()
    }

    pub fn active_mut(&mut self) -> &mut BrowserSession {
        self.pane_mut(self.active)
            .expect("split invariant keeps the active pane present")
    }

    pub fn opposite(&self) -> Option<&BrowserSession> {
        self.pane(self.active.opposite())
    }

    pub fn split(&mut self, secondary: BrowserSession) -> Result<(), SplitStateError> {
        if self.secondary.is_some() {
            return Err(SplitStateError::AlreadySplit);
        }
        if secondary.id() == self.primary.id() {
            return Err(SplitStateError::DuplicatePaneId(secondary.id().get()));
        }
        self.secondary = Some(secondary);
        Ok(())
    }

    pub fn activate(&mut self, side: SplitSide) -> Result<bool, SplitStateError> {
        if side == SplitSide::Secondary && self.secondary.is_none() {
            return Err(SplitStateError::MissingSecondary);
        }
        let changed = self.active != side;
        self.active = side;
        Ok(changed)
    }

    pub fn set_ratio(&mut self, ratio: SplitRatio) {
        self.ratio = ratio;
    }

    pub fn close(&mut self, side: SplitSide) -> Result<BrowserSession, SplitStateError> {
        let secondary = self.secondary.take().ok_or(SplitStateError::NotSplit)?;
        let removed = match side {
            SplitSide::Primary => {
                let retained = secondary.duplicate(self.primary.id());
                std::mem::replace(&mut self.primary, retained)
            }
            SplitSide::Secondary => secondary,
        };
        self.active = SplitSide::Primary;
        Ok(removed)
    }

    pub fn swap(&mut self) -> Result<(), SplitStateError> {
        let secondary = self.secondary.as_ref().ok_or(SplitStateError::NotSplit)?;
        let primary_id = self.primary.id();
        let secondary_id = secondary.id();
        let new_primary = secondary.duplicate(primary_id);
        let new_secondary = self.primary.duplicate(secondary_id);
        self.primary = new_primary;
        self.secondary = Some(new_secondary);
        self.active = self.active.opposite();
        Ok(())
    }

    pub(crate) fn duplicate_with_ids(
        &self,
        primary_id: BrowserSessionId,
        secondary_id: Option<BrowserSessionId>,
    ) -> Result<Self, SplitStateError> {
        match (&self.secondary, secondary_id) {
            (None, None) => Self::from_parts(
                self.primary.duplicate(primary_id),
                None,
                SplitSide::Primary,
                self.ratio,
            ),
            (Some(secondary), Some(secondary_id)) => Self::from_parts(
                self.primary.duplicate(primary_id),
                Some(secondary.duplicate(secondary_id)),
                self.active,
                self.ratio,
            ),
            _ => Err(SplitStateError::PaneIdCountMismatch),
        }
    }

    pub(crate) fn pane_ids(&self) -> impl Iterator<Item = BrowserSessionId> + '_ {
        std::iter::once(self.primary.id()).chain(self.secondary.as_ref().map(BrowserSession::id))
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SplitStateError {
    #[error("browser context is already split")]
    AlreadySplit,
    #[error("browser context is not split")]
    NotSplit,
    #[error("secondary pane is unavailable")]
    MissingSecondary,
    #[error("split side value {0} is invalid")]
    InvalidSide(u8),
    #[error("split ratio {value} is outside {minimum}..={maximum} basis points")]
    RatioOutOfRange {
        value: u16,
        minimum: u16,
        maximum: u16,
    },
    #[error("split panes repeat session ID {0}")]
    DuplicatePaneId(u64),
    #[error("split duplication pane ID count does not match pane count")]
    PaneIdCountMismatch,
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        os::unix::ffi::OsStringExt,
        path::{Path, PathBuf},
    };

    use crate::{FolderViewState, ViewMode};

    use super::*;

    fn session(id: u64, path: PathBuf) -> BrowserSession {
        BrowserSession::new(
            BrowserSessionId::new(id).expect("nonzero fixture ID"),
            path,
            FolderViewState::default(),
        )
        .expect("absolute fixture path")
    }

    #[test]
    fn phase_7d_split_state_enforces_ratio_focus_close_and_swap() {
        assert!(SplitRatio::new(SPLIT_RATIO_MIN - 1).is_err());
        assert!(SplitRatio::new(SPLIT_RATIO_MAX + 1).is_err());
        let ratio = SplitRatio::new(6_250).expect("bounded ratio");
        let mut split = BrowserSplit::new(session(1, PathBuf::from("/left")));
        let stable_id = split.id();
        assert_eq!(
            split.activate(SplitSide::Secondary),
            Err(SplitStateError::MissingSecondary)
        );
        split
            .split(session(2, PathBuf::from("/right")))
            .expect("second pane");
        split.set_ratio(ratio);
        split
            .activate(SplitSide::Secondary)
            .expect("secondary focus");
        split.swap().expect("swap panes");
        assert_eq!(split.id(), stable_id);
        assert_eq!(split.active_side(), SplitSide::Primary);
        assert_eq!(split.primary().current().path(), Path::new("/right"));
        assert_eq!(
            split.secondary().expect("secondary").current().path(),
            Path::new("/left")
        );
        assert_eq!(split.ratio(), ratio);
        let removed = split.close(SplitSide::Primary).expect("close primary");
        assert_eq!(removed.current().path(), Path::new("/right"));
        assert_eq!(split.id(), stable_id);
        assert_eq!(split.current().path(), Path::new("/left"));
        assert!(!split.is_split());
        assert_eq!(split.active_side(), SplitSide::Primary);
    }

    #[test]
    fn phase_7d_independent_panes_preserve_raw_history_selection_and_view() {
        let raw = PathBuf::from(OsString::from_vec(b"/tmp/floe-split-\xff".to_vec()));
        let mut split = BrowserSplit::new(session(7, raw.clone()));
        split
            .active_mut()
            .navigate_to(
                PathBuf::from("/left-next"),
                FolderViewState {
                    mode: ViewMode::Grid,
                    ..FolderViewState::default()
                },
            )
            .expect("left navigation");
        split
            .active_mut()
            .set_selection(vec![PathBuf::from("/left-next/item")])
            .expect("left selection");
        split
            .split(session(8, PathBuf::from("/right")))
            .expect("secondary pane");
        split
            .activate(SplitSide::Secondary)
            .expect("secondary focus");
        split
            .active_mut()
            .navigate_to(PathBuf::from("/right-next"), FolderViewState::default())
            .expect("right navigation");
        assert_eq!(split.primary().back_history()[0].path(), raw);
        assert_eq!(split.primary().current().view().mode, ViewMode::Grid);
        assert_eq!(
            split.primary().current().selection(),
            &[PathBuf::from("/left-next/item")]
        );
        assert_eq!(
            split.secondary().expect("secondary").current().path(),
            Path::new("/right-next")
        );
        assert_eq!(
            split.secondary().expect("secondary").back_history()[0].path(),
            Path::new("/right")
        );
    }
}
