//! Exact, GTK-independent browser session state and versioned in-memory codec.
//!
//! Phase 7A deliberately performs no persistence. Phase 7C may persist this
//! bounded representation only after its privacy and lifecycle policy is set.

use std::{
    collections::HashSet,
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::{
    DirectoryGrouping, DirectoryPlacement, DirectorySort, FileViewDensity, FolderViewState,
    GridSize, ListColumnLayout, SortColumn, SortDirection, ViewMode,
};

pub const SESSION_HISTORY_CAPACITY: usize = 512;
pub const SESSION_SELECTION_CAPACITY: usize = 65_536;
pub const SESSION_MAX_PATH_BYTES: usize = 1_048_576;
pub const SESSION_MAX_SERIALIZED_BYTES: usize = 64 * 1_048_576;

const SESSION_MAGIC: &[u8; 8] = b"FLOESESS";
const SESSION_CODEC_VERSION: u16 = 1;
const MAX_POLICY_TEXT_BYTES: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BrowserSessionId(u64);

impl BrowserSessionId {
    pub fn new(value: u64) -> Result<Self, SessionStateError> {
        if value == 0 {
            Err(SessionStateError::InvalidSessionId)
        } else {
            Ok(Self(value))
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionScrollAnchor {
    path: Option<PathBuf>,
    index: usize,
}

impl SessionScrollAnchor {
    pub fn new(path: Option<PathBuf>, index: usize) -> Result<Self, SessionStateError> {
        if let Some(path) = path.as_deref() {
            validate_path(path)?;
        }
        Ok(Self { path, index })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub const fn index(&self) -> usize {
        self.index
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionLocation {
    path: PathBuf,
    selection: Vec<PathBuf>,
    scroll_anchor: Option<SessionScrollAnchor>,
    view: FolderViewState,
}

impl SessionLocation {
    pub fn new(path: PathBuf, view: FolderViewState) -> Result<Self, SessionStateError> {
        validate_path(&path)?;
        Ok(Self {
            path,
            selection: Vec::new(),
            scroll_anchor: None,
            view,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn selection(&self) -> &[PathBuf] {
        &self.selection
    }

    pub fn scroll_anchor(&self) -> Option<&SessionScrollAnchor> {
        self.scroll_anchor.as_ref()
    }

    pub const fn view(&self) -> FolderViewState {
        self.view
    }

    pub fn set_selection(&mut self, paths: Vec<PathBuf>) -> Result<(), SessionStateError> {
        if paths.len() > SESSION_SELECTION_CAPACITY {
            return Err(SessionStateError::TooManySelectedPaths {
                count: paths.len(),
                maximum: SESSION_SELECTION_CAPACITY,
            });
        }
        let mut seen = HashSet::with_capacity(paths.len());
        for path in &paths {
            validate_path(path)?;
            if !seen.insert(path) {
                return Err(SessionStateError::DuplicateSelectionPath);
            }
        }
        self.selection = paths;
        Ok(())
    }

    pub fn set_scroll_anchor(
        &mut self,
        anchor: Option<SessionScrollAnchor>,
    ) -> Result<(), SessionStateError> {
        if let Some(path) = anchor.as_ref().and_then(SessionScrollAnchor::path) {
            validate_path(path)?;
        }
        self.scroll_anchor = anchor;
        Ok(())
    }

    pub fn set_view(&mut self, view: FolderViewState) {
        self.view = view;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserSession {
    id: BrowserSessionId,
    current: SessionLocation,
    back: Vec<SessionLocation>,
    forward: Vec<SessionLocation>,
}

impl BrowserSession {
    pub fn new(
        id: BrowserSessionId,
        initial_path: PathBuf,
        view: FolderViewState,
    ) -> Result<Self, SessionStateError> {
        Ok(Self {
            id,
            current: SessionLocation::new(initial_path, view)?,
            back: Vec::new(),
            forward: Vec::new(),
        })
    }

    pub const fn id(&self) -> BrowserSessionId {
        self.id
    }

    pub fn current(&self) -> &SessionLocation {
        &self.current
    }

    pub fn back_history(&self) -> &[SessionLocation] {
        &self.back
    }

    pub fn forward_history(&self) -> &[SessionLocation] {
        &self.forward
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    pub fn can_go_parent(&self) -> bool {
        self.current.path.parent().is_some()
    }

    pub fn set_selection(&mut self, paths: Vec<PathBuf>) -> Result<(), SessionStateError> {
        self.current.set_selection(paths)
    }

    pub fn set_scroll_anchor(
        &mut self,
        anchor: Option<SessionScrollAnchor>,
    ) -> Result<(), SessionStateError> {
        self.current.set_scroll_anchor(anchor)
    }

    pub fn set_view(&mut self, view: FolderViewState) {
        self.current.set_view(view);
    }

    pub fn navigate_to(
        &mut self,
        destination: PathBuf,
        view: FolderViewState,
    ) -> Result<bool, SessionStateError> {
        if destination == self.current.path {
            return Ok(false);
        }
        let destination = SessionLocation::new(destination, view)?;
        let previous = std::mem::replace(&mut self.current, destination);
        push_bounded(&mut self.back, previous);
        self.forward.clear();
        Ok(true)
    }

    pub fn go_back(&mut self) -> bool {
        let Some(destination) = self.back.pop() else {
            return false;
        };
        let previous = std::mem::replace(&mut self.current, destination);
        push_bounded(&mut self.forward, previous);
        true
    }

    pub fn go_forward(&mut self) -> bool {
        let Some(destination) = self.forward.pop() else {
            return false;
        };
        let previous = std::mem::replace(&mut self.current, destination);
        push_bounded(&mut self.back, previous);
        true
    }

    pub fn go_parent(&mut self, view: FolderViewState) -> Result<bool, SessionStateError> {
        let Some(parent) = self.current.path.parent().map(Path::to_path_buf) else {
            return Ok(false);
        };
        self.navigate_to(parent, view)
    }

    pub fn duplicate(&self, id: BrowserSessionId) -> Self {
        let mut duplicate = self.clone();
        duplicate.id = id;
        duplicate
    }

    pub fn encode(&self) -> Result<Vec<u8>, SessionCodecError> {
        let mut encoder = Encoder::default();
        encoder.write_bytes(SESSION_MAGIC)?;
        encoder.write_u16(SESSION_CODEC_VERSION)?;
        encoder.write_u64(self.id.get())?;
        encoder.write_count(self.back.len(), "back history")?;
        for location in &self.back {
            encoder.write_location(location)?;
        }
        encoder.write_location(&self.current)?;
        encoder.write_count(self.forward.len(), "forward history")?;
        for location in &self.forward {
            encoder.write_location(location)?;
        }
        Ok(encoder.finish())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, SessionCodecError> {
        if bytes.len() > SESSION_MAX_SERIALIZED_BYTES {
            return Err(SessionCodecError::LimitExceeded("serialized session"));
        }
        let mut decoder = Decoder::new(bytes);
        if decoder.read_exact(SESSION_MAGIC.len())? != SESSION_MAGIC {
            return Err(SessionCodecError::InvalidHeader);
        }
        let version = decoder.read_u16()?;
        if version != SESSION_CODEC_VERSION {
            return Err(SessionCodecError::UnsupportedVersion(version));
        }
        let id = BrowserSessionId::new(decoder.read_u64()?)?;
        let back_count = decoder.read_count("back history", SESSION_HISTORY_CAPACITY)?;
        let mut back = Vec::with_capacity(back_count);
        for _ in 0..back_count {
            back.push(decoder.read_location()?);
        }
        let current = decoder.read_location()?;
        let forward_count = decoder.read_count("forward history", SESSION_HISTORY_CAPACITY)?;
        let mut forward = Vec::with_capacity(forward_count);
        for _ in 0..forward_count {
            forward.push(decoder.read_location()?);
        }
        if !decoder.is_finished() {
            return Err(SessionCodecError::TrailingBytes);
        }
        Ok(Self {
            id,
            current,
            back,
            forward,
        })
    }
}

fn push_bounded(history: &mut Vec<SessionLocation>, location: SessionLocation) {
    if history.len() == SESSION_HISTORY_CAPACITY {
        history.remove(0);
    }
    history.push(location);
}

fn validate_path(path: &Path) -> Result<(), SessionStateError> {
    if !path.is_absolute() {
        return Err(SessionStateError::RelativePath);
    }
    let length = path.as_os_str().as_bytes().len();
    if length > SESSION_MAX_PATH_BYTES {
        return Err(SessionStateError::PathTooLong {
            length,
            maximum: SESSION_MAX_PATH_BYTES,
        });
    }
    Ok(())
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SessionStateError {
    #[error("session IDs must be nonzero")]
    InvalidSessionId,
    #[error("session paths must be absolute local paths")]
    RelativePath,
    #[error("session path has {length} bytes; maximum is {maximum}")]
    PathTooLong { length: usize, maximum: usize },
    #[error("selection has {count} paths; maximum is {maximum}")]
    TooManySelectedPaths { count: usize, maximum: usize },
    #[error("selection contains a duplicate exact path")]
    DuplicateSelectionPath,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SessionCodecError {
    #[error(transparent)]
    InvalidState(#[from] SessionStateError),
    #[error("invalid Floe session header")]
    InvalidHeader,
    #[error("unsupported Floe session version {0}")]
    UnsupportedVersion(u16),
    #[error("truncated Floe session")]
    Truncated,
    #[error("{0} exceeds the session codec limit")]
    LimitExceeded(&'static str),
    #[error("invalid session field: {0}")]
    InvalidField(&'static str),
    #[error("session contains trailing bytes")]
    TrailingBytes,
}

#[derive(Default)]
struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), SessionCodecError> {
        let revised = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or(SessionCodecError::LimitExceeded("serialized session"))?;
        if revised > SESSION_MAX_SERIALIZED_BYTES {
            return Err(SessionCodecError::LimitExceeded("serialized session"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn write_u8(&mut self, value: u8) -> Result<(), SessionCodecError> {
        self.write_bytes(&[value])
    }

    fn write_u16(&mut self, value: u16) -> Result<(), SessionCodecError> {
        self.write_bytes(&value.to_le_bytes())
    }

    fn write_u32(&mut self, value: u32) -> Result<(), SessionCodecError> {
        self.write_bytes(&value.to_le_bytes())
    }

    fn write_u64(&mut self, value: u64) -> Result<(), SessionCodecError> {
        self.write_bytes(&value.to_le_bytes())
    }

    fn write_count(&mut self, count: usize, field: &'static str) -> Result<(), SessionCodecError> {
        let count = u32::try_from(count).map_err(|_| SessionCodecError::LimitExceeded(field))?;
        self.write_u32(count)
    }

    fn write_path(&mut self, path: &Path) -> Result<(), SessionCodecError> {
        validate_path(path)?;
        let bytes = path.as_os_str().as_bytes();
        let length =
            u32::try_from(bytes.len()).map_err(|_| SessionCodecError::LimitExceeded("path"))?;
        self.write_u32(length)?;
        self.write_bytes(bytes)
    }

    fn write_text(&mut self, text: &str) -> Result<(), SessionCodecError> {
        if text.len() > MAX_POLICY_TEXT_BYTES {
            return Err(SessionCodecError::LimitExceeded("view policy text"));
        }
        let length = u16::try_from(text.len())
            .map_err(|_| SessionCodecError::LimitExceeded("view policy text"))?;
        self.write_u16(length)?;
        self.write_bytes(text.as_bytes())
    }

    fn write_view(&mut self, view: FolderViewState) -> Result<(), SessionCodecError> {
        self.write_text(view.mode.persisted())?;
        self.write_u16(view.grid_size.edge())?;
        self.write_text(view.density.persisted())?;
        self.write_text(view.sort.column.persisted())?;
        self.write_text(view.sort.direction.persisted())?;
        self.write_text(view.sort.directories.persisted())?;
        self.write_text(view.sort.grouping.persisted())?;
        self.write_text(&view.columns.visible_names())?;
        self.write_text(&view.columns.widths_text())
    }

    fn write_location(&mut self, location: &SessionLocation) -> Result<(), SessionCodecError> {
        self.write_path(location.path())?;
        self.write_view(location.view())?;
        self.write_count(location.selection.len(), "selection")?;
        for path in &location.selection {
            self.write_path(path)?;
        }
        match location.scroll_anchor.as_ref() {
            Some(anchor) => {
                self.write_u8(1)?;
                match anchor.path() {
                    Some(path) => {
                        self.write_u8(1)?;
                        self.write_path(path)?;
                    }
                    None => self.write_u8(0)?,
                }
                self.write_u64(
                    u64::try_from(anchor.index)
                        .map_err(|_| SessionCodecError::LimitExceeded("scroll index"))?,
                )
            }
            None => self.write_u8(0),
        }
    }
}

struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn is_finished(&self) -> bool {
        self.offset == self.bytes.len()
    }

    fn read_exact(&mut self, length: usize) -> Result<&'a [u8], SessionCodecError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(SessionCodecError::Truncated)?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(SessionCodecError::Truncated)?;
        self.offset = end;
        Ok(bytes)
    }

    fn read_u8(&mut self) -> Result<u8, SessionCodecError> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_u16(&mut self) -> Result<u16, SessionCodecError> {
        let bytes: [u8; 2] = self
            .read_exact(2)?
            .try_into()
            .map_err(|_| SessionCodecError::Truncated)?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn read_u32(&mut self) -> Result<u32, SessionCodecError> {
        let bytes: [u8; 4] = self
            .read_exact(4)?
            .try_into()
            .map_err(|_| SessionCodecError::Truncated)?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn read_u64(&mut self) -> Result<u64, SessionCodecError> {
        let bytes: [u8; 8] = self
            .read_exact(8)?
            .try_into()
            .map_err(|_| SessionCodecError::Truncated)?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn read_count(
        &mut self,
        field: &'static str,
        maximum: usize,
    ) -> Result<usize, SessionCodecError> {
        let count = usize::try_from(self.read_u32()?)
            .map_err(|_| SessionCodecError::LimitExceeded(field))?;
        if count > maximum {
            Err(SessionCodecError::LimitExceeded(field))
        } else {
            Ok(count)
        }
    }

    fn read_path(&mut self) -> Result<PathBuf, SessionCodecError> {
        let length = usize::try_from(self.read_u32()?)
            .map_err(|_| SessionCodecError::LimitExceeded("path"))?;
        if length > SESSION_MAX_PATH_BYTES {
            return Err(SessionCodecError::LimitExceeded("path"));
        }
        let path = PathBuf::from(OsString::from_vec(self.read_exact(length)?.to_vec()));
        validate_path(&path)?;
        Ok(path)
    }

    fn read_text(&mut self) -> Result<&'a str, SessionCodecError> {
        let length = usize::from(self.read_u16()?);
        if length > MAX_POLICY_TEXT_BYTES {
            return Err(SessionCodecError::LimitExceeded("view policy text"));
        }
        std::str::from_utf8(self.read_exact(length)?)
            .map_err(|_| SessionCodecError::InvalidField("view policy text"))
    }

    fn read_view(&mut self) -> Result<FolderViewState, SessionCodecError> {
        let mode = ViewMode::from_persisted(self.read_text()?)
            .ok_or(SessionCodecError::InvalidField("view mode"))?;
        let grid_size = GridSize::from_persisted(self.read_u16()?)
            .ok_or(SessionCodecError::InvalidField("grid size"))?;
        let density = FileViewDensity::from_persisted(self.read_text()?)
            .ok_or(SessionCodecError::InvalidField("file density"))?;
        let column = SortColumn::from_persisted(self.read_text()?)
            .ok_or(SessionCodecError::InvalidField("sort column"))?;
        let direction = SortDirection::from_persisted(self.read_text()?)
            .ok_or(SessionCodecError::InvalidField("sort direction"))?;
        let directories = DirectoryPlacement::from_persisted(self.read_text()?)
            .ok_or(SessionCodecError::InvalidField("directory placement"))?;
        let grouping = DirectoryGrouping::from_persisted(self.read_text()?)
            .ok_or(SessionCodecError::InvalidField("grouping"))?;
        let visible = self.read_text()?;
        let mut columns = ListColumnLayout::parse_visible(visible);
        if columns.visible_names() != visible {
            return Err(SessionCodecError::InvalidField("visible columns"));
        }
        let widths = self.read_text()?;
        columns.apply_widths_text(widths);
        if columns.widths_text() != widths {
            return Err(SessionCodecError::InvalidField("column widths"));
        }
        Ok(FolderViewState {
            mode,
            grid_size,
            density,
            sort: DirectorySort::new(column, direction)
                .with_directories(directories)
                .with_grouping(grouping),
            columns,
        })
    }

    fn read_location(&mut self) -> Result<SessionLocation, SessionCodecError> {
        let path = self.read_path()?;
        let view = self.read_view()?;
        let selection_count = self.read_count("selection", SESSION_SELECTION_CAPACITY)?;
        let mut selection = Vec::with_capacity(selection_count);
        for _ in 0..selection_count {
            selection.push(self.read_path()?);
        }
        let scroll_anchor = match self.read_u8()? {
            0 => None,
            1 => {
                let path = match self.read_u8()? {
                    0 => None,
                    1 => Some(self.read_path()?),
                    _ => return Err(SessionCodecError::InvalidField("scroll anchor path")),
                };
                let index = usize::try_from(self.read_u64()?)
                    .map_err(|_| SessionCodecError::LimitExceeded("scroll index"))?;
                Some(SessionScrollAnchor::new(path, index)?)
            }
            _ => return Err(SessionCodecError::InvalidField("scroll anchor")),
        };
        let mut location = SessionLocation::new(path, view)?;
        location.set_selection(selection)?;
        location.set_scroll_anchor(scroll_anchor)?;
        Ok(location)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    fn detailed_view() -> FolderViewState {
        let mut columns = ListColumnLayout::default();
        columns.set_visible(crate::ListColumn::Mime, true);
        columns.set_width(crate::ListColumn::Name, 344);
        FolderViewState {
            mode: ViewMode::Grid,
            grid_size: GridSize::from_persisted(160).expect("grid size"),
            density: FileViewDensity::Spacious,
            sort: DirectorySort::new(SortColumn::Extension, SortDirection::Descending)
                .with_directories(DirectoryPlacement::Last)
                .with_grouping(DirectoryGrouping::Extension),
            columns,
        }
    }

    #[test]
    fn phase_7a_session_navigation_restores_complete_location_state() {
        let first = PathBuf::from("/first");
        let second = PathBuf::from("/second");
        let selected = PathBuf::from("/first/selected.txt");
        let anchor = SessionScrollAnchor::new(Some(selected.clone()), 37).expect("anchor");
        let mut session = BrowserSession::new(
            BrowserSessionId::new(7).expect("ID"),
            first.clone(),
            detailed_view(),
        )
        .expect("session");
        session
            .set_selection(vec![selected.clone()])
            .expect("selection");
        session
            .set_scroll_anchor(Some(anchor.clone()))
            .expect("anchor");
        let first_state = session.current().clone();

        assert!(
            session
                .navigate_to(second.clone(), FolderViewState::default())
                .expect("navigate")
        );
        assert_eq!(session.current().path(), second);
        assert!(session.go_back());
        assert_eq!(session.current(), &first_state);
        assert!(session.go_forward());
        assert_eq!(session.current().path(), second);
        assert!(
            session
                .navigate_to(PathBuf::from("/third"), detailed_view())
                .expect("new navigation")
        );
        assert!(!session.can_go_forward());
        assert_eq!(session.current().scroll_anchor(), None);
    }

    #[test]
    fn phase_7a_session_enforces_identity_and_history_bounds() {
        assert_eq!(
            BrowserSessionId::new(0),
            Err(SessionStateError::InvalidSessionId)
        );
        assert!(matches!(
            BrowserSession::new(
                BrowserSessionId::new(1).expect("ID"),
                PathBuf::from("relative"),
                FolderViewState::default()
            ),
            Err(SessionStateError::RelativePath)
        ));
        let mut session = BrowserSession::new(
            BrowserSessionId::new(1).expect("ID"),
            PathBuf::from("/start"),
            FolderViewState::default(),
        )
        .expect("session");
        assert_eq!(
            session.set_selection(vec![PathBuf::from("/same"), PathBuf::from("/same")]),
            Err(SessionStateError::DuplicateSelectionPath)
        );
        assert_eq!(
            session.set_selection(vec![
                PathBuf::from("/selected");
                SESSION_SELECTION_CAPACITY + 1
            ]),
            Err(SessionStateError::TooManySelectedPaths {
                count: SESSION_SELECTION_CAPACITY + 1,
                maximum: SESSION_SELECTION_CAPACITY,
            })
        );
        for index in 0..=SESSION_HISTORY_CAPACITY {
            session
                .navigate_to(
                    PathBuf::from(format!("/history/{index}")),
                    FolderViewState::default(),
                )
                .expect("bounded navigation");
        }
        assert_eq!(session.back_history().len(), SESSION_HISTORY_CAPACITY);
        assert_eq!(session.back_history()[0].path(), Path::new("/history/0"));
    }

    #[test]
    fn phase_7a_session_parent_and_duplicate_keep_independent_exact_state() {
        let mut session = BrowserSession::new(
            BrowserSessionId::new(11).expect("ID"),
            PathBuf::from("/one/two"),
            detailed_view(),
        )
        .expect("session");
        assert!(
            session
                .go_parent(FolderViewState::default())
                .expect("parent")
        );
        assert_eq!(session.current().path(), Path::new("/one"));
        assert!(
            session
                .go_parent(FolderViewState::default())
                .expect("parent")
        );
        assert_eq!(session.current().path(), Path::new("/"));
        assert!(!session.go_parent(FolderViewState::default()).expect("root"));
        let duplicate = session.duplicate(BrowserSessionId::new(12).expect("ID"));
        assert_eq!(duplicate.id().get(), 12);
        assert_eq!(duplicate.current(), session.current());
        assert_eq!(duplicate.back_history(), session.back_history());
    }

    #[test]
    fn phase_7a_codec_round_trips_non_utf8_complete_state() {
        let raw = PathBuf::from("/tmp").join(OsString::from_vec(b"raw-\xff".to_vec()));
        let selected = raw.join(OsString::from_vec(b"selected-\xfe".to_vec()));
        let mut session = BrowserSession::new(
            BrowserSessionId::new(99).expect("ID"),
            raw.clone(),
            detailed_view(),
        )
        .expect("session");
        session
            .set_selection(vec![selected.clone()])
            .expect("selection");
        session
            .set_scroll_anchor(Some(
                SessionScrollAnchor::new(Some(selected), 12_345).expect("anchor"),
            ))
            .expect("anchor");
        session
            .navigate_to(PathBuf::from("/next"), FolderViewState::default())
            .expect("navigate");
        session.go_back();
        let encoded = session.encode().expect("encode");
        let decoded = BrowserSession::decode(&encoded).expect("decode");
        assert_eq!(decoded, session);
        assert_eq!(decoded.current().path(), raw);
    }

    #[test]
    fn phase_7a_codec_rejects_header_version_truncation_and_trailing_data() {
        let session = BrowserSession::new(
            BrowserSessionId::new(1).expect("ID"),
            PathBuf::from("/safe"),
            detailed_view(),
        )
        .expect("session");
        let encoded = session.encode().expect("encode");
        let mut bad_header = encoded.clone();
        bad_header[0] = b'X';
        assert_eq!(
            BrowserSession::decode(&bad_header),
            Err(SessionCodecError::InvalidHeader)
        );
        let mut bad_version = encoded.clone();
        bad_version[8..10].copy_from_slice(&2_u16.to_le_bytes());
        assert_eq!(
            BrowserSession::decode(&bad_version),
            Err(SessionCodecError::UnsupportedVersion(2))
        );
        let mut zero_id = encoded.clone();
        zero_id[10..18].copy_from_slice(&0_u64.to_le_bytes());
        assert_eq!(
            BrowserSession::decode(&zero_id),
            Err(SessionCodecError::InvalidState(
                SessionStateError::InvalidSessionId
            ))
        );
        let mut oversized_history = encoded.clone();
        oversized_history[18..22].copy_from_slice(
            &u32::try_from(SESSION_HISTORY_CAPACITY + 1)
                .expect("history limit fits the codec")
                .to_le_bytes(),
        );
        assert_eq!(
            BrowserSession::decode(&oversized_history),
            Err(SessionCodecError::LimitExceeded("back history"))
        );
        for length in 0..encoded.len() {
            assert!(BrowserSession::decode(&encoded[..length]).is_err());
        }
        let mut trailing = encoded;
        trailing.push(0);
        assert_eq!(
            BrowserSession::decode(&trailing),
            Err(SessionCodecError::TrailingBytes)
        );
    }

    #[test]
    fn phase_7a_codec_rejects_relative_oversized_and_invalid_policy_fields() {
        let session = BrowserSession::new(
            BrowserSessionId::new(1).expect("ID"),
            PathBuf::from("/safe"),
            detailed_view(),
        )
        .expect("session");
        let encoded = session.encode().expect("encode");

        let mut relative = encoded.clone();
        let path_offset = relative
            .windows(b"/safe".len())
            .position(|window| window == b"/safe")
            .expect("path bytes");
        relative[path_offset] = b'x';
        assert!(matches!(
            BrowserSession::decode(&relative),
            Err(SessionCodecError::InvalidState(
                SessionStateError::RelativePath
            ))
        ));

        let mut invalid_mode = encoded;
        let mode_offset = invalid_mode
            .windows(b"grid".len())
            .position(|window| window == b"grid")
            .expect("mode bytes");
        invalid_mode[mode_offset] = b'x';
        assert_eq!(
            BrowserSession::decode(&invalid_mode),
            Err(SessionCodecError::InvalidField("view mode"))
        );

        let mut oversized_path = Vec::new();
        oversized_path.extend_from_slice(SESSION_MAGIC);
        oversized_path.extend_from_slice(&SESSION_CODEC_VERSION.to_le_bytes());
        oversized_path.extend_from_slice(&1_u64.to_le_bytes());
        oversized_path.extend_from_slice(&0_u32.to_le_bytes());
        oversized_path.extend_from_slice(
            &u32::try_from(SESSION_MAX_PATH_BYTES + 1)
                .expect("path bound")
                .to_le_bytes(),
        );
        assert_eq!(
            BrowserSession::decode(&oversized_path),
            Err(SessionCodecError::LimitExceeded("path"))
        );
    }
}
