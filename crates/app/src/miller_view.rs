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
const MILLER_TRACKPAD_SCALE: f64 = 48.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MillerNavigationCommand {
    Parent,
    Child,
}

#[derive(Clone, Debug)]
pub struct MillerNavigation {
    pub depth: usize,
    pub command: MillerNavigationCommand,
    pub selected_entry: Option<Arc<DirectoryEntry>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MillerItemCommand {
    Previous,
    Next,
    First,
    Last,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MillerMotionPolicy {
    kinetic_scrolling: bool,
}

impl MillerMotionPolicy {
    const fn from_animations_enabled(enabled: bool) -> Self {
        Self {
            kinetic_scrolling: enabled,
        }
    }
}

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
type NavigationHandler = Box<dyn Fn(MillerNavigation)>;

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

#[derive(Clone, Default)]
struct MillerNavigationDispatcher(Rc<RefCell<Option<NavigationHandler>>>);

impl MillerNavigationDispatcher {
    fn bind(&self, handler: impl Fn(MillerNavigation) + 'static) {
        self.0.replace(Some(Box::new(handler)));
    }

    fn dispatch(&self, navigation: MillerNavigation) {
        if let Some(handler) = self.0.borrow().as_ref() {
            handler(navigation);
        }
    }
}

pub struct MillerView {
    scroller: gtk::ScrolledWindow,
    columns: gtk::Box,
    width: Cell<MillerColumnWidth>,
    dispatcher: MillerActivationDispatcher,
    navigation_dispatcher: MillerNavigationDispatcher,
    active_list: RefCell<Option<gtk::ListView>>,
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
        let animations_enabled = gtk::Settings::default()
            .map(|settings| settings.is_gtk_enable_animations())
            .unwrap_or(false);
        let motion = MillerMotionPolicy::from_animations_enabled(animations_enabled);
        scroller.set_kinetic_scrolling(motion.kinetic_scrolling);
        if !motion.kinetic_scrolling {
            scroller.add_css_class("floe-reduced-motion");
        }

        let mut scroll_flags = gtk::EventControllerScrollFlags::BOTH_AXES;
        if motion.kinetic_scrolling {
            scroll_flags.insert(gtk::EventControllerScrollFlags::KINETIC);
        }
        let horizontal_scroll = gtk::EventControllerScroll::new(scroll_flags);
        let scroller_for_scroll = scroller.clone();
        horizontal_scroll.connect_scroll(move |_, delta_x, delta_y| {
            if !trackpad_prefers_horizontal(delta_x, delta_y) {
                return glib::Propagation::Proceed;
            }
            let adjustment = scroller_for_scroll.hadjustment();
            adjustment.set_value(horizontal_scroll_target(
                adjustment.value(),
                delta_x * MILLER_TRACKPAD_SCALE,
                adjustment.lower(),
                adjustment.upper(),
                adjustment.page_size(),
            ));
            glib::Propagation::Stop
        });
        scroller.add_controller(horizontal_scroll);

        Self {
            scroller,
            columns,
            width: Cell::new(MillerColumnWidth::default()),
            dispatcher: MillerActivationDispatcher::default(),
            navigation_dispatcher: MillerNavigationDispatcher::default(),
            active_list: RefCell::new(None),
        }
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.scroller
    }

    pub fn bind_activate(&self, handler: impl Fn(MillerActivation) + 'static) {
        self.dispatcher.bind(handler);
    }

    pub fn bind_navigation(&self, handler: impl Fn(MillerNavigation) + 'static) {
        self.navigation_dispatcher.bind(handler);
    }

    pub fn focus_active(&self) {
        if let Some(list) = self.active_list.borrow().as_ref() {
            list.grab_focus();
        } else {
            self.scroller.grab_focus();
        }
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
        self.active_list.borrow_mut().take();

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
        list.update_property(&[gtk::accessible::Property::Description(&format!(
            "Miller column {}. Use Up and Down to select items; logical Left and Right move between folders.",
            column.depth + 1
        ))]);

        let key_navigation = gtk::EventControllerKey::new();
        let navigation_dispatcher = self.navigation_dispatcher.clone();
        let list_for_keys = list.clone();
        let depth_for_keys = column.depth;
        key_navigation.connect_key_pressed(move |_, key, _, modifiers| {
            if !navigation_modifiers_allowed(modifiers) {
                return glib::Propagation::Proceed;
            }
            if let Some(command) = item_command_for_key(key) {
                let Some(model) = list_for_keys.model() else {
                    return glib::Propagation::Proceed;
                };
                if let Some(target) =
                    item_selection_target(first_selected_index(&model), model.n_items(), command)
                {
                    model.select_item(target, true);
                    list_for_keys.scroll_to(
                        target,
                        gtk::ListScrollFlags::FOCUS,
                        None::<gtk::ScrollInfo>,
                    );
                }
                return glib::Propagation::Stop;
            }
            let rtl = list_for_keys.direction() == gtk::TextDirection::Rtl;
            let Some(command) = logical_navigation_for_key(key, rtl) else {
                return glib::Propagation::Proceed;
            };
            let selected_entry = list_for_keys
                .model()
                .and_then(|model| selected_entry_from_model(&model));
            navigation_dispatcher.dispatch(MillerNavigation {
                depth: depth_for_keys,
                command,
                selected_entry,
            });
            glib::Propagation::Stop
        });
        list.add_controller(key_navigation);
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
        if column.is_active {
            self.active_list.replace(Some(list.clone()));
        }

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

fn logical_navigation_for_key(key: gtk::gdk::Key, rtl: bool) -> Option<MillerNavigationCommand> {
    match (key, rtl) {
        (gtk::gdk::Key::Left, false) | (gtk::gdk::Key::Right, true) => {
            Some(MillerNavigationCommand::Parent)
        }
        (gtk::gdk::Key::Right, false) | (gtk::gdk::Key::Left, true) => {
            Some(MillerNavigationCommand::Child)
        }
        _ => None,
    }
}

fn item_command_for_key(key: gtk::gdk::Key) -> Option<MillerItemCommand> {
    match key {
        gtk::gdk::Key::Up => Some(MillerItemCommand::Previous),
        gtk::gdk::Key::Down => Some(MillerItemCommand::Next),
        gtk::gdk::Key::Home => Some(MillerItemCommand::First),
        gtk::gdk::Key::End => Some(MillerItemCommand::Last),
        _ => None,
    }
}

fn navigation_modifiers_allowed(modifiers: gtk::gdk::ModifierType) -> bool {
    !modifiers.intersects(
        gtk::gdk::ModifierType::CONTROL_MASK
            | gtk::gdk::ModifierType::ALT_MASK
            | gtk::gdk::ModifierType::SHIFT_MASK
            | gtk::gdk::ModifierType::SUPER_MASK,
    )
}

fn item_selection_target(
    selected: Option<u32>,
    item_count: u32,
    command: MillerItemCommand,
) -> Option<u32> {
    if item_count == 0 {
        return None;
    }
    let last = item_count - 1;
    Some(match command {
        MillerItemCommand::Previous => selected.unwrap_or(0).saturating_sub(1),
        MillerItemCommand::Next => selected.map_or(0, |index| index.saturating_add(1).min(last)),
        MillerItemCommand::First => 0,
        MillerItemCommand::Last => last,
    })
}

fn first_selected_index(model: &gtk::SelectionModel) -> Option<u32> {
    (0..model.n_items()).find(|index| model.is_selected(*index))
}

fn selected_entry_from_model(model: &gtk::SelectionModel) -> Option<Arc<DirectoryEntry>> {
    let index = first_selected_index(model)?;
    let object = model.item(index)?.downcast::<glib::BoxedAnyObject>().ok()?;
    Some(object.borrow::<Arc<DirectoryEntry>>().clone())
}

fn trackpad_prefers_horizontal(delta_x: f64, delta_y: f64) -> bool {
    delta_x != 0.0 && delta_x.abs() > delta_y.abs()
}

fn horizontal_scroll_target(
    current: f64,
    delta: f64,
    lower: f64,
    upper: f64,
    page_size: f64,
) -> f64 {
    let maximum = (upper - page_size).max(lower);
    (current + delta).clamp(lower, maximum)
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::ffi::OsStringExt, path::PathBuf, sync::Arc};

    use floe_core::{
        MILLER_COLUMN_CAPACITY, MillerChildKind, MillerColumnModel, enumerate_directory,
    };
    use tempfile::tempdir;

    use super::{
        MILLER_SNAPSHOT_ENTRY_CAPACITY, MillerItemCommand, MillerMotionPolicy,
        MillerNavigationCommand, MillerPresentationState, horizontal_scroll_target,
        item_command_for_key, item_selection_target, logical_navigation_for_key,
        miller_column_accessible_description, navigation_modifiers_allowed,
        trackpad_prefers_horizontal,
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

    #[test]
    fn phase_8c_policy_maps_logical_directions_rtl_items_and_reduced_motion() {
        assert_eq!(
            logical_navigation_for_key(gtk::gdk::Key::Left, false),
            Some(MillerNavigationCommand::Parent)
        );
        assert_eq!(
            logical_navigation_for_key(gtk::gdk::Key::Right, false),
            Some(MillerNavigationCommand::Child)
        );
        assert_eq!(
            logical_navigation_for_key(gtk::gdk::Key::Left, true),
            Some(MillerNavigationCommand::Child)
        );
        assert_eq!(
            logical_navigation_for_key(gtk::gdk::Key::Right, true),
            Some(MillerNavigationCommand::Parent)
        );
        assert_eq!(
            item_command_for_key(gtk::gdk::Key::Home),
            Some(MillerItemCommand::First)
        );
        assert!(!MillerMotionPolicy::from_animations_enabled(false).kinetic_scrolling);
        assert!(MillerMotionPolicy::from_animations_enabled(true).kinetic_scrolling);
    }

    #[test]
    fn phase_8c_focus_selection_targets_are_bounded_and_predictable() {
        assert_eq!(
            item_selection_target(None, 0, MillerItemCommand::Next),
            None
        );
        assert_eq!(
            item_selection_target(None, 4, MillerItemCommand::Next),
            Some(0)
        );
        assert_eq!(
            item_selection_target(Some(0), 4, MillerItemCommand::Previous),
            Some(0)
        );
        assert_eq!(
            item_selection_target(Some(3), 4, MillerItemCommand::Next),
            Some(3)
        );
        assert_eq!(
            item_selection_target(Some(2), 4, MillerItemCommand::First),
            Some(0)
        );
        assert_eq!(
            item_selection_target(Some(1), 4, MillerItemCommand::Last),
            Some(3)
        );
    }

    #[test]
    fn phase_8c_trackpad_consumes_only_dominant_horizontal_motion_and_clamps() {
        assert!(trackpad_prefers_horizontal(2.0, 0.5));
        assert!(trackpad_prefers_horizontal(-2.0, 0.5));
        assert!(!trackpad_prefers_horizontal(0.5, 2.0));
        assert!(!trackpad_prefers_horizontal(0.0, 0.0));
        assert_eq!(
            horizontal_scroll_target(20.0, -50.0, 0.0, 500.0, 100.0),
            0.0
        );
        assert_eq!(
            horizontal_scroll_target(390.0, 50.0, 0.0, 500.0, 100.0),
            400.0
        );
    }

    #[test]
    fn phase_8c_integration_preserves_modified_shortcuts_for_other_surfaces() {
        assert!(navigation_modifiers_allowed(gtk::gdk::ModifierType::empty()));
        assert!(!navigation_modifiers_allowed(
            gtk::gdk::ModifierType::SHIFT_MASK
        ));
        assert!(!navigation_modifiers_allowed(
            gtk::gdk::ModifierType::CONTROL_MASK
        ));
        assert!(!navigation_modifiers_allowed(
            gtk::gdk::ModifierType::ALT_MASK
        ));
        assert!(VIEW_ACTIONS.contains(&("view-list", ViewCommand::List)));
        assert!(VIEW_ACTIONS.contains(&("view-grid", ViewCommand::Grid)));
    }
}
