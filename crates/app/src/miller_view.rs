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

use crate::{
    drag_drop::{
        DropDestination, DropDispatcher, DropHoverTarget, install_drag_source, install_drop_target,
        install_drop_target_with_hover,
    },
    miller_detail::{MillerDetailPresentation, MillerDetailState, MillerDetailSurface},
    preview::PreviewContent,
    view::MillerColumnWidth,
};

pub const MILLER_SNAPSHOT_ENTRY_CAPACITY: usize = 4_096;
const MILLER_TRACKPAD_SCALE: f64 = 48.0;
const PREVIEW_ZOOM_MIN: u16 = 50;
const PREVIEW_ZOOM_MAX: u16 = 400;

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

pub const MILLER_ACTION_SELECTION_CAPACITY: usize = 4_096;

#[derive(Clone, Debug)]
pub struct MillerActionContext {
    pub depth: usize,
    pub directory: PathBuf,
    pub selected_entries: Vec<Arc<DirectoryEntry>>,
    pub background: bool,
    pub overflowed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MillerActionContextError {
    Overflowed,
    ShapeMismatch,
    NotDirectChild,
    StaleEntry,
}

pub fn resolve_action_context_entries(
    context: &MillerActionContext,
    available: &[Arc<DirectoryEntry>],
) -> Result<Vec<Arc<DirectoryEntry>>, MillerActionContextError> {
    if context.overflowed {
        return Err(MillerActionContextError::Overflowed);
    }
    if context.background != context.selected_entries.is_empty() {
        return Err(MillerActionContextError::ShapeMismatch);
    }
    let mut resolved = Vec::with_capacity(context.selected_entries.len());
    for selected in &context.selected_entries {
        if selected.path().parent() != Some(context.directory.as_path()) {
            return Err(MillerActionContextError::NotDirectChild);
        }
        let entry = available
            .iter()
            .find(|entry| entry.path() == selected.path())
            .ok_or(MillerActionContextError::StaleEntry)?;
        resolved.push(Arc::clone(entry));
    }
    Ok(resolved)
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
type ActionContextHandler = Box<dyn Fn(MillerActionContext)>;

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

#[derive(Clone, Default)]
struct MillerActionDispatcher(Rc<RefCell<Option<ActionContextHandler>>>);

impl MillerActionDispatcher {
    fn bind(&self, handler: impl Fn(MillerActionContext) + 'static) {
        self.0.replace(Some(Box::new(handler)));
    }

    fn dispatch(&self, context: MillerActionContext) {
        if let Some(handler) = self.0.borrow().as_ref() {
            handler(context);
        }
    }
}

pub struct MillerView {
    scroller: gtk::ScrolledWindow,
    columns: gtk::Box,
    width: Cell<MillerColumnWidth>,
    detail_width: Cell<MillerColumnWidth>,
    dispatcher: MillerActivationDispatcher,
    navigation_dispatcher: MillerNavigationDispatcher,
    action_dispatcher: MillerActionDispatcher,
    active_list: RefCell<Option<gtk::ListView>>,
    detail_widget: RefCell<Option<gtk::Box>>,
    detail_media: RefCell<Option<gtk::MediaFile>>,
    detail_zoom_percent: Cell<u16>,
    file_context_model: gio::MenuModel,
    background_context_model: gio::MenuModel,
    drop_dispatcher: DropDispatcher,
    vim_mode: Rc<Cell<bool>>,
}

impl MillerView {
    pub fn new(
        file_context_model: &gio::MenuModel,
        background_context_model: &gio::MenuModel,
        drop_dispatcher: &DropDispatcher,
    ) -> Self {
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
            detail_width: Cell::new(MillerColumnWidth::default()),
            dispatcher: MillerActivationDispatcher::default(),
            navigation_dispatcher: MillerNavigationDispatcher::default(),
            action_dispatcher: MillerActionDispatcher::default(),
            active_list: RefCell::new(None),
            detail_widget: RefCell::new(None),
            detail_media: RefCell::new(None),
            detail_zoom_percent: Cell::new(100),
            file_context_model: file_context_model.clone(),
            background_context_model: background_context_model.clone(),
            drop_dispatcher: drop_dispatcher.clone(),
            vim_mode: Rc::new(Cell::new(false)),
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

    pub fn bind_action_context(&self, handler: impl Fn(MillerActionContext) + 'static) {
        self.action_dispatcher.bind(handler);
    }

    pub fn focus_active(&self) {
        if let Some(list) = self.active_list.borrow().as_ref() {
            list.grab_focus();
        } else {
            self.scroller.grab_focus();
        }
    }

    pub fn focus_detail(&self) -> bool {
        self.detail_widget
            .borrow()
            .as_ref()
            .is_some_and(|detail| detail.grab_focus())
    }

    pub fn width(&self) -> MillerColumnWidth {
        self.width.get()
    }

    pub fn set_vim_mode(&self, enabled: bool) {
        self.vim_mode.set(enabled);
        self.scroller
            .update_property(&[gtk::accessible::Property::Description(if enabled {
                "Miller column browser. Vim navigation mode enabled."
            } else {
                "Miller column browser. Vim navigation mode disabled."
            })]);
    }

    pub fn detail_width(&self) -> MillerColumnWidth {
        self.detail_width.get()
    }

    pub fn preview_zoom_in(&self) {
        self.detail_zoom_percent
            .set(adjust_preview_zoom(self.detail_zoom_percent.get(), 25));
    }

    pub fn preview_zoom_out(&self) {
        self.detail_zoom_percent
            .set(adjust_preview_zoom(self.detail_zoom_percent.get(), -25));
    }

    pub fn preview_zoom_reset(&self) {
        self.detail_zoom_percent.set(100);
    }

    pub fn set_width(&self, width: MillerColumnWidth) {
        self.width.set(width);
        let mut child = self.columns.first_child();
        while let Some(widget) = child {
            if !widget.has_css_class("floe-miller-detail-column") {
                widget.set_width_request(i32::from(width.get()));
            }
            child = widget.next_sibling();
        }
        let description = format!("Miller column width: {} pixels", width.get());
        self.scroller
            .update_property(&[gtk::accessible::Property::Description(&description)]);
    }

    pub fn set_detail_width(&self, width: MillerColumnWidth) {
        self.detail_width.set(width);
        if let Some(detail) = self.detail_widget.borrow().as_ref() {
            detail.set_width_request(i32::from(width.get()));
            detail.update_property(&[gtk::accessible::Property::Description(&format!(
                "Inspector and Preview column width: {} pixels",
                width.get()
            ))]);
        }
    }

    pub fn render(
        &self,
        columns: &[MillerRenderColumn],
        active_selection: &gtk::MultiSelection,
        detail_state: &MillerDetailState,
    ) {
        if let Some(media) = self.detail_media.borrow_mut().take() {
            media.pause();
            media.clear();
        }
        while let Some(child) = self.columns.first_child() {
            self.columns.remove(&child);
        }
        self.active_list.borrow_mut().take();
        self.detail_widget.borrow_mut().take();

        for column in columns {
            let shell = self.build_column(column, active_selection);
            self.columns.append(&shell);
        }
        if detail_state.is_visible() {
            let detail = self.build_detail_column(detail_state);
            self.columns.append(&detail);
            self.detail_widget.replace(Some(detail));
        }
    }

    fn build_detail_column(&self, state: &MillerDetailState) -> gtk::Box {
        let presentation = MillerDetailPresentation::from(state);
        let heading = gtk::Label::builder()
            .label(presentation.title)
            .xalign(0.0)
            .margin_start(12)
            .margin_end(12)
            .margin_top(10)
            .margin_bottom(8)
            .build();
        heading.add_css_class("heading");
        let icon_name = match state.surface() {
            Some(MillerDetailSurface::Preview) => "document-open-symbolic",
            Some(MillerDetailSurface::Inspector) | None => "dialog-information-symbolic",
        };
        let icon = gtk::Image::builder()
            .icon_name(icon_name)
            .pixel_size(42)
            .margin_top(24)
            .build();
        icon.set_accessible_role(gtk::AccessibleRole::Presentation);
        let message = gtk::Label::builder()
            .label(&presentation.message)
            .wrap(true)
            .justify(gtk::Justification::Center)
            .margin_start(18)
            .margin_end(18)
            .margin_top(12)
            .build();
        message.add_css_class("dim-label");

        let provided_content = match state {
            MillerDetailState::Provided { payload, .. } => match &payload.content {
                PreviewContent::Image {
                    width,
                    height,
                    rowstride,
                    rgba,
                    ..
                } => {
                    let Ok(width) = i32::try_from(*width) else {
                        return self.build_unavailable_detail_column(
                            state,
                            "Image dimensions exceed GTK limits.",
                        );
                    };
                    let Ok(height) = i32::try_from(*height) else {
                        return self.build_unavailable_detail_column(
                            state,
                            "Image dimensions exceed GTK limits.",
                        );
                    };
                    let bytes = glib::Bytes::from_owned(Arc::clone(rgba));
                    let texture = gtk::gdk::MemoryTexture::new(
                        width,
                        height,
                        gtk::gdk::MemoryFormat::R8g8b8a8,
                        &bytes,
                        *rowstride,
                    );
                    let picture = gtk::Picture::for_paintable(&texture);
                    picture.set_can_shrink(true);
                    picture.set_content_fit(gtk::ContentFit::Contain);
                    picture.set_margin_top(12);
                    picture.set_margin_start(10);
                    picture.set_margin_end(10);
                    picture.set_accessible_role(gtk::AccessibleRole::Img);
                    self.apply_preview_zoom(&picture, width, height);
                    picture.upcast::<gtk::Widget>()
                }
                PreviewContent::Text { text, .. } => {
                    let buffer = gtk::TextBuffer::new(None);
                    buffer.set_text(text);
                    let text_view = gtk::TextView::with_buffer(&buffer);
                    text_view.set_editable(false);
                    text_view.set_cursor_visible(true);
                    text_view.set_monospace(true);
                    text_view.set_wrap_mode(gtk::WrapMode::None);
                    text_view.set_left_margin(10);
                    text_view.set_right_margin(10);
                    text_view.update_property(&[gtk::accessible::Property::Description(
                        "Selectable passive source preview. File content is not executed.",
                    )]);
                    let scroller = gtk::ScrolledWindow::builder()
                        .hscrollbar_policy(gtk::PolicyType::Automatic)
                        .vscrollbar_policy(gtk::PolicyType::Automatic)
                        .min_content_height(280)
                        .margin_top(10)
                        .margin_start(8)
                        .margin_end(8)
                        .child(&text_view)
                        .build();
                    scroller.upcast::<gtk::Widget>()
                }
                PreviewContent::Document {
                    width,
                    height,
                    rowstride,
                    rgba,
                    ..
                } => {
                    let Ok(width) = i32::try_from(*width) else {
                        return self.build_unavailable_detail_column(
                            state,
                            "Document rendition dimensions exceed GTK limits.",
                        );
                    };
                    let Ok(height) = i32::try_from(*height) else {
                        return self.build_unavailable_detail_column(
                            state,
                            "Document rendition dimensions exceed GTK limits.",
                        );
                    };
                    let bytes = glib::Bytes::from_owned(Arc::clone(rgba));
                    let texture = gtk::gdk::MemoryTexture::new(
                        width,
                        height,
                        gtk::gdk::MemoryFormat::R8g8b8a8,
                        &bytes,
                        *rowstride,
                    );
                    let picture = gtk::Picture::for_paintable(&texture);
                    picture.set_can_shrink(true);
                    picture.set_content_fit(gtk::ContentFit::Contain);
                    picture.set_margin_top(12);
                    picture.set_margin_start(10);
                    picture.set_margin_end(10);
                    picture.set_accessible_role(gtk::AccessibleRole::Img);
                    self.apply_preview_zoom(&picture, width, height);
                    picture.update_property(&[gtk::accessible::Property::Description(
                        "Passive first-page document rendition.",
                    )]);
                    picture.upcast::<gtk::Widget>()
                }
                PreviewContent::Media {
                    path,
                    is_video,
                    poster,
                    ..
                } => {
                    let media = gtk::MediaFile::for_file(&gio::File::for_path(path));
                    let controls = gtk::MediaControls::new(Some(&media));
                    controls.update_property(&[gtk::accessible::Property::Description(
                        "Native media controls with play, pause, and seek.",
                    )]);
                    let media_box = gtk::Box::new(gtk::Orientation::Vertical, 10);
                    media_box.set_margin_top(10);
                    media_box.set_margin_start(8);
                    media_box.set_margin_end(8);
                    if *is_video {
                        let video = gtk::Video::for_media_stream(Some(&media));
                        video.set_autoplay(false);
                        video.set_loop(false);
                        video.set_hexpand(true);
                        video.set_vexpand(true);
                        video.set_accessible_role(gtk::AccessibleRole::Img);
                        if let Some(poster) = poster {
                            let Ok(width) = i32::try_from(poster.width) else {
                                return self.build_unavailable_detail_column(
                                    state,
                                    "Media poster dimensions exceed GTK limits.",
                                );
                            };
                            let Ok(height) = i32::try_from(poster.height) else {
                                return self.build_unavailable_detail_column(
                                    state,
                                    "Media poster dimensions exceed GTK limits.",
                                );
                            };
                            let bytes = glib::Bytes::from_owned(Arc::clone(&poster.rgba));
                            let texture = gtk::gdk::MemoryTexture::new(
                                width,
                                height,
                                gtk::gdk::MemoryFormat::R8g8b8a8,
                                &bytes,
                                poster.rowstride,
                            );
                            let picture = gtk::Picture::for_paintable(&texture);
                            picture.set_can_shrink(true);
                            picture.set_content_fit(gtk::ContentFit::Contain);
                            let stack = gtk::Stack::new();
                            stack.add_named(&picture, Some("poster"));
                            stack.add_named(&video, Some("video"));
                            stack.set_visible_child_name("poster");
                            let stack_for_prepared = stack.clone();
                            media.connect_prepared_notify(move |stream| {
                                if stream.is_prepared() {
                                    stack_for_prepared.set_visible_child_name("video");
                                }
                            });
                            media_box.append(&stack);
                        } else {
                            media_box.append(&video);
                        }
                    } else {
                        let audio_icon = gtk::Image::builder()
                            .icon_name("audio-x-generic-symbolic")
                            .pixel_size(72)
                            .margin_top(24)
                            .margin_bottom(16)
                            .build();
                        audio_icon.set_accessible_role(gtk::AccessibleRole::Presentation);
                        media_box.append(&audio_icon);
                    }
                    media_box.append(&controls);
                    self.detail_media.replace(Some(media));
                    media_box.upcast::<gtk::Widget>()
                }
                PreviewContent::Font {
                    width,
                    height,
                    rowstride,
                    rgba,
                    ..
                } => {
                    let Ok(width) = i32::try_from(*width) else {
                        return self.build_unavailable_detail_column(
                            state,
                            "Font specimen dimensions exceed GTK limits.",
                        );
                    };
                    let Ok(height) = i32::try_from(*height) else {
                        return self.build_unavailable_detail_column(
                            state,
                            "Font specimen dimensions exceed GTK limits.",
                        );
                    };
                    let bytes = glib::Bytes::from_owned(Arc::clone(rgba));
                    let texture = gtk::gdk::MemoryTexture::new(
                        width,
                        height,
                        gtk::gdk::MemoryFormat::R8g8b8a8,
                        &bytes,
                        *rowstride,
                    );
                    let picture = gtk::Picture::for_paintable(&texture);
                    picture.set_can_shrink(true);
                    picture.set_content_fit(gtk::ContentFit::Contain);
                    picture.set_margin_top(12);
                    picture.set_margin_start(10);
                    picture.set_margin_end(10);
                    picture.update_property(&[gtk::accessible::Property::Description(
                        "Passive font specimen image. The font is not installed.",
                    )]);
                    self.apply_preview_zoom(&picture, width, height);
                    picture.upcast::<gtk::Widget>()
                }
                PreviewContent::Archive { listing, .. } => {
                    let buffer = gtk::TextBuffer::new(None);
                    buffer.set_text(listing);
                    let text_view = gtk::TextView::with_buffer(&buffer);
                    text_view.set_editable(false);
                    text_view.set_cursor_visible(true);
                    text_view.set_monospace(true);
                    text_view.set_wrap_mode(gtk::WrapMode::None);
                    text_view.set_left_margin(10);
                    text_view.set_right_margin(10);
                    text_view.update_property(&[gtk::accessible::Property::Description(
                        "Selectable read-only archive listing. No member is extracted.",
                    )]);
                    let scroller = gtk::ScrolledWindow::builder()
                        .hscrollbar_policy(gtk::PolicyType::Automatic)
                        .vscrollbar_policy(gtk::PolicyType::Automatic)
                        .min_content_height(280)
                        .margin_top(10)
                        .margin_start(8)
                        .margin_end(8)
                        .child(&text_view)
                        .build();
                    scroller.upcast::<gtk::Widget>()
                }
                PreviewContent::None => icon.clone().upcast::<gtk::Widget>(),
            },
            _ => icon.clone().upcast::<gtk::Widget>(),
        };
        let close = gtk::Button::with_label("Close Details");
        close.set_halign(gtk::Align::Center);
        close.set_margin_top(18);
        close.set_action_name(Some(match state.surface() {
            Some(MillerDetailSurface::Preview) => "win.miller-preview-hook",
            Some(MillerDetailSurface::Inspector) | None => "win.miller-inspector-hook",
        }));
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .vexpand(true)
            .build();
        content.append(&provided_content);
        if matches!(
            state,
            MillerDetailState::Provided {
                payload: crate::preview::PreviewPayload {
                    content: PreviewContent::Image { .. }
                        | PreviewContent::Document { .. }
                        | PreviewContent::Font { .. },
                    ..
                },
                ..
            }
        ) {
            let zoom_controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            zoom_controls.set_halign(gtk::Align::Center);
            zoom_controls.set_margin_top(8);
            for (label, action, description) in [
                ("−", "win.preview-zoom-out", "Zoom preview out"),
                ("100%", "win.preview-zoom-reset", "Reset preview zoom"),
                ("+", "win.preview-zoom-in", "Zoom preview in"),
                (
                    "Fullscreen",
                    "win.preview-fullscreen",
                    "Toggle fullscreen preview",
                ),
            ] {
                let button = gtk::Button::with_label(label);
                button.set_action_name(Some(action));
                button.update_property(&[gtk::accessible::Property::Description(description)]);
                zoom_controls.append(&button);
            }
            content.append(&zoom_controls);
        }
        content.append(&message);
        if matches!(state, MillerDetailState::Provided { .. }) {
            let clear_cache = gtk::Button::with_label("Clear Preview Cache");
            clear_cache.set_halign(gtk::Align::Center);
            clear_cache.set_margin_top(8);
            clear_cache.set_action_name(Some("win.preview-clear-cache"));
            clear_cache.update_property(&[gtk::accessible::Property::Description(
                "Clear Floe's memory-only Preview cache and cancel current Preview work.",
            )]);
            content.append(&clear_cache);
        }
        if state.surface() == Some(MillerDetailSurface::Inspector) {
            let width_controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            width_controls.set_halign(gtk::Align::Center);
            width_controls.set_margin_top(8);
            for (label, action, description) in [
                (
                    "Narrower",
                    "win.narrow-inspector",
                    "Narrow Inspector column",
                ),
                ("Wider", "win.widen-inspector", "Widen Inspector column"),
            ] {
                let button = gtk::Button::with_label(label);
                button.set_action_name(Some(action));
                button.update_property(&[gtk::accessible::Property::Description(description)]);
                width_controls.append(&button);
            }
            content.append(&width_controls);
        }
        content.append(&close);

        let shell = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .width_request(i32::from(self.detail_width.get().get()))
            .focusable(true)
            .build();
        shell.add_css_class("floe-panel");
        shell.add_css_class("floe-miller-column");
        shell.add_css_class("floe-miller-detail-column");
        shell.update_property(&[gtk::accessible::Property::Description(
            &presentation.accessible_description,
        )]);
        shell.append(&heading);
        shell.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        shell.append(&content);
        shell
    }

    fn apply_preview_zoom(&self, picture: &gtk::Picture, width: i32, height: i32) {
        let percent = i32::from(self.detail_zoom_percent.get());
        let scaled_width = width.saturating_mul(percent).saturating_div(100).max(1);
        let scaled_height = height.saturating_mul(percent).saturating_div(100).max(1);
        picture.set_size_request(scaled_width, scaled_height);
        picture.update_property(&[gtk::accessible::Property::Description(&format!(
            "Preview image at {percent} percent zoom; GTK scales for the active monitor."
        ))]);
    }

    fn build_unavailable_detail_column(&self, state: &MillerDetailState, reason: &str) -> gtk::Box {
        let shell = gtk::Box::new(gtk::Orientation::Vertical, 8);
        shell.set_width_request(i32::from(self.detail_width.get().get()));
        shell.add_css_class("floe-panel");
        shell.add_css_class("floe-miller-column");
        shell.add_css_class("floe-miller-detail-column");
        let heading = gtk::Label::builder()
            .label(
                state
                    .surface()
                    .map_or("Details", MillerDetailSurface::title),
            )
            .xalign(0.0)
            .margin_start(12)
            .margin_top(10)
            .build();
        heading.add_css_class("heading");
        let message = gtk::Label::builder()
            .label(reason)
            .wrap(true)
            .margin_start(18)
            .margin_end(18)
            .margin_top(18)
            .build();
        shell.append(&heading);
        shell.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        shell.append(&message);
        shell
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

        let item_menu = gtk::PopoverMenu::from_model(Some(&self.file_context_model));
        item_menu.set_has_arrow(false);
        let background_menu = gtk::PopoverMenu::from_model(Some(&self.background_context_model));
        background_menu.set_has_arrow(false);

        let factory = gtk::SignalListItemFactory::new();
        let row_model = model.clone();
        let row_menu = item_menu.clone();
        let row_dispatcher = self.action_dispatcher.clone();
        let row_drop_dispatcher = self.drop_dispatcher.clone();
        let row_directory = column.directory.clone();
        let row_depth = column.depth;
        factory.connect_setup(move |_, object| {
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
            let secondary_click = gtk::GestureClick::new();
            secondary_click.set_button(gtk::gdk::BUTTON_SECONDARY);
            let item_weak = item.downgrade();
            let selection = row_model.clone();
            let menu = row_menu.clone();
            let dispatcher = row_dispatcher.clone();
            let directory = row_directory.clone();
            secondary_click.connect_pressed(move |gesture, _, x, y| {
                let Some(item) = item_weak.upgrade() else {
                    return;
                };
                let position = item.position();
                if position == gtk::INVALID_LIST_POSITION {
                    return;
                }
                if !selection.is_selected(position) {
                    selection.select_item(position, true);
                }
                let (selected_entries, overflowed) = action_selection(&selection);
                dispatcher.dispatch(MillerActionContext {
                    depth: row_depth,
                    directory: directory.clone(),
                    selected_entries,
                    background: false,
                    overflowed,
                });
                if let Some(widget) = gesture.widget() {
                    popup_at_widget_point(&menu, &widget, x, y);
                }
                gesture.set_state(gtk::EventSequenceState::Claimed);
            });
            label.add_controller(secondary_click);

            let destination_item = item.downgrade();
            let destination = Rc::new(move || {
                let item = destination_item.upgrade()?;
                let object = item.item()?.downcast::<glib::BoxedAnyObject>().ok()?;
                let entry = object.borrow::<Arc<DirectoryEntry>>();
                entry
                    .is_navigable_directory()
                    .then(|| DropDestination::Directory(entry.path().to_path_buf()))
            });
            let hover = Rc::new(move |destination: &DropDestination| {
                miller_child_hover_target(row_depth, destination)
            });
            install_drop_target_with_hover(
                &label,
                destination,
                hover,
                row_drop_dispatcher.clone(),
                true,
            );
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

        let drag_model = model.clone();
        let list = gtk::ListView::new(Some(model), Some(factory));
        list.set_single_click_activate(false);
        list.add_css_class("floe-miller-column-list");
        list.update_property(&[gtk::accessible::Property::Description(&format!(
            "Miller column {}. Use Up and Down to select items; logical Left and Right move between folders.",
            column.depth + 1
        ))]);
        install_drag_source(&list, Rc::new(move || drag_paths(&drag_model)));
        let column_destination = column.directory.clone();
        install_drop_target(
            &list,
            Rc::new(move || Some(DropDestination::Directory(column_destination.clone()))),
            self.drop_dispatcher.clone(),
            false,
            true,
        );
        item_menu.set_parent(&list);
        background_menu.set_parent(&list);

        let background_click = gtk::GestureClick::new();
        background_click.set_button(gtk::gdk::BUTTON_SECONDARY);
        let background_menu_for_click = background_menu.clone();
        let background_dispatcher = self.action_dispatcher.clone();
        let background_directory = column.directory.clone();
        let background_depth = column.depth;
        background_click.connect_pressed(move |gesture, _, x, y| {
            let Some(widget) = gesture.widget() else {
                return;
            };
            if picked_entry_widget(&widget, x, y) {
                return;
            }
            background_dispatcher.dispatch(MillerActionContext {
                depth: background_depth,
                directory: background_directory.clone(),
                selected_entries: Vec::new(),
                background: true,
                overflowed: false,
            });
            popup_at_widget_point(&background_menu_for_click, &widget, x, y);
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        list.add_controller(background_click);

        let key_navigation = gtk::EventControllerKey::new();
        let navigation_dispatcher = self.navigation_dispatcher.clone();
        let activation_dispatcher = self.dispatcher.clone();
        let vim_mode = Rc::clone(&self.vim_mode);
        let action_dispatcher = self.action_dispatcher.clone();
        let item_menu_for_keys = item_menu.clone();
        let background_menu_for_keys = background_menu.clone();
        let action_directory = column.directory.clone();
        let list_for_keys = list.clone();
        let depth_for_keys = column.depth;
        key_navigation.connect_key_pressed(move |_, key, _, modifiers| {
            if context_menu_key(key, modifiers) {
                let Some(model) = list_for_keys.model() else {
                    return glib::Propagation::Proceed;
                };
                let (selected_entries, overflowed) = action_selection(&model);
                let background = selected_entries.is_empty();
                action_dispatcher.dispatch(MillerActionContext {
                    depth: depth_for_keys,
                    directory: action_directory.clone(),
                    selected_entries,
                    background,
                    overflowed,
                });
                if background {
                    background_menu_for_keys.set_pointing_to(None);
                    background_menu_for_keys.popup();
                } else {
                    item_menu_for_keys.set_pointing_to(None);
                    item_menu_for_keys.popup();
                }
                return glib::Propagation::Stop;
            }
            if !navigation_modifiers_allowed(modifiers) {
                return glib::Propagation::Proceed;
            }
            if let Some(command) = crate::vim_mode::command_for_input(
                vim_mode.get(),
                true,
                key.to_unicode(),
                crate::vim_mode::VimModifiers {
                    control: modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK),
                    alt: modifiers.contains(gtk::gdk::ModifierType::ALT_MASK),
                    shift: modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK),
                    super_key: modifiers.contains(gtk::gdk::ModifierType::SUPER_MASK),
                },
            ) {
                use crate::vim_mode::VimCommand;

                match command {
                    VimCommand::Previous
                    | VimCommand::Next
                    | VimCommand::First
                    | VimCommand::Last => {
                        let Some(model) = list_for_keys.model() else {
                            return glib::Propagation::Proceed;
                        };
                        let item_command = match command {
                            VimCommand::Previous => MillerItemCommand::Previous,
                            VimCommand::Next => MillerItemCommand::Next,
                            VimCommand::First => MillerItemCommand::First,
                            VimCommand::Last => MillerItemCommand::Last,
                            _ => unreachable!(),
                        };
                        if let Some(target) = item_selection_target(
                            first_selected_index(&model),
                            model.n_items(),
                            item_command,
                        ) {
                            model.select_item(target, true);
                            list_for_keys.scroll_to(
                                target,
                                gtk::ListScrollFlags::FOCUS,
                                None::<gtk::ScrollInfo>,
                            );
                        }
                    }
                    VimCommand::Parent | VimCommand::Child => {
                        let selected_entry = list_for_keys
                            .model()
                            .and_then(|model| selected_entry_from_model(&model));
                        navigation_dispatcher.dispatch(MillerNavigation {
                            depth: depth_for_keys,
                            command: if command == VimCommand::Parent {
                                MillerNavigationCommand::Parent
                            } else {
                                MillerNavigationCommand::Child
                            },
                            selected_entry,
                        });
                    }
                    VimCommand::Open => {
                        if let Some(entry) = list_for_keys
                            .model()
                            .and_then(|model| selected_entry_from_model(&model))
                        {
                            activation_dispatcher.dispatch(MillerActivation {
                                depth: depth_for_keys,
                                entry,
                            });
                        }
                    }
                }
                return glib::Propagation::Stop;
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
        if column.is_active {
            let detail_controls = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(6)
                .halign(gtk::Align::Center)
                .margin_top(4)
                .build();
            let preview = gtk::Button::with_label("Preview");
            preview.set_action_name(Some("win.miller-preview-hook"));
            preview.set_tooltip_text(Some("Prepare the selected file for Quick Preview"));
            let inspector = gtk::Button::with_label("Inspector");
            inspector.set_action_name(Some("win.miller-inspector-hook"));
            inspector.set_tooltip_text(Some("Prepare the selection for Inspector"));
            detail_controls.append(&preview);
            detail_controls.append(&inspector);
            shell.append(&detail_controls);
        }
        shell.append(&status);
        shell
    }
}

fn adjust_preview_zoom(current: u16, delta: i16) -> u16 {
    let adjusted = i32::from(current).saturating_add(i32::from(delta));
    adjusted.clamp(i32::from(PREVIEW_ZOOM_MIN), i32::from(PREVIEW_ZOOM_MAX)) as u16
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

fn context_menu_key(key: gtk::gdk::Key, modifiers: gtk::gdk::ModifierType) -> bool {
    key == gtk::gdk::Key::Menu
        || (key == gtk::gdk::Key::F10 && modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK))
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

fn action_selection(model: &gtk::SelectionModel) -> (Vec<Arc<DirectoryEntry>>, bool) {
    let selected = model.selection();
    let Some((indices, first)) = gtk::BitsetIter::init_first(&selected) else {
        return (Vec::new(), false);
    };
    let mut entries = Vec::new();
    let mut overflowed = false;
    for position in std::iter::once(first).chain(indices) {
        if entries.len() == MILLER_ACTION_SELECTION_CAPACITY {
            overflowed = true;
            break;
        }
        if let Some(object) = model.item(position).and_downcast::<glib::BoxedAnyObject>() {
            entries.push(object.borrow::<Arc<DirectoryEntry>>().clone());
        }
    }
    (entries, overflowed)
}

fn drag_paths(model: &gtk::SelectionModel) -> Vec<PathBuf> {
    let (entries, overflowed) = action_selection(model);
    drag_paths_for_entries(entries, overflowed)
}

fn drag_paths_for_entries(entries: Vec<Arc<DirectoryEntry>>, overflowed: bool) -> Vec<PathBuf> {
    if overflowed {
        Vec::new()
    } else {
        entries
            .into_iter()
            .map(|entry| entry.path().to_path_buf())
            .collect()
    }
}

fn miller_child_hover_target(
    depth: usize,
    destination: &DropDestination,
) -> Option<DropHoverTarget> {
    match destination {
        DropDestination::Directory(path) => Some(DropHoverTarget::MillerChild {
            depth,
            path: path.clone(),
        }),
        DropDestination::Trash => None,
    }
}

fn popup_at_widget_point(menu: &gtk::PopoverMenu, widget: &gtk::Widget, x: f64, y: f64) {
    let Some(parent) = menu.parent() else {
        return;
    };
    let point = gtk::graphene::Point::new(x as f32, y as f32);
    let Some(point) = widget.compute_point(&parent, &point) else {
        return;
    };
    menu.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
        point.x().round() as i32,
        point.y().round() as i32,
        1,
        1,
    )));
    menu.popup();
}

fn picked_entry_widget(widget: &gtk::Widget, x: f64, y: f64) -> bool {
    let Some(target) = widget.pick(x, y, gtk::PickFlags::DEFAULT) else {
        return false;
    };
    let mut current = Some(target);
    while let Some(candidate) = current {
        if candidate.has_css_class("floe-entry-name") {
            return true;
        }
        if candidate == *widget {
            break;
        }
        current = candidate.parent();
    }
    false
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
    use std::{
        fs,
        os::unix::ffi::{OsStrExt, OsStringExt},
        path::PathBuf,
        sync::Arc,
    };

    use floe_core::{
        MILLER_COLUMN_CAPACITY, MillerChildKind, MillerColumnModel, enumerate_directory,
    };
    use tempfile::tempdir;

    use super::{
        MILLER_ACTION_SELECTION_CAPACITY, MILLER_SNAPSHOT_ENTRY_CAPACITY, MillerActionContext,
        MillerActionContextError, MillerItemCommand, MillerMotionPolicy, MillerNavigationCommand,
        MillerPresentationState, MillerRenderColumn, PREVIEW_ZOOM_MAX, PREVIEW_ZOOM_MIN,
        adjust_preview_zoom, context_menu_key, drag_paths_for_entries, horizontal_scroll_target,
        item_command_for_key, item_selection_target, logical_navigation_for_key,
        miller_child_hover_target, miller_column_accessible_description,
        navigation_modifiers_allowed, resolve_action_context_entries, trackpad_prefers_horizontal,
    };

    #[test]
    fn phase_9f_presentation_zoom_is_bounded_resettable_and_presentation_only() {
        assert_eq!(adjust_preview_zoom(100, 25), 125);
        assert_eq!(adjust_preview_zoom(PREVIEW_ZOOM_MAX, 25), PREVIEW_ZOOM_MAX);
        assert_eq!(adjust_preview_zoom(PREVIEW_ZOOM_MIN, -25), PREVIEW_ZOOM_MIN);
        assert_eq!(adjust_preview_zoom(100, -25), 75);
        assert_eq!(PREVIEW_ZOOM_MIN, 50);
        assert_eq!(PREVIEW_ZOOM_MAX, 400);
    }
    use crate::drag_drop::{DropDestination, DropHoverTarget};
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

    #[test]
    fn phase_8d_context_preserves_raw_identity_and_rejects_stale_ownership() {
        let root = tempdir().expect("temporary root");
        let raw_name = std::ffi::OsString::from_vec(b"raw-\xff".to_vec());
        let raw_path = root.path().join(&raw_name);
        fs::write(&raw_path, b"x").expect("raw fixture");
        let available = enumerate_directory(root.path())
            .expect("fixture listing")
            .into_entries()
            .into_iter()
            .map(Arc::new)
            .collect::<Vec<_>>();
        let selected = available
            .iter()
            .find(|entry| entry.path() == raw_path)
            .expect("raw entry")
            .clone();
        let context = MillerActionContext {
            depth: 3,
            directory: root.path().to_path_buf(),
            selected_entries: vec![selected],
            background: false,
            overflowed: false,
        };
        let resolved = resolve_action_context_entries(&context, &available).expect("valid owner");
        assert_eq!(
            resolved[0].path().as_os_str().as_bytes(),
            raw_path.as_os_str().as_bytes()
        );

        let stale_root = tempdir().expect("stale root");
        let stale = MillerActionContext {
            directory: stale_root.path().to_path_buf(),
            ..context.clone()
        };
        assert!(matches!(
            resolve_action_context_entries(&stale, &available),
            Err(MillerActionContextError::NotDirectChild)
        ));
        let overflowed = MillerActionContext {
            overflowed: true,
            ..context
        };
        assert!(matches!(
            resolve_action_context_entries(&overflowed, &available),
            Err(MillerActionContextError::Overflowed)
        ));
    }

    #[test]
    fn phase_8d_menu_has_pointer_keyboard_and_non_color_owner_contracts() {
        assert!(context_menu_key(
            gtk::gdk::Key::F10,
            gtk::gdk::ModifierType::SHIFT_MASK
        ));
        assert!(context_menu_key(
            gtk::gdk::Key::Menu,
            gtk::gdk::ModifierType::empty()
        ));
        assert!(!context_menu_key(
            gtk::gdk::Key::F10,
            gtk::gdk::ModifierType::empty()
        ));
        let active = MillerRenderColumn {
            depth: 2,
            directory: PathBuf::from("/projects/floe"),
            selected_child: None,
            entries: Vec::new(),
            total_entries: 0,
            is_active: true,
        };
        assert!(
            miller_column_accessible_description(&active).starts_with("Active Miller column 3:")
        );
    }

    #[test]
    fn phase_8d_parity_bounds_exact_selection_collection() {
        assert_eq!(
            MILLER_ACTION_SELECTION_CAPACITY,
            MILLER_SNAPSHOT_ENTRY_CAPACITY
        );
        let background = MillerActionContext {
            depth: 0,
            directory: PathBuf::from("/"),
            selected_entries: Vec::new(),
            background: true,
            overflowed: false,
        };
        assert!(resolve_action_context_entries(&background, &[]).is_ok());
    }

    #[test]
    fn phase_8d_integration_keeps_list_grid_and_miller_view_commands() {
        assert!(VIEW_ACTIONS.contains(&("view-list", ViewCommand::List)));
        assert!(VIEW_ACTIONS.contains(&("view-grid", ViewCommand::Grid)));
        assert!(VIEW_ACTIONS.contains(&("view-miller", ViewCommand::Miller)));
        assert!(navigation_modifiers_allowed(gtk::gdk::ModifierType::empty()));
    }

    #[test]
    fn phase_8e_miller_drags_exact_raw_selection_and_targets_exact_children() {
        let root = tempdir().expect("temporary root");
        let raw_name = std::ffi::OsString::from_vec(b"drag-\xff".to_vec());
        let raw_path = root.path().join(&raw_name);
        fs::write(&raw_path, b"x").expect("raw fixture");
        let entries = enumerate_directory(root.path())
            .expect("fixture listing")
            .into_entries()
            .into_iter()
            .map(Arc::new)
            .collect::<Vec<_>>();
        assert_eq!(
            drag_paths_for_entries(entries.clone(), false),
            vec![raw_path.clone()]
        );
        assert!(drag_paths_for_entries(entries, true).is_empty());

        let destination = DropDestination::Directory(raw_path.clone());
        assert_eq!(
            miller_child_hover_target(5, &destination),
            Some(DropHoverTarget::MillerChild {
                depth: 5,
                path: raw_path,
            })
        );
        assert_eq!(miller_child_hover_target(5, &DropDestination::Trash), None);
    }
}
