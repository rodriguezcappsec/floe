//! Bounded GTK presentation for Floe's Miller navigation model.
//!
//! This module never enumerates the filesystem. Historical columns are
//! snapshots of results already returned by `BrowserWorker`; the active column
//! shares the browser's existing selection model.

use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use floe_core::{DirectoryEntry, MILLER_COLUMN_CAPACITY, MillerColumnModel};
use gtk::{gio, glib, prelude::*};

use crate::view::MillerColumnWidth;

pub const MILLER_SNAPSHOT_ENTRY_CAPACITY: usize = 4_096;

#[derive(Clone, Debug)]
struct MillerSnapshot {
    depth: usize,
    directory: PathBuf,
    entries: Vec<Arc<DirectoryEntry>>,
    total_entries: usize,
}

#[derive(Clone, Debug, Default)]
pub struct MillerPresentationState {
    snapshots: VecDeque<MillerSnapshot>,
}

impl MillerPresentationState {
    pub fn clear(&mut self) {
        self.snapshots.clear();
    }

    pub fn capture(&mut self, depth: usize, directory: PathBuf, entries: &[Arc<DirectoryEntry>]) {
        self.snapshots
            .retain(|snapshot| snapshot.depth != depth && snapshot.directory != directory);
        self.snapshots.push_back(MillerSnapshot {
            depth,
            directory,
            entries: entries
                .iter()
                .take(MILLER_SNAPSHOT_ENTRY_CAPACITY)
                .cloned()
                .collect(),
            total_entries: entries.len(),
        });
        while self.snapshots.len() > MILLER_COLUMN_CAPACITY {
            self.snapshots.pop_front();
        }
    }

    pub fn truncate_after(&mut self, depth: usize) {
        self.snapshots.retain(|snapshot| snapshot.depth <= depth);
    }

    pub fn columns(
        &self,
        model: &MillerColumnModel,
        current_directory: &Path,
    ) -> Vec<MillerRenderColumn> {
        model
            .columns()
            .map(|column| {
                let depth = column.depth().get();
                let is_active = column.directory() == current_directory;
                let snapshot = self.snapshots.iter().find(|snapshot| {
                    snapshot.depth == depth && snapshot.directory == column.directory()
                });
                MillerRenderColumn {
                    depth,
                    directory: column.directory().to_path_buf(),
                    selected_child: column.selected_child().map(Path::to_path_buf),
                    entries: snapshot
                        .map(|snapshot| snapshot.entries.clone())
                        .unwrap_or_default(),
                    total_entries: snapshot.map_or(0, |snapshot| snapshot.total_entries),
                    is_active,
                }
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct MillerRenderColumn {
    pub depth: usize,
    pub directory: PathBuf,
    pub selected_child: Option<PathBuf>,
    pub entries: Vec<Arc<DirectoryEntry>>,
    pub total_entries: usize,
    pub is_active: bool,
}

#[derive(Clone, Debug)]
pub struct MillerActivation {
    pub depth: usize,
    pub entry: Arc<DirectoryEntry>,
}

type ActivationHandler = Box<dyn Fn(MillerActivation)>;

#[derive(Clone, Default)]
pub struct MillerActivationDispatcher(Rc<RefCell<Option<ActivationHandler>>>);

impl MillerActivationDispatcher {
    pub fn bind(&self, handler: impl Fn(MillerActivation) + 'static) {
        self.0.replace(Some(Box::new(handler)));
    }

    fn dispatch(&self, activation: MillerActivation) {
        if let Some(handler) = self.0.borrow().as_ref() {
            handler(activation);
        }
    }
}

pub struct MillerView {
    scroller: gtk::ScrolledWindow,
    columns: gtk::Box,
    width: Cell<MillerColumnWidth>,
    dispatcher: MillerActivationDispatcher,
}

impl MillerView {
    pub fn new() -> Self {
        let columns = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .margin_start(12)
            .margin_end(12)
            .margin_top(12)
            .margin_bottom(12)
            .build();
        columns.add_css_class("floe-miller-columns");

        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .child(&columns)
            .hexpand(true)
            .vexpand(true)
            .build();
        scroller.add_css_class("floe-miller-view");
        scroller.update_property(&[gtk::accessible::Property::Label("Miller column browser")]);

        Self {
            scroller,
            columns,
            width: Cell::new(MillerColumnWidth::default()),
            dispatcher: MillerActivationDispatcher::default(),
        }
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.scroller
    }

    pub fn bind_activate(&self, handler: impl Fn(MillerActivation) + 'static) {
        self.dispatcher.bind(handler);
    }

    pub fn width(&self) -> MillerColumnWidth {
        self.width.get()
    }

    pub fn set_width(&self, width: MillerColumnWidth) {
        self.width.set(width);
        let mut child = self.columns.first_child();
        while let Some(widget) = child {
            widget.set_width_request(i32::from(width.get()));
            child = widget.next_sibling();
        }
        let description = format!("Miller column width: {} pixels", width.get());
        self.scroller
            .update_property(&[gtk::accessible::Property::Description(&description)]);
    }

    pub fn render(&self, columns: &[MillerRenderColumn], active_selection: &gtk::MultiSelection) {
        while let Some(child) = self.columns.first_child() {
            self.columns.remove(&child);
        }

        for column in columns {
            let shell = self.build_column(column, active_selection);
            self.columns.append(&shell);
        }
    }

    fn build_column(
        &self,
        column: &MillerRenderColumn,
        active_selection: &gtk::MultiSelection,
    ) -> gtk::Box {
        let title = column
            .directory
            .file_name()
            .unwrap_or(column.directory.as_os_str())
            .to_string_lossy();
        let heading = gtk::Label::builder()
            .label(title.as_ref())
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .tooltip_text(column.directory.to_string_lossy())
            .margin_start(10)
            .margin_end(10)
            .margin_top(8)
            .margin_bottom(6)
            .build();
        heading.add_css_class("heading");

        let model: gtk::SelectionModel = if column.is_active {
            active_selection.clone().upcast()
        } else {
            let store = gio::ListStore::new::<glib::BoxedAnyObject>();
            for entry in &column.entries {
                store.append(&glib::BoxedAnyObject::new(Arc::clone(entry)));
            }
            let selection = gtk::SingleSelection::new(Some(store));
            if let Some(selected) = column.selected_child.as_deref() {
                if let Some(index) = column
                    .entries
                    .iter()
                    .position(|entry| entry.path() == selected)
                    .and_then(|index| u32::try_from(index).ok())
                {
                    selection.set_selected(index);
                }
            }
            selection.upcast()
        };

        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, object| {
            let Some(item) = object.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let label = gtk::Label::builder()
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .margin_start(10)
                .margin_end(10)
                .margin_top(4)
                .margin_bottom(4)
                .build();
            label.add_css_class("floe-entry-name");
            item.set_child(Some(&label));
        });
        factory.connect_bind(|_, object| {
            let Some(item) = object.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let Some(label) = item.child().and_downcast::<gtk::Label>() else {
                return;
            };
            let Some(object) = item.item().and_downcast::<glib::BoxedAnyObject>() else {
                return;
            };
            let entry = object.borrow::<Arc<DirectoryEntry>>();
            let name = entry.display_name_lossy();
            label.set_label(&name);
            label.set_tooltip_text(Some(&name));
        });

        let list = gtk::ListView::new(Some(model), Some(factory));
        list.set_single_click_activate(false);
        list.add_css_class("floe-miller-column-list");
        let dispatcher = self.dispatcher.clone();
        let depth = column.depth;
        list.connect_activate(move |list, position| {
            let Some(model) = list.model() else {
                return;
            };
            let Some(object) = model.item(position).and_downcast::<glib::BoxedAnyObject>() else {
                return;
            };
            dispatcher.dispatch(MillerActivation {
                depth,
                entry: object.borrow::<Arc<DirectoryEntry>>().clone(),
            });
        });

        let list_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .child(&list)
            .vexpand(true)
            .build();

        let status_text = miller_column_status(column);
        let status = gtk::Label::builder()
            .label(&status_text)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .margin_start(10)
            .margin_end(10)
            .margin_top(5)
            .margin_bottom(7)
            .build();
        status.add_css_class("caption");
        status.add_css_class("dim-label");

        let shell = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .width_request(i32::from(self.width.get().get()))
            .vexpand(true)
            .build();
        shell.add_css_class("floe-panel");
        shell.add_css_class("floe-miller-column");
        if column.is_active {
            shell.add_css_class("floe-miller-column-active");
        }
        shell.update_property(&[gtk::accessible::Property::Description(
            &miller_column_accessible_description(column),
        )]);
        shell.append(&heading);
        shell.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        shell.append(&list_scroller);
        shell.append(&status);
        shell
    }
}

fn miller_column_status(column: &MillerRenderColumn) -> String {
    if column.is_active {
        return "Active column".to_owned();
    }
    if column.total_entries > column.entries.len() {
        return format!(
            "Cached first {} of {} items",
            column.entries.len(),
            column.total_entries
        );
    }
    match column.total_entries {
        0 => "No retained listing; activate a visible folder to continue".to_owned(),
        1 => "1 cached item".to_owned(),
        count => format!("{count} cached items"),
    }
}

fn miller_column_accessible_description(column: &MillerRenderColumn) -> String {
    let state = if column.is_active {
        "Active"
    } else {
        "Retained"
    };
    format!(
        "{state} Miller column {}: {}",
        column.depth + 1,
        column.directory.to_string_lossy()
    )
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::ffi::OsStringExt, path::PathBuf, sync::Arc};

    use floe_core::{
        MILLER_COLUMN_CAPACITY, MillerChildKind, MillerColumnModel, enumerate_directory,
    };
    use tempfile::tempdir;

    use super::{
        MILLER_SNAPSHOT_ENTRY_CAPACITY, MillerPresentationState,
        miller_column_accessible_description,
    };
    use crate::view::{
        MILLER_COLUMN_WIDTH_DEFAULT, MILLER_COLUMN_WIDTH_MAX, MILLER_COLUMN_WIDTH_MIN,
        MillerColumnWidth, VIEW_ACTIONS, ViewCommand, ViewMode,
    };

    #[test]
    fn phase_8b_policy_bounds_width_and_retained_snapshots() {
        assert_eq!(MillerColumnWidth::new(0).get(), MILLER_COLUMN_WIDTH_MIN);
        assert_eq!(
            MillerColumnWidth::default().get(),
            MILLER_COLUMN_WIDTH_DEFAULT
        );
        assert_eq!(
            MillerColumnWidth::new(u16::MAX).get(),
            MILLER_COLUMN_WIDTH_MAX
        );

        let root = tempdir().expect("temporary root");
        let entries = (0..(MILLER_SNAPSHOT_ENTRY_CAPACITY + 7))
            .map(|index| {
                let path = root.path().join(format!("entry-{index}"));
                fs::write(&path, b"x").expect("fixture file");
                path
            })
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), MILLER_SNAPSHOT_ENTRY_CAPACITY + 7);
        let listing = enumerate_directory(root.path()).expect("fixture listing");
        let shared = listing
            .into_entries()
            .into_iter()
            .map(Arc::new)
            .collect::<Vec<_>>();
        let mut state = MillerPresentationState::default();
        for depth in 0..(MILLER_COLUMN_CAPACITY + 3) {
            state.capture(depth, PathBuf::from(format!("/root/{depth}")), &shared);
        }
        assert_eq!(state.snapshots.len(), MILLER_COLUMN_CAPACITY);
        assert!(
            state
                .snapshots
                .iter()
                .all(|snapshot| snapshot.entries.len() == MILLER_SNAPSHOT_ENTRY_CAPACITY)
        );
    }

    #[test]
    fn phase_8b_policy_keeps_exact_non_utf8_column_identity() {
        let raw = PathBuf::from(std::ffi::OsString::from_vec(vec![b'/', b'r', 0xff]));
        let model = MillerColumnModel::new(raw.clone()).expect("raw root");
        let columns = MillerPresentationState::default().columns(&model, &raw);
        assert_eq!(columns.len(), 1);
        assert_eq!(columns[0].directory, raw);
        assert!(columns[0].is_active);
    }

    #[test]
    fn phase_8b_ui_description_names_active_column_without_color_only_state() {
        let root = PathBuf::from("/projects");
        let mut model = MillerColumnModel::new(root.clone()).expect("model");
        model
            .select_child(
                model.active_depth().expect("active depth"),
                root.join("floe"),
                MillerChildKind::Directory,
            )
            .expect("descent");
        let columns = MillerPresentationState::default().columns(&model, &root.join("floe"));
        let description = miller_column_accessible_description(&columns[1]);
        assert!(description.starts_with("Active Miller column 2:"));
        assert!(description.contains("/projects/floe"));
    }

    #[test]
    fn phase_8b_pipeline_active_column_uses_shared_model_not_snapshot_entries() {
        let root = PathBuf::from("/projects");
        let model = MillerColumnModel::new(root.clone()).expect("model");
        let columns = MillerPresentationState::default().columns(&model, &root);
        assert_eq!(columns.len(), 1);
        assert!(columns[0].is_active);
        assert!(columns[0].entries.is_empty());
        assert_eq!(columns[0].total_entries, 0);
    }

    #[test]
    fn phase_8b_integration_exposes_miller_without_removing_list_or_grid() {
        assert_eq!(ViewMode::Miller.stack_name(), "miller");
        assert!(VIEW_ACTIONS.contains(&("view-list", ViewCommand::List)));
        assert!(VIEW_ACTIONS.contains(&("view-grid", ViewCommand::Grid)));
        assert!(VIEW_ACTIONS.contains(&("view-miller", ViewCommand::Miller)));
    }
}
