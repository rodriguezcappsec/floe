//! Native Floe file-selection invocation, validation, and result policy.

use std::{
    ffi::OsString,
    fs, io,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

use floe_core::{DirectoryEntry, EntryKind, FolderFilterMode, FolderFilterPattern};
use gtk::gio::prelude::FileExt;
use thiserror::Error;

pub const SELECTION_PATH_CAPACITY: usize = 128;
const VALIDATION_QUEUE_CAPACITY: usize = 4;
const VISUAL_FILTER_QUEUE_CAPACITY: usize = 1;

pub fn selection_application_id(process_id: u32) -> String {
    format!("io.github.rodriguezcappsec.Floe.Selection.p{process_id}")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionMode {
    OpenFile,
    OpenFiles,
    SelectFolder,
    SaveFile,
}

impl SelectionMode {
    pub const fn title(self) -> &'static str {
        match self {
            Self::OpenFile => "Open File",
            Self::OpenFiles => "Open Files",
            Self::SelectFolder => "Select Folder",
            Self::SaveFile => "Save File",
        }
    }

    pub const fn accept_label(self) -> &'static str {
        match self {
            Self::OpenFile | Self::OpenFiles => "Open",
            Self::SelectFolder => "Select",
            Self::SaveFile => "Save",
        }
    }

    pub const fn needs_filename(self) -> bool {
        matches!(self, Self::SaveFile)
    }

    #[cfg(test)]
    pub const fn presentation(self) -> SelectionPresentation {
        SelectionPresentation {
            title: self.title(),
            accept_label: self.accept_label(),
            filename_visible: self.needs_filename(),
            multiple: matches!(self, Self::OpenFiles),
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectionPresentation {
    pub title: &'static str,
    pub accept_label: &'static str,
    pub filename_visible: bool,
    pub multiple: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionConfig {
    pub mode: SelectionMode,
    pub initial_directory: Option<PathBuf>,
    pub suggested_name: Option<String>,
    pub title: Option<String>,
    pub accept_label: Option<String>,
    pub parent_window: Option<String>,
    pub modal: bool,
    pub chooser_options: SelectionChooserOptions,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SelectionChooserOptions {
    pub filters: Vec<SelectionFilter>,
    pub current_filter: Option<usize>,
    pub choices: Vec<SelectionChoice>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionFilter {
    pub label: String,
    pub rules: Vec<SelectionFilterRule>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionFilterRule {
    Glob(String),
    Mime(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionChoice {
    pub id: String,
    pub label: String,
    pub options: Vec<(String, String)>,
    pub initial: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SelectionOptionResult {
    pub current_filter: Option<usize>,
    pub choices: Vec<(String, String)>,
}

impl SelectionConfig {
    pub fn initial_directory_or_else(&self, fallback: impl FnOnce() -> PathBuf) -> PathBuf {
        self.initial_directory.clone().unwrap_or_else(fallback)
    }

    pub fn presentation(&self) -> SelectionPresentationOwned {
        SelectionPresentationOwned {
            title: self
                .title
                .clone()
                .unwrap_or_else(|| self.mode.title().to_owned()),
            accept_label: self
                .accept_label
                .clone()
                .unwrap_or_else(|| self.mode.accept_label().to_owned()),
            filename_visible: self.mode.needs_filename(),
            multiple: self.mode == SelectionMode::OpenFiles,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionPresentationOwned {
    pub title: String,
    pub accept_label: String,
    pub filename_visible: bool,
    pub multiple: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum SelectionArgumentError {
    #[error("choose exactly one of --choose-open, --choose-folder, or --choose-save")]
    ConflictingModes,
    #[error("{0} requires a value")]
    MissingValue(&'static str),
    #[error("--multiple is valid only with --choose-open")]
    InvalidMultiple,
    #[error("--suggested-name is valid only with --choose-save")]
    InvalidSuggestedName,
    #[error("the suggested filename must be valid UTF-8")]
    NonUtf8SuggestedName,
    #[error("{0} must be nonempty, contain no control characters, and be at most 200 characters")]
    InvalidPresentation(&'static str),
    #[error("unknown Selection Mode option: {0}")]
    UnknownOption(String),
    #[error("invalid bounded chooser option payload")]
    InvalidChooserOptions,
}

pub fn parse_selection_invocation(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<Option<SelectionConfig>, SelectionArgumentError> {
    let mut arguments = arguments.into_iter();
    let _program = arguments.next();
    let arguments = arguments.collect::<Vec<_>>();
    let chooser_requested = arguments.iter().any(|argument| {
        matches!(
            argument.to_str(),
            Some("--choose-open" | "--choose-folder" | "--choose-save")
        )
    });
    if !chooser_requested {
        return Ok(None);
    }

    let mut mode = None;
    let mut multiple = false;
    let mut initial_directory = None;
    let mut suggested_name = None;
    let mut title = None;
    let mut accept_label = None;
    let mut parent_window = None;
    let mut modal = false;
    let mut chooser_options = SelectionChooserOptions::default();
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        match argument.to_str() {
            Some("--choose-open") => set_mode(&mut mode, SelectionMode::OpenFile)?,
            Some("--choose-folder") => set_mode(&mut mode, SelectionMode::SelectFolder)?,
            Some("--choose-save") => set_mode(&mut mode, SelectionMode::SaveFile)?,
            Some("--multiple") => multiple = true,
            Some("--initial-directory") => {
                index += 1;
                initial_directory =
                    Some(PathBuf::from(arguments.get(index).ok_or(
                        SelectionArgumentError::MissingValue("--initial-directory"),
                    )?));
            }
            Some("--suggested-name") => {
                index += 1;
                let value = arguments
                    .get(index)
                    .ok_or(SelectionArgumentError::MissingValue("--suggested-name"))?;
                suggested_name = Some(
                    value
                        .to_str()
                        .ok_or(SelectionArgumentError::NonUtf8SuggestedName)?
                        .to_owned(),
                );
            }
            Some("--chooser-title") => {
                index += 1;
                title = Some(parse_presentation_argument(
                    arguments
                        .get(index)
                        .ok_or(SelectionArgumentError::MissingValue("--chooser-title"))?,
                    "--chooser-title",
                )?);
            }
            Some("--chooser-accept-label") => {
                index += 1;
                accept_label = Some(parse_presentation_argument(
                    arguments
                        .get(index)
                        .ok_or(SelectionArgumentError::MissingValue(
                            "--chooser-accept-label",
                        ))?,
                    "--chooser-accept-label",
                )?);
            }
            Some("--chooser-parent") => {
                index += 1;
                parent_window =
                    Some(parse_parent_argument(arguments.get(index).ok_or(
                        SelectionArgumentError::MissingValue("--chooser-parent"),
                    )?)?);
            }
            Some("--chooser-modal") => modal = true,
            Some("--chooser-options-v1") => {
                index += 1;
                let value = arguments
                    .get(index)
                    .and_then(|value| value.to_str())
                    .ok_or(SelectionArgumentError::MissingValue("--chooser-options-v1"))?;
                chooser_options = decode_chooser_options(value)
                    .ok_or(SelectionArgumentError::InvalidChooserOptions)?;
            }
            Some(value) => return Err(SelectionArgumentError::UnknownOption(value.to_owned())),
            None => {
                return Err(SelectionArgumentError::UnknownOption(
                    argument.to_string_lossy().into_owned(),
                ));
            }
        }
        index += 1;
    }

    let mut mode = mode.ok_or(SelectionArgumentError::ConflictingModes)?;
    if multiple {
        if mode != SelectionMode::OpenFile {
            return Err(SelectionArgumentError::InvalidMultiple);
        }
        mode = SelectionMode::OpenFiles;
    }
    if suggested_name.is_some() && mode != SelectionMode::SaveFile {
        return Err(SelectionArgumentError::InvalidSuggestedName);
    }
    Ok(Some(SelectionConfig {
        mode,
        initial_directory,
        suggested_name,
        title,
        accept_label,
        parent_window,
        modal,
        chooser_options,
    }))
}

fn parse_presentation_argument(
    value: &OsString,
    option: &'static str,
) -> Result<String, SelectionArgumentError> {
    let value = value
        .to_str()
        .ok_or(SelectionArgumentError::InvalidPresentation(option))?;
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 200 || trimmed.chars().any(char::is_control)
    {
        return Err(SelectionArgumentError::InvalidPresentation(option));
    }
    Ok(trimmed.replace('_', ""))
}

fn parse_parent_argument(value: &OsString) -> Result<String, SelectionArgumentError> {
    let value = value
        .to_str()
        .ok_or(SelectionArgumentError::InvalidPresentation(
            "--chooser-parent",
        ))?;
    let Some(handle) = value.strip_prefix("wayland:") else {
        return Err(SelectionArgumentError::InvalidPresentation(
            "--chooser-parent",
        ));
    };
    if handle.is_empty()
        || handle.len() > 512
        || !handle.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(SelectionArgumentError::InvalidPresentation(
            "--chooser-parent",
        ));
    }
    Ok(handle.to_owned())
}

pub fn encode_chooser_options(options: &SelectionChooserOptions) -> Option<String> {
    if options.filters.len() > 32 || options.choices.len() > 32 {
        return None;
    }
    let mut bytes = b"FLOEOPT1".to_vec();
    write_u16(&mut bytes, options.filters.len())?;
    for filter in &options.filters {
        write_text(&mut bytes, &filter.label, 200)?;
        if filter.rules.is_empty() || filter.rules.len() > 32 {
            return None;
        }
        write_u16(&mut bytes, filter.rules.len())?;
        for rule in &filter.rules {
            match rule {
                SelectionFilterRule::Glob(value) => {
                    bytes.push(0);
                    write_text(&mut bytes, value, 256)?;
                }
                SelectionFilterRule::Mime(value) => {
                    bytes.push(1);
                    write_text(&mut bytes, value, 256)?;
                }
            }
        }
    }
    let current = options
        .current_filter
        .and_then(|value| u16::try_from(value).ok())
        .unwrap_or(u16::MAX);
    if current != u16::MAX && usize::from(current) >= options.filters.len() {
        return None;
    }
    bytes.extend_from_slice(&current.to_le_bytes());
    write_u16(&mut bytes, options.choices.len())?;
    for choice in &options.choices {
        write_text(&mut bytes, &choice.id, 200)?;
        write_text(&mut bytes, &choice.label, 200)?;
        if choice.options.len() > 64 {
            return None;
        }
        write_u16(&mut bytes, choice.options.len())?;
        for (id, label) in &choice.options {
            write_text(&mut bytes, id, 200)?;
            write_text(&mut bytes, label, 200)?;
        }
        write_text_allow_empty(&mut bytes, &choice.initial, 200)?;
    }
    Some(hex_encode(&bytes))
}

pub fn decode_chooser_options(value: &str) -> Option<SelectionChooserOptions> {
    let bytes = hex_decode(value)?;
    let mut input = bytes.as_slice();
    if take(&mut input, 8)? != b"FLOEOPT1" {
        return None;
    }
    let filter_count = usize::from(read_u16(&mut input)?);
    if filter_count > 32 {
        return None;
    }
    let mut filters = Vec::with_capacity(filter_count);
    for _ in 0..filter_count {
        let label = read_text(&mut input, 200, false)?;
        let rule_count = usize::from(read_u16(&mut input)?);
        if rule_count == 0 || rule_count > 32 {
            return None;
        }
        let mut rules = Vec::with_capacity(rule_count);
        for _ in 0..rule_count {
            let kind = *take(&mut input, 1)?.first()?;
            let value = read_text(&mut input, 256, false)?;
            rules.push(match kind {
                0 => SelectionFilterRule::Glob(value),
                1 => SelectionFilterRule::Mime(value),
                _ => return None,
            });
        }
        filters.push(SelectionFilter { label, rules });
    }
    let current = read_u16(&mut input)?;
    let current_filter = (current != u16::MAX).then_some(usize::from(current));
    if current_filter.is_some_and(|index| index >= filters.len()) {
        return None;
    }
    let choice_count = usize::from(read_u16(&mut input)?);
    if choice_count > 32 {
        return None;
    }
    let mut choices = Vec::with_capacity(choice_count);
    let mut choice_ids = std::collections::HashSet::new();
    for _ in 0..choice_count {
        let id = read_text(&mut input, 200, false)?;
        if !choice_ids.insert(id.clone()) {
            return None;
        }
        let label = read_text(&mut input, 200, false)?;
        let option_count = usize::from(read_u16(&mut input)?);
        if option_count > 64 {
            return None;
        }
        let mut options = Vec::with_capacity(option_count);
        let mut option_ids = std::collections::HashSet::new();
        for _ in 0..option_count {
            let id = read_text(&mut input, 200, false)?;
            if !option_ids.insert(id.clone()) {
                return None;
            }
            options.push((id, read_text(&mut input, 200, false)?));
        }
        let initial = read_text(&mut input, 200, true)?;
        let valid_initial = if options.is_empty() {
            matches!(initial.as_str(), "" | "true" | "false")
        } else {
            initial.is_empty() || options.iter().any(|(id, _)| *id == initial)
        };
        if !valid_initial {
            return None;
        }
        choices.push(SelectionChoice {
            id,
            label,
            options,
            initial,
        });
    }
    input.is_empty().then_some(SelectionChooserOptions {
        filters,
        current_filter,
        choices,
    })
}

pub fn encode_option_result(result: &SelectionOptionResult) -> Option<String> {
    if result.choices.len() > 32 {
        return None;
    }
    let mut bytes = b"FLOERES1".to_vec();
    let current = result
        .current_filter
        .and_then(|value| u16::try_from(value).ok())
        .unwrap_or(u16::MAX);
    bytes.extend_from_slice(&current.to_le_bytes());
    write_u16(&mut bytes, result.choices.len())?;
    for (id, value) in &result.choices {
        write_text(&mut bytes, id, 200)?;
        write_text_allow_empty(&mut bytes, value, 200)?;
    }
    Some(hex_encode(&bytes))
}

pub fn decode_option_result(value: &str) -> Option<SelectionOptionResult> {
    let bytes = hex_decode(value)?;
    let mut input = bytes.as_slice();
    if take(&mut input, 8)? != b"FLOERES1" {
        return None;
    }
    let current = read_u16(&mut input)?;
    let count = usize::from(read_u16(&mut input)?);
    if count > 32 {
        return None;
    }
    let mut choices = Vec::with_capacity(count);
    let mut seen = std::collections::HashSet::new();
    for _ in 0..count {
        let id = read_text(&mut input, 200, false)?;
        if !seen.insert(id.clone()) {
            return None;
        }
        choices.push((id, read_text(&mut input, 200, true)?));
    }
    input.is_empty().then_some(SelectionOptionResult {
        current_filter: (current != u16::MAX).then_some(usize::from(current)),
        choices,
    })
}

fn write_u16(output: &mut Vec<u8>, value: usize) -> Option<()> {
    output.extend_from_slice(&u16::try_from(value).ok()?.to_le_bytes());
    Some(())
}

fn write_text(output: &mut Vec<u8>, value: &str, maximum: usize) -> Option<()> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return None;
    }
    write_text_allow_empty(output, value, maximum)
}

fn write_text_allow_empty(output: &mut Vec<u8>, value: &str, maximum: usize) -> Option<()> {
    if value.len() > maximum || value.chars().any(char::is_control) {
        return None;
    }
    write_u16(output, value.len())?;
    output.extend_from_slice(value.as_bytes());
    Some(())
}

fn read_u16(input: &mut &[u8]) -> Option<u16> {
    let bytes = take(input, 2)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_text(input: &mut &[u8], maximum: usize, allow_empty: bool) -> Option<String> {
    let length = usize::from(read_u16(input)?);
    if length > maximum || (!allow_empty && length == 0) {
        return None;
    }
    let value = std::str::from_utf8(take(input, length)?).ok()?.to_owned();
    if value.chars().any(char::is_control) {
        return None;
    }
    Some(value)
}

fn take<'a>(input: &mut &'a [u8], length: usize) -> Option<&'a [u8]> {
    if input.len() < length {
        return None;
    }
    let (taken, remaining) = input.split_at(length);
    *input = remaining;
    Some(taken)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 || value.len() > 256 * 1024 {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = char::from(pair[0]).to_digit(16)?;
            let low = char::from(pair[1]).to_digit(16)?;
            u8::try_from((high << 4) | low).ok()
        })
        .collect()
}

fn set_mode(
    mode: &mut Option<SelectionMode>,
    value: SelectionMode,
) -> Result<(), SelectionArgumentError> {
    if mode.replace(value).is_some() {
        return Err(SelectionArgumentError::ConflictingModes);
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct SelectionFilterRequest {
    pub generation: u64,
    pub filter: SelectionFilter,
    pub entries: Arc<[Arc<DirectoryEntry>]>,
    pub selected_paths: Vec<PathBuf>,
    pub focus_list: bool,
}

#[derive(Debug)]
pub struct SelectionFilterResult {
    pub generation: u64,
    pub entries: Vec<Arc<DirectoryEntry>>,
    pub selected_paths: Vec<PathBuf>,
    pub focus_list: bool,
}

#[derive(Debug)]
pub enum SelectionFilterSubmitError {
    Busy(SelectionFilterRequest),
    Stopped(SelectionFilterRequest),
}

pub struct SelectionFilterWorker {
    sender: Option<SyncSender<SelectionFilterRequest>>,
    latest_result: Arc<Mutex<Option<SelectionFilterResult>>>,
    join: Option<JoinHandle<()>>,
}

impl SelectionFilterWorker {
    pub fn spawn() -> io::Result<Self> {
        let (sender, requests) =
            mpsc::sync_channel::<SelectionFilterRequest>(VISUAL_FILTER_QUEUE_CAPACITY);
        let latest_result = Arc::new(Mutex::new(None::<SelectionFilterResult>));
        let worker_result = Arc::clone(&latest_result);
        let join = thread::Builder::new()
            .name("floe-selection-filter".to_owned())
            .spawn(move || {
                while let Ok(request) = requests.recv() {
                    let rules = prepare_selection_filter(&request.filter);
                    let entries = request
                        .entries
                        .iter()
                        .filter(|entry| {
                            matches!(
                                entry.kind(),
                                EntryKind::Directory
                                    | EntryKind::SymbolicLink {
                                        target_is_directory: true
                                    }
                            ) || prepared_selection_filter_matches(
                                &rules,
                                entry.path(),
                                entry.display_name(),
                            )
                        })
                        .cloned()
                        .collect();
                    let result = SelectionFilterResult {
                        generation: request.generation,
                        entries,
                        selected_paths: request.selected_paths,
                        focus_list: request.focus_list,
                    };
                    let Ok(mut latest) = worker_result.lock() else {
                        break;
                    };
                    if latest
                        .as_ref()
                        .is_none_or(|previous| result.generation >= previous.generation)
                    {
                        *latest = Some(result);
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            latest_result,
            join: Some(join),
        })
    }

    pub fn try_submit(
        &self,
        request: SelectionFilterRequest,
    ) -> Result<(), SelectionFilterSubmitError> {
        let Some(sender) = self.sender.as_ref() else {
            return Err(SelectionFilterSubmitError::Stopped(request));
        };
        match sender.try_send(request) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(request)) => Err(SelectionFilterSubmitError::Busy(request)),
            Err(TrySendError::Disconnected(request)) => {
                Err(SelectionFilterSubmitError::Stopped(request))
            }
        }
    }

    pub fn try_result(&self) -> Option<SelectionFilterResult> {
        self.latest_result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }
}

impl Drop for SelectionFilterWorker {
    fn drop(&mut self) {
        self.sender.take();
        self.join.take();
    }
}

enum PreparedSelectionFilterRule {
    Glob(FolderFilterPattern),
    MimeExact(String),
    MimePrefix(String),
}

fn prepare_selection_filter(filter: &SelectionFilter) -> Vec<PreparedSelectionFilterRule> {
    filter
        .rules
        .iter()
        .filter_map(|rule| match rule {
            SelectionFilterRule::Glob(pattern) => {
                FolderFilterPattern::compile_with_case(FolderFilterMode::Glob, pattern, true)
                    .ok()
                    .map(PreparedSelectionFilterRule::Glob)
            }
            SelectionFilterRule::Mime(pattern) => pattern
                .strip_suffix("/*")
                .map(|prefix| PreparedSelectionFilterRule::MimePrefix(format!("{prefix}/")))
                .or_else(|| Some(PreparedSelectionFilterRule::MimeExact(pattern.clone()))),
        })
        .collect()
}

fn prepared_selection_filter_matches(
    rules: &[PreparedSelectionFilterRule],
    path: &Path,
    name: &std::ffi::OsStr,
) -> bool {
    let content_type = rules
        .iter()
        .any(|rule| {
            matches!(
                rule,
                PreparedSelectionFilterRule::MimeExact(_)
                    | PreparedSelectionFilterRule::MimePrefix(_)
            )
        })
        .then(|| gtk::gio::content_type_guess(Some(path), None::<&[u8]>).0);
    rules.iter().any(|rule| match rule {
        PreparedSelectionFilterRule::Glob(pattern) => pattern.matches(name),
        PreparedSelectionFilterRule::MimeExact(pattern) => content_type
            .as_ref()
            .is_some_and(|actual| actual.as_str() == pattern),
        PreparedSelectionFilterRule::MimePrefix(prefix) => content_type
            .as_ref()
            .is_some_and(|actual| actual.as_str().starts_with(prefix)),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionValidationRequest {
    pub id: u64,
    pub mode: SelectionMode,
    pub current_directory: PathBuf,
    pub selected_paths: Vec<PathBuf>,
    pub filename: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionValidationOutcome {
    Ready(Vec<PathBuf>),
    ReplaceConfirmation(PathBuf),
    Invalid(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionValidationResult {
    pub id: u64,
    pub outcome: SelectionValidationOutcome,
}

enum ValidationCommand {
    Validate(SelectionValidationRequest),
}

#[derive(Debug, Error)]
pub enum SelectionValidationSpawnError {
    #[error("Selection validation queue capacity must be nonzero")]
    ZeroCapacity,
    #[error("could not start Selection validation worker: {0}")]
    Spawn(#[source] io::Error),
}

#[derive(Debug, Error)]
pub enum SelectionValidationSubmitError {
    #[error("Selection validation worker is busy")]
    QueueFull,
    #[error("Selection validation worker stopped")]
    Stopped,
}

pub struct SelectionValidationWorker {
    sender: Option<SyncSender<ValidationCommand>>,
    latest_result: Arc<Mutex<Option<SelectionValidationResult>>>,
    join: Option<JoinHandle<()>>,
}

impl SelectionValidationWorker {
    pub fn spawn() -> Result<Self, SelectionValidationSpawnError> {
        Self::spawn_with_capacity(VALIDATION_QUEUE_CAPACITY)
    }

    fn spawn_with_capacity(capacity: usize) -> Result<Self, SelectionValidationSpawnError> {
        if capacity == 0 {
            return Err(SelectionValidationSpawnError::ZeroCapacity);
        }
        let (sender, commands) = mpsc::sync_channel(capacity);
        let latest_result = Arc::new(Mutex::new(None));
        let worker_result = Arc::clone(&latest_result);
        let join = thread::Builder::new()
            .name("floe-selection-validation".to_owned())
            .spawn(move || {
                while let Ok(ValidationCommand::Validate(request)) = commands.recv() {
                    let result = SelectionValidationResult {
                        id: request.id,
                        outcome: validate_selection(&request),
                    };
                    *worker_result
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(result);
                }
            })
            .map_err(SelectionValidationSpawnError::Spawn)?;
        Ok(Self {
            sender: Some(sender),
            latest_result,
            join: Some(join),
        })
    }

    pub fn submit(
        &self,
        request: SelectionValidationRequest,
    ) -> Result<(), SelectionValidationSubmitError> {
        let Some(sender) = self.sender.as_ref() else {
            return Err(SelectionValidationSubmitError::Stopped);
        };
        sender
            .try_send(ValidationCommand::Validate(request))
            .map_err(|error| match error {
                TrySendError::Full(_) => SelectionValidationSubmitError::QueueFull,
                TrySendError::Disconnected(_) => SelectionValidationSubmitError::Stopped,
            })
    }

    pub fn try_result(&self) -> Option<SelectionValidationResult> {
        self.latest_result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }
}

impl Drop for SelectionValidationWorker {
    fn drop(&mut self) {
        self.sender.take();
        self.join.take();
    }
}

fn validate_selection(request: &SelectionValidationRequest) -> SelectionValidationOutcome {
    if !is_normalized_absolute(&request.current_directory) {
        return SelectionValidationOutcome::Invalid(
            "The current folder is not a normalized local path".to_owned(),
        );
    }
    match request.mode {
        SelectionMode::OpenFile | SelectionMode::OpenFiles => validate_open(request),
        SelectionMode::SelectFolder => validate_folder(request),
        SelectionMode::SaveFile => validate_save(request),
    }
}

fn validate_open(request: &SelectionValidationRequest) -> SelectionValidationOutcome {
    if request.selected_paths.is_empty() {
        return SelectionValidationOutcome::Invalid("Select a file to open".to_owned());
    }
    if request.selected_paths.len() > SELECTION_PATH_CAPACITY {
        return SelectionValidationOutcome::Invalid(format!(
            "Select at most {SELECTION_PATH_CAPACITY} files"
        ));
    }
    if request.mode == SelectionMode::OpenFile && request.selected_paths.len() != 1 {
        return SelectionValidationOutcome::Invalid("Select exactly one file".to_owned());
    }
    for path in &request.selected_paths {
        if !is_normalized_absolute(path) {
            return SelectionValidationOutcome::Invalid("A selected path is invalid".to_owned());
        }
        match fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => {
                return SelectionValidationOutcome::Invalid(
                    "Only regular files can be opened in this mode".to_owned(),
                );
            }
            Err(error) => return SelectionValidationOutcome::Invalid(inspect_error(error)),
        }
    }
    SelectionValidationOutcome::Ready(request.selected_paths.clone())
}

fn validate_folder(request: &SelectionValidationRequest) -> SelectionValidationOutcome {
    let path = match request.selected_paths.as_slice() {
        [] => &request.current_directory,
        [path] => path,
        _ => {
            return SelectionValidationOutcome::Invalid(
                "Select one folder or use the current folder".to_owned(),
            );
        }
    };
    if !is_normalized_absolute(path) {
        return SelectionValidationOutcome::Invalid("The selected folder is invalid".to_owned());
    }
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => {
            SelectionValidationOutcome::Ready(vec![path.to_path_buf()])
        }
        Ok(_) => SelectionValidationOutcome::Invalid("Select a folder".to_owned()),
        Err(error) => SelectionValidationOutcome::Invalid(inspect_error(error)),
    }
}

fn validate_save(request: &SelectionValidationRequest) -> SelectionValidationOutcome {
    match fs::metadata(&request.current_directory) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            return SelectionValidationOutcome::Invalid(
                "The current location is not a folder".to_owned(),
            );
        }
        Err(error) => return SelectionValidationOutcome::Invalid(inspect_error(error)),
    }
    let Some(filename) = request.filename.as_deref() else {
        return SelectionValidationOutcome::Invalid("Enter a filename".to_owned());
    };
    if !valid_filename_component(filename) {
        return SelectionValidationOutcome::Invalid(
            "Enter one filename without /, . or ..".to_owned(),
        );
    }
    let destination = request.current_directory.join(filename);
    match fs::symlink_metadata(&destination) {
        Ok(metadata) if metadata.file_type().is_file() => {
            SelectionValidationOutcome::ReplaceConfirmation(destination)
        }
        Ok(_) => SelectionValidationOutcome::Invalid(
            "A folder or unsupported item already uses that name".to_owned(),
        ),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            SelectionValidationOutcome::Ready(vec![destination])
        }
        Err(error) => SelectionValidationOutcome::Invalid(inspect_error(error)),
    }
}

fn valid_filename_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains('/')
        && !value.contains('\0')
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn is_normalized_absolute(path: &Path) -> bool {
    path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::RootDir | Component::Normal(_)))
}

fn inspect_error(error: io::Error) -> String {
    match error.kind() {
        io::ErrorKind::NotFound => "The selected item no longer exists".to_owned(),
        io::ErrorKind::PermissionDenied => "The selected item is no longer accessible".to_owned(),
        _ => format!("Could not validate the selected item: {error}"),
    }
}

pub fn result_uris(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| gtk::gio::File::for_path(path).uri().to_string())
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionCompletion {
    Accepted(Vec<PathBuf>),
    AcceptedWithOptions(Vec<PathBuf>, SelectionOptionResult),
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionProcessOutput {
    Accepted(Vec<String>),
    AcceptedWithOptions(Vec<String>, SelectionOptionResult),
    Cancelled,
    Failed,
}

pub fn process_output(
    completion: Option<&SelectionCompletion>,
    process_succeeded: bool,
) -> SelectionProcessOutput {
    match completion {
        Some(SelectionCompletion::Accepted(paths)) => {
            SelectionProcessOutput::Accepted(result_uris(paths))
        }
        Some(SelectionCompletion::AcceptedWithOptions(paths, options)) => {
            SelectionProcessOutput::AcceptedWithOptions(result_uris(paths), options.clone())
        }
        Some(SelectionCompletion::Cancelled) => SelectionProcessOutput::Cancelled,
        Some(SelectionCompletion::Failed) => SelectionProcessOutput::Failed,
        None if process_succeeded => SelectionProcessOutput::Cancelled,
        None => SelectionProcessOutput::Failed,
    }
}

#[cfg(test)]
mod tests {
    use std::{os::unix::ffi::OsStringExt, time::Duration};

    use tempfile::tempdir;

    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn phase_22a_contract_parses_all_modes_and_rejects_invalid_combinations() {
        assert_eq!(
            parse_selection_invocation(args(&["floe", "--choose-open", "--multiple"])),
            Ok(Some(SelectionConfig {
                mode: SelectionMode::OpenFiles,
                initial_directory: None,
                suggested_name: None,
                title: None,
                accept_label: None,
                parent_window: None,
                modal: false,
                chooser_options: SelectionChooserOptions::default(),
            }))
        );
        assert_eq!(
            parse_selection_invocation(args(&[
                "floe",
                "--choose-save",
                "--initial-directory",
                "/tmp",
                "--suggested-name",
                "report.txt",
            ])),
            Ok(Some(SelectionConfig {
                mode: SelectionMode::SaveFile,
                initial_directory: Some(PathBuf::from("/tmp")),
                suggested_name: Some("report.txt".to_owned()),
                title: None,
                accept_label: None,
                parent_window: None,
                modal: false,
                chooser_options: SelectionChooserOptions::default(),
            }))
        );
        assert_eq!(
            parse_selection_invocation(args(&["floe", "--choose-folder", "--choose-save"])),
            Err(SelectionArgumentError::ConflictingModes)
        );
        assert_eq!(
            parse_selection_invocation(args(&["floe", "--choose-folder", "--multiple"])),
            Err(SelectionArgumentError::InvalidMultiple)
        );
        assert_eq!(
            parse_selection_invocation(args(&["floe", "/tmp"])),
            Ok(None)
        );
    }

    #[test]
    fn phase_22a_contract_uri_results_preserve_raw_linux_path_identity() {
        let raw = PathBuf::from(OsString::from_vec(b"/tmp/raw-\xff-name\nnext".to_vec()));
        let uris = result_uris(&[raw]);
        assert_eq!(uris, ["file:///tmp/raw-%FF-name%0Anext"]);

        let initial = OsString::from_vec(b"/tmp/raw-\xff-folder".to_vec());
        let parsed = parse_selection_invocation(vec![
            OsString::from("floe"),
            OsString::from("--choose-folder"),
            OsString::from("--initial-directory"),
            initial.clone(),
        ])
        .expect("raw invocation")
        .expect("selection config");
        assert_eq!(parsed.initial_directory, Some(PathBuf::from(initial)));
    }

    #[test]
    fn phase_22a_ui_presentation_is_mode_specific_and_unambiguous() {
        assert_eq!(
            SelectionMode::OpenFile.presentation(),
            SelectionPresentation {
                title: "Open File",
                accept_label: "Open",
                filename_visible: false,
                multiple: false,
            }
        );
        assert!(SelectionMode::OpenFiles.presentation().multiple);
        assert_eq!(SelectionMode::SelectFolder.accept_label(), "Select");
        assert!(SelectionMode::SaveFile.presentation().filename_visible);
        assert_eq!(SelectionMode::SaveFile.title(), "Save File");
    }

    #[test]
    fn phase_22a_lifecycle_emits_only_accepted_exact_uris() {
        let accepted = SelectionCompletion::Accepted(vec![PathBuf::from("/tmp/report.txt")]);
        assert_eq!(
            process_output(Some(&accepted), true),
            SelectionProcessOutput::Accepted(vec!["file:///tmp/report.txt".to_owned()])
        );
        assert_eq!(
            process_output(Some(&SelectionCompletion::Cancelled), true),
            SelectionProcessOutput::Cancelled
        );
        assert_eq!(
            process_output(Some(&SelectionCompletion::Failed), true),
            SelectionProcessOutput::Failed
        );
        assert_eq!(
            process_output(None, true),
            SelectionProcessOutput::Cancelled
        );
        assert_eq!(process_output(None, false), SelectionProcessOutput::Failed);
        let application_id = selection_application_id(42);
        assert_eq!(
            application_id,
            "io.github.rodriguezcappsec.Floe.Selection.p42"
        );
        assert!(gtk::gio::Application::id_is_valid(&application_id));
    }

    #[test]
    fn phase_22a_validation_covers_open_folder_save_and_conflict_policy() {
        let fixture = tempdir().expect("fixture");
        let file = fixture.path().join("file.txt");
        fs::write(&file, b"data").expect("file");
        let request = SelectionValidationRequest {
            id: 1,
            mode: SelectionMode::OpenFile,
            current_directory: fixture.path().to_path_buf(),
            selected_paths: vec![file.clone()],
            filename: None,
        };
        assert_eq!(
            validate_selection(&request),
            SelectionValidationOutcome::Ready(vec![file])
        );

        let folder = SelectionValidationRequest {
            id: 2,
            mode: SelectionMode::SelectFolder,
            current_directory: fixture.path().to_path_buf(),
            selected_paths: Vec::new(),
            filename: None,
        };
        assert_eq!(
            validate_selection(&folder),
            SelectionValidationOutcome::Ready(vec![fixture.path().to_path_buf()])
        );

        let save = SelectionValidationRequest {
            id: 3,
            mode: SelectionMode::SaveFile,
            current_directory: fixture.path().to_path_buf(),
            selected_paths: Vec::new(),
            filename: Some("file.txt".to_owned()),
        };
        assert_eq!(
            validate_selection(&save),
            SelectionValidationOutcome::ReplaceConfirmation(fixture.path().join("file.txt"))
        );
        let invalid = SelectionValidationRequest {
            filename: Some("../escape".to_owned()),
            ..save
        };
        assert!(matches!(
            validate_selection(&invalid),
            SelectionValidationOutcome::Invalid(_)
        ));
    }

    #[test]
    fn phase_22a_validation_worker_is_bounded_and_shuts_down() {
        assert!(matches!(
            SelectionValidationWorker::spawn_with_capacity(0),
            Err(SelectionValidationSpawnError::ZeroCapacity)
        ));
        let fixture = tempdir().expect("fixture");
        let worker = SelectionValidationWorker::spawn_with_capacity(1).expect("worker");
        worker
            .submit(SelectionValidationRequest {
                id: 7,
                mode: SelectionMode::SelectFolder,
                current_directory: fixture.path().to_path_buf(),
                selected_paths: Vec::new(),
                filename: None,
            })
            .expect("submit");
        for _ in 0..100 {
            if let Some(result) = worker.try_result() {
                assert_eq!(result.id, 7);
                assert!(matches!(
                    result.outcome,
                    SelectionValidationOutcome::Ready(_)
                ));
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("Selection validation timed out");
    }

    #[test]
    fn phase_22a_validation_rejects_missing_races_capacity_and_path_escape() {
        let fixture = tempdir().expect("fixture");
        let removed = fixture.path().join("removed");
        fs::write(&removed, b"data").expect("file");
        fs::remove_file(&removed).expect("remove");
        let missing = SelectionValidationRequest {
            id: 1,
            mode: SelectionMode::OpenFile,
            current_directory: fixture.path().to_path_buf(),
            selected_paths: vec![removed],
            filename: None,
        };
        assert!(matches!(
            validate_selection(&missing),
            SelectionValidationOutcome::Invalid(_)
        ));

        let too_many = SelectionValidationRequest {
            id: 2,
            mode: SelectionMode::OpenFiles,
            current_directory: fixture.path().to_path_buf(),
            selected_paths: (0..=SELECTION_PATH_CAPACITY)
                .map(|index| fixture.path().join(index.to_string()))
                .collect(),
            filename: None,
        };
        assert!(matches!(
            validate_selection(&too_many),
            SelectionValidationOutcome::Invalid(_)
        ));

        let spaced = SelectionValidationRequest {
            id: 3,
            mode: SelectionMode::SaveFile,
            current_directory: fixture.path().to_path_buf(),
            selected_paths: Vec::new(),
            filename: Some(" report .txt ".to_owned()),
        };
        assert_eq!(
            validate_selection(&spaced),
            SelectionValidationOutcome::Ready(vec![fixture.path().join(" report .txt ")])
        );
    }

    #[test]
    fn phase_22c_selection_options_visual_filter_keeps_navigation_and_filters_files() {
        let fixture = tempdir().expect("fixture");
        fs::create_dir(fixture.path().join("folder")).expect("folder");
        std::os::unix::fs::symlink("folder", fixture.path().join("folder-link"))
            .expect("directory symlink");
        fs::write(fixture.path().join("notes.txt"), b"notes").expect("text file");
        fs::write(fixture.path().join("photo.png"), b"not actually png").expect("png file");
        let entries = floe_core::enumerate_directory(fixture.path())
            .expect("listing")
            .into_entries()
            .into_iter()
            .map(Arc::new)
            .collect::<Vec<_>>();
        let worker = SelectionFilterWorker::spawn().expect("selection filter worker");
        worker
            .try_submit(SelectionFilterRequest {
                generation: 11,
                filter: SelectionFilter {
                    label: "Text".to_owned(),
                    rules: vec![SelectionFilterRule::Glob("*.txt".to_owned())],
                },
                entries: Arc::from(entries),
                selected_paths: vec![fixture.path().join("notes.txt")],
                focus_list: true,
            })
            .expect("submit filter");

        for _ in 0..200 {
            if let Some(result) = worker.try_result() {
                assert_eq!(result.generation, 11);
                assert_eq!(result.selected_paths, [fixture.path().join("notes.txt")]);
                assert!(result.focus_list);
                let names = result
                    .entries
                    .iter()
                    .map(|entry| entry.display_name().to_string_lossy().into_owned())
                    .collect::<std::collections::BTreeSet<_>>();
                assert_eq!(
                    names,
                    ["folder", "folder-link", "notes.txt"]
                        .into_iter()
                        .map(str::to_owned)
                        .collect()
                );
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("Selection visual filter timed out");
    }
}
