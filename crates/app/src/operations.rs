use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    path::Path,
    rc::Rc,
    time::Duration,
};

use adw::prelude::*;
use floe_core::{CopyRequest, JobEvent, JobEventKind, JobFailure, JobFailureKind, JobId};
use gtk::glib;

use crate::{state::ApplicationState, ui::OperationWidgets};

const JOB_POLL_INTERVAL: Duration = Duration::from_millis(50);
const TERMINAL_VISIBILITY: Duration = Duration::from_secs(3);

pub struct OperationController {
    window: adw::ApplicationWindow,
    toast_overlay: adw::ToastOverlay,
    widgets: OperationWidgets,
    state: Rc<ApplicationState>,
    active_jobs: RefCell<VecDeque<JobId>>,
    visible_job: Cell<Option<JobId>>,
    indeterminate: Cell<bool>,
    visibility_generation: Rc<Cell<u64>>,
    on_copy_completed: Box<dyn Fn(&Path)>,
}

impl OperationController {
    pub fn new(
        window: adw::ApplicationWindow,
        toast_overlay: adw::ToastOverlay,
        widgets: OperationWidgets,
        state: Rc<ApplicationState>,
        on_copy_completed: impl Fn(&Path) + 'static,
    ) -> Rc<Self> {
        Rc::new(Self {
            window,
            toast_overlay,
            widgets,
            state,
            active_jobs: RefCell::new(VecDeque::new()),
            visible_job: Cell::new(None),
            indeterminate: Cell::new(false),
            visibility_generation: Rc::new(Cell::new(0)),
            on_copy_completed: Box::new(on_copy_completed),
        })
    }

    pub fn wire(self: &Rc<Self>) {
        let controller = Rc::downgrade(self);
        self.widgets.operation_cancel.connect_clicked(move |_| {
            let Some(controller) = controller.upgrade() else {
                return;
            };
            controller.cancel_visible_copy();
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
                self.show_running(event.job_id(), "Waiting to copy…", None);
            }
            JobEventKind::Started | JobEventKind::Resumed => {
                self.track_active(event.job_id());
                self.show_running(event.job_id(), "Preparing copy…", None);
            }
            JobEventKind::Progressed(progress) => {
                self.track_active(event.job_id());
                let detail = match progress.total() {
                    Some(total) => format!("{} of {total} items", progress.completed()),
                    None => "Copying…".to_owned(),
                };
                self.show_running(event.job_id(), &detail, progress.fraction());
            }
            JobEventKind::Paused => {
                self.track_active(event.job_id());
                self.show_running(event.job_id(), "Copy paused", None);
            }
            JobEventKind::Completed => self.finish(event.job_id(), TerminalResult::Completed),
            JobEventKind::Cancelled => self.finish(event.job_id(), TerminalResult::Cancelled),
            JobEventKind::Failed(failure) => {
                tracing::warn!(
                    job_id = event.job_id().get(),
                    failure_kind = ?failure.kind(),
                    "copy job failed"
                );
                self.finish(event.job_id(), TerminalResult::Failed(failure));
            }
        }
    }

    fn track_active(&self, job_id: JobId) {
        let mut jobs = self.active_jobs.borrow_mut();
        if !jobs.contains(&job_id) {
            jobs.push_back(job_id);
        }
    }

    fn show_running(&self, job_id: JobId, detail: &str, fraction: Option<f64>) {
        self.visibility_generation
            .set(self.visibility_generation.get().wrapping_add(1));
        self.visible_job.set(Some(job_id));
        self.widgets
            .operation_label
            .set_label(&operation_title(self.state.copy_request(job_id).as_ref()));
        self.widgets.operation_detail.set_label(detail);
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
        let request = self.state.finish_copy(job_id);

        match result {
            TerminalResult::Completed => {
                if let Some(destination) = request
                    .as_ref()
                    .and_then(|request| request.destination().parent())
                {
                    (self.on_copy_completed)(destination);
                }
                self.show_terminal(request.as_ref(), "Copy complete", "Copied successfully");
                self.show_toast(&format!("Copied {}", operation_name(request.as_ref())), 4);
            }
            TerminalResult::Cancelled => {
                self.show_terminal(
                    request.as_ref(),
                    "Copy cancelled",
                    "No partial copy was kept",
                );
                self.show_toast("Copy cancelled", 4);
            }
            TerminalResult::Failed(failure) => {
                self.show_terminal(request.as_ref(), "Copy failed", failure_summary(failure));
                self.show_toast(&failure_recovery(request.as_ref(), failure), 7);
            }
        }

        if let Some(next_job) = self.active_jobs.borrow().back().copied() {
            self.show_running(next_job, "Copying…", None);
        } else {
            self.schedule_hide();
        }
    }

    fn show_terminal(&self, request: Option<&CopyRequest>, title: &str, detail: &str) {
        self.visible_job.set(None);
        self.indeterminate.set(false);
        self.widgets
            .operation_label
            .set_label(&format!("{title}: {}", operation_name(request)));
        self.widgets.operation_detail.set_label(detail);
        self.widgets
            .operation_progress
            .set_fraction(if title == "Copy complete" { 1.0 } else { 0.0 });
        self.widgets.operation_cancel.set_sensitive(false);
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

    fn cancel_visible_copy(&self) {
        let Some(job_id) = self.visible_job.get() else {
            return;
        };
        match self.state.cancel_copy(job_id) {
            Ok(()) => {
                self.widgets.operation_cancel.set_sensitive(false);
                self.widgets.operation_detail.set_label("Cancelling…");
            }
            Err(error) => self.show_toast(&format!("Could not cancel copy: {error}"), 6),
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

fn operation_title(request: Option<&CopyRequest>) -> String {
    format!("Copying {}", operation_name(request))
}

fn operation_name(request: Option<&CopyRequest>) -> String {
    request
        .and_then(|request| request.source().file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "item".to_owned())
}

fn failure_summary(failure: &JobFailure) -> &'static str {
    match failure.kind() {
        JobFailureKind::Conflict => "The destination conflicts with this copy",
        JobFailureKind::PermissionDenied => "Permission was denied",
        JobFailureKind::Unsupported => "This file type is not supported",
        JobFailureKind::Io => "A filesystem error interrupted the copy",
        JobFailureKind::Internal => "The copy service could not continue",
    }
}

fn failure_recovery(request: Option<&CopyRequest>, failure: &JobFailure) -> String {
    let name = operation_name(request);
    match failure.kind() {
        JobFailureKind::Conflict => {
            format!("Choose another destination, or rename/remove {name}, then paste again.")
        }
        JobFailureKind::PermissionDenied => {
            format!("Could not copy {name}. Check folder permissions and try again.")
        }
        JobFailureKind::Unsupported => {
            format!("Could not copy {name} because its file type is unsupported.")
        }
        JobFailureKind::Io | JobFailureKind::Internal => {
            format!("Could not copy {name}. Check the destination and try again.")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use floe_core::{ConflictPolicy, SymlinkPolicy};

    use super::*;

    #[test]
    fn copy_interaction_feedback_uses_filename_and_recovery_action() {
        let request = CopyRequest::new(
            PathBuf::from("/source/notes.txt"),
            PathBuf::from("/destination/notes.txt"),
            ConflictPolicy::FailIfExists,
            SymlinkPolicy::Preserve,
        );
        let failure = JobFailure::new(JobFailureKind::Conflict, "fixture conflict");

        assert_eq!(operation_title(Some(&request)), "Copying notes.txt");
        assert_eq!(
            failure_recovery(Some(&request), &failure),
            "Choose another destination, or rename/remove notes.txt, then paste again."
        );
    }

    #[test]
    fn copy_interaction_unknown_job_uses_safe_generic_label() {
        assert_eq!(operation_name(None), "item");
    }
}
