//! Native, review-first duplicate finder presentation.

use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::Arc};

use floe_core::{DuplicateScanOutcome, DuplicateScanSummary};
use gtk::prelude::*;

#[cfg(test)]
pub const DUPLICATE_ACTION_LABEL: &str = "Check for Duplicates…";
pub const DUPLICATE_REVIEW_POLICY: &str = "Nothing is deleted automatically. Hard-link aliases use no additional file data. Select exact paths only after review; Trash remains recoverable.";

pub struct DuplicateProgressDialog {
    window: gtk::Window,
    status: gtk::Label,
}

impl DuplicateProgressDialog {
    pub fn present(parent: &adw::ApplicationWindow, on_cancel: impl Fn() + 'static) -> Self {
        let window = gtk::Window::builder()
            .title("Checking for Duplicates")
            .transient_for(parent)
            .modal(false)
            .default_width(440)
            .resizable(false)
            .build();
        let heading = gtk::Label::builder()
            .label("Checking selected files and folders…")
            .xalign(0.0)
            .css_classes(["title-3"])
            .build();
        let policy = gtk::Label::builder()
            .label("Candidates are grouped by size, hashed with SHA-256, then confirmed byte-for-byte. Symbolic links are not followed.")
            .xalign(0.0)
            .wrap(true)
            .build();
        let status = gtk::Label::builder()
            .label("Discovering same-size candidates…")
            .xalign(0.0)
            .wrap(true)
            .build();
        status.set_accessible_role(gtk::AccessibleRole::Status);
        let cancel = gtk::Button::with_label("Cancel scan");
        cancel.add_css_class("destructive-action");
        cancel.update_property(&[gtk::accessible::Property::Label("Cancel duplicate scan")]);
        let weak_window = window.downgrade();
        cancel.connect_clicked(move |_| {
            on_cancel();
            if let Some(window) = weak_window.upgrade() {
                window.close();
            }
        });
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_start(18)
            .margin_end(18)
            .margin_top(18)
            .margin_bottom(18)
            .build();
        content.append(&heading);
        content.append(&policy);
        content.append(&status);
        content.append(&cancel);
        window.set_child(Some(&content));
        window.present();
        Self { window, status }
    }

    pub fn update(&self, summary: DuplicateScanSummary) {
        self.status.set_label(&format!(
            "Examined {} files in {} folders; {} candidates; hashed {} files ({}).",
            summary.examined_files,
            summary.examined_directories,
            summary.candidate_files,
            summary.hashed_files,
            format_size(summary.hashed_bytes)
        ));
    }

    pub fn close(self) {
        self.window.close();
    }
}

pub fn present_duplicate_review(
    parent: &adw::ApplicationWindow,
    outcome: Arc<DuplicateScanOutcome>,
    on_reveal: impl Fn(PathBuf) + 'static,
    on_trash: impl Fn(Vec<PathBuf>) + 'static,
) {
    let window = gtk::Window::builder()
        .title("Duplicate Files")
        .transient_for(parent)
        .modal(false)
        .default_width(760)
        .default_height(620)
        .build();
    let header = gtk::HeaderBar::new();
    let title = gtk::Label::new(Some("Duplicate Files"));
    title.add_css_class("title");
    header.set_title_widget(Some(&title));
    let close = gtk::Button::with_label("Close");
    header.pack_start(&close);

    let summary = gtk::Label::builder()
        .label(format!(
            "{} confirmed groups · {} potentially reclaimable",
            outcome.groups().len(),
            format_size(outcome.reclaimable_bytes())
        ))
        .xalign(0.0)
        .build();
    summary.add_css_class("title-4");
    let policy = gtk::Label::builder()
        .label(DUPLICATE_REVIEW_POLICY)
        .xalign(0.0)
        .wrap(true)
        .build();
    policy.add_css_class("dim-label");

    let selected: Rc<RefCell<Vec<(gtk::CheckButton, PathBuf)>>> = Rc::new(RefCell::new(Vec::new()));
    let reveal = Rc::new(on_reveal);
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("boxed-list");
    if outcome.groups().is_empty() {
        let empty = gtk::Label::builder()
            .label("No byte-for-byte duplicate files were found in the completed scan.")
            .wrap(true)
            .margin_top(24)
            .margin_bottom(24)
            .build();
        list.append(&empty);
    }
    for (group_index, group) in outcome.groups().iter().enumerate() {
        let heading = gtk::Label::builder()
            .label(format!(
                "Group {} · {} each · {} independent copies · {} reclaimable",
                group_index + 1,
                format_size(group.size()),
                group.independent_copies(),
                format_size(group.reclaimable_bytes())
            ))
            .xalign(0.0)
            .margin_start(10)
            .margin_end(10)
            .margin_top(12)
            .margin_bottom(6)
            .build();
        heading.add_css_class("heading");
        list.append(&heading);
        for item in group.items() {
            let check = gtk::CheckButton::new();
            check.update_property(&[gtk::accessible::Property::Label(
                "Select exact duplicate path for Trash",
            )]);
            let path = item.path().to_path_buf();
            selected.borrow_mut().push((check.clone(), path.clone()));
            let path_label = gtk::Label::builder()
                .label(path.to_string_lossy())
                .xalign(0.0)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::Middle)
                .tooltip_text(path.to_string_lossy())
                .build();
            let kind = gtk::Label::new(Some(if item.is_hard_link_alias() {
                "Hard-link alias · no extra file data"
            } else {
                "Independent copy"
            }));
            kind.add_css_class("caption");
            kind.add_css_class("dim-label");
            let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
            labels.append(&path_label);
            labels.append(&kind);
            let reveal_button = gtk::Button::with_label("Reveal");
            reveal_button.update_property(&[gtk::accessible::Property::Label(
                "Reveal duplicate in folder",
            )]);
            let exact = path.clone();
            let reveal = Rc::clone(&reveal);
            reveal_button.connect_clicked(move |_| reveal(exact.clone()));
            let row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(10)
                .margin_start(10)
                .margin_end(10)
                .margin_top(6)
                .margin_bottom(6)
                .build();
            row.append(&check);
            row.append(&labels);
            row.append(&reveal_button);
            list.append(&row);
        }
    }

    let scroller = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    let feedback = gtk::Label::builder().xalign(0.0).wrap(true).build();
    feedback.set_accessible_role(gtk::AccessibleRole::Status);
    let trash = gtk::Button::with_label("Move selected to Trash");
    trash.add_css_class("destructive-action");
    trash.set_sensitive(!outcome.groups().is_empty());
    trash.update_property(&[gtk::accessible::Property::Label(
        "Move explicitly selected duplicate paths to Trash",
    )]);
    let trash_callback = Rc::new(on_trash);
    let selected_for_trash = Rc::clone(&selected);
    let feedback_for_trash = feedback.clone();
    trash.connect_clicked(move |_| {
        let paths = selected_for_trash
            .borrow()
            .iter()
            .filter(|(check, _)| check.is_active())
            .map(|(_, path)| path.clone())
            .collect::<Vec<_>>();
        if paths.is_empty() {
            feedback_for_trash
                .set_label("Select one or more exact paths first. Nothing was trashed.");
            return;
        }
        trash_callback(paths);
    });

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    footer.append(&feedback);
    feedback.set_hexpand(true);
    footer.append(&trash);
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .margin_start(14)
        .margin_end(14)
        .margin_top(10)
        .margin_bottom(14)
        .build();
    content.append(&summary);
    content.append(&policy);
    content.append(&scroller);
    content.append(&footer);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.append(&header);
    root.append(&content);
    let weak_window = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(window) = weak_window.upgrade() {
            window.close();
        }
    });
    window.set_child(Some(&root));
    window.present();
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_13g_duplicate_ui_is_review_first_and_truthful() {
        assert_eq!(DUPLICATE_ACTION_LABEL, "Check for Duplicates…");
        assert!(DUPLICATE_REVIEW_POLICY.contains("Nothing is deleted automatically"));
        assert!(DUPLICATE_REVIEW_POLICY.contains("Hard-link aliases"));
        assert!(DUPLICATE_REVIEW_POLICY.contains("Trash remains recoverable"));
        assert_eq!(format_size(1024), "1.0 KiB");
    }
}
