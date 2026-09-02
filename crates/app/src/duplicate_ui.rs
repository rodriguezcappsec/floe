//! Native, review-first duplicate finder presentation.

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use floe_core::{DuplicateScanOutcome, DuplicateScanPhase, DuplicateScanSummary};
use gtk::prelude::*;

#[cfg(test)]
pub const DUPLICATE_ACTION_LABEL: &str = "Check for Duplicates…";
pub const DUPLICATE_REVIEW_POLICY: &str = "Nothing is deleted automatically. Hard-link aliases use no additional file data. Select exact paths only after review; Trash remains recoverable.";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DuplicateSelectionKind {
    RegularFile,
    Directory,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateSelection {
    pub path: PathBuf,
    pub kind: DuplicateSelectionKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DuplicateScanChoice {
    FolderTree(PathBuf),
    CopiesOfFile { reference: PathBuf, folder: PathBuf },
    SelectedItems(Vec<PathBuf>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DuplicateSetupMode {
    FolderTree,
    CopiesOfFile,
    SelectedItems,
}

pub fn duplicate_setup_default(selection: &[DuplicateSelection]) -> DuplicateSetupMode {
    match selection {
        [item] if item.kind == DuplicateSelectionKind::RegularFile => {
            DuplicateSetupMode::CopiesOfFile
        }
        [item] if item.kind == DuplicateSelectionKind::Directory => DuplicateSetupMode::FolderTree,
        items
            if !items.is_empty()
                && items.iter().all(|item| {
                    matches!(
                        item.kind,
                        DuplicateSelectionKind::RegularFile | DuplicateSelectionKind::Directory
                    )
                }) =>
        {
            DuplicateSetupMode::SelectedItems
        }
        _ => DuplicateSetupMode::FolderTree,
    }
}

pub fn duplicate_scan_choice(
    mode: DuplicateSetupMode,
    scope: &Path,
    selection: &[DuplicateSelection],
) -> Option<DuplicateScanChoice> {
    match mode {
        DuplicateSetupMode::FolderTree => {
            Some(DuplicateScanChoice::FolderTree(scope.to_path_buf()))
        }
        DuplicateSetupMode::CopiesOfFile => selection
            .iter()
            .find(|item| item.kind == DuplicateSelectionKind::RegularFile)
            .map(|item| DuplicateScanChoice::CopiesOfFile {
                reference: item.path.clone(),
                folder: scope.to_path_buf(),
            }),
        DuplicateSetupMode::SelectedItems
            if !selection.is_empty()
                && selection.iter().all(|item| {
                    matches!(
                        item.kind,
                        DuplicateSelectionKind::RegularFile | DuplicateSelectionKind::Directory
                    )
                }) =>
        {
            Some(DuplicateScanChoice::SelectedItems(
                selection.iter().map(|item| item.path.clone()).collect(),
            ))
        }
        DuplicateSetupMode::SelectedItems => None,
    }
}

pub fn present_duplicate_setup(
    parent: &adw::ApplicationWindow,
    current_folder: PathBuf,
    selection: Vec<DuplicateSelection>,
    on_start: impl Fn(DuplicateScanChoice) + 'static,
) -> gtk::Window {
    let default_mode = duplicate_setup_default(&selection);
    let reference = selection
        .iter()
        .find(|item| item.kind == DuplicateSelectionKind::RegularFile)
        .map(|item| item.path.clone());
    let selected_items_available = !selection.is_empty()
        && selection.iter().all(|item| {
            matches!(
                item.kind,
                DuplicateSelectionKind::RegularFile | DuplicateSelectionKind::Directory
            )
        });
    let initial_scope = if default_mode == DuplicateSetupMode::FolderTree {
        selection
            .iter()
            .find(|item| item.kind == DuplicateSelectionKind::Directory)
            .map_or(current_folder, |item| item.path.clone())
    } else {
        current_folder
    };
    let scope = Rc::new(RefCell::new(initial_scope));

    let window = gtk::Window::builder()
        .title("Find Duplicate Files")
        .transient_for(parent)
        .modal(true)
        .default_width(560)
        .resizable(false)
        .build();
    let heading = gtk::Label::builder()
        .label("Choose what Floe should compare")
        .xalign(0.0)
        .css_classes(["title-3"])
        .build();
    let explanation = gtk::Label::builder()
        .label("Exact duplicates have identical bytes. Floe scans chosen folders and all subfolders without following symbolic links. Visually similar media is a separate future workflow.")
        .xalign(0.0)
        .wrap(true)
        .build();
    explanation.add_css_class("dim-label");

    let folder_mode = gtk::CheckButton::with_label("All duplicates in a folder tree");
    folder_mode.set_tooltip_text(Some(
        "Find every exact duplicate group in the chosen folder and all subfolders",
    ));
    let reference_mode = gtk::CheckButton::with_label("Copies of the selected file");
    reference_mode.set_group(Some(&folder_mode));
    reference_mode.set_sensitive(reference.is_some());
    reference_mode.set_tooltip_text(Some(
        "Find exact byte-for-byte copies of the selected file in the chosen folder and all subfolders",
    ));
    let selected_mode =
        gtk::CheckButton::with_label(&format!("Selected files and folders ({})", selection.len()));
    selected_mode.set_group(Some(&folder_mode));
    selected_mode.set_sensitive(selected_items_available);
    selected_mode.set_tooltip_text(Some(
        "Compare the selected files; selected folders include all of their subfolders",
    ));
    match default_mode {
        DuplicateSetupMode::FolderTree => folder_mode.set_active(true),
        DuplicateSetupMode::CopiesOfFile => reference_mode.set_active(true),
        DuplicateSetupMode::SelectedItems => selected_mode.set_active(true),
    }

    let scope_title = gtk::Label::builder()
        .label("Search folder")
        .xalign(0.0)
        .css_classes(["heading"])
        .build();
    let scope_label = gtk::Label::builder()
        .label(scope.borrow().to_string_lossy())
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .tooltip_text(scope.borrow().to_string_lossy())
        .build();
    let browse = gtk::Button::with_label("Choose Folder…");
    browse.update_property(&[gtk::accessible::Property::Label(
        "Choose duplicate search folder",
    )]);
    let scope_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    scope_row.append(&scope_label);
    scope_row.append(&browse);
    let scope_enabled = !selected_mode.is_active();
    scope_title.set_sensitive(scope_enabled);
    scope_row.set_sensitive(scope_enabled);
    let scope_title_for_mode = scope_title.clone();
    let scope_row_for_mode = scope_row.clone();
    selected_mode.connect_toggled(move |mode| {
        let enabled = !mode.is_active();
        scope_title_for_mode.set_sensitive(enabled);
        scope_row_for_mode.set_sensitive(enabled);
    });

    let cancel = gtk::Button::with_label("Cancel");
    let start = gtk::Button::with_label("Start Scan");
    start.add_css_class("suggested-action");
    start.update_property(&[gtk::accessible::Property::Label("Start duplicate scan")]);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.set_halign(gtk::Align::End);
    actions.append(&cancel);
    actions.append(&start);

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_start(18)
        .margin_end(18)
        .margin_top(18)
        .margin_bottom(18)
        .build();
    content.append(&heading);
    content.append(&explanation);
    content.append(&folder_mode);
    content.append(&reference_mode);
    content.append(&selected_mode);
    content.append(&scope_title);
    content.append(&scope_row);
    content.append(&actions);
    window.set_child(Some(&content));
    crate::contextual_help::install_on_tree(&window);

    let weak_window = window.downgrade();
    cancel.connect_clicked(move |_| {
        if let Some(window) = weak_window.upgrade() {
            window.close();
        }
    });

    let weak_window = window.downgrade();
    let scope_for_picker = Rc::clone(&scope);
    let scope_label_for_picker = scope_label.clone();
    browse.connect_clicked(move |_| {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        let chooser = gtk::FileDialog::builder()
            .title("Choose Duplicate Search Folder")
            .modal(true)
            .build();
        chooser.set_initial_folder(Some(&gtk::gio::File::for_path(&*scope_for_picker.borrow())));
        let scope = Rc::clone(&scope_for_picker);
        let scope_label = scope_label_for_picker.clone();
        glib::MainContext::default().spawn_local(async move {
            if let Ok(folder) = chooser.select_folder_future(Some(&window)).await {
                if let Some(path) = folder.path() {
                    scope_label.set_label(&path.to_string_lossy());
                    scope_label.set_tooltip_text(Some(&path.to_string_lossy()));
                    scope.replace(path);
                }
            }
        });
    });

    let weak_window = window.downgrade();
    start.connect_clicked(move |_| {
        let mode = if selected_mode.is_active() {
            DuplicateSetupMode::SelectedItems
        } else if reference_mode.is_active() {
            DuplicateSetupMode::CopiesOfFile
        } else {
            DuplicateSetupMode::FolderTree
        };
        let Some(choice) = duplicate_scan_choice(mode, &scope.borrow(), &selection) else {
            return;
        };
        if let Some(window) = weak_window.upgrade() {
            window.close();
        }
        on_start(choice);
    });

    window.present();
    window
}

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
            .label("Scanning files and folder trees…")
            .xalign(0.0)
            .css_classes(["title-3"])
            .build();
        let policy = gtk::Label::builder()
            .label("Candidates are grouped by size, quick-filtered from bounded first/last samples, hashed with SHA-256 or a validated private cache, then confirmed byte-for-byte. Symbolic links are not followed.")
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
        crate::contextual_help::install_on_tree(&window);
        window.present();
        Self { window, status }
    }

    pub fn update(&self, summary: DuplicateScanSummary) {
        self.status.set_label(&duplicate_progress_text(summary));
    }

    pub fn close(self) {
        self.window.close();
    }
}

pub fn duplicate_progress_text(summary: DuplicateScanSummary) -> String {
    match summary.phase {
        DuplicateScanPhase::Discovering => format!(
            "Discovering candidates… {} files in {} folders examined.",
            summary.examined_files, summary.examined_directories
        ),
        DuplicateScanPhase::QuickFiltering => format!(
            "Quick filtering… {} same-size candidates; {} files sampled ({} read).",
            summary.candidate_files,
            summary.quick_checked_files,
            format_size(summary.quick_read_bytes)
        ),
        DuplicateScanPhase::Hashing => format!(
            "Hashing candidates… {} calculated ({}), {} reused from validated cache ({}).",
            summary.hashed_files,
            format_size(summary.hashed_bytes),
            summary.reused_hashes,
            format_size(summary.reused_hash_bytes)
        ),
        DuplicateScanPhase::Confirming => format!(
            "Confirming matching hashes byte-for-byte… {} compared.",
            format_size(summary.compared_bytes)
        ),
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
    crate::contextual_help::install_on_tree(&window);
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

    #[test]
    fn phase_13g3_progress_names_real_stages_and_cache_reuse_without_fake_percentages() {
        let discovery = duplicate_progress_text(DuplicateScanSummary {
            examined_files: 12,
            examined_directories: 3,
            ..DuplicateScanSummary::default()
        });
        assert!(discovery.starts_with("Discovering candidates"));

        let quick = duplicate_progress_text(DuplicateScanSummary {
            phase: DuplicateScanPhase::QuickFiltering,
            candidate_files: 8,
            quick_checked_files: 6,
            quick_read_bytes: 4096,
            ..DuplicateScanSummary::default()
        });
        assert!(quick.starts_with("Quick filtering"));

        let hashing = duplicate_progress_text(DuplicateScanSummary {
            phase: DuplicateScanPhase::Hashing,
            hashed_files: 2,
            hashed_bytes: 2048,
            reused_hashes: 4,
            reused_hash_bytes: 8192,
            ..DuplicateScanSummary::default()
        });
        assert!(hashing.contains("4 reused from validated cache"));
        assert!(!hashing.contains('%'));

        let confirming = duplicate_progress_text(DuplicateScanSummary {
            phase: DuplicateScanPhase::Confirming,
            compared_bytes: 1024,
            ..DuplicateScanSummary::default()
        });
        assert!(confirming.starts_with("Confirming matching hashes byte-for-byte"));
    }

    #[test]
    fn phase_13g2_setup_defaults_match_selection_context() {
        assert_eq!(duplicate_setup_default(&[]), DuplicateSetupMode::FolderTree);
        assert_eq!(
            duplicate_setup_default(&[selection("/tmp/file", DuplicateSelectionKind::RegularFile)]),
            DuplicateSetupMode::CopiesOfFile
        );
        assert_eq!(
            duplicate_setup_default(&[selection("/tmp/folder", DuplicateSelectionKind::Directory)]),
            DuplicateSetupMode::FolderTree
        );
        assert_eq!(
            duplicate_setup_default(&[
                selection("/tmp/one", DuplicateSelectionKind::RegularFile),
                selection("/tmp/two", DuplicateSelectionKind::Directory),
            ]),
            DuplicateSetupMode::SelectedItems
        );
    }

    #[test]
    fn phase_13g2_choice_preserves_exact_scope_and_rejects_unsupported_items() {
        let selected = vec![selection(
            "/tmp/reference",
            DuplicateSelectionKind::RegularFile,
        )];
        let scope = Path::new("/tmp/search tree");
        assert_eq!(
            duplicate_scan_choice(DuplicateSetupMode::CopiesOfFile, scope, &selected),
            Some(DuplicateScanChoice::CopiesOfFile {
                reference: PathBuf::from("/tmp/reference"),
                folder: scope.to_path_buf(),
            })
        );

        let mixed = vec![
            selection("/tmp/file", DuplicateSelectionKind::RegularFile),
            selection("/tmp/socket", DuplicateSelectionKind::Unsupported),
        ];
        assert_eq!(
            duplicate_scan_choice(DuplicateSetupMode::SelectedItems, scope, &mixed),
            None
        );
    }

    fn selection(path: &str, kind: DuplicateSelectionKind) -> DuplicateSelection {
        DuplicateSelection {
            path: PathBuf::from(path),
            kind,
        }
    }

    #[test]
    #[ignore = "requires a real disposable GTK display"]
    fn phase_testing_gtk_phase_13g2_duplicate_setup_contract() {
        gtk::init().expect("GTK duplicate setup gate requires a display");
        adw::init().expect("libadwaita must initialize");
        let parent = adw::ApplicationWindow::builder().build();
        let dialog = present_duplicate_setup(
            &parent,
            PathBuf::from("/tmp"),
            vec![selection(
                "/tmp/reference",
                DuplicateSelectionKind::RegularFile,
            )],
            |_| {},
        );

        assert_eq!(dialog.title().as_deref(), Some("Find Duplicate Files"));
        assert!(dialog.is_modal());
        assert!(dialog.transient_for().is_some());
        dialog.close();
        parent.close();
    }
}
