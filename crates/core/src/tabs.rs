//! Bounded, toolkit-independent live tab state.

use std::path::PathBuf;

use thiserror::Error;

use crate::{BrowserSession, BrowserSessionId, FolderViewState, SessionStateError};

pub const TAB_CAPACITY: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TabActivation {
    Foreground,
    Background,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClosedTab {
    pub session: BrowserSession,
    pub active_changed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserTabs {
    sessions: Vec<BrowserSession>,
    active: usize,
    next_id: u64,
}

impl BrowserTabs {
    pub fn new(initial_path: PathBuf, view: FolderViewState) -> Result<Self, TabError> {
        let session = BrowserSession::new(BrowserSessionId::new(1)?, initial_path, view)?;
        Ok(Self {
            sessions: vec![session],
            active: 0,
            next_id: 2,
        })
    }

    pub fn sessions(&self) -> &[BrowserSession] {
        &self.sessions
    }

    pub fn active(&self) -> &BrowserSession {
        &self.sessions[self.active]
    }

    pub fn active_mut(&mut self) -> &mut BrowserSession {
        &mut self.sessions[self.active]
    }

    pub fn active_id(&self) -> BrowserSessionId {
        self.active().id()
    }

    pub const fn active_index(&self) -> usize {
        self.active
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    pub fn session(&self, id: BrowserSessionId) -> Option<&BrowserSession> {
        self.sessions.iter().find(|session| session.id() == id)
    }

    pub fn activate(&mut self, id: BrowserSessionId) -> Result<bool, TabError> {
        let index = self.index_of(id)?;
        if index == self.active {
            return Ok(false);
        }
        self.active = index;
        Ok(true)
    }

    pub fn activate_relative(&mut self, delta: isize) -> bool {
        if self.sessions.len() < 2 {
            return false;
        }
        let len = self.sessions.len() as isize;
        self.active = (self.active as isize + delta).rem_euclid(len) as usize;
        true
    }

    pub fn open(
        &mut self,
        path: PathBuf,
        view: FolderViewState,
        activation: TabActivation,
    ) -> Result<BrowserSessionId, TabError> {
        self.ensure_capacity()?;
        let id = self.allocate_id()?;
        let session = BrowserSession::new(id, path, view)?;
        self.sessions.push(session);
        if activation == TabActivation::Foreground {
            self.active = self.sessions.len() - 1;
        }
        Ok(id)
    }

    pub fn duplicate(
        &mut self,
        source: BrowserSessionId,
        activation: TabActivation,
    ) -> Result<BrowserSessionId, TabError> {
        self.ensure_capacity()?;
        let index = self.index_of(source)?;
        let id = self.allocate_id()?;
        let duplicate = self.sessions[index].duplicate(id);
        self.sessions.insert(index + 1, duplicate);
        if activation == TabActivation::Foreground {
            self.active = index + 1;
        } else if self.active > index {
            self.active += 1;
        }
        Ok(id)
    }

    pub fn close(&mut self, id: BrowserSessionId) -> Result<ClosedTab, TabError> {
        if self.sessions.len() == 1 {
            return Err(TabError::LastTab);
        }
        let index = self.index_of(id)?;
        let was_active = index == self.active;
        let session = self.sessions.remove(index);
        if index < self.active || self.active == self.sessions.len() {
            self.active = self.active.saturating_sub(1);
        }
        Ok(ClosedTab {
            session,
            active_changed: was_active,
        })
    }

    pub fn move_before(
        &mut self,
        source: BrowserSessionId,
        target: BrowserSessionId,
    ) -> Result<bool, TabError> {
        let source_index = self.index_of(source)?;
        let target_index = self.index_of(target)?;
        if source_index == target_index || source_index + 1 == target_index {
            return Ok(false);
        }
        let active_id = self.active_id();
        let session = self.sessions.remove(source_index);
        let insertion = if source_index < target_index {
            target_index - 1
        } else {
            target_index
        };
        self.sessions.insert(insertion, session);
        self.active = self.index_of(active_id)?;
        Ok(true)
    }

    pub fn move_active(&mut self, delta: isize) -> bool {
        let target = (self.active as isize + delta)
            .clamp(0, self.sessions.len().saturating_sub(1) as isize) as usize;
        if target == self.active {
            return false;
        }
        self.sessions.swap(self.active, target);
        self.active = target;
        true
    }

    fn index_of(&self, id: BrowserSessionId) -> Result<usize, TabError> {
        self.sessions
            .iter()
            .position(|session| session.id() == id)
            .ok_or(TabError::UnknownTab(id.get()))
    }

    fn ensure_capacity(&self) -> Result<(), TabError> {
        if self.sessions.len() >= TAB_CAPACITY {
            Err(TabError::Capacity(TAB_CAPACITY))
        } else {
            Ok(())
        }
    }

    fn allocate_id(&mut self) -> Result<BrowserSessionId, TabError> {
        let id = BrowserSessionId::new(self.next_id)?;
        self.next_id = self.next_id.checked_add(1).ok_or(TabError::IdExhausted)?;
        Ok(id)
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TabError {
    #[error(transparent)]
    Session(#[from] SessionStateError),
    #[error("the maximum of {0} tabs is already open")]
    Capacity(usize),
    #[error("tab {0} does not exist")]
    UnknownTab(u64),
    #[error("the last tab closes the window rather than leaving an empty browser")]
    LastTab,
    #[error("tab ID space is exhausted")]
    IdExhausted,
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

    use crate::{FolderViewState, ViewMode};

    use super::{BrowserTabs, TAB_CAPACITY, TabActivation, TabError};

    #[test]
    fn phase_7b_tabs_new_activate_duplicate_close_preserve_exact_sessions() {
        let raw = PathBuf::from(OsString::from_vec(b"/tmp/floe-\xff".to_vec()));
        let mut tabs =
            BrowserTabs::new(raw.clone(), FolderViewState::default()).expect("phase 7B fixture");
        let background = tabs
            .open(
                PathBuf::from("/background"),
                FolderViewState::default(),
                TabActivation::Background,
            )
            .expect("phase 7B fixture");
        assert_eq!(tabs.active().current().path(), raw);
        assert!(tabs.activate(background).expect("phase 7B fixture"));
        tabs.active_mut().set_view(FolderViewState {
            mode: ViewMode::Grid,
            ..FolderViewState::default()
        });
        let duplicate = tabs
            .duplicate(background, TabActivation::Foreground)
            .expect("phase 7B fixture");
        assert_eq!(tabs.active_id(), duplicate);
        assert_eq!(tabs.active().current().view().mode, ViewMode::Grid);
        let closed = tabs.close(duplicate).expect("phase 7B fixture");
        assert!(closed.active_changed);
        assert_eq!(tabs.active_id(), background);
    }

    #[test]
    fn phase_7b_tabs_reorder_and_relative_switch_are_deterministic() {
        let mut tabs = BrowserTabs::new(PathBuf::from("/one"), FolderViewState::default())
            .expect("phase 7B fixture");
        let two = tabs
            .open(
                PathBuf::from("/two"),
                FolderViewState::default(),
                TabActivation::Foreground,
            )
            .expect("phase 7B fixture");
        let three = tabs
            .open(
                PathBuf::from("/three"),
                FolderViewState::default(),
                TabActivation::Foreground,
            )
            .expect("phase 7B fixture");
        let one = tabs.sessions()[0].id();
        assert!(tabs.move_before(three, one).expect("phase 7B fixture"));
        assert_eq!(
            tabs.sessions()
                .iter()
                .map(|tab| tab.id())
                .collect::<Vec<_>>(),
            vec![three, one, two]
        );
        assert!(tabs.activate_relative(1));
        assert_eq!(tabs.active_id(), one);
        assert!(tabs.move_active(1));
        assert_eq!(tabs.active_id(), one);
        assert_eq!(tabs.active_index(), 2);
    }

    #[test]
    fn phase_7b_tabs_enforce_last_tab_and_capacity_bounds() {
        let mut tabs = BrowserTabs::new(PathBuf::from("/"), FolderViewState::default())
            .expect("phase 7B fixture");
        assert_eq!(tabs.close(tabs.active_id()), Err(TabError::LastTab));
        for index in 1..TAB_CAPACITY {
            tabs.open(
                PathBuf::from(format!("/{index}")),
                FolderViewState::default(),
                TabActivation::Background,
            )
            .expect("phase 7B fixture");
        }
        assert_eq!(
            tabs.open(
                PathBuf::from("/overflow"),
                FolderViewState::default(),
                TabActivation::Background
            ),
            Err(TabError::Capacity(TAB_CAPACITY))
        );
    }
}
