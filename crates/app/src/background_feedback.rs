//! Presentation policy and generation-safe state for window-owned background work.
//!
//! The filesystem and security workers remain application-owned. This module only
//! models durable user feedback so navigation, selection, or focus changes cannot
//! accidentally turn a still-running task invisible.

use std::collections::HashMap;

const RETAINED_OUTCOME_CAPACITY: usize = 8;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BackgroundActivity {
    Properties,
    PrivacyInspection,
    ThreatScan,
    MetadataSanitization,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackgroundOutcomeKind {
    Completed,
    Partial,
    Cancelled,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeedbackPresentation {
    pub title: &'static str,
    pub button_label: Option<&'static str>,
    pub action_name: Option<&'static str>,
    pub persistent: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackgroundOutcome {
    pub activity: BackgroundActivity,
    pub generation: u64,
    pub kind: BackgroundOutcomeKind,
}

#[derive(Default)]
pub struct BackgroundFeedbackState {
    active: HashMap<BackgroundActivity, u64>,
    outcomes: Vec<BackgroundOutcome>,
}

impl BackgroundFeedbackState {
    pub fn start(&mut self, activity: BackgroundActivity, generation: u64) -> bool {
        if generation == 0 || self.active.contains_key(&activity) {
            return false;
        }
        self.active.insert(activity, generation);
        true
    }

    pub fn is_active(&self, activity: BackgroundActivity, generation: u64) -> bool {
        self.active.get(&activity).copied() == Some(generation)
    }

    pub fn finish(
        &mut self,
        activity: BackgroundActivity,
        generation: u64,
        kind: BackgroundOutcomeKind,
    ) -> bool {
        if !self.is_active(activity, generation) {
            return false;
        }
        self.active.remove(&activity);
        self.outcomes.push(BackgroundOutcome {
            activity,
            generation,
            kind,
        });
        if self.outcomes.len() > RETAINED_OUTCOME_CAPACITY {
            self.outcomes.remove(0);
        }
        true
    }

    #[cfg(test)]
    fn outcomes(&self) -> &[BackgroundOutcome] {
        &self.outcomes
    }
}

pub fn running_presentation(activity: BackgroundActivity) -> FeedbackPresentation {
    let (title, button_label, action_name) = match activity {
        BackgroundActivity::Properties => ("Loading read-only Properties…", None, None),
        BackgroundActivity::PrivacyInspection => (
            "Inspecting privacy and safety signals locally…",
            Some("Cancel"),
            Some("win.cancel-privacy-inspection"),
        ),
        BackgroundActivity::ThreatScan => (
            "Scanning locally with ClamAV…",
            Some("Cancel"),
            Some("win.cancel-threat-scan"),
        ),
        BackgroundActivity::MetadataSanitization => (
            "Creating and verifying sanitized copies…",
            Some("Cancel"),
            Some("win.cancel-sanitization"),
        ),
    };
    FeedbackPresentation {
        title,
        button_label,
        action_name,
        persistent: true,
    }
}

pub fn stopping_presentation(activity: BackgroundActivity) -> FeedbackPresentation {
    let title = match activity {
        BackgroundActivity::Properties => "Finishing read-only Properties…",
        BackgroundActivity::PrivacyInspection => "Stopping privacy inspection…",
        BackgroundActivity::ThreatScan => "Stopping local ClamAV scan…",
        BackgroundActivity::MetadataSanitization => {
            "Stopping metadata sanitization after the current item…"
        }
    };
    FeedbackPresentation {
        title,
        button_label: None,
        action_name: None,
        persistent: true,
    }
}

pub fn result_action(activity: BackgroundActivity) -> Option<(&'static str, &'static str)> {
    match activity {
        BackgroundActivity::Properties => Some(("View", "win.show-last-properties")),
        BackgroundActivity::PrivacyInspection => {
            Some(("View Results", "win.show-last-privacy-report"))
        }
        BackgroundActivity::ThreatScan => Some(("View Results", "win.show-last-threat-report")),
        BackgroundActivity::MetadataSanitization => {
            Some(("Reveal", "win.reveal-last-sanitized-copy"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_feedback_contract_running_state_is_persistent_and_actionable() {
        for activity in [
            BackgroundActivity::Properties,
            BackgroundActivity::PrivacyInspection,
            BackgroundActivity::ThreatScan,
            BackgroundActivity::MetadataSanitization,
        ] {
            let presentation = running_presentation(activity);
            assert!(presentation.persistent);
            assert!(!presentation.title.is_empty());
        }
        assert_eq!(
            running_presentation(BackgroundActivity::ThreatScan).action_name,
            Some("win.cancel-threat-scan")
        );
        assert_eq!(
            running_presentation(BackgroundActivity::PrivacyInspection).action_name,
            Some("win.cancel-privacy-inspection")
        );
        assert_eq!(
            result_action(BackgroundActivity::MetadataSanitization),
            Some(("Reveal", "win.reveal-last-sanitized-copy"))
        );
    }

    #[test]
    fn background_feedback_lifecycle_survives_unrelated_ui_transitions() {
        let mut state = BackgroundFeedbackState::default();
        assert!(state.start(BackgroundActivity::ThreatScan, 41));

        // Focus, navigation, selection, tab, and pane changes deliberately have no
        // transition in this model; only the matching worker generation can finish it.
        assert!(state.is_active(BackgroundActivity::ThreatScan, 41));
        assert!(!state.finish(
            BackgroundActivity::ThreatScan,
            40,
            BackgroundOutcomeKind::Completed
        ));
        assert!(state.is_active(BackgroundActivity::ThreatScan, 41));
        assert!(state.finish(
            BackgroundActivity::ThreatScan,
            41,
            BackgroundOutcomeKind::Completed
        ));
        assert!(!state.is_active(BackgroundActivity::ThreatScan, 41));
        assert_eq!(state.outcomes().len(), 1);
    }

    #[test]
    fn background_feedback_routing_keeps_concurrent_tasks_and_rejects_stale_results() {
        let mut state = BackgroundFeedbackState::default();
        assert!(state.start(BackgroundActivity::ThreatScan, 100));
        assert!(state.start(BackgroundActivity::PrivacyInspection, 101));
        assert!(!state.start(BackgroundActivity::ThreatScan, 102));
        assert!(!state.finish(
            BackgroundActivity::ThreatScan,
            99,
            BackgroundOutcomeKind::Failed
        ));
        assert!(state.is_active(BackgroundActivity::ThreatScan, 100));
        assert!(state.is_active(BackgroundActivity::PrivacyInspection, 101));
        assert!(state.finish(
            BackgroundActivity::PrivacyInspection,
            101,
            BackgroundOutcomeKind::Cancelled
        ));
        assert!(state.is_active(BackgroundActivity::ThreatScan, 100));
    }
}
