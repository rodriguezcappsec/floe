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

use gtk::gio::prelude::FileExt;
use thiserror::Error;

pub const SELECTION_PATH_CAPACITY: usize = 128;
const VALIDATION_QUEUE_CAPACITY: usize = 4;

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

    pub const fn presentation(self) -> SelectionPresentation {
        SelectionPresentation {
            title: self.title(),
            accept_label: self.accept_label(),
            filename_visible: self.needs_filename(),
            multiple: matches!(self, Self::OpenFiles),
        }
    }
}

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
}

impl SelectionConfig {
    pub fn initial_directory_or_else(&self, fallback: impl FnOnce() -> PathBuf) -> PathBuf {
        self.initial_directory.clone().unwrap_or_else(fallback)
    }
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
    #[error("unknown Selection Mode option: {0}")]
    UnknownOption(String),
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
    }))
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
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
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
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionProcessOutput {
    Accepted(Vec<String>),
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
}
