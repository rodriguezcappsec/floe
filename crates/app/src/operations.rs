use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};

use adw::prelude::*;
use floe_core::{
    ArchiveOutcome, DestructiveScope, FileIdentity, JobEvent, JobEventKind, JobFailure,
    JobFailureKind, JobId, JobProgress, ProgressUnit,
};
use gtk::{gio, glib};

use crate::{
    archive_ui::archive_failure_text,
    checksum_executor::ChecksumOutcome,
    checksum_ui::present_checksum,
    guardrail_controller::GuardrailAuthorizationItem,
    guardrail_preflight::PreflightEnvironment,
    guardrail_ui::review_and_authorize,
    integrity_executor::IntegrityOutcome,
    integrity_ui::{build_integrity_results_dialog, integrity_title, present_integrity},
    operation_control::{BatchId, BatchSnapshot, BatchStatus, TransferEstimate, TransferTelemetry},
    operation_hub::{OperationEventHub, WindowRuntimeId},
    operation_recovery::{RecoveryPathStatus, RecoveryStoreHealth},
    operation_reveal::OperationRevealRequest,
    state::{
        ApplicationState, ConflictDecision, ConflictResolution, TerminalOperation, TerminalOutcome,
        TrackedOperation, VerifiedCopyCompletion, validate_rename_name,
    },
    ui::{
        OperationHistoryItem, OperationWidgets, RecoveryDialogItem, build_checksum_results_dialog,
        build_conflict_dialog, build_operation_history_dialog, build_recovery_dialog,
        build_verified_copy_result_dialog,
    },
    undo_history::{UndoHistoryAction, UndoHistoryHealth, UndoHistoryRecord, UndoHistoryState},
    verified_copy_executor::{VerifiedCopyResult, present_verified_copy},
};

const JOB_POLL_INTERVAL: Duration = Duration::from_millis(50);
const TERMINAL_VISIBILITY: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalAction {
    ResolveConflict(JobId),
    Retry(JobId),
}

#[derive(Debug, Default)]
struct ConflictInteractions {
    pending: VecDeque<JobId>,
    dialog_job: Option<JobId>,
}

pub struct OperationCallbacks {
    on_operation_completed: Box<dyn Fn(&Path)>,
    on_operation_result: Box<dyn Fn(OperationRevealRequest)>,
    reveal_path: Box<dyn Fn(PathBuf)>,
    on_background_completed: Box<dyn Fn(JobId)>,
}

impl OperationCallbacks {
    pub fn new(
        on_operation_completed: impl Fn(&Path) + 'static,
        on_operation_result: impl Fn(OperationRevealRequest) + 'static,
        reveal_path: impl Fn(PathBuf) + 'static,
        on_background_completed: impl Fn(JobId) + 'static,
    ) -> Self {
        Self {
            on_operation_completed: Box::new(on_operation_completed),
            on_operation_result: Box::new(on_operation_result),
            reveal_path: Box::new(reveal_path),
            on_background_completed: Box::new(on_background_completed),
        }
    }
}

#[derive(Debug)]
struct JobTelemetry {
    started_at: Instant,
    sampler: TransferTelemetry,
}

impl JobTelemetry {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            sampler: TransferTelemetry::default(),
        }
    }
}

impl ConflictInteractions {
    fn enqueue(&mut self, job_id: JobId) {
        if !self.pending.contains(&job_id) {
            self.pending.push_back(job_id);
        }
    }

    fn current(&self) -> Option<JobId> {
        self.pending.front().copied()
    }

    fn begin_dialog(&mut self, job_id: JobId) -> bool {
        if self.dialog_job.is_some() || !self.pending.contains(&job_id) {
            return false;
        }
        self.dialog_job = Some(job_id);
        true
    }

    fn dismiss_dialog(&mut self, job_id: JobId) -> bool {
        if self.dialog_job != Some(job_id) {
            return false;
        }
        self.dialog_job = None;
        true
    }

    fn resolve(&mut self, job_id: JobId) {
        self.pending.retain(|pending| *pending != job_id);
        if self.dialog_job == Some(job_id) {
            self.dialog_job = None;
        }
    }
}

pub struct OperationController {
    window: adw::ApplicationWindow,
    toast_overlay: adw::ToastOverlay,
    widgets: OperationWidgets,
    state: Rc<ApplicationState>,
    event_hub: Rc<OperationEventHub>,
    window_runtime_id: WindowRuntimeId,
    active_jobs: RefCell<VecDeque<JobId>>,
    visible_job: Cell<Option<JobId>>,
    visible_batch: Cell<Option<BatchId>>,
    retryable_job: Cell<Option<JobId>>,
    conflicts: RefCell<ConflictInteractions>,
    telemetry: RefCell<HashMap<JobId, JobTelemetry>>,
    indeterminate: Cell<bool>,
    visibility_generation: Rc<Cell<u64>>,
    guardrail_environment: Box<dyn Fn() -> PreflightEnvironment>,
    on_operation_completed: Box<dyn Fn(&Path)>,
    on_operation_result: Box<dyn Fn(OperationRevealRequest)>,
    reveal_path: Box<dyn Fn(PathBuf)>,
    on_background_completed: Box<dyn Fn(JobId)>,
}

impl OperationController {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        window: adw::ApplicationWindow,
        toast_overlay: adw::ToastOverlay,
        widgets: OperationWidgets,
        state: Rc<ApplicationState>,
        event_hub: Rc<OperationEventHub>,
        window_runtime_id: WindowRuntimeId,
        guardrail_environment: impl Fn() -> PreflightEnvironment + 'static,
        callbacks: OperationCallbacks,
    ) -> Rc<Self> {
        let OperationCallbacks {
            on_operation_completed,
            on_operation_result,
            reveal_path,
            on_background_completed,
        } = callbacks;
        Rc::new(Self {
            window,
            toast_overlay,
            widgets,
            state,
            event_hub,
            window_runtime_id,
            active_jobs: RefCell::new(VecDeque::new()),
            visible_job: Cell::new(None),
            visible_batch: Cell::new(None),
            retryable_job: Cell::new(None),
            conflicts: RefCell::new(ConflictInteractions::default()),
            telemetry: RefCell::new(HashMap::new()),
            indeterminate: Cell::new(false),
            visibility_generation: Rc::new(Cell::new(0)),
            guardrail_environment: Box::new(guardrail_environment),
            on_operation_completed,
            on_operation_result,
            reveal_path,
            on_background_completed,
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
                controller.activate_terminal_action();
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.operation_pause.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.toggle_visible_batch_pause();
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.operation_history.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.present_operation_history();
            }
        });
        let history_action = gio::SimpleAction::new("operation-history", None);
        let controller = Rc::downgrade(self);
        history_action.connect_activate(move |_, _| {
            if let Some(controller) = controller.upgrade() {
                controller.present_operation_history();
            }
        });
        self.window.add_action(&history_action);

        let recovery_action = gio::SimpleAction::new("recovery-center", None);
        let controller = Rc::downgrade(self);
        recovery_action.connect_activate(move |_, _| {
            if let Some(controller) = controller.upgrade() {
                controller.present_recovery_center();
            }
        });
        self.window.add_action(&recovery_action);

        match self.state.recovery_store_health() {
            RecoveryStoreHealth::Ready { pending_records } if pending_records > 0 => {
                self.toast_overlay.add_toast(
                    adw::Toast::builder()
                        .title(format!(
                            "{pending_records} interrupted operation{} need review",
                            if pending_records == 1 { "" } else { "s" }
                        ))
                        .button_label("Review")
                        .action_name("win.recovery-center")
                        .timeout(0)
                        .build(),
                );
            }
            RecoveryStoreHealth::Blocked { .. } => {
                self.toast_overlay.add_toast(
                    adw::Toast::builder()
                        .title("Operation recovery is blocked; copy, move, rename, and create will not run")
                        .button_label("Review")
                        .action_name("win.recovery-center")
                        .timeout(0)
                        .build(),
                );
            }
            RecoveryStoreHealth::Ready { .. } => {}
        }
        match self.state.undo_history_health() {
            UndoHistoryHealth::Ready { review, .. } if review > 0 => {
                self.toast_overlay.add_toast(
                    adw::Toast::builder()
                        .title("Interrupted Undo/Redo needs review")
                        .button_label("Review")
                        .action_name("win.recovery-center")
                        .timeout(0)
                        .build(),
                );
            }
            UndoHistoryHealth::Blocked { .. } => {
                self.toast_overlay.add_toast(
                    adw::Toast::builder()
                        .title(
                            "Durable Undo/Redo is blocked; protected file operations will not run",
                        )
                        .button_label("Review")
                        .action_name("win.recovery-center")
                        .timeout(0)
                        .build(),
                );
            }
            UndoHistoryHealth::Ready { .. } => {}
        }

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

    fn present_recovery_center(self: &Rc<Self>) {
        if let RecoveryStoreHealth::Blocked { reason } = self.state.recovery_store_health() {
            let dialog = adw::AlertDialog::builder()
                .heading("Operation recovery is blocked")
                .body(format!(
                    "Floe cannot safely journal new copy, move, rename, or create work: {reason}\n\nReset only if you accept discarding the unreadable recovery records. Existing files are not removed."
                ))
                .default_response("cancel")
                .close_response("cancel")
                .build();
            dialog.add_responses(&[("cancel", "Cancel"), ("reset", "Reset Recovery Store")]);
            dialog.set_response_appearance("reset", adw::ResponseAppearance::Destructive);
            let controller = Rc::downgrade(self);
            dialog.connect_response(None, move |dialog, response| {
                if response == "reset" {
                    if let Some(controller) = controller.upgrade() {
                        match controller.state.reset_blocked_recovery_store() {
                            Ok(()) => controller.show_toast(
                                "Recovery store reset; protected operations are available again",
                                6,
                            ),
                            Err(error) => controller
                                .show_toast(&format!("Could not reset recovery store: {error}"), 0),
                        }
                    }
                }
                dialog.close();
            });
            dialog.present(Some(&self.window));
            return;
        }
        if let UndoHistoryHealth::Blocked { reason } = self.state.undo_history_health() {
            let dialog = adw::AlertDialog::builder()
                .heading("Durable Undo/Redo history blocked")
                .body(format!(
                    "Floe cannot safely record reversible Copy, Move, Rename, or Create work: {reason}\n\nReset only if you accept discarding unreadable Undo/Redo records. Reset changes no source, destination, or Trash item."
                ))
                .default_response("cancel")
                .close_response("cancel")
                .build();
            dialog.add_responses(&[("cancel", "Cancel"), ("reset", "Reset Undo History")]);
            dialog.set_response_appearance("reset", adw::ResponseAppearance::Destructive);
            let controller = Rc::downgrade(self);
            dialog.connect_response(None, move |dialog, response| {
                if response == "reset"
                    && let Some(controller) = controller.upgrade()
                {
                    match controller.state.reset_undo_history() {
                        Ok(()) => controller.show_toast(
                            "Undo/Redo history reset; protected operations are available again",
                            6,
                        ),
                        Err(error) => controller
                            .show_toast(&format!("Could not reset Undo history: {error}"), 7),
                    }
                }
                dialog.close();
            });
            dialog.present(Some(&self.window));
            return;
        }

        let reviews = match self.state.recovery_reviews() {
            Ok(reviews) => reviews,
            Err(error) => {
                self.show_toast(&format!("Could not read recovery records: {error}"), 0);
                return;
            }
        };
        let recovery_count = reviews.len();
        let mut items = reviews
            .iter()
            .map(|review| {
                let record = review.record();
                let source_state = recovery_path_status(review.source_status());
                let destination_state = recovery_path_status(Some(review.destination_status()));
                RecoveryDialogItem {
                    id: record.id(),
                    title: format!("Interrupted {}", record.kind().label()),
                    detail: format!(
                        "Source: {source_state} • Destination: {destination_state}\n{} → {}",
                        record
                            .source()
                            .map(|path| path.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "Created item".to_owned()),
                        record.destination().to_string_lossy()
                    ),
                    can_retry: review.can_retry(),
                    can_resolve: review.can_resolve(),
                    source: record.source().map(Path::to_path_buf),
                    destination: record.destination().to_path_buf(),
                }
            })
            .collect::<Vec<_>>();
        match self.state.persistent_undo_reviews() {
            Ok(undo_reviews) => {
                items.extend(undo_reviews.into_iter().map(|record| RecoveryDialogItem {
                    id: record.id(),
                    title: format!("Interrupted {} Undo/Redo", record.recipe().label()),
                    detail: format!(
                        "State: {:?}\nReview exact paths before resolving this record.\n{} → {}",
                        record.state(),
                        record
                            .recipe()
                            .source()
                            .map(|path| path.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "Created item".to_owned()),
                        record.recipe().destination().to_string_lossy()
                    ),
                    can_retry: false,
                    can_resolve: true,
                    source: record.recipe().source().map(Path::to_path_buf),
                    destination: record.recipe().destination().to_path_buf(),
                }))
            }
            Err(error) => self.show_toast(
                &format!("Could not read interrupted Undo/Redo records: {error}"),
                7,
            ),
        }
        let widgets = build_recovery_dialog(&items);
        for (index, item) in items.iter().enumerate() {
            let id = item.id;
            let controller = Rc::downgrade(self);
            let dialog = widgets.dialog.clone();
            widgets.retry_buttons[index].connect_clicked(move |_| {
                if let Some(controller) = controller.upgrade() {
                    match controller.state.retry_recovery_record(id) {
                        Ok(_) => {
                            controller.show_toast("Interrupted operation queued safely", 5);
                            dialog.close();
                        }
                        Err(error) => {
                            controller.show_toast(&format!("Could not retry operation: {error}"), 7)
                        }
                    }
                }
            });

            if let Some(source) = item.source.clone() {
                let controller = Rc::downgrade(self);
                let dialog = widgets.dialog.clone();
                widgets.reveal_source_buttons[index].connect_clicked(move |_| {
                    if let Some(controller) = controller.upgrade() {
                        (controller.reveal_path)(source.clone());
                        dialog.close();
                    }
                });
            }
            let destination = item.destination.clone();
            let controller = Rc::downgrade(self);
            let dialog = widgets.dialog.clone();
            widgets.reveal_destination_buttons[index].connect_clicked(move |_| {
                if let Some(controller) = controller.upgrade() {
                    (controller.reveal_path)(destination.clone());
                    dialog.close();
                }
            });

            let controller = Rc::downgrade(self);
            let dialog = widgets.dialog.clone();
            let is_undo_review = index >= recovery_count;
            widgets.resolve_buttons[index].connect_clicked(move |_| {
                if let Some(controller) = controller.upgrade() {
                    let result = if is_undo_review {
                        controller
                            .state
                            .resolve_undo_history_review(id)
                            .map_err(|error| error.to_string())
                    } else {
                        controller
                            .state
                            .resolve_recovery_record(id)
                            .map_err(|error| error.to_string())
                    };
                    match result {
                        Ok(()) => {
                            controller
                                .show_toast("Recovery record resolved; no files were changed", 5);
                            dialog.close();
                        }
                        Err(error) => controller
                            .show_toast(&format!("Could not resolve recovery record: {error}"), 7),
                    }
                }
            });
        }
        widgets.dialog.present(Some(&self.window));
    }

    fn review_guardrail(
        &self,
        scopes: Vec<DestructiveScope>,
        on_authorized: impl FnOnce(Vec<GuardrailAuthorizationItem>) + 'static,
    ) {
        review_and_authorize(
            &self.window,
            Rc::clone(&self.state),
            scopes,
            (self.guardrail_environment)(),
            on_authorized,
        );
    }

    fn resolve_conflict_with_review(
        self: &Rc<Self>,
        job_id: JobId,
        decision: ConflictDecision,
        on_resolved: impl FnOnce(Result<ConflictResolution, crate::state::CopyInteractionError>)
        + 'static,
    ) {
        let scope = match self.state.conflict_guardrail_scope(job_id, &decision) {
            Ok(scope) => scope,
            Err(error) => {
                on_resolved(Err(error));
                return;
            }
        };
        let Some(scope) = scope else {
            on_resolved(self.state.resolve_conflict(job_id, decision));
            return;
        };
        let state = Rc::clone(&self.state);
        let weak_controller = Rc::downgrade(self);
        self.review_guardrail(vec![scope], move |mut authorizations| {
            let Some(controller) = weak_controller.upgrade() else {
                return;
            };
            let Some(authorization) = authorizations.pop() else {
                controller.show_toast("Could not authorize conflict retry", 7);
                return;
            };
            on_resolved(state.resolve_conflict_authorized(job_id, decision, authorization));
        });
    }

    fn confirm_replace_conflict(
        self: &Rc<Self>,
        job_id: JobId,
        source_identity: FileIdentity,
        destination_identity: FileIdentity,
        replace_all: bool,
        conflict_dialog: glib::WeakRef<adw::Dialog>,
    ) {
        let scope = if replace_all {
            "Later compatible conflicts in only this batch will be replaced after Floe captures fresh identities for each item."
        } else {
            "Only this exact conflict will be replaced."
        };
        let dialog = adw::AlertDialog::builder()
            .heading(if replace_all {
                "Replace compatible batch conflicts?"
            } else {
                "Replace the existing item?"
            })
            .body(format!(
                "{scope}\n\nFloe rechecks the incoming and existing identities immediately before an atomic exchange. The old destination is retained in a private, bounded backup for Undo. If that private backup area is full or either item changes, replacement stops without overwriting it."
            ))
            .default_response("cancel")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[
            ("cancel", "Cancel"),
            (
                "replace",
                if replace_all {
                    "Replace All"
                } else {
                    "Replace"
                },
            ),
        ]);
        dialog.set_response_appearance("replace", adw::ResponseAppearance::Destructive);

        let controller = Rc::downgrade(self);
        dialog.connect_response(None, move |dialog, response| {
            dialog.close();
            if response != "replace" {
                return;
            }
            let Some(controller) = controller.upgrade() else {
                return;
            };
            let decision = if replace_all {
                ConflictDecision::ReplaceAll {
                    source_identity,
                    destination_identity,
                }
            } else {
                ConflictDecision::Replace {
                    source_identity,
                    destination_identity,
                }
            };
            let weak_controller = Rc::downgrade(&controller);
            let conflict_dialog = conflict_dialog.clone();
            controller.resolve_conflict_with_review(job_id, decision, move |result| {
                let Some(controller) = weak_controller.upgrade() else {
                    return;
                };
                match result {
                    Ok(ConflictResolution::Retried(submission)) => {
                        controller.conflicts.borrow_mut().resolve(job_id);
                        if let Some(dialog) = conflict_dialog.upgrade() {
                            dialog.close();
                        }
                        controller.track_active(submission.job_id());
                        controller.show_running(
                            submission.job_id(),
                            "Safe replacement queued…",
                            None,
                        );
                    }
                    Ok(ConflictResolution::KeptExisting) => {
                        controller.show_toast("Could not submit safe replacement", 7);
                    }
                    Err(error) => {
                        controller.show_toast(&format!("Could not replace item: {error}"), 7);
                    }
                }
            });
        });
        dialog.present(Some(&self.window));
    }

    fn drain_job_events(self: &Rc<Self>) {
        for event in self.event_hub.drain_for(self.window_runtime_id) {
            self.handle_event(&event);
        }
    }

    fn handle_event(self: &Rc<Self>, event: &JobEvent) {
        match event.kind() {
            JobEventKind::Queued => {
                self.track_active(event.job_id());
                self.telemetry
                    .borrow_mut()
                    .entry(event.job_id())
                    .or_insert_with(JobTelemetry::new);
                self.show_running(
                    event.job_id(),
                    waiting_detail(self.request(event.job_id())),
                    None,
                );
            }
            JobEventKind::Started | JobEventKind::Resumed => {
                self.track_active(event.job_id());
                self.telemetry
                    .borrow_mut()
                    .entry(event.job_id())
                    .or_insert_with(JobTelemetry::new);
                self.show_running(
                    event.job_id(),
                    running_detail(self.request(event.job_id())),
                    None,
                );
            }
            JobEventKind::Progressed(progress) => {
                self.track_active(event.job_id());
                let detail = self.progress_detail(event.job_id(), *progress);
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

    fn progress_detail(&self, job_id: JobId, progress: JobProgress) -> String {
        let mut telemetry = self.telemetry.borrow_mut();
        let sample = telemetry.entry(job_id).or_insert_with(JobTelemetry::new);
        let estimate = sample
            .sampler
            .observe(sample.started_at.elapsed(), progress);
        progress_detail(progress, estimate)
    }

    fn update_batch_controls(&self, job_id: JobId) {
        let Some(batch_id) = self.state.batch_for_job(job_id) else {
            self.visible_batch.set(None);
            self.widgets.operation_pause.set_visible(false);
            return;
        };
        self.visible_batch.set(Some(batch_id));
        let Some(snapshot) = self.state.batch_snapshot(batch_id) else {
            self.visible_batch.set(None);
            self.widgets.operation_pause.set_visible(false);
            return;
        };
        if snapshot.status().is_terminal() {
            self.widgets.operation_pause.set_visible(false);
            return;
        }
        let resuming = matches!(
            snapshot.status(),
            BatchStatus::Paused | BatchStatus::Pausing
        );
        self.widgets.operation_pause.set_label(if resuming {
            "Resume"
        } else {
            "Pause after current"
        });
        self.widgets
            .operation_pause
            .set_tooltip_text(Some(if resuming {
                "Resume this batch"
            } else {
                "Pause this batch after the current item finishes"
            }));
        self.widgets.operation_pause.set_visible(true);
        self.widgets.operation_pause.set_sensitive(true);
    }

    fn show_running(&self, job_id: JobId, detail: &str, fraction: Option<f64>) {
        self.hide_retry();
        self.visibility_generation
            .set(self.visibility_generation.get().wrapping_add(1));
        self.visible_job.set(Some(job_id));
        let request = self.request(job_id);
        let permission_operation = self.state.is_permission_operation(job_id);
        let checksum_operation = self.state.is_checksum_operation(job_id);
        let integrity_operation = self.state.is_integrity_operation(job_id);
        let verified_copy_operation = self.state.is_verified_copy_operation(job_id);
        let verified_usb_operation = self.state.is_verified_usb_copy_operation(job_id);
        let archive_operation = self.state.is_archive_operation(job_id);
        let batch_rename_operation = self.state.is_batch_rename_operation(job_id);
        let title = if verified_usb_operation {
            "Verified removable transfer".to_owned()
        } else if verified_copy_operation {
            "Copying and verifying".to_owned()
        } else if integrity_operation {
            integrity_title(self.state.integrity_request(job_id).as_ref()).to_owned()
        } else if checksum_operation {
            "Calculating checksums".to_owned()
        } else if permission_operation {
            "Changing permissions".to_owned()
        } else if archive_operation {
            "Archive operation".to_owned()
        } else if batch_rename_operation {
            "Batch rename".to_owned()
        } else {
            operation_title(request.as_ref())
        };
        self.widgets.operation_label.set_label(&title);
        self.widgets.operation_detail.set_label(detail);
        let cancel_tooltip = if verified_usb_operation {
            "Cancel verified removable transfer".to_owned()
        } else if verified_copy_operation {
            "Cancel Copy and Verify".to_owned()
        } else if integrity_operation {
            "Cancel integrity operation".to_owned()
        } else if checksum_operation {
            "Cancel checksum calculation".to_owned()
        } else if permission_operation {
            "Cancel permission change".to_owned()
        } else if archive_operation {
            "Cancel archive operation".to_owned()
        } else if batch_rename_operation {
            "Cancel batch rename".to_owned()
        } else {
            format!("Cancel {}", operation_verb(request.as_ref()).to_lowercase())
        };
        self.widgets
            .operation_cancel
            .set_tooltip_text(Some(&cancel_tooltip));
        self.widgets.operation_cancel.set_sensitive(true);
        self.update_batch_controls(job_id);
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

    fn finish(self: &Rc<Self>, job_id: JobId, result: TerminalResult<'_>) {
        self.active_jobs
            .borrow_mut()
            .retain(|active| *active != job_id);
        let notify_background = self
            .telemetry
            .borrow()
            .get(&job_id)
            .is_some_and(|telemetry| {
                crate::completeness::completion_notification_elapsed_is_eligible(
                    telemetry.started_at.elapsed(),
                )
            });
        self.telemetry.borrow_mut().remove(&job_id);
        let outcome = terminal_outcome(&result);
        self.retryable_job.set(updated_retryable_job(
            self.retryable_job.get(),
            job_id,
            outcome,
        ));
        let permission_operation = self.state.is_permission_operation(job_id);
        let checksum_operation = self.state.is_checksum_operation(job_id);
        let integrity_operation = self.state.is_integrity_operation(job_id);
        let verified_copy_operation = self.state.is_verified_copy_operation(job_id);
        let verified_usb_operation = self.state.is_verified_usb_copy_operation(job_id);
        let archive_operation = self.state.is_archive_operation(job_id);
        let batch_rename_operation = self.state.is_batch_rename_operation(job_id);
        let permission_directories = self.state.permission_affected_directories(job_id);
        let archive_directories = self.state.archive_affected_directories(job_id);
        let batch_rename_directories = self.state.batch_rename_affected_directories(job_id);
        let batch_id = self.state.batch_for_job(job_id);
        let request = self.state.finish_operation(job_id, outcome);
        let checksum_outcome = if checksum_operation {
            self.state.finish_checksum(job_id)
        } else {
            None
        };
        let integrity_outcome = if integrity_operation {
            self.state.finish_integrity(job_id)
        } else {
            None
        };
        let verified_copy_completion = if verified_copy_operation {
            self.state.finish_verified_copy(job_id)
        } else {
            None
        };
        let archive_outcome = if archive_operation {
            self.state.finish_archive(job_id)
        } else {
            None
        };
        let batch_rename_outcome = if batch_rename_operation {
            self.state.finish_batch_rename(job_id)
        } else {
            None
        };
        let conflict_pending =
            outcome == TerminalOutcome::Conflict && self.state.pending_conflict(job_id).is_ok();
        if conflict_pending {
            self.conflicts.borrow_mut().enqueue(job_id);
        }

        match result {
            TerminalResult::Completed => {
                if let Some(reveal) = request
                    .as_ref()
                    .and_then(TrackedOperation::completed_result_path)
                    .and_then(|path| OperationRevealRequest::new(job_id, batch_id, path))
                {
                    (self.on_operation_result)(reveal);
                }
                for directory in &permission_directories {
                    (self.on_operation_completed)(directory);
                }
                for directory in &archive_directories {
                    (self.on_operation_completed)(directory);
                }
                for directory in &batch_rename_directories {
                    (self.on_operation_completed)(directory);
                }
                if batch_rename_outcome.is_some()
                    && let Some(action) = self
                        .window
                        .lookup_action("undo-batch-rename")
                        .and_downcast::<gio::SimpleAction>()
                {
                    action.set_enabled(true);
                }
                if let Some(request) = request.as_ref() {
                    for directory in request.affected_directories() {
                        (self.on_operation_completed)(&directory);
                    }
                }
                self.show_terminal(
                    request.as_ref(),
                    if verified_copy_operation {
                        "Copy verified"
                } else if integrity_operation {
                "Integrity operation complete"
            } else if checksum_operation {
                        "Checksums calculated"
                    } else if permission_operation {
                        "Permissions updated"
                    } else if archive_operation {
                        archive_completed_title(archive_outcome.as_ref())
                    } else if batch_rename_operation {
                        "Batch rename complete"
                    } else {
                        completed_title(request.as_ref())
                    },
                    if verified_usb_operation {
                        "Byte verification completed; preparing the removable-device flush"
                    } else if verified_copy_operation {
                        "Destination synced and source/destination bytes matched with SHA-256"
                } else if integrity_operation {
                "Integrity results are ready"
            } else if checksum_operation {
                        "Checksum results are ready"
                    } else if permission_operation {
                        "Selected permission changes completed"
                    } else if archive_operation {
                        archive_completed_detail(archive_outcome.as_ref())
                    } else if batch_rename_operation {
                        if batch_rename_outcome.is_some() {
                            "All validated names were applied; Undo is available for the completed mapping"
                        } else {
                            "Batch rename completed"
                        }
                    } else {
                        completed_detail(request.as_ref())
                    },
                    true,
                );
                let toast = if verified_usb_operation {
                    "Copy verified; removable transfer is continuing".to_owned()
                } else if verified_copy_operation {
                    "Copy verified".to_owned()
                } else if integrity_operation {
                    "Integrity operation complete".to_owned()
                } else if checksum_operation {
                    "Checksums calculated".to_owned()
                } else if permission_operation {
                    "Permissions updated".to_owned()
                } else if archive_operation {
                    archive_completed_title(archive_outcome.as_ref()).to_owned()
                } else if batch_rename_operation {
                    "Batch rename complete".to_owned()
                } else {
                    completed_toast(request.as_ref())
                };
                self.show_toast(&toast, 4);
                if notify_background {
                    (self.on_background_completed)(job_id);
                }
                if let Some(outcome) = checksum_outcome {
                    self.present_checksum_results(outcome);
                }
                if let Some(outcome) = integrity_outcome {
                    self.present_integrity_results(outcome);
                }
            }
            TerminalResult::Cancelled => {
                let permanent_delete =
                    matches!(request.as_ref(), Some(TrackedOperation::PermanentDelete(_)));
                self.show_terminal(
                    request.as_ref(),
                    if verified_copy_operation {
                        "Copy and Verify cancelled"
                    } else if integrity_operation {
                        "Integrity operation cancelled"
                    } else if checksum_operation {
                        "Checksum calculation cancelled"
                    } else if permission_operation {
                        "Permission change cancelled"
                    } else if archive_operation {
                        "Archive operation cancelled"
                    } else if batch_rename_operation {
                        "Batch rename cancelled"
                    } else if permanent_delete {
                        "Cancelled before deletion"
                    } else {
                        "Operation cancelled"
                    },
                    if verified_copy_operation {
                        "The destination may remain without verification; review the result"
                    } else if integrity_operation {
                        "No integrity result was kept"
                    } else if checksum_operation {
                        "No checksum result was kept"
                    } else if permission_operation {
                        "No permission change was committed"
                    } else if archive_operation {
                        "No archive result was published"
                    } else if batch_rename_operation {
                        "No name was changed"
                    } else if permanent_delete {
                        "No selected item was deleted"
                    } else {
                        "No partial change was kept"
                    },
                    false,
                );
                self.show_toast(
                    if verified_copy_operation {
                        "Copy and Verify cancelled"
                    } else if integrity_operation {
                        "Integrity operation cancelled"
                    } else if checksum_operation {
                        "Checksum calculation cancelled"
                    } else if permission_operation {
                        "Permission change cancelled before it started"
                    } else if archive_operation {
                        "Archive operation cancelled"
                    } else if batch_rename_operation {
                        "Batch rename cancelled"
                    } else if permanent_delete {
                        "Permanent deletion cancelled before it started"
                    } else {
                        "Operation cancelled"
                    },
                    4,
                );
            }
            TerminalResult::Failed(failure) => {
                if failure.kind() == JobFailureKind::Partial && batch_rename_operation {
                    for directory in &batch_rename_directories {
                        (self.on_operation_completed)(directory);
                    }
                }
                if failure.kind() == JobFailureKind::Partial {
                    for directory in &permission_directories {
                        (self.on_operation_completed)(directory);
                    }
                    if let Some(request) = request.as_ref() {
                        for directory in request.affected_directories() {
                            (self.on_operation_completed)(&directory);
                        }
                    }
                }
                let archive_failure = archive_operation
                    .then(|| archive_failure_text(failure.kind(), failure.message()));
                let title = if verified_copy_operation {
                    "Copy and Verify failed"
                } else if integrity_operation {
                    "Integrity verification failed"
                } else if checksum_operation {
                    "Checksum calculation failed"
                } else if batch_rename_operation {
                    if failure.kind() == JobFailureKind::Conflict {
                        "Batch rename conflict"
                    } else if failure.kind() == JobFailureKind::Partial {
                        "Batch rename partially completed"
                    } else {
                        "Batch rename failed"
                    }
                } else if let Some((title, _)) = archive_failure.as_ref() {
                    title
                } else {
                    standard_failure_title(request.as_ref(), failure.kind())
                };
                let detail = archive_failure.as_ref().map_or_else(
                    || failure_summary(request.as_ref(), failure).to_owned(),
                    |(_, detail)| detail.clone(),
                );
                self.show_terminal(request.as_ref(), title, &detail, false);
                if let Some((_, detail)) = archive_failure {
                    self.show_toast(&detail, 7);
                } else {
                    self.show_toast(&failure_recovery(request.as_ref(), failure), 7);
                }
            }
        }
        self.state.finish_permission(job_id);

        if let Some(batch_id) = self.state.batch_for_job(job_id)
            && let Some(snapshot) = self.state.batch_snapshot(batch_id)
        {
            let (title, detail) = batch_summary(snapshot);
            self.widgets.operation_label.set_label(title);
            self.widgets.operation_detail.set_label(&detail);
            self.visible_batch.set(Some(batch_id));
            if snapshot.status().is_terminal() {
                self.widgets.operation_pause.set_visible(false);
            } else {
                self.update_batch_controls(job_id);
            }
        }

        if let Some(next_job) = self.active_jobs.borrow().back().copied() {
            let request = self.request(next_job);
            self.show_running(next_job, waiting_detail(request), None);
        } else {
            self.show_available_terminal_action();
        }

        if conflict_pending {
            self.present_conflict(job_id);
        }
        if let Some(completion) = verified_copy_completion {
            match completion {
                VerifiedCopyCompletion::Ordinary(result) => {
                    self.present_verified_copy_result(result);
                }
                VerifiedCopyCompletion::VerifiedUsb(result) => {
                    self.state.dispatch_verified_usb_completion(job_id, result);
                }
            }
        }
    }

    fn present_checksum_results(&self, outcome: ChecksumOutcome) {
        let presentation = present_checksum(&outcome);
        let copy_text = presentation.copy_text.clone();
        let widgets = build_checksum_results_dialog(&presentation);
        let dialog = widgets.dialog.downgrade();
        widgets.close_button.connect_clicked(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        });
        let clipboard = self.window.clipboard();
        let toast_overlay = self.toast_overlay.clone();
        widgets.copy_button.connect_clicked(move |_| {
            clipboard.set_text(&copy_text);
            toast_overlay.add_toast(
                adw::Toast::builder()
                    .title("Digest text copied")
                    .timeout(3)
                    .build(),
            );
        });
        widgets.dialog.present(Some(&self.window));
        widgets.close_button.grab_focus();
    }

    fn present_integrity_results(&self, outcome: IntegrityOutcome) {
        let presentation = present_integrity(&outcome);
        let widgets = build_integrity_results_dialog(&presentation);
        let dialog = widgets.dialog.downgrade();
        widgets.close_button.connect_clicked(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        });
        widgets.dialog.present(Some(&self.window));
        widgets.close_button.grab_focus();
    }

    fn present_verified_copy_result(&self, result: VerifiedCopyResult) {
        let presentation = present_verified_copy(&result);
        let retry_request = result
            .as_ref()
            .err()
            .filter(|_| presentation.retry_enabled)
            .map(|failure| failure.retry().request().clone());
        let widgets = build_verified_copy_result_dialog(&presentation);
        let dialog = widgets.dialog.downgrade();
        widgets.close_button.connect_clicked(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        });
        if let Some(request) = retry_request {
            let state = Rc::clone(&self.state);
            let toast_overlay = self.toast_overlay.clone();
            let dialog = widgets.dialog.downgrade();
            widgets.retry_button.connect_clicked(move |_| {
                match state.submit_verified_copy(request.clone()) {
                    Ok(_) => {
                        if let Some(dialog) = dialog.upgrade() {
                            dialog.close();
                        }
                        toast_overlay.add_toast(
                            adw::Toast::builder()
                                .title("Copy and Verify retry queued")
                                .timeout(4)
                                .build(),
                        );
                    }
                    Err(error) => toast_overlay.add_toast(
                        adw::Toast::builder()
                            .title(format!("Could not retry Copy and Verify: {error}"))
                            .timeout(7)
                            .build(),
                    ),
                }
            });
        }
        widgets.dialog.present(Some(&self.window));
        widgets.close_button.grab_focus();
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
        let result = self.state.batch_for_job(job_id).map_or_else(
            || self.state.cancel_operation(job_id),
            |batch| self.state.cancel_batch(batch),
        );
        match result {
            Ok(()) => {
                self.widgets.operation_cancel.set_sensitive(false);
                self.widgets.operation_pause.set_sensitive(false);
                self.widgets
                    .operation_detail
                    .set_label("Cancelling active and queued items…");
            }
            Err(error) => self.show_toast(&format!("Could not cancel operation: {error}"), 6),
        }
    }

    fn toggle_visible_batch_pause(&self) {
        let Some(batch_id) = self.visible_batch.get() else {
            return;
        };
        let Some(snapshot) = self.state.batch_snapshot(batch_id) else {
            self.widgets.operation_pause.set_visible(false);
            return;
        };
        let result = if matches!(
            snapshot.status(),
            BatchStatus::Paused | BatchStatus::Pausing
        ) {
            self.state.resume_batch(batch_id)
        } else {
            self.state.pause_batch(batch_id)
        };
        match result {
            Ok(()) => {
                if let Some(job_id) = self.visible_job.get() {
                    self.update_batch_controls(job_id);
                } else if let Some(updated) = self.state.batch_snapshot(batch_id) {
                    let resuming =
                        !matches!(updated.status(), BatchStatus::Paused | BatchStatus::Pausing);
                    self.widgets.operation_pause.set_label(if resuming {
                        "Pause after current"
                    } else {
                        "Resume"
                    });
                }
            }
            Err(error) => self.show_toast(&format!("Could not change batch state: {error}"), 6),
        }
    }

    fn present_operation_history(self: &Rc<Self>) {
        let entries = self.state.terminal_history();
        let persistent = match self.state.persistent_undo_history() {
            Ok(records) => records,
            Err(error) => {
                self.show_toast(&format!("Could not read durable Undo history: {error}"), 7);
                Vec::new()
            }
        };
        let mut items = entries
            .iter()
            .map(|entry| history_item(entry, self.state.can_undo(entry.job_id())))
            .collect::<Vec<_>>();
        items.extend(persistent.iter().map(persistent_history_item));
        let can_clear = entries
            .iter()
            .any(|entry| entry.outcome() == TerminalOutcome::Completed);
        let history = build_operation_history_dialog(&items, can_clear);

        for (button, entry) in history.undo_buttons.iter().zip(entries.iter()) {
            let controller = Rc::downgrade(self);
            let dialog = history.dialog.downgrade();
            let job_id = entry.job_id();
            button.connect_clicked(move |button| {
                let Some(controller) = controller.upgrade() else {
                    return;
                };
                let scope = match controller.state.undo_operation_guardrail_scope(job_id) {
                    Ok(scope) => scope,
                    Err(error) => {
                        controller.show_toast(&format!("Could not undo operation: {error}"), 7);
                        return;
                    }
                };
                let state = Rc::clone(&controller.state);
                let weak_controller = Rc::downgrade(&controller);
                let button = button.clone();
                let dialog = dialog.clone();
                controller.review_guardrail(vec![scope], move |mut authorizations| {
                    let Some(controller) = weak_controller.upgrade() else {
                        return;
                    };
                    let Some(authorization) = authorizations.pop() else {
                        controller.show_toast("Could not authorize operation undo", 7);
                        return;
                    };
                    button.set_sensitive(false);
                    match state.undo_operation_authorized(job_id, authorization) {
                        Ok(submission) => {
                            if let Some(dialog) = dialog.upgrade() {
                                dialog.close();
                            }
                            controller.track_active(submission.job_id());
                            controller.show_running(
                                submission.job_id(),
                                waiting_detail(controller.request(submission.job_id())),
                                None,
                            );
                        }
                        Err(error) => {
                            button.set_sensitive(true);
                            controller.show_toast(&format!("Could not undo operation: {error}"), 7);
                        }
                    }
                });
            });
        }
        let persistent_offset = entries.len();
        for (index, record) in persistent.iter().enumerate() {
            let history_id = record.id();
            let dialog = history.dialog.downgrade();
            let controller = Rc::downgrade(self);
            history.undo_buttons[persistent_offset + index].connect_clicked(move |button| {
                let Some(controller) = controller.upgrade() else {
                    return;
                };
                controller.activate_persistent_history_action(
                    history_id,
                    UndoHistoryAction::Undo,
                    button.clone(),
                    dialog.clone(),
                );
            });
            let dialog = history.dialog.downgrade();
            let controller = Rc::downgrade(self);
            history.redo_buttons[persistent_offset + index].connect_clicked(move |button| {
                let Some(controller) = controller.upgrade() else {
                    return;
                };
                controller.activate_persistent_history_action(
                    history_id,
                    UndoHistoryAction::Redo,
                    button.clone(),
                    dialog.clone(),
                );
            });
        }

        let controller = Rc::downgrade(self);
        let dialog = history.dialog.downgrade();
        history
            .clear_completed_button
            .connect_clicked(move |button| {
                let Some(controller) = controller.upgrade() else {
                    return;
                };
                let removed = controller.state.clear_completed_history();
                button.set_sensitive(false);
                controller.show_toast(&format!("Cleared {removed} completed operations"), 4);
                if let Some(dialog) = dialog.upgrade() {
                    dialog.close();
                }
            });
        history.dialog.present(Some(&self.window));
    }

    fn activate_persistent_history_action(
        self: &Rc<Self>,
        history_id: u64,
        action: UndoHistoryAction,
        button: gtk::Button,
        dialog: glib::WeakRef<adw::Dialog>,
    ) {
        let scope = match self
            .state
            .persistent_history_action_scope(history_id, action)
        {
            Ok(scope) => scope,
            Err(error) => {
                self.show_toast(&format!("Could not prepare Undo/Redo: {error}"), 7);
                return;
            }
        };
        if let Some(scope) = scope {
            let controller = Rc::downgrade(self);
            self.review_guardrail(vec![scope], move |mut authorizations| {
                let Some(controller) = controller.upgrade() else {
                    return;
                };
                let authorization = authorizations.pop();
                controller.dispatch_persistent_history_action(
                    history_id,
                    action,
                    authorization,
                    &button,
                    &dialog,
                );
            });
        } else {
            self.dispatch_persistent_history_action(history_id, action, None, &button, &dialog);
        }
    }

    fn dispatch_persistent_history_action(
        self: &Rc<Self>,
        history_id: u64,
        action: UndoHistoryAction,
        authorization: Option<GuardrailAuthorizationItem>,
        button: &gtk::Button,
        dialog: &glib::WeakRef<adw::Dialog>,
    ) {
        button.set_sensitive(false);
        match self
            .state
            .submit_persistent_history_action(history_id, action, authorization)
        {
            Ok(submission) => {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.close();
                }
                self.track_active(submission.job_id());
                self.show_running(
                    submission.job_id(),
                    waiting_detail(self.request(submission.job_id())),
                    None,
                );
            }
            Err(error) => {
                button.set_sensitive(true);
                self.show_toast(&format!("Could not apply Undo/Redo: {error}"), 7);
            }
        }
    }

    fn show_retry(&self, job_id: JobId) {
        self.visibility_generation
            .set(self.visibility_generation.get().wrapping_add(1));
        self.retryable_job.set(Some(job_id));
        self.widgets.operation_retry.set_label("Retry");
        self.widgets
            .operation_retry
            .set_tooltip_text(Some("Retry file operation"));
        self.widgets.operation_retry.set_sensitive(true);
        self.widgets.operation_retry.set_visible(true);
    }

    fn show_conflict_action(&self) {
        self.visibility_generation
            .set(self.visibility_generation.get().wrapping_add(1));
        self.widgets.operation_retry.set_label("Resolve Conflict");
        self.widgets
            .operation_retry
            .set_tooltip_text(Some("Resolve destination conflict"));
        self.widgets.operation_retry.set_sensitive(true);
        self.widgets.operation_retry.set_visible(true);
        self.widgets.revealer.set_reveal_child(true);
    }

    fn show_available_terminal_action(&self) {
        let action = available_terminal_action(&self.conflicts.borrow(), self.retryable_job.get());
        match action {
            Some(TerminalAction::ResolveConflict(_)) => self.show_conflict_action(),
            Some(TerminalAction::Retry(job_id)) => self.show_retry(job_id),
            None => {}
        }
        if terminal_action_auto_hides(action) {
            self.schedule_hide();
        }
    }

    fn clear_retry(&self) {
        self.retryable_job.set(None);
        self.hide_retry();
    }

    fn hide_retry(&self) {
        self.widgets.operation_retry.set_visible(false);
    }

    fn activate_terminal_action(self: &Rc<Self>) {
        let action = available_terminal_action(&self.conflicts.borrow(), self.retryable_job.get());
        match action {
            Some(TerminalAction::ResolveConflict(job_id)) => self.present_conflict(job_id),
            Some(TerminalAction::Retry(_)) => self.retry_terminal_operation(),
            None => {}
        }
    }

    fn retry_terminal_operation(self: &Rc<Self>) {
        let Some(job_id) = self.retryable_job.get() else {
            return;
        };
        let scope = match self.state.retry_operation_guardrail_scope(job_id) {
            Ok(scope) => scope,
            Err(error) => {
                self.show_toast(&format!("Could not retry operation: {error}"), 7);
                return;
            }
        };
        let Some(scope) = scope else {
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
            return;
        };
        let state = Rc::clone(&self.state);
        let weak_controller = Rc::downgrade(self);
        self.review_guardrail(vec![scope], move |mut authorizations| {
            let Some(controller) = weak_controller.upgrade() else {
                return;
            };
            let Some(authorization) = authorizations.pop() else {
                controller.show_toast("Could not authorize operation retry", 7);
                return;
            };
            controller.widgets.operation_retry.set_sensitive(false);
            match state.retry_operation_authorized(job_id, authorization) {
                Ok(_) => {
                    controller.clear_retry();
                    controller
                        .widgets
                        .operation_detail
                        .set_label("Retry queued…");
                }
                Err(error) => {
                    controller.widgets.operation_retry.set_sensitive(true);
                    controller.show_toast(&format!("Could not retry operation: {error}"), 7);
                }
            }
        });
    }

    fn present_conflict(self: &Rc<Self>, job_id: JobId) {
        if !self.conflicts.borrow_mut().begin_dialog(job_id) {
            return;
        }

        let pending = match self.state.pending_conflict(job_id) {
            Ok(pending) => pending,
            Err(error) => {
                self.conflicts.borrow_mut().resolve(job_id);
                self.show_toast(&format!("Could not reopen conflict: {error}"), 7);
                self.show_available_terminal_action();
                return;
            }
        };
        let source = pending.source().to_string_lossy().into_owned();
        let destination = pending.destination().to_string_lossy().into_owned();
        let existing_name = pending.destination().file_name().map(OsString::from);
        let conflict = build_conflict_dialog(
            &source,
            &destination,
            pending.source_description(),
            pending.destination_description(),
            pending.replace_supported(),
            pending.replace_all_supported(),
        );
        conflict
            .skip_all_button
            .set_visible(self.state.batch_for_job(job_id).is_some());

        let controller = Rc::downgrade(self);
        let dialog = conflict.dialog.downgrade();
        let source_identity = pending.source_identity();
        let destination_identity = pending.destination_identity();
        conflict.replace_button.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.confirm_replace_conflict(
                    job_id,
                    source_identity,
                    destination_identity,
                    false,
                    dialog.clone(),
                );
            }
        });

        let controller = Rc::downgrade(self);
        let dialog = conflict.dialog.downgrade();
        let source_identity = pending.source_identity();
        let destination_identity = pending.destination_identity();
        conflict.replace_all_button.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.confirm_replace_conflict(
                    job_id,
                    source_identity,
                    destination_identity,
                    true,
                    dialog.clone(),
                );
            }
        });

        let retry_button = conflict.retry_button.clone();
        let name_error = conflict.name_error.clone();
        let existing_for_validation = existing_name.clone();
        conflict.name_entry.connect_changed(move |entry| {
            let text = entry.text();
            if text.is_empty() {
                retry_button.set_sensitive(false);
                name_error.set_visible(false);
                return;
            }
            match conflict_retry_name(text.as_str(), existing_for_validation.as_deref()) {
                Ok(_) => {
                    retry_button.set_sensitive(true);
                    name_error.set_visible(false);
                }
                Err(message) => {
                    retry_button.set_sensitive(false);
                    name_error.set_label(message);
                    name_error.set_visible(true);
                }
            }
        });

        let dialog = conflict.dialog.downgrade();
        conflict.cancel_button.connect_clicked(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        });

        let controller = Rc::downgrade(self);
        let dialog = conflict.dialog.downgrade();
        let retry_button = conflict.retry_button.clone();
        let name_entry_for_keep = conflict.name_entry.clone();
        let existing_for_keep = existing_name.clone();
        conflict
            .keep_existing_button
            .connect_clicked(move |button| {
                let Some(controller) = controller.upgrade() else {
                    return;
                };
                button.set_sensitive(false);
                retry_button.set_sensitive(false);
                match controller
                    .state
                    .resolve_conflict(job_id, ConflictDecision::KeepExisting)
                {
                    Ok(ConflictResolution::KeptExisting) => {
                        controller.conflicts.borrow_mut().resolve(job_id);
                        controller
                            .widgets
                            .operation_label
                            .set_label("Conflict resolved");
                        controller
                            .widgets
                            .operation_detail
                            .set_label("Existing destination kept");
                        controller.show_toast("Kept the existing destination", 4);
                        if let Some(dialog) = dialog.upgrade() {
                            dialog.close();
                        }
                        if controller.active_jobs.borrow().is_empty() {
                            controller.show_available_terminal_action();
                        }
                    }
                    Ok(ConflictResolution::Retried(_)) => {
                        button.set_sensitive(true);
                        retry_button.set_sensitive(
                            conflict_retry_name(
                                name_entry_for_keep.text().as_str(),
                                existing_for_keep.as_deref(),
                            )
                            .is_ok(),
                        );
                        controller.show_toast("Could not keep the existing destination", 7);
                    }
                    Err(error) => {
                        button.set_sensitive(true);
                        retry_button.set_sensitive(
                            conflict_retry_name(
                                name_entry_for_keep.text().as_str(),
                                existing_for_keep.as_deref(),
                            )
                            .is_ok(),
                        );
                        controller.show_toast(&format!("Could not resolve conflict: {error}"), 7);
                    }
                }
            });

        let controller = Rc::downgrade(self);
        let dialog = conflict.dialog.downgrade();
        conflict.keep_both_button.connect_clicked(move |button| {
            let Some(controller) = controller.upgrade() else {
                return;
            };
            let weak_controller = Rc::downgrade(&controller);
            let button = button.clone();
            let dialog = dialog.clone();
            controller.resolve_conflict_with_review(
                job_id,
                ConflictDecision::KeepBoth,
                move |result| {
                    let Some(controller) = weak_controller.upgrade() else {
                        return;
                    };
                    button.set_sensitive(false);
                    match result {
                        Ok(ConflictResolution::Retried(submission)) => {
                            controller.conflicts.borrow_mut().resolve(job_id);
                            if let Some(dialog) = dialog.upgrade() {
                                dialog.close();
                            }
                            controller.track_active(submission.job_id());
                            controller.show_running(
                                submission.job_id(),
                                waiting_detail(controller.request(submission.job_id())),
                                None,
                            );
                        }
                        Ok(ConflictResolution::KeptExisting) => {
                            button.set_sensitive(true);
                            controller.show_toast("Could not create a Keep Both retry", 7);
                        }
                        Err(error) => {
                            button.set_sensitive(true);
                            controller
                                .show_toast(&format!("Could not keep both items: {error}"), 7);
                        }
                    }
                },
            );
        });

        let controller = Rc::downgrade(self);
        let dialog = conflict.dialog.downgrade();
        conflict.skip_all_button.connect_clicked(move |button| {
            let Some(controller) = controller.upgrade() else {
                return;
            };
            button.set_sensitive(false);
            match controller
                .state
                .resolve_conflict(job_id, ConflictDecision::SkipAll)
            {
                Ok(ConflictResolution::KeptExisting) => {
                    controller.conflicts.borrow_mut().resolve(job_id);
                    controller.show_toast("Skipped this and later batch conflicts", 4);
                    if let Some(dialog) = dialog.upgrade() {
                        dialog.close();
                    }
                    controller.show_available_terminal_action();
                }
                Ok(ConflictResolution::Retried(_)) => {
                    button.set_sensitive(true);
                    controller.show_toast("Could not apply Skip All", 7);
                }
                Err(error) => {
                    button.set_sensitive(true);
                    controller.show_toast(&format!("Could not skip batch conflicts: {error}"), 7);
                }
            }
        });

        let controller = Rc::downgrade(self);
        let dialog = conflict.dialog.downgrade();
        let name_entry = conflict.name_entry.clone();
        let name_error = conflict.name_error.clone();
        let keep_existing_button = conflict.keep_existing_button.clone();
        conflict.retry_button.connect_clicked(move |button| {
            let Some(controller) = controller.upgrade() else {
                return;
            };
            let new_name =
                match conflict_retry_name(name_entry.text().as_str(), existing_name.as_deref()) {
                    Ok(new_name) => new_name,
                    Err(message) => {
                        name_error.set_label(message);
                        name_error.set_visible(true);
                        button.set_sensitive(false);
                        return;
                    }
                };
            let weak_controller = Rc::downgrade(&controller);
            let button = button.clone();
            let keep_existing_button = keep_existing_button.clone();
            let dialog = dialog.clone();
            controller.resolve_conflict_with_review(
                job_id,
                ConflictDecision::RetryWithName(new_name),
                move |result| {
                    let Some(controller) = weak_controller.upgrade() else {
                        return;
                    };
                    button.set_sensitive(false);
                    keep_existing_button.set_sensitive(false);
                    match result {
                        Ok(ConflictResolution::Retried(submission)) => {
                            controller.conflicts.borrow_mut().resolve(job_id);
                            if let Some(dialog) = dialog.upgrade() {
                                dialog.close();
                            }
                            controller.track_active(submission.job_id());
                            controller.show_running(submission.job_id(), "Retry queued…", None);
                        }
                        Ok(ConflictResolution::KeptExisting) => {
                            button.set_sensitive(true);
                            keep_existing_button.set_sensitive(true);
                            controller.show_toast("Could not submit the revised operation", 7);
                        }
                        Err(error) => {
                            button.set_sensitive(true);
                            keep_existing_button.set_sensitive(true);
                            controller
                                .show_toast(&format!("Could not retry with that name: {error}"), 7);
                        }
                    }
                },
            );
        });

        let controller = Rc::downgrade(self);
        conflict.dialog.connect_closed(move |_| {
            let Some(controller) = controller.upgrade() else {
                return;
            };
            if controller.conflicts.borrow_mut().dismiss_dialog(job_id)
                && controller.active_jobs.borrow().is_empty()
            {
                controller.show_available_terminal_action();
            }
        });

        conflict.dialog.present(Some(&self.window));
        conflict.name_entry.grab_focus();
    }

    fn show_toast(&self, title: &str, timeout: u32) {
        self.toast_overlay
            .add_toast(adw::Toast::builder().title(title).timeout(timeout).build());
    }
}

fn standard_failure_title(
    request: Option<&TrackedOperation>,
    failure_kind: JobFailureKind,
) -> &'static str {
    match (failure_kind, request) {
        (JobFailureKind::Conflict, _) => "Destination conflict",
        (JobFailureKind::Partial, Some(TrackedOperation::PermanentDelete(_))) => {
            "Permanent deletion partially completed"
        }
        (JobFailureKind::Partial, Some(TrackedOperation::Restore(_))) => {
            "Restore completed with cleanup warning"
        }
        (JobFailureKind::Partial, Some(TrackedOperation::Copy(_))) => "Copy partially completed",
        (JobFailureKind::Partial, Some(TrackedOperation::Move(_))) => "Move partially completed",
        (JobFailureKind::Partial, Some(TrackedOperation::Rename(_))) => {
            "Rename partially completed"
        }
        (JobFailureKind::Partial, Some(TrackedOperation::Trash(_))) => {
            "Trash operation partially completed"
        }
        (JobFailureKind::Partial, Some(TrackedOperation::Create(_))) => {
            "Create partially completed"
        }
        (JobFailureKind::Partial, Some(TrackedOperation::Replace(_))) => {
            "Replacement partially completed"
        }
        (JobFailureKind::Partial, Some(TrackedOperation::UndoMove { .. })) => {
            "Undo move partially completed"
        }
        (JobFailureKind::Partial, Some(TrackedOperation::PersistentHistoryAction { .. })) => {
            "Undo/Redo needs review"
        }
        (JobFailureKind::Partial, None) => "Operation partially completed",
        _ => "Operation failed",
    }
}

fn batch_summary(snapshot: BatchSnapshot) -> (&'static str, String) {
    let title = match snapshot.status() {
        BatchStatus::Queued | BatchStatus::Running => "Batch in progress",
        BatchStatus::Pausing => "Batch will pause after current item",
        BatchStatus::Paused => "Batch paused",
        BatchStatus::Cancelling => "Cancelling batch",
        BatchStatus::Completed => "Batch complete",
        BatchStatus::CompletedWithIssues => "Batch completed with issues",
        BatchStatus::Cancelled => "Batch cancelled",
    };
    let mut details = vec![format!(
        "{} of {} items processed",
        snapshot.processed(),
        snapshot.total()
    )];
    if snapshot.completed() > 0 {
        details.push(format!("{} completed", snapshot.completed()));
    }
    if snapshot.skipped() > 0 {
        details.push(format!("{} skipped", snapshot.skipped()));
    }
    if snapshot.failed() > 0 {
        details.push(format!("{} failed", snapshot.failed()));
    }
    if snapshot.cancelled() > 0 {
        details.push(format!("{} cancelled", snapshot.cancelled()));
    }
    (title, details.join(" • "))
}

fn progress_detail(progress: JobProgress, estimate: Option<TransferEstimate>) -> String {
    match progress.unit() {
        ProgressUnit::Bytes => {
            let mut detail = match progress.total() {
                Some(total) => format!(
                    "{} of {}",
                    format_transfer_bytes(progress.completed()),
                    format_transfer_bytes(total)
                ),
                None => format!(
                    "{} transferred",
                    format_transfer_bytes(progress.completed())
                ),
            };
            if let Some(estimate) = estimate {
                detail.push_str(&format!(
                    " • {}/s • {} remaining",
                    format_transfer_bytes(estimate.bytes_per_second()),
                    format_eta(estimate.eta())
                ));
            }
            detail
        }
        ProgressUnit::Items => match progress.total() {
            Some(total) => format!("{} of {total} items", progress.completed()),
            None => format!("{} items", progress.completed()),
        },
        ProgressUnit::Unknown => match progress.total() {
            Some(total) => format!("{} of {total}", progress.completed()),
            None => "Working…".to_owned(),
        },
    }
}

fn format_transfer_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_eta(duration: Duration) -> String {
    let seconds = duration.as_secs().max(1);
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3_600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {}m", seconds / 3_600, (seconds % 3_600) / 60)
    }
}

fn history_item(entry: &TerminalOperation, can_undo: bool) -> OperationHistoryItem {
    let outcome = match entry.outcome() {
        TerminalOutcome::Completed => "Completed",
        TerminalOutcome::Cancelled => "Cancelled",
        TerminalOutcome::Conflict => "Needs conflict resolution",
        TerminalOutcome::PartialFailure => "Completed with partial changes",
        TerminalOutcome::Failed => "Failed",
    };
    let detail = entry.batch_id().map_or_else(
        || outcome.to_owned(),
        |batch_id| format!("{outcome} • batch {}", batch_id.get()),
    );
    OperationHistoryItem {
        title: operation_title(Some(entry.operation())),
        detail,
        can_undo,
        can_redo: false,
    }
}

fn persistent_history_item(record: &UndoHistoryRecord) -> OperationHistoryItem {
    let name = record
        .recipe()
        .destination()
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "item".to_owned());
    let state = match record.state() {
        UndoHistoryState::Applied => "Applied • Undo available",
        UndoHistoryState::Undone => "Undone • Redo available",
        UndoHistoryState::InProgress => "Interrupted before completion • review required",
        UndoHistoryState::Undoing => "Interrupted during Undo • review required",
        UndoHistoryState::Redoing => "Interrupted during Redo • review required",
        UndoHistoryState::NeedsReview => "Uncertain result • review required",
    };
    OperationHistoryItem {
        title: format!("{} {name}", record.recipe().label()),
        detail: format!("{state} • expires at Unix {}", record.expires_at()),
        can_undo: record.can_undo(),
        can_redo: record.can_redo(),
    }
}

fn recovery_path_status(status: Option<RecoveryPathStatus>) -> &'static str {
    match status {
        None => "Not applicable",
        Some(RecoveryPathStatus::Missing) => "Missing",
        Some(RecoveryPathStatus::Present) => "Present",
        Some(RecoveryPathStatus::Inaccessible) => "Cannot inspect",
    }
}

fn available_terminal_action(
    conflicts: &ConflictInteractions,
    retryable_job: Option<JobId>,
) -> Option<TerminalAction> {
    conflicts
        .current()
        .map(TerminalAction::ResolveConflict)
        .or_else(|| retryable_job.map(TerminalAction::Retry))
}

fn terminal_action_auto_hides(action: Option<TerminalAction>) -> bool {
    !matches!(action, Some(TerminalAction::ResolveConflict(_)))
}

fn conflict_retry_name(
    input: &str,
    existing_name: Option<&OsStr>,
) -> Result<OsString, &'static str> {
    if input.is_empty() {
        return Err("Enter a different filename");
    }
    let new_name = OsString::from(input);
    validate_rename_name(&new_name).map_err(|_| "Enter one filename without slashes")?;
    if existing_name == Some(new_name.as_os_str()) {
        return Err("Choose a different filename");
    }
    Ok(new_name)
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

fn terminal_outcome(result: &TerminalResult<'_>) -> TerminalOutcome {
    match result {
        TerminalResult::Completed => TerminalOutcome::Completed,
        TerminalResult::Cancelled => TerminalOutcome::Cancelled,
        TerminalResult::Failed(failure) if failure.kind() == JobFailureKind::Conflict => {
            TerminalOutcome::Conflict
        }
        TerminalResult::Failed(failure) if failure.kind() == JobFailureKind::Partial => {
            TerminalOutcome::PartialFailure
        }
        TerminalResult::Failed(_) => TerminalOutcome::Failed,
    }
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
    if let Some(TrackedOperation::PermanentDelete(request)) = request {
        let count = request.targets().len();
        return if count == 1 {
            request.targets()[0]
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "item".to_owned())
        } else {
            format!("{count} items")
        };
    }
    request
        .and_then(|request| request.source().file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "item".to_owned())
}

fn operation_verb(request: Option<&TrackedOperation>) -> &'static str {
    match request {
        Some(TrackedOperation::Copy(_)) => "Copy",
        Some(TrackedOperation::Create(_)) => "Create",
        Some(TrackedOperation::Move(_)) => "Move",
        Some(TrackedOperation::Rename(_)) => "Rename",
        Some(TrackedOperation::Trash(_)) => "Move to Trash",
        Some(TrackedOperation::PermanentDelete(_)) => "Delete Permanently",
        Some(TrackedOperation::Restore(_)) => "Restore",
        Some(TrackedOperation::UndoMove { .. }) => "Undo Move",
        Some(TrackedOperation::PersistentHistoryAction { .. }) => "Undo/Redo",
        Some(TrackedOperation::Replace(_)) => "Replace",
        None => "Operation",
    }
}

fn operation_verb_ing(request: Option<&TrackedOperation>) -> &'static str {
    match request {
        Some(TrackedOperation::Copy(_)) => "Copying",
        Some(TrackedOperation::Create(_)) => "Creating",
        Some(TrackedOperation::Move(_)) => "Moving",
        Some(TrackedOperation::Rename(_)) => "Renaming",
        Some(TrackedOperation::Trash(_)) => "Moving to Trash",
        Some(TrackedOperation::PermanentDelete(_)) => "Deleting permanently",
        Some(TrackedOperation::Restore(_)) => "Restoring",
        Some(TrackedOperation::UndoMove { .. }) => "Undoing move for",
        Some(TrackedOperation::PersistentHistoryAction { .. }) => "Applying Undo/Redo for",
        Some(TrackedOperation::Replace(_)) => "Replacing",
        None => "Working on",
    }
}

fn waiting_detail(request: Option<TrackedOperation>) -> &'static str {
    match request {
        Some(TrackedOperation::Copy(_)) => "Waiting to copy…",
        Some(TrackedOperation::Create(_)) => "Waiting to create…",
        Some(TrackedOperation::Move(_)) => "Waiting to move…",
        Some(TrackedOperation::Rename(_)) => "Waiting to rename…",
        Some(TrackedOperation::Trash(_)) => "Waiting to move to Trash…",
        Some(TrackedOperation::PermanentDelete(_)) => "Preparing permanent deletion…",
        Some(TrackedOperation::Restore(_)) => "Waiting to restore…",
        Some(TrackedOperation::UndoMove { .. }) => "Waiting to undo move…",
        Some(TrackedOperation::PersistentHistoryAction { .. }) => "Waiting to apply Undo/Redo…",
        Some(TrackedOperation::Replace(_)) => "Waiting to replace safely…",
        None => "Waiting…",
    }
}

fn running_detail(request: Option<TrackedOperation>) -> &'static str {
    match request {
        Some(TrackedOperation::Copy(_)) => "Preparing copy…",
        Some(TrackedOperation::Create(_)) => "Creating item…",
        Some(TrackedOperation::Move(_)) => "Moving on this filesystem…",
        Some(TrackedOperation::Rename(_)) => "Renaming…",
        Some(TrackedOperation::Trash(_)) => "Moving to Trash through GIO…",
        Some(TrackedOperation::PermanentDelete(_)) => "Deleting permanently…",
        Some(TrackedOperation::Restore(_)) => "Restoring to the original location…",
        Some(TrackedOperation::UndoMove { .. }) => "Restoring the original location…",
        Some(TrackedOperation::PersistentHistoryAction { .. }) => "Applying durable Undo/Redo…",
        Some(TrackedOperation::Replace(_)) => "Preparing backup and replacing…",
        None => "Working…",
    }
}

fn completed_title(request: Option<&TrackedOperation>) -> &'static str {
    match request {
        Some(TrackedOperation::Copy(_)) => "Copy complete",
        Some(TrackedOperation::Create(_)) => "Creation complete",
        Some(TrackedOperation::Move(_)) => "Move complete",
        Some(TrackedOperation::Rename(_)) => "Rename complete",
        Some(TrackedOperation::Trash(_)) => "Moved to Trash",
        Some(TrackedOperation::PermanentDelete(_)) => "Deleted permanently",
        Some(TrackedOperation::Restore(_)) => "Restore complete",
        Some(TrackedOperation::UndoMove { .. }) => "Undo complete",
        Some(TrackedOperation::PersistentHistoryAction { .. }) => "Undo/Redo complete",
        Some(TrackedOperation::Replace(_)) => "Replacement complete",
        None => "Operation complete",
    }
}

fn completed_detail(request: Option<&TrackedOperation>) -> &'static str {
    match request {
        Some(TrackedOperation::Copy(_)) => "Copied successfully",
        Some(TrackedOperation::Create(_)) => "Created successfully",
        Some(TrackedOperation::Move(_)) => "Moved successfully",
        Some(TrackedOperation::Rename(_)) => "Renamed successfully",
        Some(TrackedOperation::Trash(_)) => "Item is available in Trash",
        Some(TrackedOperation::PermanentDelete(_)) => "Permanent deletion completed",
        Some(TrackedOperation::Restore(_)) => "Restored to the original location",
        Some(TrackedOperation::UndoMove { .. }) => "Moved back to the original location",
        Some(TrackedOperation::PersistentHistoryAction { .. }) => "Undo/Redo completed safely",
        Some(TrackedOperation::Replace(_)) => "Replaced with durable Undo available",
        None => "Completed successfully",
    }
}

fn completed_toast(request: Option<&TrackedOperation>) -> String {
    match request {
        Some(TrackedOperation::Trash(_)) => {
            format!("Moved {} to Trash", operation_name(request))
        }
        Some(TrackedOperation::Copy(_)) => format!("Copied {}", operation_name(request)),
        Some(TrackedOperation::Create(_)) => format!("Created {}", operation_name(request)),
        Some(TrackedOperation::Move(_)) => format!("Moved {}", operation_name(request)),
        Some(TrackedOperation::Rename(_)) => format!("Renamed {}", operation_name(request)),
        Some(TrackedOperation::PermanentDelete(_)) => {
            format!("Deleted {} permanently", operation_name(request))
        }
        Some(TrackedOperation::Restore(_)) => format!("Restored {}", operation_name(request)),
        Some(TrackedOperation::UndoMove { .. }) => {
            format!("Undid move for {}", operation_name(request))
        }
        Some(TrackedOperation::PersistentHistoryAction { .. }) => {
            format!("Updated {} from durable history", operation_name(request))
        }
        Some(TrackedOperation::Replace(_)) => {
            format!(
                "Replaced {} with durable Undo available",
                operation_name(request)
            )
        }
        None => "Operation completed".to_owned(),
    }
}

fn failure_summary(request: Option<&TrackedOperation>, failure: &JobFailure) -> &'static str {
    match failure.kind() {
        JobFailureKind::Conflict => "The destination already exists",
        JobFailureKind::PermissionDenied => "Permission was denied",
        JobFailureKind::Partial => match request {
            Some(TrackedOperation::Restore(_)) => {
                "Item restored, but Trash metadata cleanup failed"
            }
            _ => "Some items were deleted permanently before the failure",
        },
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
            format!("Keep the existing item or choose a different name for {name}.")
        }
        JobFailureKind::Conflict => {
            format!("Keep the existing destination or retry {name} with a different filename.")
        }
        JobFailureKind::PermissionDenied => {
            format!("Could not change {name}. Check folder permissions and try again.")
        }
        JobFailureKind::Partial => failure.message().to_owned(),
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

fn archive_completed_title(outcome: Option<&ArchiveOutcome>) -> &'static str {
    match outcome {
        Some(ArchiveOutcome::Extracted { .. }) => "Archive extracted",
        Some(ArchiveOutcome::Compressed { .. }) => "Archive created",
        Some(ArchiveOutcome::Listed { .. }) => "Archive listed",
        None => "Archive operation completed",
    }
}

fn archive_completed_detail(outcome: Option<&ArchiveOutcome>) -> &'static str {
    match outcome {
        Some(ArchiveOutcome::Extracted { .. }) => "Extracted files are ready",
        Some(ArchiveOutcome::Compressed { .. }) => "The archive was published without overwriting",
        Some(ArchiveOutcome::Listed { .. }) => "Archive contents are ready",
        None => "Archive work completed",
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        num::NonZeroU64,
        path::PathBuf,
        thread,
        time::{Duration, Instant},
    };

    use floe_core::{
        ConflictPolicy, FileIdentity, JobFailure, JobState, MoveRequest, PermanentDeleteRequest,
        RenameRequest,
    };
    use tempfile::tempdir;

    use super::*;
    use crate::{
        trash_executor::TrashRequest,
        undo_history::{UndoHistoryStore, UndoRecipe},
    };

    fn wait_for_terminal(state: &ApplicationState, job_id: JobId) -> JobState {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(job_state) = state
                .jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .record(job_id)
                .map(|record| record.state())
                && job_state.is_terminal()
            {
                return job_state;
            }
            assert!(Instant::now() < deadline, "job did not become terminal");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn phase_5b_only_failed_and_cancelled_operations_are_retryable() {
        assert!(!outcome_is_retryable(TerminalOutcome::Completed));
        assert!(outcome_is_retryable(TerminalOutcome::Cancelled));
        assert!(!outcome_is_retryable(TerminalOutcome::Conflict));
        assert!(!outcome_is_retryable(TerminalOutcome::PartialFailure));
        assert!(outcome_is_retryable(TerminalOutcome::Failed));
    }

    #[test]
    fn phase_6w_ui_labels_trash_history_and_exposes_only_valid_action() {
        let fixture = tempdir().expect("fixture");
        let original = fixture.path().join("Documents/report.txt");
        let payload = fixture.path().join("Trash/files/report.txt");
        let info = fixture.path().join("Trash/info/report.txt.trashinfo");
        fs::create_dir_all(payload.parent().expect("payload parent")).expect("files");
        fs::create_dir_all(info.parent().expect("info parent")).expect("info");
        fs::write(&payload, b"report").expect("payload");
        fs::write(&info, b"[Trash Info]\n").expect("metadata");
        let store =
            UndoHistoryStore::open_at(fixture.path().join("state/undo.bin")).expect("history");
        let ticket = store
            .begin(UndoRecipe::trash(&original, &payload, &info))
            .expect("begin");
        store
            .complete_trash(
                ticket,
                FileIdentity::capture(&payload).expect("payload identity"),
                FileIdentity::capture(&info).expect("info identity"),
            )
            .expect("complete");
        let record = store.history().pop().expect("record");
        let item = persistent_history_item(&record);
        assert_eq!(item.title, "Trash report.txt");
        assert!(item.detail.contains("Undo available"));
        assert!(item.can_undo);
        assert!(!item.can_redo);
    }

    #[test]
    fn phase_5e_conflicts_have_a_distinct_non_retryable_terminal_outcome() {
        let conflict = JobFailure::new(JobFailureKind::Conflict, "destination exists");
        let failure = JobFailure::new(JobFailureKind::Io, "filesystem unavailable");

        assert_eq!(
            terminal_outcome(&TerminalResult::Failed(&conflict)),
            TerminalOutcome::Conflict
        );
        assert_eq!(
            terminal_outcome(&TerminalResult::Failed(&failure)),
            TerminalOutcome::Failed
        );
    }

    #[test]
    fn phase_6m_feedback_is_truthful_and_partial_failure_is_not_retryable() {
        let request = TrackedOperation::PermanentDelete(
            PermanentDeleteRequest::new(vec![
                PathBuf::from("/virtual/first"),
                PathBuf::from("/virtual/second"),
            ])
            .expect("fixture request should be valid"),
        );
        let partial = JobFailure::new(
            JobFailureKind::Partial,
            "permanent deletion stopped after removing 1 of 2 planned items",
        );

        assert_eq!(
            terminal_outcome(&TerminalResult::Failed(&partial)),
            TerminalOutcome::PartialFailure
        );
        assert_eq!(
            operation_title(Some(&request)),
            "Deleting permanently 2 items"
        );
        assert_eq!(completed_title(Some(&request)), "Deleted permanently");
        assert_eq!(
            failure_recovery(Some(&request), &partial),
            partial.message()
        );
        assert!(!failure_recovery(Some(&request), &partial).contains("Retry"));
    }

    #[test]
    fn reliability_partial_title_never_calls_non_delete_work_permanent_deletion() {
        let moved = TrackedOperation::Move(MoveRequest::new(
            PathBuf::from("/source/video.mkv"),
            PathBuf::from("/destination/video.mkv"),
            ConflictPolicy::FailIfExists,
        ));
        let history = TrackedOperation::PersistentHistoryAction {
            history_id: 7,
            action: UndoHistoryAction::Undo,
            source: Some(PathBuf::from("/source/video.mkv")),
            destination: PathBuf::from("/destination/video.mkv"),
            completed_result: None,
        };
        let deleted = TrackedOperation::PermanentDelete(
            PermanentDeleteRequest::new(vec![PathBuf::from("/virtual/video.mkv")])
                .expect("permanent-delete fixture"),
        );

        assert_eq!(
            standard_failure_title(Some(&moved), JobFailureKind::Partial),
            "Move partially completed"
        );
        assert_eq!(
            standard_failure_title(Some(&history), JobFailureKind::Partial),
            "Undo/Redo needs review"
        );
        assert_eq!(
            standard_failure_title(None, JobFailureKind::Partial),
            "Operation partially completed"
        );
        assert_eq!(
            standard_failure_title(Some(&deleted), JobFailureKind::Partial),
            "Permanent deletion partially completed"
        );
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
    fn failed_terminal_retry_does_not_make_later_popups_permanent() {
        let failed_job = JobId::new(NonZeroU64::new(43).expect("non-zero fixture job id"));
        let later_job = JobId::new(NonZeroU64::new(44).expect("non-zero fixture job id"));
        let conflicts = ConflictInteractions::default();

        let retryable = updated_retryable_job(None, failed_job, TerminalOutcome::Failed);
        let failed_action = available_terminal_action(&conflicts, retryable);
        assert_eq!(failed_action, Some(TerminalAction::Retry(failed_job)));
        assert!(terminal_action_auto_hides(failed_action));

        let retained_retry =
            updated_retryable_job(retryable, later_job, TerminalOutcome::Completed);
        let later_action = available_terminal_action(&conflicts, retained_retry);
        assert_eq!(later_action, Some(TerminalAction::Retry(failed_job)));
        assert!(terminal_action_auto_hides(later_action));
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
            "Keep the existing item or choose a different name for notes.txt."
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
    fn phase_6p_ui_progress_and_batch_summaries_are_truthful() {
        let items = JobProgress::items(2, Some(5)).expect("item progress");
        assert_eq!(progress_detail(items, None), "2 of 5 items");

        let mut telemetry = TransferTelemetry::default();
        let start = JobProgress::bytes(0, Some(2_048)).expect("starting byte progress");
        assert_eq!(telemetry.observe(Duration::ZERO, start), None);
        let progressed = JobProgress::bytes(1_024, Some(2_048)).expect("byte progress");
        let estimate = telemetry
            .observe(Duration::from_secs(1), progressed)
            .expect("meaningful byte progress should estimate");
        assert_eq!(
            progress_detail(progressed, Some(estimate)),
            "1.0 KiB of 2.0 KiB • 1.0 KiB/s • 1s remaining"
        );

        let batch_id = BatchId::new(7).expect("non-zero batch id");
        let snapshot = BatchSnapshot::new(
            batch_id,
            BatchStatus::CompletedWithIssues,
            5,
            3,
            1,
            1,
            0,
            false,
        );
        assert_eq!(
            batch_summary(snapshot),
            (
                "Batch completed with issues",
                "5 of 5 items processed • 3 completed • 1 skipped • 1 failed".to_owned()
            )
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

    #[test]
    fn phase_5f_conflict_action_survives_dismissal_and_yields_to_retry_after_resolution() {
        let conflict_job = JobId::new(NonZeroU64::new(51).expect("non-zero conflict job id"));
        let retry_job = JobId::new(NonZeroU64::new(52).expect("non-zero retry job id"));
        let mut conflicts = ConflictInteractions::default();
        conflicts.enqueue(conflict_job);

        assert_eq!(
            available_terminal_action(&conflicts, Some(retry_job)),
            Some(TerminalAction::ResolveConflict(conflict_job))
        );
        assert!(conflicts.begin_dialog(conflict_job));
        assert!(!conflicts.begin_dialog(conflict_job));
        assert!(conflicts.dismiss_dialog(conflict_job));
        assert_eq!(conflicts.current(), Some(conflict_job));
        assert!(conflicts.begin_dialog(conflict_job));

        conflicts.resolve(conflict_job);
        assert_eq!(
            available_terminal_action(&conflicts, Some(retry_job)),
            Some(TerminalAction::Retry(retry_job))
        );
    }

    #[test]
    fn phase_5f_retry_name_requires_one_different_filename_without_normalizing_it() {
        let existing = OsStr::new("notes.txt");

        assert_eq!(
            conflict_retry_name("", Some(existing)),
            Err("Enter a different filename")
        );
        assert_eq!(
            conflict_retry_name("folder/notes.txt", Some(existing)),
            Err("Enter one filename without slashes")
        );
        assert_eq!(
            conflict_retry_name("notes.txt", Some(existing)),
            Err("Choose a different filename")
        );
        assert_eq!(
            conflict_retry_name(" notes (copy).txt ", Some(existing)),
            Ok(OsString::from(" notes (copy).txt "))
        );
    }

    #[test]
    fn phase_5f_ui_decisions_keep_existing_or_submit_a_fresh_named_retry() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source_directory = fixture.path().join("source");
        let destination_directory = fixture.path().join("destination");
        fs::create_dir(&source_directory).expect("source directory");
        fs::create_dir(&destination_directory).expect("destination directory");
        let state = ApplicationState::new().expect("application state should start");

        let kept_source = source_directory.join("keep-item");
        let kept_destination = destination_directory.join("keep-item");
        fs::write(&kept_source, b"incoming").expect("incoming keep fixture");
        fs::write(&kept_destination, b"existing").expect("existing keep fixture");
        state
            .stage_copy(kept_source.clone())
            .expect("keep copy should stage");
        let kept_conflict = state
            .submit_paste(&destination_directory)
            .expect("keep conflict should submit");
        assert_eq!(
            wait_for_terminal(&state, kept_conflict.job_id()),
            JobState::Failed
        );
        state.finish_operation(kept_conflict.job_id(), TerminalOutcome::Conflict);
        assert_eq!(
            state
                .resolve_conflict(kept_conflict.job_id(), ConflictDecision::KeepExisting)
                .expect("keep-existing decision should resolve"),
            ConflictResolution::KeptExisting
        );
        assert_eq!(fs::read(kept_source).expect("source remains"), b"incoming");
        assert_eq!(
            fs::read(kept_destination).expect("existing remains"),
            b"existing"
        );

        let retry_source = source_directory.join("retry-item");
        let retry_destination = destination_directory.join("retry-item");
        fs::write(&retry_source, b"incoming-retry").expect("incoming retry fixture");
        fs::write(&retry_destination, b"existing-retry").expect("existing retry fixture");
        state
            .stage_copy(retry_source)
            .expect("retry copy should stage");
        let retry_conflict = state
            .submit_paste(&destination_directory)
            .expect("retry conflict should submit");
        assert_eq!(
            wait_for_terminal(&state, retry_conflict.job_id()),
            JobState::Failed
        );
        state.finish_operation(retry_conflict.job_id(), TerminalOutcome::Conflict);
        let ConflictResolution::Retried(retried) = state
            .resolve_conflict(
                retry_conflict.job_id(),
                ConflictDecision::RetryWithName(OsString::from("retry-item-copy")),
            )
            .expect("retry-name decision should submit")
        else {
            panic!("retry-name decision should create a fresh attempt");
        };
        assert_eq!(retried.operation_id(), retry_conflict.operation_id());
        assert_ne!(retried.job_id(), retry_conflict.job_id());
        assert_eq!(
            wait_for_terminal(&state, retried.job_id()),
            JobState::Completed
        );
        assert_eq!(
            fs::read(retry_destination).expect("existing retry remains"),
            b"existing-retry"
        );
        assert_eq!(
            fs::read(destination_directory.join("retry-item-copy")).expect("revised retry exists"),
            b"incoming-retry"
        );
    }
}
