//! Bounded, toolkit-independent live tab state.

use std::path::PathBuf;
use std::{collections::HashSet, convert::TryInto};

use thiserror::Error;

use crate::{BrowserSession, BrowserSessionId, FolderViewState, SessionStateError};

pub const TAB_CAPACITY: usize = 64;
pub const RECENTLY_CLOSED_CAPACITY: usize = 32;
pub const WORKSPACE_MAX_SERIALIZED_BYTES: usize = 64 * 1_048_576;
const WORKSPACE_MAGIC: &[u8; 8] = b"FLOETABS";
const WORKSPACE_VERSION: u16 = 1;

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
    recently_closed: Vec<BrowserSession>,
}

impl BrowserTabs {
    pub fn new(initial_path: PathBuf, view: FolderViewState) -> Result<Self, TabError> {
        let session = BrowserSession::new(BrowserSessionId::new(1)?, initial_path, view)?;
        Ok(Self {
            sessions: vec![session],
            active: 0,
            next_id: 2,
            recently_closed: Vec::new(),
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

    pub fn recently_closed_len(&self) -> usize {
        self.recently_closed.len()
    }

    pub fn can_reopen_closed(&self) -> bool {
        !self.recently_closed.is_empty() && self.sessions.len() < TAB_CAPACITY
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
        let closed = session.clone();
        self.push_recently_closed(session);
        Ok(ClosedTab {
            session: closed,
            active_changed: was_active,
        })
    }

    pub fn reopen_closed(&mut self) -> Result<BrowserSessionId, TabError> {
        self.ensure_capacity()?;
        let closed = self.recently_closed.pop().ok_or(TabError::NoClosedTab)?;
        let id = self.allocate_id()?;
        self.sessions.insert(self.active + 1, closed.duplicate(id));
        self.active += 1;
        Ok(id)
    }

    pub fn close_left_of(&mut self, id: BrowserSessionId) -> Result<usize, TabError> {
        let index = self.index_of(id)?;
        let targets = self.sessions[..index]
            .iter()
            .map(BrowserSession::id)
            .collect::<Vec<_>>();
        self.close_many(targets)
    }

    pub fn close_right_of(&mut self, id: BrowserSessionId) -> Result<usize, TabError> {
        let index = self.index_of(id)?;
        let targets = self.sessions[index + 1..]
            .iter()
            .map(BrowserSession::id)
            .rev()
            .collect::<Vec<_>>();
        self.close_many(targets)
    }

    pub fn close_others(&mut self, id: BrowserSessionId) -> Result<usize, TabError> {
        self.activate(id)?;
        let targets = self
            .sessions
            .iter()
            .filter(|session| session.id() != id)
            .map(BrowserSession::id)
            .collect::<Vec<_>>();
        self.close_many(targets)
    }

    pub fn encode_workspace(&self) -> Result<Vec<u8>, WorkspaceCodecError> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(WORKSPACE_MAGIC);
        bytes.extend_from_slice(&WORKSPACE_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.active_id().get().to_le_bytes());
        write_count(&mut bytes, self.sessions.len())?;
        for session in &self.sessions {
            write_session(&mut bytes, session)?;
        }
        write_count(&mut bytes, self.recently_closed.len())?;
        for session in &self.recently_closed {
            write_session(&mut bytes, session)?;
        }
        if bytes.len() > WORKSPACE_MAX_SERIALIZED_BYTES {
            return Err(WorkspaceCodecError::LimitExceeded("workspace bytes"));
        }
        Ok(bytes)
    }

    pub fn decode_workspace(bytes: &[u8]) -> Result<Self, WorkspaceCodecError> {
        if bytes.len() > WORKSPACE_MAX_SERIALIZED_BYTES {
            return Err(WorkspaceCodecError::LimitExceeded("workspace bytes"));
        }
        let mut decoder = WorkspaceDecoder::new(bytes);
        if decoder.read_exact(WORKSPACE_MAGIC.len())? != WORKSPACE_MAGIC {
            return Err(WorkspaceCodecError::InvalidHeader);
        }
        let version = decoder.read_u16()?;
        if version != WORKSPACE_VERSION {
            return Err(WorkspaceCodecError::UnsupportedVersion(version));
        }
        let active_id = BrowserSessionId::new(decoder.read_u64()?)?;
        let session_count = decoder.read_count(TAB_CAPACITY)?;
        if session_count == 0 {
            return Err(WorkspaceCodecError::EmptyWorkspace);
        }
        let mut sessions = Vec::with_capacity(session_count);
        for _ in 0..session_count {
            sessions.push(decoder.read_session()?);
        }
        let closed_count = decoder.read_count(RECENTLY_CLOSED_CAPACITY)?;
        let mut recently_closed = Vec::with_capacity(closed_count);
        for _ in 0..closed_count {
            recently_closed.push(decoder.read_session()?);
        }
        if !decoder.finished() {
            return Err(WorkspaceCodecError::TrailingBytes);
        }
        let mut ids = HashSet::with_capacity(sessions.len() + recently_closed.len());
        for session in sessions.iter().chain(&recently_closed) {
            if !ids.insert(session.id()) {
                return Err(WorkspaceCodecError::DuplicateSessionId(session.id().get()));
            }
        }
        let active = sessions
            .iter()
            .position(|session| session.id() == active_id)
            .ok_or(WorkspaceCodecError::UnknownActiveTab(active_id.get()))?;
        let maximum = ids.iter().map(|id| id.get()).max().unwrap_or(0);
        let next_id = maximum.checked_add(1).ok_or(TabError::IdExhausted)?;
        Ok(Self {
            sessions,
            active,
            next_id,
            recently_closed,
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

    fn push_recently_closed(&mut self, session: BrowserSession) {
        if self.recently_closed.len() == RECENTLY_CLOSED_CAPACITY {
            self.recently_closed.remove(0);
        }
        self.recently_closed.push(session);
    }

    fn close_many(&mut self, targets: Vec<BrowserSessionId>) -> Result<usize, TabError> {
        let mut closed = 0;
        for target in targets {
            self.close(target)?;
            closed += 1;
        }
        Ok(closed)
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
    #[error("there is no recently closed tab")]
    NoClosedTab,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum WorkspaceCodecError {
    #[error(transparent)]
    Tab(#[from] TabError),
    #[error(transparent)]
    Session(#[from] crate::SessionCodecError),
    #[error(transparent)]
    SessionState(#[from] SessionStateError),
    #[error("workspace header is invalid")]
    InvalidHeader,
    #[error("workspace version {0} is unsupported")]
    UnsupportedVersion(u16),
    #[error("workspace has no live tab")]
    EmptyWorkspace,
    #[error("workspace active tab {0} does not exist")]
    UnknownActiveTab(u64),
    #[error("workspace repeats session ID {0}")]
    DuplicateSessionId(u64),
    #[error("workspace is truncated")]
    Truncated,
    #[error("workspace has trailing bytes")]
    TrailingBytes,
    #[error("workspace exceeds the {0} limit")]
    LimitExceeded(&'static str),
}

fn write_count(bytes: &mut Vec<u8>, count: usize) -> Result<(), WorkspaceCodecError> {
    let count = u32::try_from(count).map_err(|_| WorkspaceCodecError::LimitExceeded("count"))?;
    bytes.extend_from_slice(&count.to_le_bytes());
    Ok(())
}

fn write_session(bytes: &mut Vec<u8>, session: &BrowserSession) -> Result<(), WorkspaceCodecError> {
    let encoded = session.encode()?;
    let projected = bytes
        .len()
        .checked_add(4)
        .and_then(|length| length.checked_add(encoded.len()))
        .ok_or(WorkspaceCodecError::LimitExceeded("workspace bytes"))?;
    if projected > WORKSPACE_MAX_SERIALIZED_BYTES {
        return Err(WorkspaceCodecError::LimitExceeded("workspace bytes"));
    }
    write_count(bytes, encoded.len())?;
    bytes.extend_from_slice(&encoded);
    Ok(())
}

struct WorkspaceDecoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> WorkspaceDecoder<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_exact(&mut self, count: usize) -> Result<&'a [u8], WorkspaceCodecError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(WorkspaceCodecError::Truncated)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(WorkspaceCodecError::Truncated)?;
        self.offset = end;
        Ok(value)
    }

    fn read_u16(&mut self) -> Result<u16, WorkspaceCodecError> {
        Ok(u16::from_le_bytes(
            self.read_exact(2)?
                .try_into()
                .map_err(|_| WorkspaceCodecError::Truncated)?,
        ))
    }

    fn read_u32(&mut self) -> Result<u32, WorkspaceCodecError> {
        Ok(u32::from_le_bytes(
            self.read_exact(4)?
                .try_into()
                .map_err(|_| WorkspaceCodecError::Truncated)?,
        ))
    }

    fn read_u64(&mut self) -> Result<u64, WorkspaceCodecError> {
        Ok(u64::from_le_bytes(
            self.read_exact(8)?
                .try_into()
                .map_err(|_| WorkspaceCodecError::Truncated)?,
        ))
    }

    fn read_count(&mut self, maximum: usize) -> Result<usize, WorkspaceCodecError> {
        let count = self.read_u32()? as usize;
        if count > maximum {
            return Err(WorkspaceCodecError::LimitExceeded("count"));
        }
        Ok(count)
    }

    fn read_session(&mut self) -> Result<BrowserSession, WorkspaceCodecError> {
        let length = self.read_count(crate::SESSION_MAX_SERIALIZED_BYTES)?;
        BrowserSession::decode(self.read_exact(length)?).map_err(Into::into)
    }

    fn finished(&self) -> bool {
        self.offset == self.bytes.len()
    }
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

    #[test]
    fn phase_7c_closed_tabs_are_bounded_lifo_and_reopen_with_fresh_id() {
        let mut tabs = BrowserTabs::new(PathBuf::from("/keep"), FolderViewState::default())
            .expect("initial tab");
        let mut last_closed_path = PathBuf::new();
        for index in 0..(super::RECENTLY_CLOSED_CAPACITY + 5) {
            let path = PathBuf::from(format!("/closed-{index}"));
            let id = tabs
                .open(
                    path.clone(),
                    FolderViewState::default(),
                    TabActivation::Foreground,
                )
                .expect("tab below live capacity");
            tabs.close(id).expect("tab should close");
            last_closed_path = path;
        }
        assert_eq!(tabs.recently_closed_len(), super::RECENTLY_CLOSED_CAPACITY);
        let old_max = tabs
            .sessions()
            .iter()
            .map(|session| session.id().get())
            .max()
            .expect("live tab ID");
        let reopened = tabs.reopen_closed().expect("recent tab should reopen");
        assert!(reopened.get() > old_max);
        assert_eq!(tabs.active().current().path(), last_closed_path);
    }

    #[test]
    fn phase_7c_closed_tabs_close_variants_preserve_target_and_active_owner() {
        let mut tabs = BrowserTabs::new(PathBuf::from("/one"), FolderViewState::default())
            .expect("initial tab");
        let two = tabs
            .open(
                PathBuf::from("/two"),
                FolderViewState::default(),
                TabActivation::Foreground,
            )
            .expect("second tab");
        let three = tabs
            .open(
                PathBuf::from("/three"),
                FolderViewState::default(),
                TabActivation::Foreground,
            )
            .expect("third tab");
        let four = tabs
            .open(
                PathBuf::from("/four"),
                FolderViewState::default(),
                TabActivation::Foreground,
            )
            .expect("fourth tab");
        assert_eq!(tabs.close_left_of(three).expect("close left"), 2);
        assert_eq!(
            tabs.sessions()
                .iter()
                .map(|tab| tab.id())
                .collect::<Vec<_>>(),
            vec![three, four]
        );
        assert_eq!(tabs.active_id(), four);
        assert_eq!(tabs.close_right_of(three).expect("close right"), 1);
        assert_eq!(tabs.active_id(), three);
        let extra = tabs
            .open(
                PathBuf::from("/extra"),
                FolderViewState::default(),
                TabActivation::Background,
            )
            .expect("extra tab");
        assert_eq!(tabs.close_others(extra).expect("close others"), 1);
        assert_eq!(tabs.active_id(), extra);
        assert_eq!(tabs.len(), 1);
        assert!(tabs.recently_closed_len() >= 4);
        assert_ne!(two, extra);
    }

    #[test]
    fn phase_7c_workspace_codec_round_trips_raw_tabs_and_closed_state() {
        let raw = PathBuf::from(OsString::from_vec(b"/tmp/tab-\xff".to_vec()));
        let mut tabs =
            BrowserTabs::new(raw.clone(), FolderViewState::default()).expect("raw absolute path");
        let closed = tabs
            .open(
                PathBuf::from("/closed"),
                FolderViewState::default(),
                TabActivation::Foreground,
            )
            .expect("closed fixture");
        tabs.close(closed).expect("close fixture");
        let active = tabs
            .open(
                PathBuf::from("/active"),
                FolderViewState::default(),
                TabActivation::Foreground,
            )
            .expect("active fixture");
        let encoded = tabs.encode_workspace().expect("workspace encode");
        let restored = BrowserTabs::decode_workspace(&encoded).expect("workspace decode");
        assert_eq!(restored.active_id(), active);
        assert_eq!(restored.sessions()[0].current().path(), raw);
        assert_eq!(restored.recently_closed_len(), 1);
    }

    #[test]
    fn phase_7c_workspace_codec_rejects_hostile_envelopes() {
        let tabs = BrowserTabs::new(PathBuf::from("/"), FolderViewState::default())
            .expect("root workspace");
        let encoded = tabs.encode_workspace().expect("workspace encode");
        let mut bad_header = encoded.clone();
        bad_header[0] ^= 0xff;
        assert!(matches!(
            BrowserTabs::decode_workspace(&bad_header),
            Err(super::WorkspaceCodecError::InvalidHeader)
        ));
        let mut bad_version = encoded.clone();
        bad_version[8..10].copy_from_slice(&99_u16.to_le_bytes());
        assert!(matches!(
            BrowserTabs::decode_workspace(&bad_version),
            Err(super::WorkspaceCodecError::UnsupportedVersion(99))
        ));
        let mut unknown_active = encoded.clone();
        unknown_active[10..18].copy_from_slice(&999_u64.to_le_bytes());
        assert!(matches!(
            BrowserTabs::decode_workspace(&unknown_active),
            Err(super::WorkspaceCodecError::UnknownActiveTab(999))
        ));
        let mut empty = encoded.clone();
        empty[18..22].copy_from_slice(&0_u32.to_le_bytes());
        assert!(matches!(
            BrowserTabs::decode_workspace(&empty),
            Err(super::WorkspaceCodecError::EmptyWorkspace)
        ));
        assert!(matches!(
            BrowserTabs::decode_workspace(&encoded[..encoded.len() - 1]),
            Err(super::WorkspaceCodecError::Truncated)
        ));
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert!(matches!(
            BrowserTabs::decode_workspace(&trailing),
            Err(super::WorkspaceCodecError::TrailingBytes)
        ));
        let oversized = vec![0; super::WORKSPACE_MAX_SERIALIZED_BYTES + 1];
        assert!(matches!(
            BrowserTabs::decode_workspace(&oversized),
            Err(super::WorkspaceCodecError::LimitExceeded("workspace bytes"))
        ));

        let mut duplicate_tabs =
            BrowserTabs::new(PathBuf::from("/first"), FolderViewState::default())
                .expect("first tab");
        duplicate_tabs
            .open(
                PathBuf::from("/second"),
                FolderViewState::default(),
                TabActivation::Foreground,
            )
            .expect("second tab");
        let mut duplicate = duplicate_tabs
            .encode_workspace()
            .expect("two-tab workspace");
        let first_length = u32::from_le_bytes(
            duplicate[22..26]
                .try_into()
                .expect("first session length bytes"),
        ) as usize;
        let first_id = duplicate[36..44].to_vec();
        let second_id_offset = 26 + first_length + 4 + 10;
        duplicate[second_id_offset..second_id_offset + 8].copy_from_slice(&first_id);
        assert!(matches!(
            BrowserTabs::decode_workspace(&duplicate),
            Err(super::WorkspaceCodecError::DuplicateSessionId(_))
        ));
    }
}
