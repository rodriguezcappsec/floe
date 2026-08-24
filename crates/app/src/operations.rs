use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    path::Path,
    rc::Rc,
    time::Duration,
};

use adw::prelude::*;
use floe_core::{JobEvent, JobEventKind, JobFailure, JobFailureKind, JobId};
use gtk::glib;

use crate::{
    state::{ApplicationState, TerminalOutcome, TrackedOperation},
    ui::OperationWidgets,
};

const JOB_POLL_INTERVAL: Duration = Duration::from_millis(50);
const TERMINAL_VISIBILITY: Duration = Duration::from_secs(3);

pub struct OperationController {
    window: adw::ApplicationWindow,
    toast_overlay: adw::ToastOverlay,
    widgets: OperationWidgets,
    state: Rc<ApplicationState>,
    active_jobs: RefCell<VecDeque<JobId>>,
    visible_job: Cell<Option<JobId>>,
    retryable_job: Cell<Option<JobId>>,
    indeterminate: Cell<bool>,
    visibility_generation: Rc<Cell<u64>>,
    on_operation_completed: Box<dyn Fn(&Path)>,
}

impl OperationController {
    pub fn new(
        window: adw::ApplicationWindow,
        toast_overlay: adw::ToastOverlay,
        widgets: OperationWidgets,
        state: Rc<ApplicationState>,
        on_operation_completed: impl Fn(&Path) + 'static,
    ) -> Rc<Self> {
        Rc::new(Self {
            window,
            toast_overlay,
            widgets,
            state,
            active_jobs: RefCell::new(VecDeque::new()),
            visible_job: Cell::new(None),
            retryable_job: Cell::new(None),
            indeterminate: Cell::new(false),
            visibility_generation: Rc::new(Cell::new(0)),
            on_operation_completed: Box::new(on_operation_completed),
        })
    }

    pub fn wire(self: &Rc<Self>) {
        let controller = Rc::downgrade(self);
        self.widgets.operation_cancel.connect_clicked(move |_| {
            let Some(controller) = controller.upgrade() else {
                return;
            };
            controller.cancel_visible_operation();
        });
        let controller = Rc::downgrade(self);
        self.widgets.operation_retry.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.retry_terminal_operation();
            }
        });

        let controller = Rc::clone(self);
        glib::timeout_add_local(JOB_POLL_INTERVAL, move || {
            if !controller.window.is_visible() {
                return glib::ControlFlow::Break;
            }
            controller.drain_job_events();
            if controller.indeterminate.get() && controller.widgets.revealer.reveals_child() {
                controller.widgets.operation_progress.pulse();
            }
            glib::ControlFlow::Continue
        });
    }

    fn drain_job_events(&self) {
        for event in self.state.drain_job_events() {
            self.handle_event(&event);
        }
    }

    fn handle_event(&self, event: &JobEvent) {
        match event.kind() {
            JobEventKind::Queued => {
                self.track_active(event.job_id());
                self.show_running(
                    event.job_id(),
                    waiting_detail(self.request(event.job_id())),
                    None,
                );
            }
            JobEventKind::Started | JobEventKind::Resumed => {
                self.track_active(event.job_id());
                self.show_running(
                    event.job_id(),
                    running_detail(self.request(event.job_id())),
                    None,
                );
            }
            JobEventKind::Progressed(progress) => {
                self.track_active(event.job_id());
                let detail = match progress.total() {
                    Some(total) => format!("{} of {total} items", progress.completed()),
                    None => running_detail(self.request(event.job_id())).to_owned(),
                };
                self.show_running(event.job_id(), &detail, progress.fraction());
            }
            JobEventKind::Paused => {
                self.track_active(event.job_id());
                self.show_running(event.job_id(), "Operation paused", None);
            }
            JobEventKind::Completed => self.finish(event.job_id(), TerminalResult::Completed),
            JobEventKind::Cancelled => self.finish(event.job_id(), TerminalResult::Cancelled),
            JobEventKind::Failed(failure) => {
                tracing::warn!(
                    job_id = event.job_id().get(),
                    failure_kind = ?failure.kind(),
                    "filesystem job failed"
                );
                self.finish(event.job_id(), TerminalResult::Failed(failure));
            }
        }
    }

    fn request(&self, job_id: JobId) -> Option<TrackedOperation> {
        self.state.operation_request(job_id)
    }

    fn track_active(&self, job_id: JobId) {
        let mut jobs = self.active_jobs.borrow_mut();
        if !jobs.contains(&job_id) {
            jobs.push_back(job_id);
        }
    }

    fn show_running(&self, job_id: JobId, detail: &str, fraction: Option<f64>) {
        self.hide_retry();
        self.visibility_generation
            .set(self.visibility_generation.get().wrapping_add(1));
        self.visible_job.set(Some(job_id));
        let request = self.request(job_id);
        self.widgets
            .operation_label
            .set_label(&operation_title(request.as_ref()));
        self.widgets.operation_detail.set_label(detail);
        self.widgets
            .operation_cancel
            .set_tooltip_text(Some(&format!(
                "Cancel {}",
                operation_verb(request.as_ref()).to_lowercase()
            )));
        self.widgets.operation_cancel.set_sensitive(true);
        match fraction {
            Some(fraction) => {
                self.indeterminate.set(false);
                self.widgets.operation_progress.set_fraction(fraction);
            }
            None => {
                self.indeterminate.set(true);
                self.widgets.operation_progress.set_fraction(0.0);
                self.widgets.operation_progress.pulse();
            }
        }
        self.widgets.revealer.set_reveal_child(true);
    }

    fn finish(&self, job_id: JobId, result: TerminalResult<'_>) {
        self.active_jobs
            .borrow_mut()
            .retain(|active| *active != job_id);
        let outcome = match &result {
            TerminalResult::Completed => TerminalOutcome::Completed,
            TerminalResult::Cancelled => TerminalOutcome::Cancelled,
            TerminalResult::Failed(_) => TerminalOutcome::Failed,
        };
        self.retryable_job.set(updated_retryable_job(
            self.retryable_job.get(),
            job_id,
            outcome,
        ));
        let request = self.state.finish_operation(job_id, outcome);

        match result {
            TerminalResult::Completed => {
                if let Some(request) = request.as_ref() {
                    for directory in request.affected_directories() {
                        (self.on_operation_completed)(&directory);
                    }
                }
                self.show_terminal(
                    request.as_ref(),
                    completed_title(request.as_ref()),
                    completed_detail(request.as_ref()),
                    true,
                );
                self.show_toast(&completed_toast(request.as_ref()), 4);
            }
            TerminalResult::Cancelled => {
                self.show_terminal(
                    request.as_ref(),
                    "Operation cancelled",
                    "No partial change was kept",
                    false,
                );
                self.show_toast("Operation cancelled", 4);
            }
            TerminalResult::Failed(failure) => {
                self.show_terminal(
                    request.as_ref(),
                    "Operation failed",
                    failure_summary(request.as_ref(), failure),
                    false,
                );
                self.show_toast(&failure_recovery(request.as_ref(), failure), 7);
            }
        }

        if let Some(next_job) = self.active_jobs.borrow().back().copied() {
            let request = self.request(next_job);
            self.show_running(next_job, waiting_detail(request), None);
        } else if let Some(retryable_job) = self.retryable_job.get() {
            self.show_retry(retryable_job);
        } else {
            self.schedule_hide();
        }
    }

    fn show_terminal(
        &self,
        request: Option<&TrackedOperation>,
        title: &str,
        detail: &str,
        succeeded: bool,
    ) {
        self.hide_retry();
        self.visible_job.set(None);
        self.indeterminate.set(false);
        self.widgets.operation_label.set_label(title);
        self.widgets.operation_detail.set_label(detail);
        self.widgets
            .operation_progress
            .set_fraction(if succeeded { 1.0 } else { 0.0 });
        self.widgets.operation_cancel.set_sensitive(false);
        self.widgets
            .operation_cancel
            .set_tooltip_text(Some(&format!("{} finished", operation_verb(request))));
        self.widgets.revealer.set_reveal_child(true);
    }

    fn schedule_hide(&self) {
        let generation = self.visibility_generation.get().wrapping_add(1);
        self.visibility_generation.set(generation);
        let revealer = self.widgets.revealer.clone();
        let visibility_generation = Rc::clone(&self.visibility_generation);
        glib::timeout_add_local_once(TERMINAL_VISIBILITY, move || {
            if visibility_generation.get() == generation {
                revealer.set_reveal_child(false);
            }
        });
    }

    fn cancel_visible_operation(&self) {
        let Some(job_id) = self.visible_job.get() else {
            return;
        };
        match self.state.cancel_operation(job_id) {
            Ok(()) => {
                self.widgets.operation_cancel.set_sensitive(false);
                self.widgets.operation_detail.set_label("Cancelling…");
            }
            Err(error) => self.show_toast(&format!("Could not cancel operation: {error}"), 6),
        }
    }

    fn show_retry(&self, job_id: JobId) {
        self.visibility_generation
            .set(self.visibility_generation.get().wrapping_add(1));
        self.retryable_job.set(Some(job_id));
        self.widgets.operation_retry.set_sensitive(true);
        self.widgets.operation_retry.set_visible(true);
    }

    fn clear_retry(&self) {
        self.retryable_job.set(None);
        self.hide_retry();
    }

    fn hide_retry(&self) {
        self.widgets.operation_retry.set_visible(false);
    }

    fn retry_terminal_operation(&self) {
        let Some(job_id) = self.retryable_job.get() else {
            return;
        };
        self.widgets.operation_retry.set_sensitive(false);
        match self.state.retry_operation(job_id) {
            Ok(_) => {
                self.clear_retry();
                self.widgets.operation_detail.set_label("Retry queued…");
            }
            Err(error) => {
                self.widgets.operation_retry.set_sensitive(true);
                self.show_toast(&format!("Could not retry operation: {error}"), 7);
            }
        }
    }

    fn show_toast(&self, title: &str, timeout: u32) {
        self.toast_overlay
            .add_toast(adw::Toast::builder().title(title).timeout(timeout).build());
    }
}

enum TerminalResult<'a> {
    Completed,
    Cancelled,
    Failed(&'a JobFailure),
}

fn outcome_is_retryable(outcome: TerminalOutcome) -> bool {
    matches!(
        outcome,
        TerminalOutcome::Cancelled | TerminalOutcome::Failed
    )
}

fn updated_retryable_job(
    current: Option<JobId>,
    finished_job: JobId,
    outcome: TerminalOutcome,
) -> Option<JobId> {
    outcome_is_retryable(outcome)
        .then_some(finished_job)
        .or(current)
}

fn operation_title(request: Option<&TrackedOperation>) -> String {
    format!(
        "{} {}",
        operation_verb_ing(request),
        operation_name(request)
    )
}

fn operation_name(request: Option<&TrackedOperation>) -> String {
    request
        .and_then(|request| request.source().file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "item".to_owned())
}

fn operation_verb(request: Option<&TrackedOperation>) -> &'static str {
    match request {
        Some(TrackedOperation::Copy(_)) => "Copy",
        Some(TrackedOperation::Move(_)) => "Move",
        Some(TrackedOperation::Rename(_)) => "Rename",
        Some(TrackedOperation::Trash(_)) => "Move to Trash",
        None => "Operation",
    }
}

fn operation_verb_ing(request: Option<&TrackedOperation>) -> &'static str {
    match request {
        Some(TrackedOperation::Copy(_)) => "Copying",
        Some(TrackedOperation::Move(_)) => "Moving",
        Some(TrackedOperation::Rename(_)) => "Renaming",
        Some(TrackedOperation::Trash(_)) => "Moving to Trash",
        None => "Working on",
    }
}

fn waiting_detail(request: Option<TrackedOperation>) -> &'static str {
    match request {
        Some(TrackedOperation::Copy(_)) => "Waiting to copy…",
        Some(TrackedOperation::Move(_)) => "Waiting to move…",
        Some(TrackedOperation::Rename(_)) => "Waiting to rename…",
        Some(TrackedOperation::Trash(_)) => "Waiting to move to Trash…",
        None => "Waiting…",
    }
}

fn running_detail(request: Option<TrackedOperation>) -> &'static str {
    match request {
        Some(TrackedOperation::Copy(_)) => "Preparing copy…",
        Some(TrackedOperation::Move(_)) => "Moving on this filesystem…",
        Some(TrackedOperation::Rename(_)) => "Renaming…",
        Some(TrackedOperation::Trash(_)) => "Moving to Trash through GIO…",
        None => "Working…",
    }
}

fn completed_title(request: Option<&TrackedOperation>) -> &'static str {
    match request {
        Some(TrackedOperation::Copy(_)) => "Copy complete",
        Some(TrackedOperation::Move(_)) => "Move complete",
        Some(TrackedOperation::Rename(_)) => "Rename complete",
        Some(TrackedOperation::Trash(_)) => "Moved to Trash",
        None => "Operation complete",
    }
}

fn completed_detail(request: Option<&TrackedOperation>) -> &'static str {
    match request {
        Some(TrackedOperation::Copy(_)) => "Copied successfully",
        Some(TrackedOperation::Move(_)) => "Moved successfully",
        Some(TrackedOperation::Rename(_)) => "Renamed successfully",
        Some(TrackedOperation::Trash(_)) => "Item is available in Trash",
        None => "Completed successfully",
    }
}

fn completed_toast(request: Option<&TrackedOperation>) -> String {
    match request {
        Some(TrackedOperation::Trash(_)) => {
            format!("Moved {} to Trash", operation_name(request))
        }
        Some(TrackedOperation::Copy(_)) => format!("Copied {}", operation_name(request)),
        Some(TrackedOperation::Move(_)) => format!("Moved {}", operation_name(request)),
        Some(TrackedOperation::Rename(_)) => format!("Renamed {}", operation_name(request)),
        None => "Operation completed".to_owned(),
    }
}

fn failure_summary(request: Option<&TrackedOperation>, failure: &JobFailure) -> &'static str {
    match failure.kind() {
        JobFailureKind::Conflict => "The destination already exists",
        JobFailureKind::PermissionDenied => "Permission was denied",
        JobFailureKind::Unsupported if matches!(request, Some(TrackedOperation::Move(_))) => {
            "Cross-filesystem move is not supported yet"
        }
        JobFailureKind::Unsupported if matches!(request, Some(TrackedOperation::Trash(_))) => {
            "This location does not support Trash"
        }
        JobFailureKind::Unsupported => "This operation is not supported",
        JobFailureKind::Io => "A filesystem error interrupted the operation",
        JobFailureKind::Internal => "The filesystem service could not continue",
    }
}

fn failure_recovery(request: Option<&TrackedOperation>, failure: &JobFailure) -> String {
    let name = operation_name(request);
    match failure.kind() {
        JobFailureKind::Conflict if matches!(request, Some(TrackedOperation::Rename(_))) => {
            format!("Choose a different name for {name}, then try again.")
        }
        JobFailureKind::Conflict => {
            format!("Choose another destination, or rename/remove {name}, then try again.")
        }
        JobFailureKind::PermissionDenied => {
            format!("Could not change {name}. Check folder permissions and try again.")
        }
        JobFailureKind::Unsupported if matches!(request, Some(TrackedOperation::Move(_))) => {
            "Choose a destination on the same filesystem, then try the move again.".to_owned()
        }
        JobFailureKind::Unsupported if matches!(request, Some(TrackedOperation::Trash(_))) => {
            format!(
                "The filesystem cannot trash {name}. Leave it unchanged or use another location."
            )
        }
        JobFailureKind::Unsupported => format!("Could not change {name}: unsupported operation."),
        JobFailureKind::Io if matches!(request, Some(TrackedOperation::Trash(_))) => {
            format!("Could not move {name} to Trash. It was not silently deleted; try again.")
        }
        JobFailureKind::Io | JobFailureKind::Internal => {
            format!("Could not change {name}. Check the destination and try again.")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{num::NonZeroU64, path::PathBuf};

    use floe_core::{ConflictPolicy, JobFailure, MoveRequest, RenameRequest};

    use super::*;
    use crate::trash_executor::TrashRequest;

    #[test]
    fn phase_5b_only_failed_and_cancelled_operations_are_retryable() {
        assert!(!outcome_is_retryable(TerminalOutcome::Completed));
        assert!(outcome_is_retryable(TerminalOutcome::Cancelled));
        assert!(outcome_is_retryable(TerminalOutcome::Failed));
    }

    #[test]
    fn phase_5b_completed_jobs_do_not_discard_a_pending_retry() {
        let failed_job = JobId::new(NonZeroU64::new(41).expect("non-zero fixture job id"));
        let completed_job = JobId::new(NonZeroU64::new(42).expect("non-zero fixture job id"));

        assert_eq!(
            updated_retryable_job(Some(failed_job), completed_job, TerminalOutcome::Completed,),
            Some(failed_job)
        );
        assert_eq!(
            updated_retryable_job(None, completed_job, TerminalOutcome::Failed),
            Some(completed_job)
        );
    }

    #[test]
    fn phase_4d_feedback_uses_operation_specific_titles_and_conflict_recovery() {
        let rename = TrackedOperation::Rename(RenameRequest::new(
            PathBuf::from("/source/notes.txt"),
            "renamed.txt",
            ConflictPolicy::FailIfExists,
        ));
        let failure = JobFailure::new(JobFailureKind::Conflict, "fixture conflict");

        assert_eq!(operation_title(Some(&rename)), "Renaming notes.txt");
        assert_eq!(
            failure_recovery(Some(&rename), &failure),
            "Choose a different name for notes.txt, then try again."
        );
    }

    #[test]
    fn phase_4d_feedback_explains_cross_filesystem_move_recovery() {
        let moved = TrackedOperation::Move(MoveRequest::new(
            PathBuf::from("/source/photos"),
            PathBuf::from("/destination/photos"),
            ConflictPolicy::FailIfExists,
        ));
        let failure = JobFailure::new(JobFailureKind::Unsupported, "fixture cross-device");

        assert_eq!(operation_title(Some(&moved)), "Moving photos");
        assert_eq!(
            failure_summary(Some(&moved), &failure),
            "Cross-filesystem move is not supported yet"
        );
        assert_eq!(
            failure_recovery(Some(&moved), &failure),
            "Choose a destination on the same filesystem, then try the move again."
        );
    }

    #[test]
    fn phase_4e_feedback_explains_unsupported_trash_without_implying_deletion() {
        let trashed = TrackedOperation::Trash(
            TrashRequest::new(PathBuf::from("/source/notes.txt")).expect("valid trash request"),
        );
        let failure = JobFailure::new(JobFailureKind::Unsupported, "fixture unsupported trash");

        assert_eq!(operation_title(Some(&trashed)), "Moving to Trash notes.txt");
        assert_eq!(
            failure_summary(Some(&trashed), &failure),
            "This location does not support Trash"
        );
        assert_eq!(
            failure_recovery(Some(&trashed), &failure),
            "The filesystem cannot trash notes.txt. Leave it unchanged or use another location."
        );
    }

    #[test]
    fn phase_4f_feedback_uses_explicit_trash_progress_and_completion_wording() {
        let trashed = TrackedOperation::Trash(
            TrashRequest::new(PathBuf::from("/source/notes.txt")).expect("valid trash request"),
        );

        assert_eq!(operation_verb(Some(&trashed)), "Move to Trash");
        assert_eq!(
            waiting_detail(Some(trashed.clone())),
            "Waiting to move to Trash…"
        );
        assert_eq!(
            running_detail(Some(trashed.clone())),
            "Moving to Trash through GIO…"
        );
        assert_eq!(completed_title(Some(&trashed)), "Moved to Trash");
        assert_eq!(completed_toast(Some(&trashed)), "Moved notes.txt to Trash");
    }
}
