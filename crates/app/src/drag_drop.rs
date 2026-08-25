use std::{
    cell::RefCell,
    collections::HashSet,
    path::{Path, PathBuf},
    rc::Rc,
};

use gtk::{gdk, gio, prelude::*};
use thiserror::Error;

const DROP_ACTIONS: gdk::DragAction = gdk::DragAction::COPY
    .union(gdk::DragAction::MOVE)
    .union(gdk::DragAction::LINK);
const EDGE_SCROLL_ZONE: f64 = 56.0;
const EDGE_SCROLL_STEP: f64 = 22.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DropAction {
    Copy,
    Move,
    Link,
    Trash,
}

impl DropAction {
    fn from_gdk(action: gdk::DragAction) -> Self {
        if action.contains(gdk::DragAction::LINK) {
            Self::Link
        } else if action.contains(gdk::DragAction::MOVE) {
            Self::Move
        } else {
            Self::Copy
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Move => "Move",
            Self::Link => "Create links in",
            Self::Trash => "Move to Trash",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DropDestination {
    Directory(PathBuf),
    Trash,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DropRequest {
    sources: Vec<PathBuf>,
    destination: DropDestination,
    action: DropAction,
}

impl DropRequest {
    pub fn new(
        sources: Vec<PathBuf>,
        destination: DropDestination,
        action: DropAction,
    ) -> Result<Self, DropPolicyError> {
        let sources = normalize_sources(sources)?;
        if matches!(destination, DropDestination::Trash) && action != DropAction::Trash {
            return Err(DropPolicyError::InvalidTrashAction);
        }
        if let DropDestination::Directory(directory) = &destination {
            validate_directory_destination(&sources, directory)?;
        }
        Ok(Self {
            sources,
            destination,
            action,
        })
    }

    pub fn sources(&self) -> &[PathBuf] {
        &self.sources
    }

    pub const fn destination(&self) -> &DropDestination {
        &self.destination
    }

    pub const fn action(&self) -> DropAction {
        self.action
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedDropItem {
    pub source: PathBuf,
    pub destination: PathBuf,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DropPolicyError {
    #[error("the drag contains no local files")]
    Empty,
    #[error("the drag contains a non-local or unavailable file")]
    NonLocalFile,
    #[error("a dragged source has no final path component: {0:?}")]
    InvalidSource(PathBuf),
    #[error("the drop directory is not an absolute usable path: {0:?}")]
    InvalidDestination(PathBuf),
    #[error("the drop would place an item onto itself: {0:?}")]
    SameDestination(PathBuf),
    #[error("the drop would place a directory inside itself: {0:?}")]
    SelfNesting(PathBuf),
    #[error("Trash accepts only the Move to Trash action")]
    InvalidTrashAction,
}

pub fn plan_directory_drop(request: &DropRequest) -> Result<Vec<PlannedDropItem>, DropPolicyError> {
    let DropDestination::Directory(directory) = request.destination() else {
        return Ok(Vec::new());
    };
    request
        .sources()
        .iter()
        .map(|source| {
            let name = source
                .file_name()
                .ok_or_else(|| DropPolicyError::InvalidSource(source.clone()))?;
            Ok(PlannedDropItem {
                source: source.clone(),
                destination: directory.join(name),
            })
        })
        .collect()
}

fn normalize_sources(sources: Vec<PathBuf>) -> Result<Vec<PathBuf>, DropPolicyError> {
    let mut seen = HashSet::with_capacity(sources.len());
    let mut normalized = Vec::with_capacity(sources.len());
    for source in sources {
        if source.file_name().is_none() {
            return Err(DropPolicyError::InvalidSource(source));
        }
        if seen.insert(source.clone()) {
            normalized.push(source);
        }
    }
    if normalized.is_empty() {
        return Err(DropPolicyError::Empty);
    }
    Ok(normalized)
}

fn validate_directory_destination(
    sources: &[PathBuf],
    directory: &Path,
) -> Result<(), DropPolicyError> {
    if !directory.is_absolute() {
        return Err(DropPolicyError::InvalidDestination(directory.to_path_buf()));
    }
    for source in sources {
        let name = source
            .file_name()
            .ok_or_else(|| DropPolicyError::InvalidSource(source.clone()))?;
        let destination = directory.join(name);
        if destination == *source {
            return Err(DropPolicyError::SameDestination(source.clone()));
        }
        if directory.starts_with(source) {
            return Err(DropPolicyError::SelfNesting(source.clone()));
        }
    }
    Ok(())
}

pub fn paths_from_file_list(files: &gdk::FileList) -> Result<Vec<PathBuf>, DropPolicyError> {
    paths_from_files(&files.files())
}

fn paths_from_files(files: &[gio::File]) -> Result<Vec<PathBuf>, DropPolicyError> {
    let mut paths = Vec::with_capacity(files.len());
    for file in files {
        let Some(path) = file.path() else {
            return Err(DropPolicyError::NonLocalFile);
        };
        paths.push(path);
    }
    normalize_sources(paths)
}

pub fn file_list_provider(paths: &[PathBuf]) -> Option<gdk::ContentProvider> {
    if paths.is_empty() {
        return None;
    }
    let files = paths.iter().map(gio::File::for_path).collect::<Vec<_>>();
    let list = gdk::FileList::from_array(&files);
    Some(gdk::ContentProvider::for_value(&list.to_value()))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DropEvent {
    Commit(DropRequest),
    Feedback(Option<String>),
    HoverEnter(PathBuf),
    HoverLeave,
}

type DropHandler = Rc<dyn Fn(DropEvent)>;

#[derive(Clone, Default)]
pub struct DropDispatcher {
    handler: Rc<RefCell<Option<DropHandler>>>,
}

impl DropDispatcher {
    pub fn bind(&self, handler: impl Fn(DropEvent) + 'static) {
        self.handler.replace(Some(Rc::new(handler)));
    }

    fn emit(&self, event: DropEvent) {
        if let Some(handler) = self.handler.borrow().as_ref() {
            handler(event);
        }
    }
}

pub type DestinationResolver = Rc<dyn Fn() -> Option<DropDestination>>;

pub fn install_drag_source(widget: &impl IsA<gtk::Widget>, sources: Rc<dyn Fn() -> Vec<PathBuf>>) {
    let drag_source = gtk::DragSource::new();
    drag_source.set_actions(DROP_ACTIONS);
    drag_source.connect_prepare(move |_, _, _| file_list_provider(&sources()));
    widget.add_controller(drag_source);
}

pub fn install_drop_target(
    widget: &impl IsA<gtk::Widget>,
    destination: DestinationResolver,
    dispatcher: DropDispatcher,
    hover_open: bool,
    autoscroll: bool,
) {
    let widget = widget.as_ref().clone();
    let target = gtk::DropTarget::new(gdk::FileList::static_type(), DROP_ACTIONS);
    target.set_preload(true);

    let enter_widget = widget.clone();
    let enter_destination = Rc::clone(&destination);
    let enter_dispatcher = dispatcher.clone();
    target.connect_enter(move |target, _, _| {
        let Some(destination) = enter_destination() else {
            target.reject();
            return gdk::DragAction::empty();
        };
        let action = negotiated_action(target, &destination);
        enter_widget.add_css_class("floe-drop-target");
        let message = drop_feedback(&destination, action);
        enter_widget.update_property(&[gtk::accessible::Property::Description(&message)]);
        enter_dispatcher.emit(DropEvent::Feedback(Some(message)));
        if hover_open && let DropDestination::Directory(path) = destination {
            enter_dispatcher.emit(DropEvent::HoverEnter(path));
        }
        action_to_gdk(action)
    });

    let motion_widget = widget.clone();
    let motion_destination = Rc::clone(&destination);
    let motion_dispatcher = dispatcher.clone();
    target.connect_motion(move |target, _, y| {
        let Some(destination) = motion_destination() else {
            target.reject();
            return gdk::DragAction::empty();
        };
        let action = negotiated_action(target, &destination);
        let message = drop_feedback(&destination, action);
        motion_widget.update_property(&[gtk::accessible::Property::Description(&message)]);
        motion_dispatcher.emit(DropEvent::Feedback(Some(message)));
        if autoscroll {
            scroll_at_edge(&motion_widget, y);
        }
        action_to_gdk(action)
    });

    let leave_widget = widget.clone();
    let leave_dispatcher = dispatcher.clone();
    target.connect_leave(move |_| {
        leave_widget.remove_css_class("floe-drop-target");
        leave_widget.update_property(&[gtk::accessible::Property::Description("")]);
        leave_dispatcher.emit(DropEvent::HoverLeave);
        leave_dispatcher.emit(DropEvent::Feedback(None));
    });

    let drop_widget = widget.clone();
    target.connect_drop(move |target, value, _, _| {
        drop_widget.remove_css_class("floe-drop-target");
        dispatcher.emit(DropEvent::HoverLeave);
        dispatcher.emit(DropEvent::Feedback(None));
        let Some(destination) = destination() else {
            return false;
        };
        let Ok(files) = value.get::<gdk::FileList>() else {
            return false;
        };
        let Ok(paths) = paths_from_file_list(&files) else {
            dispatcher.emit(DropEvent::Feedback(Some(
                "Only local filesystem items can be dropped here".to_owned(),
            )));
            return false;
        };
        let action = negotiated_action(target, &destination);
        match DropRequest::new(paths, destination, action) {
            Ok(request) => {
                dispatcher.emit(DropEvent::Commit(request));
                true
            }
            Err(error) => {
                dispatcher.emit(DropEvent::Feedback(Some(error.to_string())));
                false
            }
        }
    });

    widget.add_controller(target);
}

fn negotiated_action(target: &gtk::DropTarget, destination: &DropDestination) -> DropAction {
    if matches!(destination, DropDestination::Trash) {
        return DropAction::Trash;
    }
    let selected = target
        .current_drop()
        .and_then(|drop| drop.drag())
        .map(|drag| drag.selected_action())
        .unwrap_or(gdk::DragAction::COPY);
    DropAction::from_gdk(selected)
}

fn action_to_gdk(action: DropAction) -> gdk::DragAction {
    match action {
        DropAction::Copy => gdk::DragAction::COPY,
        DropAction::Move | DropAction::Trash => gdk::DragAction::MOVE,
        DropAction::Link => gdk::DragAction::LINK,
    }
}

fn drop_feedback(destination: &DropDestination, action: DropAction) -> String {
    match destination {
        DropDestination::Directory(path) => {
            format!(
                "{} {} — release to apply",
                action.label(),
                path.to_string_lossy()
            )
        }
        DropDestination::Trash => "Move to Trash — release to apply".to_owned(),
    }
}

pub fn edge_scroll_delta(y: f64, height: f64) -> f64 {
    if height <= 0.0 {
        0.0
    } else if y < EDGE_SCROLL_ZONE {
        -EDGE_SCROLL_STEP
    } else if y > height - EDGE_SCROLL_ZONE {
        EDGE_SCROLL_STEP
    } else {
        0.0
    }
}

fn scroll_at_edge(widget: &gtk::Widget, y: f64) {
    let delta = edge_scroll_delta(y, f64::from(widget.height()));
    if delta == 0.0 {
        return;
    }
    let Some(scroller) = widget
        .ancestor(gtk::ScrolledWindow::static_type())
        .and_downcast::<gtk::ScrolledWindow>()
    else {
        return;
    };
    let adjustment = scroller.vadjustment();
    let maximum = (adjustment.upper() - adjustment.page_size()).max(adjustment.lower());
    adjustment.set_value((adjustment.value() + delta).clamp(adjustment.lower(), maximum));
}

#[cfg(test)]
mod tests {
    use std::os::unix::ffi::OsStringExt;

    use super::*;

    #[test]
    fn phase_6r_drag_policy_preserves_raw_paths_and_rejects_unsafe_destinations() {
        let raw = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/raw-\xff".to_vec()));
        let request = DropRequest::new(
            vec![raw.clone(), raw.clone()],
            DropDestination::Directory(PathBuf::from("/destination")),
            DropAction::Copy,
        )
        .expect("exact raw path should be accepted");
        assert_eq!(request.sources(), &[raw]);
        assert!(matches!(
            DropRequest::new(
                vec![PathBuf::from("/tmp/folder")],
                DropDestination::Directory(PathBuf::from("/tmp/folder/child")),
                DropAction::Move,
            ),
            Err(DropPolicyError::SelfNesting(_))
        ));
        assert!(matches!(
            DropRequest::new(
                vec![PathBuf::from("/tmp/item")],
                DropDestination::Directory(PathBuf::from("/tmp")),
                DropAction::Copy,
            ),
            Err(DropPolicyError::SameDestination(_))
        ));
        assert!(matches!(
            DropRequest::new(
                vec![PathBuf::from("/")],
                DropDestination::Directory(PathBuf::from("/tmp")),
                DropAction::Copy,
            ),
            Err(DropPolicyError::InvalidSource(_))
        ));
    }

    #[test]
    fn phase_6r_payload_round_trips_local_gfiles_and_rejects_empty() {
        let paths = vec![PathBuf::from("/tmp/one"), PathBuf::from("/tmp/two")];
        let files = paths.iter().map(gio::File::for_path).collect::<Vec<_>>();
        assert_eq!(paths_from_files(&files).expect("local paths"), paths);
        assert_eq!(normalize_sources(Vec::new()), Err(DropPolicyError::Empty));
        assert_eq!(
            paths_from_files(&[gio::File::for_uri("sftp://host/path")]),
            Err(DropPolicyError::NonLocalFile)
        );
    }

    #[test]
    fn phase_6r_destination_plans_fifo_exact_names() {
        let request = DropRequest::new(
            vec![PathBuf::from("/a/first"), PathBuf::from("/b/second")],
            DropDestination::Directory(PathBuf::from("/target")),
            DropAction::Move,
        )
        .expect("valid request");
        let planned = plan_directory_drop(&request).expect("plan");
        assert_eq!(planned[0].destination, PathBuf::from("/target/first"));
        assert_eq!(planned[1].destination, PathBuf::from("/target/second"));
    }

    #[test]
    fn phase_6r_motion_has_bounded_edge_zones() {
        assert_eq!(edge_scroll_delta(0.0, 500.0), -EDGE_SCROLL_STEP);
        assert_eq!(edge_scroll_delta(250.0, 500.0), 0.0);
        assert_eq!(edge_scroll_delta(499.0, 500.0), EDGE_SCROLL_STEP);
        assert_eq!(edge_scroll_delta(0.0, 0.0), 0.0);
    }

    #[test]
    fn phase_6r_accessibility_feedback_names_action_and_release() {
        let message = drop_feedback(
            &DropDestination::Directory(PathBuf::from("/target")),
            DropAction::Link,
        );
        assert!(message.contains("Create links in"));
        assert!(message.contains("release"));
        assert_eq!(
            drop_feedback(&DropDestination::Trash, DropAction::Trash),
            "Move to Trash — release to apply"
        );
    }
}
