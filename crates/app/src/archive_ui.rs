use std::{
    ffi::{OsStr, OsString},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    sync::Arc,
};

use adw::prelude::*;
use floe_core::{
    ARCHIVE_SOURCE_CAPACITY, ArchiveFormat, ArchiveRequest, ArchiveRequestError, JobFailureKind,
};
use thiserror::Error;

pub const ARCHIVE_FORMATS: [ArchiveFormat; 5] = [
    ArchiveFormat::Zip,
    ArchiveFormat::Tar,
    ArchiveFormat::TarGz,
    ArchiveFormat::TarXz,
    ArchiveFormat::SevenZip,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArchiveActionEligibility {
    pub extract: bool,
    pub compress: bool,
}

impl ArchiveActionEligibility {
    pub fn new(paths: &[PathBuf], all_regular_or_directory: bool, trash_mode: bool) -> Self {
        let extract =
            !trash_mode && paths.len() == 1 && ArchiveFormat::from_path(&paths[0]).is_some();
        let compress = !trash_mode
            && all_regular_or_directory
            && !paths.is_empty()
            && paths.len() <= ARCHIVE_SOURCE_CAPACITY;
        Self { extract, compress }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ArchiveUiError {
    #[error("select exactly one supported local archive")]
    InvalidArchiveSelection,
    #[error("select between one and {ARCHIVE_SOURCE_CAPACITY} local files or folders")]
    InvalidCompressionSelection,
    #[error(
        "archive destination name cannot be empty, '.', '..', contain '/', or contain a NUL byte"
    )]
    InvalidDestinationName,
    #[error("the selected destination folder is not a local absolute path")]
    InvalidDestinationFolder,
    #[error(transparent)]
    Request(#[from] ArchiveRequestError),
}

pub fn extraction_request(
    source: PathBuf,
    destination_parent: &Path,
) -> Result<ArchiveRequest, ArchiveUiError> {
    if ArchiveFormat::from_path(&source).is_none() {
        return Err(ArchiveUiError::InvalidArchiveSelection);
    }
    if !destination_parent.is_absolute() {
        return Err(ArchiveUiError::InvalidDestinationFolder);
    }
    let name = extraction_folder_name(&source);
    ArchiveRequest::extract(source, destination_parent.join(name)).map_err(Into::into)
}

pub fn compression_request(
    sources: Arc<[PathBuf]>,
    destination_parent: &Path,
    destination_name: &OsStr,
) -> Result<ArchiveRequest, ArchiveUiError> {
    if sources.is_empty() || sources.len() > ARCHIVE_SOURCE_CAPACITY {
        return Err(ArchiveUiError::InvalidCompressionSelection);
    }
    if !destination_parent.is_absolute() {
        return Err(ArchiveUiError::InvalidDestinationFolder);
    }
    validate_destination_name(destination_name)?;
    ArchiveRequest::compress(sources.to_vec(), destination_parent.join(destination_name))
        .map_err(Into::into)
}

pub fn default_compression_name(sources: &[PathBuf], format: ArchiveFormat) -> OsString {
    let base = if sources.len() == 1 {
        sources[0]
            .file_name()
            .filter(|name| !name.as_bytes().is_empty())
            .map_or_else(|| OsString::from("Archive"), OsStr::to_os_string)
    } else {
        OsString::from("Archive")
    };
    let mut bytes = base.into_vec();
    bytes.push(b'.');
    bytes.extend_from_slice(format.extension().as_bytes());
    OsString::from_vec(bytes)
}

pub fn with_archive_extension(name: &str, format: ArchiveFormat) -> OsString {
    let suffix = format!(".{}", format.extension());
    if name.to_ascii_lowercase().ends_with(&suffix) {
        OsString::from(name)
    } else {
        OsString::from(format!("{name}{suffix}"))
    }
}

pub fn destination_preview(parent: &Path, name: &OsStr) -> String {
    parent.join(name).to_string_lossy().into_owned()
}

pub fn archive_failure_text(kind: JobFailureKind, message: &str) -> (&'static str, String) {
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("password") || normalized.contains("encrypted") {
        return (
            "Password-protected archive unsupported",
            "This archive needs a password, but Floe's reviewed archive backend does not support secret handoff yet. No password was requested, stored, logged, or passed to another program."
                .to_owned(),
        );
    }
    if kind == JobFailureKind::Conflict {
        return (
            "Archive destination exists",
            "Choose a different destination name or folder. Floe did not overwrite the existing item."
                .to_owned(),
        );
    }
    ("Archive operation failed", message.to_owned())
}

fn extraction_folder_name(source: &Path) -> OsString {
    let name = source
        .file_name()
        .map_or_else(|| OsString::from("Extracted"), OsStr::to_os_string);
    let bytes = name.as_bytes();
    let suffix = ArchiveFormat::from_path(source)
        .map(|format| format!(".{}", format.extension()).into_bytes())
        .unwrap_or_default();
    if bytes.len() > suffix.len()
        && bytes[bytes.len() - suffix.len()..].eq_ignore_ascii_case(&suffix)
    {
        OsString::from_vec(bytes[..bytes.len() - suffix.len()].to_vec())
    } else {
        OsString::from("Extracted")
    }
}

fn validate_destination_name(name: &OsStr) -> Result<(), ArchiveUiError> {
    let bytes = name.as_bytes();
    if bytes.is_empty()
        || matches!(bytes, b"." | b"..")
        || bytes.contains(&b'/')
        || bytes.contains(&0)
    {
        Err(ArchiveUiError::InvalidDestinationName)
    } else {
        Ok(())
    }
}

pub struct CompressDialogWidgets {
    pub dialog: adw::Dialog,
    pub name_entry: gtk::Entry,
    pub format_dropdown: gtk::DropDown,
    pub preview_label: gtk::Label,
    pub error_label: gtk::Label,
    pub cancel_button: gtk::Button,
    pub compress_button: gtk::Button,
}

pub fn build_compress_dialog(
    source_count: usize,
    default_name: &str,
    destination_preview_text: &str,
) -> CompressDialogWidgets {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(20)
        .margin_bottom(20)
        .margin_start(20)
        .margin_end(20)
        .build();
    let heading = gtk::Label::builder()
        .label(format!(
            "Compress {source_count} selected item{}",
            if source_count == 1 { "" } else { "s" }
        ))
        .xalign(0.0)
        .build();
    heading.add_css_class("title-2");
    content.append(&heading);

    let name_label = gtk::Label::builder()
        .label("Archive name")
        .xalign(0.0)
        .build();
    content.append(&name_label);
    let name_entry = gtk::Entry::builder()
        .text(default_name)
        .activates_default(true)
        .build();
    name_entry.update_property(&[gtk::accessible::Property::Label("Archive name")]);
    content.append(&name_entry);

    let format_label = gtk::Label::builder().label("Format").xalign(0.0).build();
    content.append(&format_label);
    let format_dropdown = gtk::DropDown::from_strings(&[
        "ZIP (.zip)",
        "TAR (.tar)",
        "Compressed TAR (.tar.gz)",
        "XZ TAR (.tar.xz)",
        "7-Zip (.7z)",
    ]);
    format_dropdown.update_property(&[gtk::accessible::Property::Label("Archive format")]);
    content.append(&format_dropdown);

    let preview_label = gtk::Label::builder()
        .label(destination_preview_text)
        .xalign(0.0)
        .wrap(true)
        .selectable(true)
        .build();
    preview_label.add_css_class("dim-label");
    preview_label.update_property(&[gtk::accessible::Property::Label(
        "Archive destination preview",
    )]);
    content.append(&preview_label);

    let notice = gtk::Label::builder()
        .label("Existing destinations are never overwritten. Password-protected archives are not created in this version.")
        .xalign(0.0)
        .wrap(true)
        .build();
    notice.add_css_class("floe-status");
    content.append(&notice);

    let error_label = gtk::Label::builder().xalign(0.0).wrap(true).build();
    error_label.add_css_class("error");
    error_label.update_property(&[gtk::accessible::Property::Label(
        "Archive validation message",
    )]);
    content.append(&error_label);

    let actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::End)
        .build();
    let cancel_button = gtk::Button::with_label("Cancel");
    let compress_button = gtk::Button::with_label("Compress");
    compress_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&compress_button);
    content.append(&actions);

    let dialog = adw::Dialog::builder()
        .title("Compress Selection")
        .content_width(560)
        .content_height(470)
        .child(&content)
        .build();
    dialog.update_property(&[gtk::accessible::Property::Label("Compress Selection")]);

    CompressDialogWidgets {
        dialog,
        name_entry,
        format_dropdown,
        preview_label,
        error_label,
        cancel_button,
        compress_button,
    }
}

pub fn selected_format(dropdown: &gtk::DropDown) -> ArchiveFormat {
    ARCHIVE_FORMATS
        .get(dropdown.selected() as usize)
        .copied()
        .unwrap_or(ArchiveFormat::Zip)
}

#[cfg(test)]
mod tests {
    use std::{
        fs, thread,
        time::{Duration, Instant},
    };

    use floe_core::{JobEventKind, JobState};
    use tempfile::tempdir;

    use super::*;
    use crate::state::ApplicationState;

    fn wait_for_terminal(state: &ApplicationState, job_id: floe_core::JobId) -> JobState {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let current = state
                .jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .record(job_id)
                .map(|record| record.state());
            if current.is_some_and(JobState::is_terminal) {
                return current.expect("terminal state");
            }
            assert!(Instant::now() < deadline, "archive UI job timed out");
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn phase_12b_archive_ui_contract_preserves_raw_names_and_destinations() {
        let raw = PathBuf::from(OsString::from_vec(b"/tmp/r\xff.tar.gz".to_vec()));
        let request = extraction_request(raw, Path::new("/tmp/out")).expect("extract request");
        assert_eq!(
            request
                .destination()
                .expect("destination")
                .as_os_str()
                .as_bytes(),
            b"/tmp/out/r\xff"
        );

        let source = PathBuf::from(OsString::from_vec(b"/tmp/n\xffme".to_vec()));
        let name = default_compression_name(std::slice::from_ref(&source), ArchiveFormat::Tar);
        assert_eq!(name.as_bytes(), b"n\xffme.tar");
        let destination = Path::new("/tmp").join(&name);
        let request = compression_request(Arc::from([source]), Path::new("/tmp"), name.as_os_str())
            .expect("compress request");
        assert_eq!(request.destination(), Some(destination.as_path()));
    }

    #[test]
    fn phase_12b_archive_ui_actions_are_bounded_and_truthful() {
        let archive = PathBuf::from("/tmp/a.zip");
        assert_eq!(
            ArchiveActionEligibility::new(std::slice::from_ref(&archive), true, false),
            ArchiveActionEligibility {
                extract: true,
                compress: true,
            }
        );
        assert!(!ArchiveActionEligibility::new(&[], true, false).compress);
        assert!(!ArchiveActionEligibility::new(&[archive], true, true).extract);
        assert_eq!(
            with_archive_extension("bundle", ArchiveFormat::TarGz),
            OsString::from("bundle.tar.gz")
        );
        assert!(matches!(
            compression_request(
                Arc::from([PathBuf::from("/tmp/a")]),
                Path::new("/tmp"),
                OsStr::new("../bad")
            ),
            Err(ArchiveUiError::InvalidDestinationName)
        ));
        let (title, detail) = archive_failure_text(
            JobFailureKind::Unsupported,
            "archive password or encryption is unsupported",
        );
        assert!(title.contains("Password-protected"));
        assert!(detail.contains("No password was requested, stored, logged"));
        let (_, conflict) = archive_failure_text(JobFailureKind::Conflict, "exists");
        assert!(conflict.contains("did not overwrite"));
    }

    #[test]
    fn phase_12b_archive_ui_jobs_reuse_progress_conflicts_and_cancellation() {
        let root = tempdir().expect("root");
        let source = root.path().join("source.txt");
        fs::write(&source, b"archive UI job").expect("source");
        let destination = root.path().join("created.zip");
        let request = ArchiveRequest::compress(vec![source.clone()], destination).expect("request");
        let state = ApplicationState::new().expect("state");
        let submission = state.submit_archive(request).expect("submit");
        assert!(state.is_archive_operation(submission.job_id()));
        assert_eq!(
            state.archive_affected_directories(submission.job_id()),
            vec![root.path().to_path_buf()]
        );
        assert_eq!(
            wait_for_terminal(&state, submission.job_id()),
            JobState::Completed
        );
        assert!(matches!(
            state.finish_archive(submission.job_id()),
            Some(floe_core::ArchiveOutcome::Compressed { .. })
        ));
        assert!(!state.is_archive_operation(submission.job_id()));

        let occupied = root.path().join("occupied.zip");
        fs::write(&occupied, b"keep me").expect("occupied");
        let request =
            ArchiveRequest::compress(vec![source.clone()], occupied.clone()).expect("request");
        let conflict = state.submit_archive(request).expect("submit conflict");
        assert_eq!(
            wait_for_terminal(&state, conflict.job_id()),
            JobState::Failed
        );
        let events = state.drain_job_events();
        assert!(events.iter().any(|event| {
            event.job_id() == conflict.job_id()
                && matches!(
                    event.kind(),
                    JobEventKind::Failed(failure) if failure.kind() == JobFailureKind::Conflict
                )
        }));
        assert_eq!(fs::read(&occupied).expect("preserved"), b"keep me");
        assert!(state.finish_archive(conflict.job_id()).is_none());

        let large = root.path().join("large.bin");
        fs::write(&large, vec![7_u8; 4 * 1024 * 1024]).expect("large");
        let request =
            ArchiveRequest::compress(vec![large], root.path().join("cancel.tar")).expect("request");
        let cancelled = state.submit_archive(request).expect("submit cancel");
        state.cancel_operation(cancelled.job_id()).expect("cancel");
        assert_eq!(
            wait_for_terminal(&state, cancelled.job_id()),
            JobState::Cancelled
        );
        assert!(state.finish_archive(cancelled.job_id()).is_none());
    }
}
