use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{
        Arc, Mutex, MutexGuard,
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

use adw::prelude::*;
use floe_core::{
    BATCH_RENAME_CAPACITY, BatchRenameCancellation, BatchRenameError, BatchRenameOutcome,
    BatchRenamePair, BatchRenameRequest, JobCommand, JobFailure, JobFailureKind, JobId,
    JobProgress, OperationId, execute_batch_rename,
};
use regex::Regex;
use thiserror::Error;

use crate::job_manager::{JobManagerError, SharedJobManager};

pub const BATCH_RENAME_QUEUE_CAPACITY: usize = 4;
pub const BATCH_RENAME_RESULT_CAPACITY: usize = 16;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RenameCase {
    #[default]
    Unchanged,
    Lower,
    Upper,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchRenameRule {
    pub find: String,
    pub replace: String,
    pub regex: bool,
    pub prefix: String,
    pub suffix: String,
    pub sequence_start: u64,
    pub sequence_padding: usize,
    pub case: RenameCase,
    pub preserve_extension: bool,
}

impl Default for BatchRenameRule {
    fn default() -> Self {
        Self {
            find: String::new(),
            replace: String::new(),
            regex: false,
            prefix: String::new(),
            suffix: String::new(),
            sequence_start: 1,
            sequence_padding: 1,
            case: RenameCase::Unchanged,
            preserve_extension: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchRenameSource {
    pub path: PathBuf,
    pub date: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchRenamePreviewRow {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub old_name: String,
    pub new_name: String,
}

#[derive(Debug, Error)]
pub enum BatchRenameModelError {
    #[error("select between two and {BATCH_RENAME_CAPACITY} local items")]
    InvalidCount,
    #[error("batch rename cannot transform a non-UTF-8 filename: {}", .0.display())]
    NonUtf8(PathBuf),
    #[error("invalid regular expression: {0}")]
    Regex(String),
    #[error("batch rename produced an empty or unsafe name")]
    UnsafeName,
    #[error("batch rename produced duplicate name: {0}")]
    DuplicateName(String),
    #[error(transparent)]
    Request(#[from] floe_core::BatchRenameRequestError),
}

pub fn build_batch_rename_preview(
    sources: &[BatchRenameSource],
    rule: &BatchRenameRule,
) -> Result<(Vec<BatchRenamePreviewRow>, BatchRenameRequest), BatchRenameModelError> {
    if sources.len() < 2 || sources.len() > BATCH_RENAME_CAPACITY {
        return Err(BatchRenameModelError::InvalidCount);
    }
    let regex = if rule.regex && !rule.find.is_empty() {
        Some(
            Regex::new(&rule.find)
                .map_err(|error| BatchRenameModelError::Regex(error.to_string()))?,
        )
    } else {
        None
    };
    let mut rows = Vec::with_capacity(sources.len());
    let mut pairs = Vec::with_capacity(sources.len());
    let mut names = HashSet::with_capacity(sources.len());
    for (index, source) in sources.iter().enumerate() {
        let old_name = source
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| BatchRenameModelError::NonUtf8(source.path.clone()))?;
        let (base, extension) = if rule.preserve_extension {
            source
                .path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| (stem, source.path.extension().and_then(|ext| ext.to_str())))
                .unwrap_or((old_name, None))
        } else {
            (old_name, None)
        };
        let replaced = if let Some(regex) = &regex {
            regex.replace_all(base, rule.replace.as_str()).into_owned()
        } else if rule.find.is_empty() {
            base.to_owned()
        } else {
            base.replace(&rule.find, &rule.replace)
        };
        let number = rule.sequence_start.saturating_add(index as u64);
        let sequence = format!(
            "{number:0width$}",
            width = rule.sequence_padding.clamp(1, 12)
        );
        let expand = |text: &str| {
            text.replace("{name}", &replaced)
                .replace("{n}", &sequence)
                .replace("{date}", &source.date)
                .replace("{ext}", extension.unwrap_or(""))
        };
        let mut transformed = format!(
            "{}{}{}",
            expand(&rule.prefix),
            replaced,
            expand(&rule.suffix)
        );
        transformed = match rule.case {
            RenameCase::Unchanged => transformed,
            RenameCase::Lower => transformed.to_lowercase(),
            RenameCase::Upper => transformed.to_uppercase(),
        };
        if rule.preserve_extension
            && let Some(extension) = extension
        {
            transformed.push('.');
            transformed.push_str(extension);
        }
        if transformed.is_empty()
            || transformed == "."
            || transformed == ".."
            || transformed.contains('/')
            || transformed.contains('\0')
        {
            return Err(BatchRenameModelError::UnsafeName);
        }
        if !names.insert(transformed.clone()) {
            return Err(BatchRenameModelError::DuplicateName(transformed));
        }
        let destination = source
            .path
            .parent()
            .expect("selected entry has parent")
            .join(&transformed);
        let pair = BatchRenamePair::new(source.path.clone(), destination.clone())?;
        pairs.push(pair);
        rows.push(BatchRenamePreviewRow {
            source: source.path.clone(),
            destination,
            old_name: old_name.to_owned(),
            new_name: transformed,
        });
    }
    Ok((rows, BatchRenameRequest::new(pairs)?))
}

pub fn preview_text(rows: &[BatchRenamePreviewRow]) -> String {
    let mut lines = rows
        .iter()
        .take(128)
        .map(|row| format!("{}  →  {}", row.old_name, row.new_name))
        .collect::<Vec<_>>();
    if rows.len() > 128 {
        lines.push(format!("… and {} more", rows.len() - 128));
    }
    lines.join("\n")
}

#[derive(Clone)]
pub struct BatchRenameDialogWidgets {
    pub dialog: adw::Dialog,
    pub find_entry: gtk::Entry,
    pub replace_entry: gtk::Entry,
    pub prefix_entry: gtk::Entry,
    pub suffix_entry: gtk::Entry,
    pub regex_check: gtk::CheckButton,
    pub preserve_extension_check: gtk::CheckButton,
    pub sequence_start: gtk::SpinButton,
    pub sequence_padding: gtk::SpinButton,
    pub case_dropdown: gtk::DropDown,
    pub preview: gtk::TextView,
    pub error_label: gtk::Label,
    pub cancel_button: gtk::Button,
    pub apply_button: gtk::Button,
}

pub fn refresh_batch_rename_dialog(
    widgets: &BatchRenameDialogWidgets,
    sources: &[BatchRenameSource],
) -> Option<BatchRenameRequest> {
    let case = match widgets.case_dropdown.selected() {
        1 => RenameCase::Lower,
        2 => RenameCase::Upper,
        _ => RenameCase::Unchanged,
    };
    let rule = BatchRenameRule {
        find: widgets.find_entry.text().to_string(),
        replace: widgets.replace_entry.text().to_string(),
        regex: widgets.regex_check.is_active(),
        prefix: widgets.prefix_entry.text().to_string(),
        suffix: widgets.suffix_entry.text().to_string(),
        sequence_start: widgets.sequence_start.value() as u64,
        sequence_padding: widgets.sequence_padding.value() as usize,
        case,
        preserve_extension: widgets.preserve_extension_check.is_active(),
    };
    match build_batch_rename_preview(sources, &rule) {
        Ok((rows, request)) => {
            widgets.preview.buffer().set_text(&preview_text(&rows));
            widgets.error_label.set_label("");
            widgets.apply_button.set_sensitive(true);
            Some(request)
        }
        Err(error) => {
            widgets.preview.buffer().set_text("");
            widgets.error_label.set_label(&error.to_string());
            widgets.apply_button.set_sensitive(false);
            None
        }
    }
}

pub fn build_batch_rename_dialog(count: usize) -> BatchRenameDialogWidgets {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();
    let heading = gtk::Label::builder()
        .label(format!("Rename {count} selected items"))
        .xalign(0.0)
        .build();
    heading.add_css_class("title-2");
    content.append(&heading);
    let helper = gtk::Label::builder()
        .label("Use {name}, {n}, {date}, or {ext} in prefix, replacement, and suffix fields. The entire batch is validated before any name changes.")
        .xalign(0.0)
        .wrap(true)
        .build();
    helper.add_css_class("dim-label");
    content.append(&helper);

    let grid = gtk::Grid::builder()
        .row_spacing(8)
        .column_spacing(10)
        .build();
    let find_entry = labelled_entry(&grid, 0, "Find", "Text or regular expression");
    let replace_entry = labelled_entry(&grid, 1, "Replace", "Replacement or template");
    let prefix_entry = labelled_entry(&grid, 2, "Prefix", "Prefix or template");
    let suffix_entry = labelled_entry(&grid, 3, "Suffix", "Suffix or template");
    content.append(&grid);

    let options = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .build();
    let regex_check = gtk::CheckButton::with_label("Regular expression");
    let preserve_extension_check = gtk::CheckButton::with_label("Preserve extension");
    preserve_extension_check.set_active(true);
    options.append(&regex_check);
    options.append(&preserve_extension_check);
    content.append(&options);

    let sequence = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .build();
    sequence.append(&gtk::Label::new(Some("Number start")));
    let sequence_start = gtk::SpinButton::with_range(0.0, u32::MAX as f64, 1.0);
    sequence_start.set_value(1.0);
    sequence_start.update_property(&[gtk::accessible::Property::Label("Sequence start")]);
    sequence.append(&sequence_start);
    sequence.append(&gtk::Label::new(Some("Padding")));
    let sequence_padding = gtk::SpinButton::with_range(1.0, 12.0, 1.0);
    sequence_padding.set_value(1.0);
    sequence_padding.update_property(&[gtk::accessible::Property::Label("Sequence padding")]);
    sequence.append(&sequence_padding);
    sequence.append(&gtk::Label::new(Some("Case")));
    let case_dropdown = gtk::DropDown::from_strings(&["Unchanged", "lowercase", "UPPERCASE"]);
    case_dropdown.update_property(&[gtk::accessible::Property::Label("Filename case")]);
    sequence.append(&case_dropdown);
    content.append(&sequence);

    let preview = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    preview.update_property(&[gtk::accessible::Property::Label("Batch rename preview")]);
    let scroller = gtk::ScrolledWindow::builder()
        .min_content_height(180)
        .vexpand(true)
        .child(&preview)
        .build();
    content.append(&scroller);
    let error_label = gtk::Label::builder().xalign(0.0).wrap(true).build();
    error_label.add_css_class("error");
    error_label.update_property(&[gtk::accessible::Property::Label(
        "Batch rename validation message",
    )]);
    content.append(&error_label);

    let actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::End)
        .build();
    let cancel_button = gtk::Button::with_label("Cancel");
    let apply_button = gtk::Button::with_label("Rename All");
    apply_button.add_css_class("suggested-action");
    apply_button.set_sensitive(false);
    actions.append(&cancel_button);
    actions.append(&apply_button);
    content.append(&actions);

    let dialog = adw::Dialog::builder()
        .title("Batch Rename")
        .content_width(680)
        .content_height(720)
        .child(&content)
        .build();
    dialog.update_property(&[gtk::accessible::Property::Label("Batch Rename")]);
    BatchRenameDialogWidgets {
        dialog,
        find_entry,
        replace_entry,
        prefix_entry,
        suffix_entry,
        regex_check,
        preserve_extension_check,
        sequence_start,
        sequence_padding,
        case_dropdown,
        preview,
        error_label,
        cancel_button,
        apply_button,
    }
}

fn labelled_entry(
    grid: &gtk::Grid,
    row: i32,
    label: &'static str,
    placeholder: &'static str,
) -> gtk::Entry {
    let visible_label = gtk::Label::builder().label(label).xalign(0.0).build();
    let entry = gtk::Entry::builder()
        .placeholder_text(placeholder)
        .hexpand(true)
        .build();
    entry.update_property(&[gtk::accessible::Property::Label(label)]);
    grid.attach(&visible_label, 0, row, 1, 1);
    grid.attach(&entry, 1, row, 1, 1);
    entry
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchRenameSubmission {
    operation_id: OperationId,
    job_id: JobId,
}

impl BatchRenameSubmission {
    pub const fn operation_id(self) -> OperationId {
        self.operation_id
    }
    pub const fn job_id(self) -> JobId {
        self.job_id
    }
}

#[derive(Debug, Error)]
pub enum BatchRenameExecutorError {
    #[error("could not spawn batch rename worker: {0}")]
    Spawn(#[source] std::io::Error),
    #[error(transparent)]
    Jobs(#[from] JobManagerError),
    #[error("batch rename queue is full for {0:?}")]
    QueueFull(JobId),
    #[error("batch rename executor stopped for {0:?}")]
    Stopped(JobId),
    #[error("batch rename job is not active: {0:?}")]
    NotActive(JobId),
}

struct Task {
    job_id: JobId,
    request: BatchRenameRequest,
    cancellation: BatchRenameCancellation,
}

enum Command {
    Execute(Task),
    Shutdown,
}

pub struct BatchRenameExecutor {
    sender: Option<SyncSender<Command>>,
    jobs: SharedJobManager,
    cancellations: Arc<Mutex<HashMap<JobId, BatchRenameCancellation>>>,
    results: Arc<Mutex<VecDeque<(JobId, BatchRenameOutcome)>>>,
    worker: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for BatchRenameExecutor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BatchRenameExecutor")
            .finish_non_exhaustive()
    }
}

impl BatchRenameExecutor {
    pub fn spawn(jobs: SharedJobManager) -> Result<Self, BatchRenameExecutorError> {
        let (sender, receiver) = mpsc::sync_channel(BATCH_RENAME_QUEUE_CAPACITY);
        let cancellations = Arc::new(Mutex::new(HashMap::new()));
        let results = Arc::new(Mutex::new(VecDeque::with_capacity(
            BATCH_RENAME_RESULT_CAPACITY,
        )));
        let worker_jobs = Arc::clone(&jobs);
        let worker_cancellations = Arc::clone(&cancellations);
        let worker_results = Arc::clone(&results);
        let worker = thread::Builder::new()
            .name("floe-batch-rename-worker".to_owned())
            .spawn(move || run_worker(receiver, worker_jobs, worker_cancellations, worker_results))
            .map_err(BatchRenameExecutorError::Spawn)?;
        Ok(Self {
            sender: Some(sender),
            jobs,
            cancellations,
            results,
            worker: Some(worker),
        })
    }

    pub fn submit(
        &self,
        request: BatchRenameRequest,
    ) -> Result<BatchRenameSubmission, BatchRenameExecutorError> {
        let queued = lock(&self.jobs).queue_operation()?;
        let submission = BatchRenameSubmission {
            operation_id: queued.operation_id(),
            job_id: queued.job_id(),
        };
        let cancellation = BatchRenameCancellation::default();
        lock(&self.cancellations).insert(submission.job_id, cancellation.clone());
        let task = Task {
            job_id: submission.job_id,
            request,
            cancellation,
        };
        match self
            .sender
            .as_ref()
            .ok_or(BatchRenameExecutorError::Stopped(submission.job_id))?
            .try_send(Command::Execute(task))
        {
            Ok(()) => Ok(submission),
            Err(TrySendError::Full(_)) => {
                lock(&self.cancellations).remove(&submission.job_id);
                fail(&self.jobs, submission.job_id, "batch rename queue is full");
                Err(BatchRenameExecutorError::QueueFull(submission.job_id))
            }
            Err(TrySendError::Disconnected(_)) => {
                lock(&self.cancellations).remove(&submission.job_id);
                fail(
                    &self.jobs,
                    submission.job_id,
                    "batch rename executor stopped",
                );
                Err(BatchRenameExecutorError::Stopped(submission.job_id))
            }
        }
    }

    pub fn cancel(&self, job_id: JobId) -> Result<(), BatchRenameExecutorError> {
        let cancellation = lock(&self.cancellations)
            .get(&job_id)
            .cloned()
            .ok_or(BatchRenameExecutorError::NotActive(job_id))?;
        cancellation.cancel();
        Ok(())
    }

    pub fn take_result(&self, job_id: JobId) -> Option<BatchRenameOutcome> {
        let mut results = lock(&self.results);
        let index = results
            .iter()
            .position(|(candidate, _)| *candidate == job_id)?;
        results.remove(index).map(|(_, outcome)| outcome)
    }
}

impl Drop for BatchRenameExecutor {
    fn drop(&mut self) {
        for cancellation in lock(&self.cancellations).values() {
            cancellation.cancel();
        }
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(Command::Shutdown);
        }
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("batch rename worker panicked during shutdown");
        }
    }
}

fn run_worker(
    receiver: Receiver<Command>,
    jobs: SharedJobManager,
    cancellations: Arc<Mutex<HashMap<JobId, BatchRenameCancellation>>>,
    results: Arc<Mutex<VecDeque<(JobId, BatchRenameOutcome)>>>,
) {
    while let Ok(command) = receiver.recv() {
        match command {
            Command::Execute(task) => {
                if transition(&jobs, task.job_id, JobCommand::Start).is_err() {
                    continue;
                }
                let result =
                    execute_batch_rename(&task.request, &task.cancellation, |completed, total| {
                        if let Ok(progress) = JobProgress::items(completed, Some(total)) {
                            let _ =
                                transition(&jobs, task.job_id, JobCommand::SetProgress(progress));
                        }
                    });
                let terminal = match result {
                    Ok(outcome) => {
                        let mut queue = lock(&results);
                        if queue.len() == BATCH_RENAME_RESULT_CAPACITY {
                            queue.pop_front();
                        }
                        queue.push_back((task.job_id, outcome));
                        JobCommand::Complete
                    }
                    Err(BatchRenameError::Cancelled) => JobCommand::Cancel,
                    Err(error) => JobCommand::Fail(batch_failure(&error)),
                };
                let _ = transition(&jobs, task.job_id, terminal);
                lock(&cancellations).remove(&task.job_id);
            }
            Command::Shutdown => return,
        }
    }
}

fn batch_failure(error: &BatchRenameError) -> JobFailure {
    let kind = match error {
        BatchRenameError::DestinationExists(_) => JobFailureKind::Conflict,
        BatchRenameError::Partial { .. } => JobFailureKind::Partial,
        _ => JobFailureKind::Io,
    };
    JobFailure::new(kind, error.to_string())
}

fn fail(jobs: &SharedJobManager, job_id: JobId, message: &'static str) {
    let _ = transition(
        jobs,
        job_id,
        JobCommand::Fail(JobFailure::new(JobFailureKind::Internal, message)),
    );
}

fn transition(
    jobs: &SharedJobManager,
    job_id: JobId,
    command: JobCommand,
) -> Result<(), JobManagerError> {
    lock(jobs).transition(job_id, command).map(|_| ())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::os::unix::ffi::OsStringExt;
    use std::{
        fs,
        sync::{Arc, Mutex},
        thread,
        time::{Duration, Instant},
    };

    use floe_core::JobState;
    use tempfile::tempdir;

    use super::*;
    use crate::job_manager::ApplicationJobManager;

    #[test]
    fn phase_12c_batch_rename_model_transforms_templates_regex_and_collisions() {
        let sources = vec![
            BatchRenameSource {
                path: PathBuf::from("/tmp/IMG-one.JPG"),
                date: "2026-08-25".to_owned(),
            },
            BatchRenameSource {
                path: PathBuf::from("/tmp/IMG-two.JPG"),
                date: "2026-08-26".to_owned(),
            },
        ];
        let rule = BatchRenameRule {
            find: "^IMG-(.*)$".to_owned(),
            replace: "$1".to_owned(),
            regex: true,
            prefix: "{date}-".to_owned(),
            suffix: "-{n}".to_owned(),
            sequence_padding: 3,
            case: RenameCase::Lower,
            ..BatchRenameRule::default()
        };
        let (rows, request) = build_batch_rename_preview(&sources, &rule).expect("preview");
        assert_eq!(rows[0].new_name, "2026-08-25-one-001.JPG");
        assert_eq!(rows[1].new_name, "2026-08-26-two-002.JPG");
        assert_eq!(request.pairs().len(), 2);

        let duplicate = BatchRenameRule {
            find: ".*".to_owned(),
            replace: "same".to_owned(),
            regex: true,
            ..BatchRenameRule::default()
        };
        assert!(matches!(
            build_batch_rename_preview(&sources, &duplicate),
            Err(BatchRenameModelError::DuplicateName(_))
        ));
        let raw = BatchRenameSource {
            path: PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/a\xff".to_vec())),
            date: String::new(),
        };
        assert!(matches!(
            build_batch_rename_preview(&[raw, sources[0].clone()], &BatchRenameRule::default()),
            Err(BatchRenameModelError::NonUtf8(_))
        ));
        let text = preview_text(&rows);
        assert!(text.contains("IMG-one.JPG"));
        assert!(text.contains("→"));
    }

    #[test]
    fn phase_12c_batch_rename_ui_preview_is_bounded_and_descriptive() {
        let rows = (0..140)
            .map(|index| BatchRenamePreviewRow {
                source: PathBuf::from(format!("/tmp/{index}")),
                destination: PathBuf::from(format!("/tmp/new-{index}")),
                old_name: index.to_string(),
                new_name: format!("new-{index}"),
            })
            .collect::<Vec<_>>();
        let text = preview_text(&rows);
        assert_eq!(text.lines().count(), 129);
        assert!(text.contains("and 12 more"));
    }

    #[test]
    fn phase_12c_batch_rename_jobs_use_shared_progress_results_and_exact_undo() {
        let root = tempdir().expect("root");
        let first = root.path().join("first");
        let second = root.path().join("second");
        fs::write(&first, b"one").expect("first");
        fs::write(&second, b"two").expect("second");
        let request = BatchRenameRequest::new(vec![
            BatchRenamePair::new(first.clone(), root.path().join("renamed-first")).expect("pair"),
            BatchRenamePair::new(second.clone(), root.path().join("renamed-second")).expect("pair"),
        ])
        .expect("request");
        let jobs = Arc::new(Mutex::new(ApplicationJobManager::new()));
        let executor = BatchRenameExecutor::spawn(Arc::clone(&jobs)).expect("executor");
        let submission = executor.submit(request).expect("submit");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let state = lock(&jobs)
                .record(submission.job_id())
                .map(|record| record.state());
            if state.is_some_and(JobState::is_terminal) {
                assert_eq!(state, Some(JobState::Completed));
                break;
            }
            assert!(Instant::now() < deadline, "batch rename worker timed out");
            thread::sleep(Duration::from_millis(2));
        }
        let outcome = executor.take_result(submission.job_id()).expect("outcome");
        assert_eq!(outcome.completed().len(), 2);
        assert!(outcome.undo_request().is_ok());
        let events = lock(&jobs).drain_events();
        assert!(events.iter().any(|event| {
            event.job_id() == submission.job_id()
                && matches!(event.kind(), floe_core::JobEventKind::Progressed(_))
        }));
    }
}
