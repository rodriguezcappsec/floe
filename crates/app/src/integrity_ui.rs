//! Native presentation helpers for Phase 18T integrity jobs.
//!
//! Paths stay as `PathBuf` until the view boundary.  The labels below use an
//! escaped byte representation so a non-UTF-8 or control-character filename
//! cannot be mistaken for different text in a result dialog.

use std::{
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use adw::prelude::*;

use crate::{
    integrity::{FingerprintVerification, ManifestEntryVerification, ManifestVerification},
    integrity_executor::{IntegrityOutcome, IntegrityRequest},
};

pub const INTEGRITY_NOTICE: &str =
    "SHA-256 hashes detect byte changes. They do not prove authenticity, safety, or authorship.";
pub const INTEGRITY_MONITOR_NOTICE: &str = "Monitoring notices changes while Floe watches; it is not intrusion detection and does not identify who or what changed a file.";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrityResultRow {
    pub display_path: String,
    pub status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrityPresentation {
    pub title: String,
    pub summary: String,
    pub rows: Vec<IntegrityResultRow>,
    pub notice: &'static str,
}

pub fn private_fingerprint_store_path(data_home: &Path) -> PathBuf {
    data_home
        .join("floe")
        .join("integrity")
        .join("fingerprints-v1")
}
pub fn private_integrity_baseline_store_path(data_home: &Path) -> PathBuf {
    data_home
        .join("floe")
        .join("integrity")
        .join("baselines-v1")
        .join("current")
}

pub fn escaped_path_label(path: &Path) -> String {
    path.as_os_str()
        .as_bytes()
        .iter()
        .flat_map(|byte| std::ascii::escape_default(*byte))
        .map(char::from)
        .collect()
}

pub fn integrity_title(request: Option<&IntegrityRequest>) -> &'static str {
    match request {
        Some(IntegrityRequest::SaveFingerprint { .. }) => "Saving SHA-256 fingerprint",
        Some(IntegrityRequest::VerifyFingerprint { .. }) => "Verifying saved fingerprint",
        Some(IntegrityRequest::GenerateSha256Sums { .. }) => "Generating SHA256SUMS",
        Some(IntegrityRequest::VerifySha256Sums { .. }) => "Verifying SHA256SUMS",
        None => "Integrity operation",
    }
}

pub fn present_integrity(outcome: &IntegrityOutcome) -> IntegrityPresentation {
    match outcome {
        IntegrityOutcome::FingerprintSaved(saved) => IntegrityPresentation {
            title: "SHA-256 fingerprint saved".to_owned(),
            summary: format!("Saved a private Floe fingerprint for {}.", escaped_path_label(saved.path())),
            rows: Vec::new(),
            notice: INTEGRITY_NOTICE,
        },
        IntegrityOutcome::FingerprintVerified(FingerprintVerification::Match) => IntegrityPresentation {
            title: "Saved fingerprint matches".to_owned(),
            summary: "The selected file matches its saved SHA-256 fingerprint.".to_owned(),
            rows: Vec::new(),
            notice: INTEGRITY_NOTICE,
        },
        IntegrityOutcome::FingerprintVerified(FingerprintVerification::Changed { .. }) => IntegrityPresentation {
            title: "Saved fingerprint changed".to_owned(),
            summary: "The selected file bytes do not match its saved SHA-256 fingerprint.".to_owned(),
            rows: Vec::new(),
            notice: INTEGRITY_NOTICE,
        },
        IntegrityOutcome::FingerprintVerified(FingerprintVerification::StaleIdentity) => IntegrityPresentation {
            title: "Saved fingerprint is stale".to_owned(),
            summary: "The selected file identity changed before its contents could be trusted for comparison.".to_owned(),
            rows: Vec::new(),
            notice: INTEGRITY_NOTICE,
        },
        IntegrityOutcome::ManifestGenerated { output_path, entries } => IntegrityPresentation {
            title: "SHA256SUMS generated".to_owned(),
            summary: format!(
                "Wrote {entries} SHA-256 {} to {} without replacing an existing manifest.",
                if *entries == 1 { "entry" } else { "entries" },
                escaped_path_label(output_path),
            ),
            rows: Vec::new(),
            notice: INTEGRITY_NOTICE,
        },
        IntegrityOutcome::ManifestVerified(verification) => present_manifest_verification(verification),
    }
}

pub fn present_integrity_monitor_diff(
    diff: &floe_core::IntegrityBaselineDiff,
) -> IntegrityPresentation {
    let mut matching = 0usize;
    let mut changed = 0usize;
    let mut missing = 0usize;
    let mut new = 0usize;
    let rows = diff
        .entries()
        .iter()
        .map(|entry| {
            let status = match entry.status() {
                floe_core::IntegrityEntryStatus::Matching => {
                    matching += 1;
                    "Matching"
                }
                floe_core::IntegrityEntryStatus::Changed { .. } => {
                    changed += 1;
                    "Changed"
                }
                floe_core::IntegrityEntryStatus::Missing => {
                    missing += 1;
                    "Missing"
                }
                floe_core::IntegrityEntryStatus::New => {
                    new += 1;
                    "New"
                }
            }
            .to_owned();
            IntegrityResultRow {
                display_path: escaped_path_label(entry.path()),
                status,
            }
        })
        .collect();
    IntegrityPresentation {
        title: "Integrity baseline results".to_owned(),
        summary: format!(
            "Matching: {matching} · Changed: {changed} · Missing: {missing} · New: {new}"
        ),
        rows,
        notice: INTEGRITY_MONITOR_NOTICE,
    }
}

fn present_manifest_verification(verification: &ManifestVerification) -> IntegrityPresentation {
    let mut matching = 0usize;
    let mut changed = 0usize;
    let mut missing = 0usize;
    let mut new = 0usize;
    let mut stale = 0usize;
    let rows = verification
        .entries
        .iter()
        .map(|(path, result)| {
            let status = match result {
                ManifestEntryVerification::Match => {
                    matching += 1;
                    "Matches"
                }
                ManifestEntryVerification::Changed { .. } => {
                    changed += 1;
                    "Changed"
                }
                ManifestEntryVerification::Missing => {
                    missing += 1;
                    "Missing"
                }
                ManifestEntryVerification::New => {
                    new += 1;
                    "New (not in manifest)"
                }
                ManifestEntryVerification::StaleIdentity => {
                    stale += 1;
                    "Stale identity"
                }
            };
            IntegrityResultRow {
                display_path: escaped_path_label(path),
                status: status.to_owned(),
            }
        })
        .collect();
    IntegrityPresentation {
        title: "SHA256SUMS verification results".to_owned(),
        summary: format!(
            "Matching: {matching}; changed: {changed}; missing: {missing}; new: {new}; stale: {stale}. Inaccessible paths end verification as an explicit operation failure."
        ),
        rows,
        notice: INTEGRITY_NOTICE,
    }
}

pub struct IntegrityResultsDialogWidgets {
    pub dialog: adw::Dialog,
    pub close_button: gtk::Button,
}

pub fn build_integrity_results_dialog(
    presentation: &IntegrityPresentation,
) -> IntegrityResultsDialogWidgets {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(20)
        .margin_bottom(20)
        .margin_start(20)
        .margin_end(20)
        .build();
    let heading = gtk::Label::builder()
        .label(&presentation.title)
        .halign(gtk::Align::Start)
        .xalign(0.0)
        .wrap(true)
        .build();
    heading.add_css_class("title-2");
    content.append(&heading);
    for text in [&presentation.summary, presentation.notice] {
        content.append(
            &gtk::Label::builder()
                .label(text)
                .halign(gtk::Align::Start)
                .xalign(0.0)
                .wrap(true)
                .build(),
        );
    }
    if !presentation.rows.is_empty() {
        let list = gtk::ListBox::new();
        list.update_property(&[gtk::accessible::Property::Label(
            "SHA256SUMS verification result paths",
        )]);
        for row in &presentation.rows {
            let result = adw::ActionRow::builder()
                .title(&row.display_path)
                .subtitle(&row.status)
                .build();
            result.update_property(&[gtk::accessible::Property::Description(&row.status)]);
            list.append(&result);
        }
        let scroll = gtk::ScrolledWindow::builder()
            .min_content_height(160)
            .max_content_height(360)
            .child(&list)
            .build();
        content.append(&scroll);
    }
    let close_button = gtk::Button::with_label("Close");
    close_button.add_css_class("suggested-action");
    close_button.set_halign(gtk::Align::End);
    content.append(&close_button);
    let dialog = adw::Dialog::builder()
        .title(&presentation.title)
        .content_width(620)
        .content_height(360)
        .child(&content)
        .build();
    dialog.update_property(&[gtk::accessible::Property::Label(&presentation.title)]);
    IntegrityResultsDialogWidgets {
        dialog,
        close_button,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::{ManifestEntryVerification, ManifestVerification};

    #[test]
    fn phase_18t_ui_uses_escaped_exact_path_labels() {
        let path = PathBuf::from("/tmp/bad\u{fffd}");
        assert!(escaped_path_label(&path).contains("\\xef\\xbf\\xbd"));
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::ffi::OsStringExt;
            assert_eq!(
                escaped_path_label(&PathBuf::from(std::ffi::OsString::from_vec(
                    b"/tmp/a\xff".to_vec()
                ))),
                "/tmp/a\\xff"
            );
        }
    }

    #[test]
    fn phase_18t_ui_reports_every_manifest_status() {
        let presentation = present_manifest_verification(&ManifestVerification {
            entries: vec![
                (PathBuf::from("match"), ManifestEntryVerification::Match),
                (
                    PathBuf::from("changed"),
                    ManifestEntryVerification::Changed {
                        expected: "a".into(),
                        actual: "b".into(),
                    },
                ),
                (PathBuf::from("missing"), ManifestEntryVerification::Missing),
                (PathBuf::from("new"), ManifestEntryVerification::New),
                (
                    PathBuf::from("stale"),
                    ManifestEntryVerification::StaleIdentity,
                ),
            ],
        });
        assert!(presentation.summary.contains("Matching: 1"));
        assert!(presentation.summary.contains("Inaccessible paths"));
        assert!(
            presentation
                .notice
                .contains("do not prove authenticity, safety, or authorship")
        );
    }

    #[test]
    fn phase_18u_ui_reports_each_baseline_status_and_truthful_monitoring_notice() {
        let baseline = floe_core::IntegrityBaseline::new(
            PathBuf::from("/tmp/integrity-root"),
            vec![
                floe_core::IntegrityBaselineEntry::new(PathBuf::from("matching"), "a".repeat(64))
                    .expect("matching baseline entry"),
                floe_core::IntegrityBaselineEntry::new(PathBuf::from("changed"), "b".repeat(64))
                    .expect("changed baseline entry"),
                floe_core::IntegrityBaselineEntry::new(PathBuf::from("missing"), "c".repeat(64))
                    .expect("missing baseline entry"),
            ],
        )
        .expect("baseline");
        let current = vec![
            floe_core::IntegrityBaselineEntry::new(PathBuf::from("matching"), "a".repeat(64))
                .expect("matching current entry"),
            floe_core::IntegrityBaselineEntry::new(PathBuf::from("changed"), "d".repeat(64))
                .expect("changed current entry"),
            floe_core::IntegrityBaselineEntry::new(PathBuf::from("new"), "e".repeat(64))
                .expect("new current entry"),
        ];
        let presentation = present_integrity_monitor_diff(
            &floe_core::IntegrityBaselineDiff::between(&baseline, &current).expect("diff"),
        );
        assert!(presentation.summary.contains("Matching: 1"));
        for status in ["Matching", "Changed", "Missing", "New"] {
            assert!(presentation.rows.iter().any(|row| row.status == status));
        }
        assert!(presentation.notice.contains("not intrusion detection"));
        assert!(presentation.notice.contains("who or what changed a file"));
    }

    #[test]
    fn phase_18t_ui_keeps_private_store_and_requests_typed() {
        assert_eq!(
            private_fingerprint_store_path(Path::new("/private/data")),
            PathBuf::from("/private/data/floe/integrity/fingerprints-v1")
        );
        assert_eq!(
            integrity_title(Some(&IntegrityRequest::GenerateSha256Sums {
                root: PathBuf::from("/root"),
                targets: vec![PathBuf::from("/root/file")],
                output_path: PathBuf::from("/root/SHA256SUMS"),
            })),
            "Generating SHA256SUMS"
        );
    }
}
