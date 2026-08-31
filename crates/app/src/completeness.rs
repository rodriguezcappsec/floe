//! Small GTK-independent Phase 20B2 interaction and presentation policies.

use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

pub const ERROR_SUMMARY_MAX_CHARS: usize = 240;
pub const ERROR_DETAILS_MAX_CHARS: usize = 2_048;
pub const GROUP_HEADER_DESCRIPTION: &str =
    "Collapse or expand this file group; collapsed items remain unchanged";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GroupRowPresentation {
    pub visible: bool,
    pub selectable: bool,
    pub expanded: bool,
}

pub const fn group_row_presentation(
    collapsed: bool,
    is_group_header: bool,
) -> GroupRowPresentation {
    GroupRowPresentation {
        visible: !collapsed || is_group_header,
        selectable: !collapsed,
        expanded: !collapsed,
    }
}

pub fn inverted_positions(item_count: u32, selected: impl IntoIterator<Item = u32>) -> Vec<u32> {
    let mut selected = selected
        .into_iter()
        .filter(|position| *position < item_count)
        .collect::<Vec<_>>();
    selected.sort_unstable();
    selected.dedup();
    let mut selected = selected.into_iter().peekable();
    (0..item_count)
        .filter(|position| {
            if selected.peek().is_some_and(|selected| selected == position) {
                selected.next();
                false
            } else {
                true
            }
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EscapeSurface {
    ContextMenu,
    InlineRename,
    LocationEditor,
    Search,
    QuickPreview,
    Selection,
}

impl EscapeSurface {
    const PRIORITY: [Self; 6] = [
        Self::ContextMenu,
        Self::InlineRename,
        Self::LocationEditor,
        Self::Search,
        Self::QuickPreview,
        Self::Selection,
    ];

    pub fn innermost(active: impl Fn(Self) -> bool) -> Option<Self> {
        Self::PRIORITY.into_iter().find(|surface| active(*surface))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentationScale {
    gtk_scale: u8,
    fractional_percent: u16,
}

impl PresentationScale {
    pub fn new(gtk_scale: i32, fractional_percent: u16) -> Self {
        Self {
            gtk_scale: u8::try_from(gtk_scale.clamp(1, 8)).unwrap_or(1),
            fractional_percent: fractional_percent.clamp(50, 400),
        }
    }

    pub const fn logical_thumbnail_edge(self, configured_edge: u16) -> u16 {
        // GTK owns device-pixel scaling. Cache identity remains the configured
        // logical edge so moving a window between monitors cannot fork cache
        // entries or trigger filesystem work on the main loop.
        configured_edge
    }

    pub fn device_pixel_hint(self, logical: u16) -> u32 {
        u32::from(logical)
            .saturating_mul(u32::from(self.gtk_scale))
            .saturating_mul(u32::from(self.fractional_percent))
            .div_ceil(100)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageId {
    OperationCompleted,
    OperationCompletedBody,
    OperationFailed,
    Details,
    Dismiss,
}

pub const fn message(id: MessageId) -> &'static str {
    match id {
        MessageId::OperationCompleted => "Operation completed",
        MessageId::OperationCompletedBody => {
            "A background file operation finished. Open Floe for details."
        }
        MessageId::OperationFailed => "Operation failed",
        MessageId::Details => "Details",
        MessageId::Dismiss => "Dismiss",
    }
}

pub fn direction_safe_path(path: &Path) -> String {
    // First-strong isolate prevents an RTL filename from reordering surrounding
    // dialog text. The original Path remains authoritative everywhere else.
    format!("\u{2068}{}\u{2069}", path.to_string_lossy())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetailedFeedback {
    pub summary: String,
    pub details: Option<String>,
    pub announce: bool,
}

impl DetailedFeedback {
    pub fn new(summary: &str, details: Option<&str>, announce: bool) -> Self {
        Self {
            summary: bounded_text(summary, ERROR_SUMMARY_MAX_CHARS),
            details: details
                .map(|details| bounded_text(details, ERROR_DETAILS_MAX_CHARS))
                .filter(|details| !details.is_empty()),
            announce,
        }
    }

    pub fn from_failure(message: &str) -> Self {
        let summary = message
            .split_once(':')
            .map(|(summary, _)| summary.trim())
            .filter(|summary| !summary.is_empty())
            .unwrap_or_else(|| message.trim());
        Self::new(summary, Some(message), true)
    }
}

fn bounded_text(value: &str, capacity: usize) -> String {
    let mut value = value
        .chars()
        .filter(|character| !matches!(character, '\0' | '\r'))
        .take(capacity + 1)
        .collect::<String>();
    if value.chars().count() > capacity {
        value = value.chars().take(capacity.saturating_sub(1)).collect();
        value.push('…');
    }
    value
}

pub const fn should_send_completion_notification(window_active: bool) -> bool {
    !window_active
}

pub const fn completion_notification_elapsed_is_eligible(elapsed: std::time::Duration) -> bool {
    elapsed.as_secs() >= 2
}

static NEXT_COMPLETION_NOTIFICATION_NAMESPACE: AtomicU64 = AtomicU64::new(1);

pub fn next_completion_notification_namespace() -> u64 {
    NEXT_COMPLETION_NOTIFICATION_NAMESPACE.fetch_add(1, Ordering::Relaxed)
}

pub fn completion_notification_id(namespace: u64, job_id: u64) -> String {
    format!("window-{namespace}-operation-{job_id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn phase_20b2_invert_selection_is_bounded_sorted_and_deduplicated() {
        assert_eq!(inverted_positions(6, [4, 1, 1, 99]), [0, 2, 3, 5]);
        assert_eq!(inverted_positions(0, [0]), Vec::<u32>::new());
        assert_eq!(inverted_positions(3, []), [0, 1, 2]);
    }

    #[test]
    fn phase_20b2_grouping_collapse_keeps_one_actionable_header_placeholder() {
        assert_eq!(
            group_row_presentation(true, true),
            GroupRowPresentation {
                visible: true,
                selectable: false,
                expanded: false,
            }
        );
        assert_eq!(
            group_row_presentation(true, false),
            GroupRowPresentation {
                visible: false,
                selectable: false,
                expanded: false,
            }
        );
        assert_eq!(
            group_row_presentation(false, false),
            GroupRowPresentation {
                visible: true,
                selectable: true,
                expanded: true,
            }
        );
    }

    #[test]
    fn phase_20b2_focus_escape_chooses_the_innermost_active_surface() {
        let target = EscapeSurface::innermost(|surface| {
            matches!(surface, EscapeSurface::Search | EscapeSurface::Selection)
        });
        assert_eq!(target, Some(EscapeSurface::Search));
        assert_eq!(EscapeSurface::innermost(|_| false), None);
    }

    #[test]
    fn phase_20b2_scaling_keeps_cache_identity_logical_and_bounds_hints() {
        let scale = PresentationScale::new(2, 125);
        assert_eq!(scale.logical_thumbnail_edge(112), 112);
        assert_eq!(scale.device_pixel_hint(112), 280);
        assert_eq!(PresentationScale::new(0, 1).device_pixel_hint(100), 50);
    }

    #[test]
    fn phase_20b2_feedback_i18n_bounds_details_and_isolates_rtl_paths() {
        let feedback = DetailedFeedback::new(
            &"s".repeat(ERROR_SUMMARY_MAX_CHARS + 10),
            Some(&"d".repeat(ERROR_DETAILS_MAX_CHARS + 10)),
            true,
        );
        assert_eq!(feedback.summary.chars().count(), ERROR_SUMMARY_MAX_CHARS);
        assert_eq!(
            feedback
                .details
                .as_deref()
                .map(str::chars)
                .map(Iterator::count),
            Some(ERROR_DETAILS_MAX_CHARS)
        );
        let path = PathBuf::from("/tmp/ملف.txt");
        let display = direction_safe_path(&path);
        assert!(display.starts_with('\u{2068}') && display.ends_with('\u{2069}'));
        assert_eq!(message(MessageId::Details), "Details");
        assert!(should_send_completion_notification(false));
        assert!(!should_send_completion_notification(true));

        let failure =
            DetailedFeedback::from_failure("Could not copy files: destination is read-only");
        assert_eq!(failure.summary, "Could not copy files");
        assert_eq!(
            failure.details.as_deref(),
            Some("Could not copy files: destination is read-only")
        );
        assert!(!message(MessageId::OperationCompletedBody).contains('/'));
    }

    #[test]
    fn phase_20b2_accessibility_contract_uses_actionable_non_color_semantics() {
        assert!(GROUP_HEADER_DESCRIPTION.contains("Collapse or expand"));
        assert!(GROUP_HEADER_DESCRIPTION.contains("unchanged"));
        assert_eq!(message(MessageId::Details), "Details");
        assert_eq!(message(MessageId::Dismiss), "Dismiss");
    }

    #[test]
    fn phase_23b_notification_policy_and_phase_23b_notification_dispatch_are_stable_path_free() {
        use std::time::Duration;

        assert!(!completion_notification_elapsed_is_eligible(
            Duration::from_millis(1_999)
        ));
        assert!(completion_notification_elapsed_is_eligible(
            Duration::from_secs(2)
        ));
        assert!(should_send_completion_notification(false));
        assert!(!should_send_completion_notification(true));
        let first = next_completion_notification_namespace();
        let second = next_completion_notification_namespace();
        assert_ne!(first, second);
        assert_eq!(
            completion_notification_id(first, 42),
            format!("window-{first}-operation-42")
        );
        assert_ne!(
            completion_notification_id(first, 1),
            completion_notification_id(second, 1)
        );
        assert!(!message(MessageId::OperationCompleted).contains('/'));
        assert!(!message(MessageId::OperationCompletedBody).contains('/'));
        assert!(!message(MessageId::OperationCompletedBody).contains("home"));
    }

    #[test]
    fn phase_23_reliability_notification_identity_namespaces_equal_local_job_ids() {
        let first = next_completion_notification_namespace();
        let second = next_completion_notification_namespace();
        let first_id = completion_notification_id(first, 1);
        let second_id = completion_notification_id(second, 1);
        assert_ne!(first_id, second_id);
        assert!(!first_id.contains('/'));
        assert!(!second_id.contains('/'));
    }
}
