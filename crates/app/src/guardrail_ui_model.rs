//! GTK-independent presentation model for Phase 18X guardrail review.

use std::{
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use floe_core::{DestructiveAction, PreflightRisk, ProtectedRoots};

use crate::guardrail_controller::GuardrailReview;

pub const GUARDRAIL_LIMITATION_TEXT: &str = "Protected Folders help prevent accidental changes in Floe. They do not encrypt files, change permissions, restrict other applications, or provide access control. Path checks are lexical and do not resolve symbolic-link or hard-link aliases.";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GuardrailActionStates {
    pub protect: bool,
    pub unprotect: bool,
}

pub fn guardrail_action_states(
    target: Option<&Path>,
    policy: &ProtectedRoots,
    store_blocked: bool,
    policy_busy: bool,
) -> GuardrailActionStates {
    let exact_protected =
        target.is_some_and(|target| policy.roots().iter().any(|protected| protected == target));
    let available = target.is_some() && !store_blocked && !policy_busy;
    GuardrailActionStates {
        protect: available && !exact_protected,
        unprotect: available && exact_protected,
    }
}

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
        let plural = if target_count == 1 { "" } else { "s" };
        Self {
            kind: GuardrailDialogKind::Confirmation,
            title: "Review Destructive Operation",
            heading: "Confirm exact action scope",
            body: format!(
                "Review {target_count} exact target{plural} before continuing. Reasons: {risks}."
            ),
            accessible_label: format!(
                "Data-loss guardrail confirmation. {target_count} exact target{plural}. Reasons: {risks}."
            ),
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
            accessible_label: "Guardrail policy storage error. Destructive actions are blocked."
                .to_owned(),
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

fn risk_label(risk: PreflightRisk) -> &'static str {
    match risk {
        PreflightRisk::ProtectedPath => "protected path",
        PreflightRisk::IrreversibleAction => "irreversible action",
        PreflightRisk::LargeItemCount => "large item count",
        PreflightRisk::LargeByteCount => "large byte count",
        PreflightRisk::DeepTree => "deep tree",
        PreflightRisk::FilesystemRoot => "filesystem root",
        PreflightRisk::MountRoot => "mounted-filesystem root",
        PreflightRisk::HomeDirectory => "home-directory root",
        PreflightRisk::IncompleteScan => "incomplete scan",
        PreflightRisk::ScanLimitExceeded => "scan limit exceeded",
        PreflightRisk::ArithmeticOverflow => "arithmetic overflow",
        PreflightRisk::UnknownFacts => "unknown facts",
    }
}

fn visible_path(path: &Path) -> String {
    if let Some(path) = path.to_str() {
        return path.to_owned();
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
