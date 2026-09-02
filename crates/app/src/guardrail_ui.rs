//! Native Phase 18X confirmation-dialog policy and accessible summary model.

use std::{
    cell::RefCell,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use adw::prelude::*;
use floe_core::{DestructiveAction, DestructiveScope, PreflightRisk};

use crate::{
    guardrail_controller::{
        GuardrailAuthorizationItem, GuardrailConfirmation, GuardrailPoll, GuardrailResolution,
        GuardrailReview,
    },
    guardrail_preflight::PreflightEnvironment,
    state::ApplicationState,
};

pub const GUARDRAIL_LIMITATION_TEXT: &str = "Protected Folders help prevent accidental changes in Floe. They do not encrypt files, change permissions, restrict other applications, or provide access control. Path checks are lexical and do not resolve symbolic-link or hard-link aliases.";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuardrailDialogKind {
    Confirmation,
    StoreBlocked,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuardrailPathSummary {
    exact_path: PathBuf,
    visible_path: String,
    non_utf8: bool,
}

impl GuardrailPathSummary {
    fn new(path: &Path) -> Self {
        Self {
            exact_path: path.to_path_buf(),
            visible_path: visible_path(path),
            non_utf8: path.to_str().is_none(),
        }
    }

    pub fn exact_path(&self) -> &Path {
        &self.exact_path
    }

    pub fn visible_path(&self) -> &str {
        &self.visible_path
    }

    pub const fn is_non_utf8(&self) -> bool {
        self.non_utf8
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuardrailScopeSummary {
    action: DestructiveAction,
    targets: Vec<GuardrailPathSummary>,
    destination: Option<GuardrailPathSummary>,
}

impl GuardrailScopeSummary {
    pub const fn action(&self) -> DestructiveAction {
        self.action
    }

    pub fn targets(&self) -> &[GuardrailPathSummary] {
        &self.targets
    }

    pub fn destination(&self) -> Option<&GuardrailPathSummary> {
        self.destination.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuardrailDialogModel {
    kind: GuardrailDialogKind,
    title: &'static str,
    heading: &'static str,
    body: String,
    accessible_label: String,
    confirm_label: Option<&'static str>,
    scopes: Vec<GuardrailScopeSummary>,
    limitation: &'static str,
}

impl GuardrailDialogModel {
    pub fn confirmation(review: &GuardrailReview) -> Self {
        let scopes = review
            .scopes()
            .map(|scope| GuardrailScopeSummary {
                action: scope.action(),
                targets: scope
                    .targets()
                    .iter()
                    .map(|path| GuardrailPathSummary::new(path))
                    .collect(),
                destination: scope.destination().map(GuardrailPathSummary::new),
            })
            .collect::<Vec<_>>();
        let target_count = scopes
            .iter()
            .map(|scope| scope.targets.len())
            .sum::<usize>();
        let risks = review
            .risks()
            .iter()
            .map(|risk| risk_label(*risk))
            .collect::<Vec<_>>()
            .join(", ");
        let body = format!(
            "Review {} exact target{} before continuing. Reasons: {}.",
            target_count,
            if target_count == 1 { "" } else { "s" },
            risks
        );
        let accessible_label = format!(
            "Data-loss guardrail confirmation. {} exact target{}. Reasons: {}.",
            target_count,
            if target_count == 1 { "" } else { "s" },
            risks
        );
        Self {
            kind: GuardrailDialogKind::Confirmation,
            title: "Review Destructive Operation",
            heading: "Confirm this exact action and scope",
            body,
            accessible_label,
            confirm_label: Some("Confirm and Continue"),
            scopes,
            limitation: GUARDRAIL_LIMITATION_TEXT,
        }
    }

    pub fn store_blocked() -> Self {
        Self {
            kind: GuardrailDialogKind::StoreBlocked,
            title: "Guardrails Unavailable",
            heading: "Destructive actions remain blocked",
            body: "Floe could not safely load the Protected Folder policy. Review and acknowledge the storage problem before resetting or replacing that policy.".to_owned(),
            accessible_label: "Guardrail policy storage error. Destructive actions are blocked.".to_owned(),
            confirm_label: None,
            scopes: Vec::new(),
            limitation: GUARDRAIL_LIMITATION_TEXT,
        }
    }

    pub fn cancelled() -> Self {
        Self {
            kind: GuardrailDialogKind::Cancelled,
            title: "Preflight Cancelled",
            heading: "No destructive action was authorized",
            body: "The operation-scale scan was cancelled before authorization.".to_owned(),
            accessible_label:
                "Guardrail preflight cancelled. No destructive action was authorized.".to_owned(),
            confirm_label: None,
            scopes: Vec::new(),
            limitation: GUARDRAIL_LIMITATION_TEXT,
        }
    }

    pub const fn kind(&self) -> GuardrailDialogKind {
        self.kind
    }

    pub const fn title(&self) -> &'static str {
        self.title
    }

    pub const fn heading(&self) -> &'static str {
        self.heading
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn accessible_label(&self) -> &str {
        &self.accessible_label
    }

    pub const fn confirm_label(&self) -> Option<&'static str> {
        self.confirm_label
    }

    pub fn scopes(&self) -> &[GuardrailScopeSummary] {
        &self.scopes
    }

    pub const fn limitation(&self) -> &'static str {
        self.limitation
    }

    pub fn exact_target_count(&self) -> usize {
        self.scopes.iter().map(|scope| scope.targets.len()).sum()
    }
}

#[derive(Clone)]
pub struct GuardrailDialogWidgets {
    pub dialog: adw::Dialog,
    pub cancel_button: gtk::Button,
    pub confirm_button: Option<gtk::Button>,
}

type GuardrailAuthorizationCallback =
    Rc<RefCell<Option<Box<dyn FnOnce(Vec<GuardrailAuthorizationItem>)>>>>;

pub fn build_guardrail_dialog(model: &GuardrailDialogModel) -> GuardrailDialogWidgets {
    let heading = gtk::Label::builder()
        .label(model.heading())
        .wrap(true)
        .xalign(0.0)
        .build();
    heading.add_css_class("title-2");
    let body = gtk::Label::builder()
        .label(model.body())
        .wrap(true)
        .xalign(0.0)
        .build();
    let scope_buffer = gtk::TextBuffer::builder()
        .text(render_scope_text(model.scopes()))
        .build();
    let scope_view = gtk::TextView::builder()
        .buffer(&scope_buffer)
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(8)
        .right_margin(8)
        .build();
    scope_view.update_property(&[
        gtk::accessible::Property::Label("Exact destructive-operation scope"),
        gtk::accessible::Property::Description(
            "Exact source targets and destinations included in this confirmation",
        ),
    ]);
    scope_view.set_visible(!model.scopes().is_empty());
    let scope_scroller = gtk::ScrolledWindow::builder()
        .min_content_height(96)
        .max_content_height(260)
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&scope_view)
        .build();
    scope_scroller.set_visible(!model.scopes().is_empty());
    let limitation = gtk::Label::builder()
        .label(model.limitation())
        .wrap(true)
        .xalign(0.0)
        .build();
    limitation.add_css_class("floe-status");
    limitation.update_property(&[gtk::accessible::Property::Description(
        "Protected Folder limitations",
    )]);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.set_halign(gtk::Align::End);
    let cancel_button = gtk::Button::with_label(if model.confirm_label().is_some() {
        "Cancel"
    } else {
        "Close"
    });
    actions.append(&cancel_button);
    let confirm_button = model.confirm_label().map(|label| {
        let button = gtk::Button::with_label(label);
        button.add_css_class("destructive-action");
        button.update_property(&[gtk::accessible::Property::Description(
            "Authorize only the exact action and scope shown above",
        )]);
        actions.append(&button);
        button
    });

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    content.append(&heading);
    content.append(&body);
    content.append(&scope_scroller);
    content.append(&limitation);
    content.append(&actions);
    let dialog = adw::Dialog::builder()
        .title(model.title())
        .content_width(620)
        .content_height(460)
        .child(&content)
        .build();
    dialog.update_property(&[gtk::accessible::Property::Label(model.accessible_label())]);
    GuardrailDialogWidgets {
        dialog,
        cancel_button,
        confirm_button,
    }
}

pub fn review_and_authorize(
    window: &adw::ApplicationWindow,
    state: Rc<ApplicationState>,
    scopes: Vec<DestructiveScope>,
    environment: PreflightEnvironment,
    on_authorized: impl FnOnce(Vec<GuardrailAuthorizationItem>) + 'static,
) {
    let submission = match state.begin_guardrail_review(scopes, environment) {
        Ok(submission) => submission,
        Err(error) => {
            present_guardrail_error(window, &error.to_string());
            return;
        }
    };
    let generation = submission.generation();
    let callback = Rc::new(RefCell::new(Some(
        Box::new(on_authorized) as Box<dyn FnOnce(Vec<GuardrailAuthorizationItem>)>
    )));
    let weak_window = window.downgrade();

    glib::timeout_add_local(Duration::from_millis(24), move || {
        let Some(window) = weak_window.upgrade() else {
            let _ = state.cancel_guardrail_review(generation);
            return glib::ControlFlow::Break;
        };
        match state.poll_guardrail_review(generation) {
            Ok(GuardrailPoll::Pending) => glib::ControlFlow::Continue,
            Ok(GuardrailPoll::Allowed(authorization)) => {
                if let Some(callback) = callback.borrow_mut().take() {
                    callback(authorization.into_items());
                }
                glib::ControlFlow::Break
            }
            Ok(GuardrailPoll::ReviewRequired(review)) => {
                present_confirmation_review(
                    &window,
                    Rc::clone(&state),
                    review,
                    Rc::clone(&callback),
                );
                glib::ControlFlow::Break
            }
            Ok(GuardrailPoll::Cancelled) => {
                present_guardrail_model(&window, GuardrailDialogModel::cancelled());
                glib::ControlFlow::Break
            }
            Ok(GuardrailPoll::Blocked(_)) => {
                present_guardrail_model(&window, GuardrailDialogModel::store_blocked());
                glib::ControlFlow::Break
            }
            Err(error) => {
                present_guardrail_error(&window, &error.to_string());
                glib::ControlFlow::Break
            }
        }
    });
}

fn present_confirmation_review(
    window: &adw::ApplicationWindow,
    state: Rc<ApplicationState>,
    review: GuardrailReview,
    callback: GuardrailAuthorizationCallback,
) {
    let generation = review.generation();
    let widgets = build_guardrail_dialog(&GuardrailDialogModel::confirmation(&review));
    let dialog = widgets.dialog.downgrade();
    let state_for_cancel = Rc::clone(&state);
    widgets.cancel_button.connect_clicked(move |_| {
        let _ = state_for_cancel.resolve_guardrail_review(generation, GuardrailConfirmation::Deny);
        if let Some(dialog) = dialog.upgrade() {
            dialog.close();
        }
    });
    if let Some(confirm_button) = widgets.confirm_button.as_ref() {
        let dialog = widgets.dialog.downgrade();
        let callback = Rc::clone(&callback);
        confirm_button.connect_clicked(move |button| {
            button.set_sensitive(false);
            match state.resolve_guardrail_review(generation, GuardrailConfirmation::Confirm) {
                Ok(GuardrailResolution::Allowed(authorization)) => {
                    if let Some(callback) = callback.borrow_mut().take() {
                        callback(authorization.into_items());
                    }
                    if let Some(dialog) = dialog.upgrade() {
                        dialog.close();
                    }
                }
                Ok(GuardrailResolution::Denied) => {
                    if let Some(dialog) = dialog.upgrade() {
                        dialog.close();
                    }
                }
                Err(error) => {
                    button.set_sensitive(true);
                    if let Some(dialog) = dialog.upgrade() {
                        if let Some(window) = dialog.root().and_downcast::<adw::ApplicationWindow>()
                        {
                            present_guardrail_error(&window, &error.to_string());
                        }
                    }
                }
            }
        });
    }
    widgets.dialog.present(Some(window));
}

fn present_guardrail_model(window: &adw::ApplicationWindow, model: GuardrailDialogModel) {
    let widgets = build_guardrail_dialog(&model);
    let dialog = widgets.dialog.downgrade();
    widgets.cancel_button.connect_clicked(move |_| {
        if let Some(dialog) = dialog.upgrade() {
            dialog.close();
        }
    });
    widgets.dialog.present(Some(window));
}

fn present_guardrail_error(window: &adw::ApplicationWindow, detail: &str) {
    let dialog = adw::AlertDialog::builder()
        .heading("Destructive action blocked")
        .body(format!("Floe did not authorize this action: {detail}"))
        .build();
    dialog.add_response("close", "Close");
    dialog.present(Some(window));
}

fn render_scope_text(scopes: &[GuardrailScopeSummary]) -> String {
    let mut output = String::new();
    for (index, scope) in scopes.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        output.push_str(action_label(scope.action));
        output.push_str(":\n");
        for target in &scope.targets {
            output.push_str("  Target: ");
            output.push_str(target.visible_path());
            if target.is_non_utf8() {
                output.push_str(" (raw-byte filename)");
            }
            output.push('\n');
        }
        if let Some(destination) = &scope.destination {
            output.push_str("  Destination: ");
            output.push_str(destination.visible_path());
            if destination.is_non_utf8() {
                output.push_str(" (raw-byte filename)");
            }
            output.push('\n');
        }
    }
    output
}

fn action_label(action: DestructiveAction) -> &'static str {
    match action {
        DestructiveAction::Trash => "Move to Trash",
        DestructiveAction::PermanentDelete => "Delete Permanently",
        DestructiveAction::Move => "Move",
        DestructiveAction::Rename => "Rename",
        DestructiveAction::Overwrite => "Overwrite",
    }
}

fn risk_label(risk: PreflightRisk) -> &'static str {
    match risk {
        PreflightRisk::ProtectedPath => "Protected Folder intersection",
        PreflightRisk::IrreversibleAction => "irreversible action",
        PreflightRisk::LargeItemCount => "large item count",
        PreflightRisk::LargeByteCount => "large byte count",
        PreflightRisk::DeepTree => "deep folder tree",
        PreflightRisk::FilesystemRoot => "filesystem root",
        PreflightRisk::MountRoot => "mounted-filesystem boundary",
        PreflightRisk::HomeDirectory => "home-directory boundary",
        PreflightRisk::IncompleteScan => "incomplete scan",
        PreflightRisk::ScanLimitExceeded => "scan limit reached",
        PreflightRisk::ArithmeticOverflow => "operation scale overflow",
        PreflightRisk::UnknownFacts => "unknown operation scale",
    }
}

fn visible_path(path: &Path) -> String {
    if let Some(value) = path.to_str() {
        return value
            .chars()
            .flat_map(char::escape_default)
            .collect::<String>();
    }
    let mut output = String::new();
    for byte in path.as_os_str().as_bytes() {
        match byte {
            b' '..=b'~' if *byte != b'\\' => output.push(char::from(*byte)),
            b'\\' => output.push_str("\\\\"),
            _ => output.push_str(&format!("\\x{byte:02X}")),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a real disposable GTK display"]
    fn phase_testing_gtk_phase_18x_guardrail_dialog_accessibility_contract() {
        gtk::init().expect("GTK display");
        let model = GuardrailDialogModel::store_blocked();
        let widgets = build_guardrail_dialog(&model);

        assert_eq!(
            widgets.dialog.accessible_role(),
            gtk::AccessibleRole::Dialog
        );
        assert_eq!(widgets.dialog.title(), model.title());
        assert_eq!(
            widgets.cancel_button.accessible_role(),
            gtk::AccessibleRole::Button
        );
        assert_eq!(widgets.cancel_button.label().as_deref(), Some("Close"));
        assert!(widgets.confirm_button.is_none());
        assert!(
            model
                .accessible_label()
                .contains("Destructive actions are blocked")
        );
        assert!(model.limitation().contains("do not encrypt"));
        assert!(model.limitation().contains("do not resolve symbolic-link"));
    }
}
