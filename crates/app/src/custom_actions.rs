//! Bounded, shell-free user configured external actions.

use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus},
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
    thread::{self, JoinHandle},
    time::Duration,
};

use adw::prelude::*;
use floe_core::{DirectoryEntry, EntryKind};
use gtk::glib;
use thiserror::Error;

pub const CUSTOM_ACTION_CAPACITY: usize = 32;
pub const CUSTOM_ACTION_ARGUMENT_CAPACITY: usize = 32;
pub const CUSTOM_ACTION_MIME_CAPACITY: usize = 16;
pub const CUSTOM_ACTION_SELECTION_CAPACITY: usize = 128;
pub const CUSTOM_ACTION_TEXT_CAPACITY: usize = 1_024;
const CUSTOM_ACTION_QUEUE_CAPACITY: usize = 4;
const CUSTOM_ACTION_RESULT_CAPACITY: usize = 16;
const CUSTOM_ACTION_CHILD_CAPACITY: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CustomActionTarget {
    Files,
    Folders,
    FilesAndFolders,
}

impl CustomActionTarget {
    pub const ALL: [Self; 3] = [Self::Files, Self::Folders, Self::FilesAndFolders];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Files => "Files only",
            Self::Folders => "Folders only",
            Self::FilesAndFolders => "Files and folders",
        }
    }

    const fn persisted(self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Folders => "folders",
            Self::FilesAndFolders => "both",
        }
    }

    fn from_persisted(value: &str) -> Option<Self> {
        match value {
            "files" => Some(Self::Files),
            "folders" => Some(Self::Folders),
            "both" => Some(Self::FilesAndFolders),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomActionDefinition {
    pub id: u64,
    pub name: String,
    pub executable: String,
    /// One argument per item. Placeholders must occupy the entire item.
    pub arguments: Vec<String>,
    pub target: CustomActionTarget,
    pub mime_patterns: Vec<String>,
    pub allow_multiple: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomActionSelection {
    pub path: PathBuf,
    pub kind: EntryKind,
    pub mime_type: Option<String>,
}

impl CustomActionSelection {
    pub fn from_entry(entry: &DirectoryEntry) -> Self {
        Self {
            path: entry.path().to_path_buf(),
            kind: entry.kind(),
            mime_type: entry.mime_type().map(str::to_owned),
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CustomActionValidationError {
    #[error("action ID must be non-zero")]
    MissingId,
    #[error("name must contain 1 to {CUSTOM_ACTION_TEXT_CAPACITY} characters")]
    InvalidName,
    #[error("executable must be a command name or absolute path")]
    InvalidExecutable,
    #[error("at most {CUSTOM_ACTION_ARGUMENT_CAPACITY} arguments are supported")]
    TooManyArguments,
    #[error("argument is empty or too long")]
    InvalidArgument,
    #[error("use at least one exact placeholder: %f, %F, %d, or %n")]
    MissingPlaceholder,
    #[error("at most {CUSTOM_ACTION_MIME_CAPACITY} MIME patterns are supported")]
    TooManyMimePatterns,
    #[error("MIME patterns must look like type/subtype or type/*")]
    InvalidMimePattern,
}

impl CustomActionDefinition {
    pub fn validate(&self) -> Result<(), CustomActionValidationError> {
        if self.id == 0 {
            return Err(CustomActionValidationError::MissingId);
        }
        if self.name.trim().is_empty() || self.name.chars().count() > CUSTOM_ACTION_TEXT_CAPACITY {
            return Err(CustomActionValidationError::InvalidName);
        }
        if !valid_executable(&self.executable) {
            return Err(CustomActionValidationError::InvalidExecutable);
        }
        if self.arguments.len() > CUSTOM_ACTION_ARGUMENT_CAPACITY {
            return Err(CustomActionValidationError::TooManyArguments);
        }
        if self.arguments.iter().any(|argument| {
            argument.is_empty()
                || argument.chars().count() > CUSTOM_ACTION_TEXT_CAPACITY
                || argument.as_bytes().contains(&0)
        }) {
            return Err(CustomActionValidationError::InvalidArgument);
        }
        if !self
            .arguments
            .iter()
            .any(|argument| matches!(argument.as_str(), "%f" | "%F" | "%d" | "%n"))
        {
            return Err(CustomActionValidationError::MissingPlaceholder);
        }
        if self.mime_patterns.len() > CUSTOM_ACTION_MIME_CAPACITY {
            return Err(CustomActionValidationError::TooManyMimePatterns);
        }
        if self
            .mime_patterns
            .iter()
            .any(|pattern| !valid_mime_pattern(pattern))
        {
            return Err(CustomActionValidationError::InvalidMimePattern);
        }
        Ok(())
    }

    pub fn eligible(&self, entries: &[CustomActionSelection]) -> bool {
        if self.validate().is_err()
            || entries.is_empty()
            || entries.len() > CUSTOM_ACTION_SELECTION_CAPACITY
            || (!self.allow_multiple && entries.len() != 1)
        {
            return false;
        }
        entries.iter().all(|entry| {
            let target_matches = match self.target {
                CustomActionTarget::Files => entry.kind != EntryKind::Directory,
                CustomActionTarget::Folders => entry.kind == EntryKind::Directory,
                CustomActionTarget::FilesAndFolders => true,
            };
            target_matches
                && (self.mime_patterns.is_empty()
                    || entry.kind == EntryKind::Directory
                    || entry.mime_type.as_deref().is_some_and(|mime| {
                        self.mime_patterns
                            .iter()
                            .any(|pattern| mime_matches(mime, pattern))
                    }))
        })
    }

    pub fn serialize_record(&self) -> Option<String> {
        self.validate().ok()?;
        Some(
            [
                self.id.to_string(),
                self.target.persisted().to_owned(),
                self.allow_multiple.to_string(),
                hex_encode(self.name.as_bytes()),
                hex_encode(self.executable.as_bytes()),
                self.arguments
                    .iter()
                    .map(|value| hex_encode(value.as_bytes()))
                    .collect::<Vec<_>>()
                    .join(","),
                self.mime_patterns
                    .iter()
                    .map(|value| hex_encode(value.as_bytes()))
                    .collect::<Vec<_>>()
                    .join(","),
            ]
            .join("\t"),
        )
    }

    pub fn parse_record(record: &str) -> Option<Self> {
        if record.len() > 128 * 1_024 {
            return None;
        }
        let mut fields = record.split('\t');
        let action = Self {
            id: fields.next()?.parse().ok()?,
            target: CustomActionTarget::from_persisted(fields.next()?)?,
            allow_multiple: fields.next()?.parse().ok()?,
            name: decode_utf8(fields.next()?)?,
            executable: decode_utf8(fields.next()?)?,
            arguments: decode_list(fields.next()?, CUSTOM_ACTION_ARGUMENT_CAPACITY)?,
            mime_patterns: decode_list(fields.next()?, CUSTOM_ACTION_MIME_CAPACITY)?,
        };
        if fields.next().is_some() || action.validate().is_err() {
            return None;
        }
        Some(action)
    }
}

fn valid_executable(value: &str) -> bool {
    if value.is_empty()
        || value.chars().count() > CUSTOM_ACTION_TEXT_CAPACITY
        || value.as_bytes().contains(&0)
    {
        return false;
    }
    let path = Path::new(value);
    path.is_absolute()
        || (path.components().count() == 1 && value != "." && value != ".." && !value.contains('/'))
}

fn valid_mime_pattern(value: &str) -> bool {
    let Some((kind, subtype)) = value.split_once('/') else {
        return false;
    };
    !kind.is_empty()
        && !subtype.is_empty()
        && !kind.contains('*')
        && (subtype == "*" || !subtype.contains('*'))
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '/' | '-' | '+' | '.' | '*')
        })
}

fn mime_matches(actual: &str, pattern: &str) -> bool {
    pattern
        .strip_suffix("/*")
        .map_or(actual == pattern, |prefix| {
            actual
                .strip_prefix(prefix)
                .is_some_and(|suffix| suffix.starts_with('/'))
        })
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CustomActionLaunchError {
    #[error(transparent)]
    InvalidDefinition(#[from] CustomActionValidationError),
    #[error("selection is not eligible for this action")]
    IneligibleSelection,
    #[error("selected path has no parent")]
    MissingParent,
    #[error("selected path has no file name")]
    MissingName,
    #[error("external action queue is full")]
    QueueFull,
    #[error("external action worker is unavailable")]
    Disconnected,
    #[error("too many external actions are still running")]
    TooManyChildren,
    #[error("could not start external action: {0}")]
    Spawn(String),
}

pub fn expand_argv(
    action: &CustomActionDefinition,
    entries: &[CustomActionSelection],
) -> Result<Vec<OsString>, CustomActionLaunchError> {
    action.validate()?;
    if !action.eligible(entries) {
        return Err(CustomActionLaunchError::IneligibleSelection);
    }
    let first = entries[0].path.as_path();
    let mut argv = Vec::with_capacity(action.arguments.len() + entries.len());
    for argument in &action.arguments {
        match argument.as_str() {
            "%f" => argv.push(first.as_os_str().to_os_string()),
            "%F" => argv.extend(
                entries
                    .iter()
                    .map(|entry| entry.path.as_os_str().to_os_string()),
            ),
            "%d" => argv.push(
                first
                    .parent()
                    .ok_or(CustomActionLaunchError::MissingParent)?
                    .as_os_str()
                    .to_os_string(),
            ),
            "%n" => argv.push(
                first
                    .file_name()
                    .ok_or(CustomActionLaunchError::MissingName)?
                    .to_os_string(),
            ),
            "%%" => argv.push(OsString::from("%")),
            literal => argv.push(OsString::from(literal)),
        }
    }
    Ok(argv)
}

#[derive(Clone, Debug)]
pub struct CustomActionLaunchRequest {
    pub action: CustomActionDefinition,
    pub entries: Vec<CustomActionSelection>,
}

#[derive(Debug)]
pub enum CustomActionEvent {
    Started {
        id: u64,
        name: String,
    },
    Finished {
        id: u64,
        status: ExitStatus,
    },
    Failed {
        id: u64,
        error: CustomActionLaunchError,
    },
}

pub struct CustomActionWorker {
    sender: Option<SyncSender<CustomActionLaunchRequest>>,
    events: Receiver<CustomActionEvent>,
    worker: Option<JoinHandle<()>>,
}

impl CustomActionWorker {
    pub fn spawn() -> io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel(CUSTOM_ACTION_QUEUE_CAPACITY);
        let (event_sender, events) = mpsc::sync_channel(CUSTOM_ACTION_RESULT_CAPACITY);
        let worker = thread::Builder::new()
            .name("floe-custom-actions".to_owned())
            .spawn(move || worker_loop(requests, event_sender))?;
        Ok(Self {
            sender: Some(sender),
            events,
            worker: Some(worker),
        })
    }

    pub fn try_launch(
        &self,
        request: CustomActionLaunchRequest,
    ) -> Result<(), CustomActionLaunchError> {
        let Some(sender) = self.sender.as_ref() else {
            return Err(CustomActionLaunchError::Disconnected);
        };
        sender.try_send(request).map_err(|error| match error {
            TrySendError::Full(_) => CustomActionLaunchError::QueueFull,
            TrySendError::Disconnected(_) => CustomActionLaunchError::Disconnected,
        })
    }

    pub fn try_event(&self) -> Option<CustomActionEvent> {
        self.events.try_recv().ok()
    }
}

impl Drop for CustomActionWorker {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn worker_loop(
    requests: Receiver<CustomActionLaunchRequest>,
    events: SyncSender<CustomActionEvent>,
) {
    let mut children: Vec<(u64, Child)> = Vec::with_capacity(CUSTOM_ACTION_CHILD_CAPACITY);
    loop {
        match requests.recv_timeout(Duration::from_millis(50)) {
            Ok(request) => launch_request(request, &mut children, &events),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        let mut index = 0;
        while index < children.len() {
            match children[index].1.try_wait() {
                Ok(Some(status)) => {
                    let (id, _) = children.swap_remove(index);
                    let _ = events.try_send(CustomActionEvent::Finished { id, status });
                }
                Ok(None) => index += 1,
                Err(error) => {
                    let (id, _) = children.swap_remove(index);
                    let _ = events.try_send(CustomActionEvent::Failed {
                        id,
                        error: CustomActionLaunchError::Spawn(error.to_string()),
                    });
                }
            }
        }
    }
    // External GUI tools outlive Floe by design. Dropping the handles on
    // shutdown does not terminate them; while Floe remains open, `try_wait`
    // reaps completed children without blocking the GTK thread.
}

fn launch_request(
    request: CustomActionLaunchRequest,
    children: &mut Vec<(u64, Child)>,
    events: &SyncSender<CustomActionEvent>,
) {
    let id = request.action.id;
    if children.len() == CUSTOM_ACTION_CHILD_CAPACITY {
        let _ = events.try_send(CustomActionEvent::Failed {
            id,
            error: CustomActionLaunchError::TooManyChildren,
        });
        return;
    }
    let argv = match expand_argv(&request.action, &request.entries) {
        Ok(argv) => argv,
        Err(error) => {
            let _ = events.try_send(CustomActionEvent::Failed { id, error });
            return;
        }
    };
    match Command::new(&request.action.executable).args(argv).spawn() {
        Ok(child) => {
            let _ = events.try_send(CustomActionEvent::Started {
                id,
                name: request.action.name,
            });
            children.push((id, child));
        }
        Err(error) => {
            let _ = events.try_send(CustomActionEvent::Failed {
                id,
                error: CustomActionLaunchError::Spawn(error.to_string()),
            });
        }
    }
}

fn decode_list(value: &str, capacity: usize) -> Option<Vec<String>> {
    if value.is_empty() {
        return Some(Vec::new());
    }
    let values = value
        .split(',')
        .map(decode_utf8)
        .collect::<Option<Vec<_>>>()?;
    (values.len() <= capacity).then_some(values)
}

fn decode_utf8(value: &str) -> Option<String> {
    String::from_utf8(hex_decode(value)?).ok()
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Some((hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?))
        .collect()
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub struct CustomActionEditorWidgets {
    pub dialog: adw::Dialog,
    pub add_button: gtk::Button,
    pub edit_buttons: Vec<gtk::Button>,
    pub remove_buttons: Vec<gtk::Button>,
    pub move_up_buttons: Vec<gtk::Button>,
    pub move_down_buttons: Vec<gtk::Button>,
}

pub fn build_editor(actions: &[CustomActionDefinition]) -> CustomActionEditorWidgets {
    let dialog = adw::Dialog::builder()
        .title("Applications & Custom Actions")
        .content_width(700)
        .content_height(620)
        .build();
    dialog.update_property(&[
        gtk::accessible::Property::Label("Applications and custom actions"),
        gtk::accessible::Property::Description(
            "Manage explicit shell-free external tools shown for eligible selections",
        ),
    ]);
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(20)
        .margin_bottom(20)
        .margin_start(20)
        .margin_end(20)
        .build();
    content.append(
        &gtk::Label::builder()
            .label("Custom actions run an executable directly. Floe never sends these fields to a shell or expands $variables, quotes, pipes, redirects, or command substitutions.")
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build(),
    );
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    list.update_property(&[gtk::accessible::Property::Label(
        "Configured custom actions",
    )]);
    let mut edit_buttons = Vec::new();
    let mut remove_buttons = Vec::new();
    let mut move_up_buttons = Vec::new();
    let mut move_down_buttons = Vec::new();
    for (index, action) in actions.iter().enumerate() {
        let row = adw::ActionRow::builder()
            .title(glib::markup_escape_text(&action.name))
            .subtitle(format!(
                "{} • {} • {}",
                action.executable,
                action.target.label(),
                if action.allow_multiple {
                    "multiple files"
                } else {
                    "one item"
                }
            ))
            .build();
        let up = gtk::Button::builder()
            .icon_name("floe-phosphor-caret-up-symbolic")
            .tooltip_text("Move action up")
            .sensitive(index > 0)
            .build();
        let down = gtk::Button::builder()
            .icon_name("floe-phosphor-caret-down-symbolic")
            .tooltip_text("Move action down")
            .sensitive(index + 1 < actions.len())
            .build();
        let edit = gtk::Button::with_label("Edit");
        let remove = gtk::Button::with_label("Remove");
        for button in [&up, &down, &edit, &remove] {
            button.add_css_class("flat");
            row.add_suffix(button);
        }
        list.append(&row);
        move_up_buttons.push(up);
        move_down_buttons.push(down);
        edit_buttons.push(edit);
        remove_buttons.push(remove);
    }
    if actions.is_empty() {
        list.append(
            &adw::ActionRow::builder()
                .title("No custom actions")
                .subtitle("Add a reviewed local executable and explicit arguments.")
                .build(),
        );
    }
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&list)
        .build();
    content.append(&scroll);
    let add_button = gtk::Button::with_label("Add Custom Action…");
    add_button.add_css_class("suggested-action");
    add_button.set_halign(gtk::Align::End);
    content.append(&add_button);
    dialog.set_child(Some(&content));
    CustomActionEditorWidgets {
        dialog,
        add_button,
        edit_buttons,
        remove_buttons,
        move_up_buttons,
        move_down_buttons,
    }
}

pub struct CustomActionFormWidgets {
    pub dialog: adw::Dialog,
    pub name: gtk::Entry,
    pub executable: gtk::Entry,
    pub arguments: gtk::TextView,
    pub target: gtk::DropDown,
    pub mime_patterns: gtk::Entry,
    pub allow_multiple: gtk::Switch,
    pub error: gtk::Label,
    pub save: gtk::Button,
}

pub fn build_form(existing: Option<&CustomActionDefinition>) -> CustomActionFormWidgets {
    let dialog = adw::Dialog::builder()
        .title(if existing.is_some() {
            "Edit Custom Action"
        } else {
            "Add Custom Action"
        })
        .content_width(640)
        .content_height(650)
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(20)
        .margin_bottom(20)
        .margin_start(20)
        .margin_end(20)
        .build();
    let name = gtk::Entry::builder()
        .placeholder_text("Action name")
        .build();
    let executable = gtk::Entry::builder()
        .placeholder_text("Executable name or absolute path")
        .build();
    let arguments = gtk::TextView::builder()
        .monospace(true)
        .wrap_mode(gtk::WrapMode::None)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(8)
        .right_margin(8)
        .build();
    arguments.set_tooltip_text(Some(
        "One argument per line. Exact placeholders: %f first path, %F all paths, %d parent, %n file name, %% percent.",
    ));
    let arguments_scroll = gtk::ScrolledWindow::builder()
        .min_content_height(150)
        .child(&arguments)
        .build();
    let target =
        gtk::DropDown::from_strings(&CustomActionTarget::ALL.map(CustomActionTarget::label));
    let mime_patterns = gtk::Entry::builder()
        .placeholder_text("Optional MIME types, comma separated: image/*, application/pdf")
        .build();
    let allow_multiple = gtk::Switch::new();
    let multiple_row = adw::ActionRow::builder()
        .title("Allow multiple selection")
        .subtitle("%F expands to each exact selected path as a separate argument")
        .build();
    multiple_row.add_suffix(&allow_multiple);
    multiple_row.set_activatable_widget(Some(&allow_multiple));
    if let Some(action) = existing {
        name.set_text(&action.name);
        executable.set_text(&action.executable);
        arguments.buffer().set_text(&action.arguments.join("\n"));
        target.set_selected(
            CustomActionTarget::ALL
                .iter()
                .position(|candidate| *candidate == action.target)
                .unwrap_or(0) as u32,
        );
        mime_patterns.set_text(&action.mime_patterns.join(", "));
        allow_multiple.set_active(action.allow_multiple);
    }
    for (label, widget) in [
        ("Name", name.clone().upcast::<gtk::Widget>()),
        ("Executable", executable.clone().upcast()),
        ("Arguments, one per line", arguments_scroll.upcast()),
        ("Eligible items", target.clone().upcast()),
        ("MIME filters", mime_patterns.clone().upcast()),
    ] {
        content.append(&gtk::Label::builder().label(label).xalign(0.0).build());
        content.append(&widget);
    }
    content.append(&multiple_row);
    let error = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .visible(false)
        .css_classes(["error"])
        .build();
    error.set_accessible_role(gtk::AccessibleRole::Alert);
    content.append(&error);
    let save = gtk::Button::with_label("Save Action");
    save.add_css_class("suggested-action");
    save.set_halign(gtk::Align::End);
    content.append(&save);
    dialog.set_child(Some(&content));
    CustomActionFormWidgets {
        dialog,
        name,
        executable,
        arguments,
        target,
        mime_patterns,
        allow_multiple,
        error,
        save,
    }
}

pub fn definition_from_form(
    id: u64,
    widgets: &CustomActionFormWidgets,
) -> Result<CustomActionDefinition, CustomActionValidationError> {
    let buffer = widgets.arguments.buffer();
    let arguments = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    let mime_patterns = widgets
        .mime_patterns
        .text()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect();
    let action = CustomActionDefinition {
        id,
        name: widgets.name.text().trim().to_owned(),
        executable: widgets.executable.text().trim().to_owned(),
        arguments,
        target: CustomActionTarget::ALL
            .get(widgets.target.selected() as usize)
            .copied()
            .unwrap_or(CustomActionTarget::Files),
        mime_patterns,
        allow_multiple: widgets.allow_multiple.is_active(),
    };
    action.validate()?;
    Ok(action)
}

#[cfg(all(test, unix))]
mod tests {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    use super::*;

    fn action() -> CustomActionDefinition {
        CustomActionDefinition {
            id: 7,
            name: "Inspect safely".to_owned(),
            executable: "/usr/bin/printf".to_owned(),
            arguments: vec!["--".to_owned(), "%F".to_owned()],
            target: CustomActionTarget::Files,
            mime_patterns: vec!["video/*".to_owned()],
            allow_multiple: true,
        }
    }

    fn entry(path: PathBuf, mime: &str) -> CustomActionSelection {
        CustomActionSelection {
            path,
            kind: EntryKind::RegularFile,
            mime_type: Some(mime.to_owned()),
        }
    }

    #[test]
    fn phase_19b_custom_action_store_round_trips_and_rejects_malformed() {
        let action = action();
        let record = action.serialize_record().expect("valid action");
        assert_eq!(CustomActionDefinition::parse_record(&record), Some(action));
        assert!(CustomActionDefinition::parse_record("7\tfiles\ttrue\tnot-hex").is_none());
        assert!(CustomActionDefinition::parse_record(&"x".repeat(128 * 1_024 + 1)).is_none());
    }

    #[test]
    fn phase_19b_custom_action_launch_preserves_raw_paths_without_shell_parsing() {
        let raw = PathBuf::from("/tmp")
            .join(OsString::from_vec(vec![b'v', 0x80, b'.', b'm', b'p', b'4']));
        let entries = vec![entry(raw.clone(), "video/mp4")];
        let argv = expand_argv(&action(), &entries).expect("eligible exact path");
        assert_eq!(argv, vec![OsString::from("--"), raw.into_os_string()]);
    }

    #[test]
    fn phase_19b_custom_action_launch_enforces_mime_target_multiple_and_placeholders() {
        let file = entry(PathBuf::from("/tmp/file.txt"), "text/plain");
        assert!(!action().eligible(&[file]));
        let mut invalid = action();
        invalid.arguments = vec!["$(touch /tmp/no)".to_owned()];
        assert_eq!(
            invalid.validate(),
            Err(CustomActionValidationError::MissingPlaceholder)
        );
    }

    #[test]
    fn phase_19b_custom_action_launch_worker_is_bounded_and_reports_lifecycle() {
        let mut action = action();
        action.executable = "/usr/bin/true".to_owned();
        action.mime_patterns.clear();
        let entries = vec![entry(
            PathBuf::from("/tmp/file.bin"),
            "application/octet-stream",
        )];
        let worker = CustomActionWorker::spawn().expect("worker");
        worker
            .try_launch(CustomActionLaunchRequest { action, entries })
            .expect("bounded launch submission");
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut started = false;
        let mut finished = false;
        while std::time::Instant::now() < deadline && !finished {
            if let Some(event) = worker.try_event() {
                match event {
                    CustomActionEvent::Started { id: 7, .. } => started = true,
                    CustomActionEvent::Finished { id: 7, status } => {
                        assert!(status.success());
                        finished = true;
                    }
                    CustomActionEvent::Failed { error, .. } => panic!("launch failed: {error}"),
                    _ => {}
                }
            } else {
                thread::yield_now();
            }
        }
        assert!(started && finished);
    }

    #[test]
    fn phase_19b_custom_action_ui_labels_are_plain_and_complete() {
        assert_eq!(
            CustomActionTarget::ALL.map(CustomActionTarget::label),
            ["Files only", "Folders only", "Files and folders"]
        );
        assert!(
            CustomActionValidationError::MissingPlaceholder
                .to_string()
                .contains("%f")
        );
    }

    #[test]
    #[ignore = "requires graphical GTK session; run documented GTK component gate"]
    fn phase_testing_gtk_phase_19b_custom_action_ui_accessibility() {
        gtk::init().expect("GTK component gate requires an available display");
        adw::init().expect("libadwaita must initialize in GTK component gate");
        let action = action();
        let editor = build_editor(std::slice::from_ref(&action));
        assert_eq!(
            editor.dialog.title().as_str(),
            "Applications & Custom Actions"
        );
        assert_eq!(editor.edit_buttons.len(), 1);
        assert_eq!(
            editor.remove_buttons[0].accessible_role(),
            gtk::AccessibleRole::Button
        );
        let form = build_form(Some(&action));
        assert_eq!(form.name.accessible_role(), gtk::AccessibleRole::TextBox);
        assert_eq!(
            form.arguments.accessible_role(),
            gtk::AccessibleRole::TextBox
        );
        assert_eq!(form.target.accessible_role(), gtk::AccessibleRole::ComboBox);
        assert_eq!(form.error.accessible_role(), gtk::AccessibleRole::Alert);
    }
}
