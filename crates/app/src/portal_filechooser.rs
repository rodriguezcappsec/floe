//! Optional freedesktop FileChooser portal backend.
//!
//! The backend is activated only by an explicit service invocation. Portal
//! request parsing, lifecycle, and subprocess supervision remain separate from
//! Selection Mode widgets.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::{OsStr, OsString},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Component, Path, PathBuf},
    rc::Rc,
};

use gtk::{
    gio,
    gio::prelude::*,
    glib::{self, variant::ToVariant},
};
use thiserror::Error;

use crate::selection_mode::{
    SELECTION_PATH_CAPACITY, SelectionChoice, SelectionChooserOptions, SelectionFilter,
    SelectionFilterRule, SelectionOptionResult, decode_option_result, encode_chooser_options,
};

pub const PORTAL_BACKEND_FLAG: &str = "--portal-filechooser-backend";
pub const PORTAL_BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.floe";
pub const PORTAL_OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_INTERFACE: &str = "org.freedesktop.impl.portal.FileChooser";
const REQUEST_INTERFACE: &str = "org.freedesktop.impl.portal.Request";
const REQUEST_CAPACITY: usize = 16;
const TEXT_CAPACITY: usize = 200;
const APP_ID_CAPACITY: usize = 255;
const PARENT_CAPACITY: usize = 512;
const CHILD_STDOUT_CAPACITY: usize = 2 * 1024 * 1024;

const PORTAL_XML: &str = r#"
<node>
  <interface name="org.freedesktop.impl.portal.FileChooser">
    <method name="OpenFile">
      <arg type="o" direction="in"/><arg type="s" direction="in"/>
      <arg type="s" direction="in"/><arg type="s" direction="in"/>
      <arg type="a{sv}" direction="in"/>
      <arg type="u" direction="out"/><arg type="a{sv}" direction="out"/>
    </method>
    <method name="SaveFile">
      <arg type="o" direction="in"/><arg type="s" direction="in"/>
      <arg type="s" direction="in"/><arg type="s" direction="in"/>
      <arg type="a{sv}" direction="in"/>
      <arg type="u" direction="out"/><arg type="a{sv}" direction="out"/>
    </method>
    <method name="SaveFiles">
      <arg type="o" direction="in"/><arg type="s" direction="in"/>
      <arg type="s" direction="in"/><arg type="s" direction="in"/>
      <arg type="a{sv}" direction="in"/>
      <arg type="u" direction="out"/><arg type="a{sv}" direction="out"/>
    </method>
  </interface>
</node>
"#;

const REQUEST_XML: &str = r#"
<node>
  <interface name="org.freedesktop.impl.portal.Request">
    <method name="Close"/>
  </interface>
</node>
"#;

#[derive(Clone, Debug, Eq, PartialEq)]
enum PortalRequestKind {
    OpenFile { multiple: bool },
    SelectFolder,
    SaveFile,
    SaveFiles { names: Vec<OsString> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PortalRequest {
    handle: String,
    app_id: String,
    parent_window: ParentWindow,
    title: String,
    accept_label: Option<String>,
    modal: bool,
    current_folder: Option<PathBuf>,
    current_name: Option<String>,
    kind: PortalRequestKind,
    chooser_options: SelectionChooserOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ParentWindow {
    None,
    Wayland(String),
    X11(u64),
}

#[derive(Debug, Error, Eq, PartialEq)]
enum PortalRequestError {
    #[error("invalid portal method parameters")]
    Parameters,
    #[error("invalid request handle")]
    Handle,
    #[error("invalid application id")]
    AppId,
    #[error("invalid parent window identifier")]
    Parent,
    #[error("invalid chooser {0}")]
    Text(&'static str),
    #[error("portal option {0} has the wrong type")]
    OptionType(&'static str),
    #[error("portal option {0} is unsupported by this backend version")]
    Unsupported(&'static str),
    #[error("portal local path option {0} is malformed")]
    LocalPath(&'static str),
    #[error("portal filename option {0} is malformed")]
    Filename(&'static str),
    #[error("too many requested files")]
    Capacity,
}

impl PortalRequest {
    fn parse(method: &str, parameters: &glib::Variant) -> Result<Self, PortalRequestError> {
        if parameters.n_children() != 5 {
            return Err(PortalRequestError::Parameters);
        }
        let handle = child_text(parameters, 0).ok_or(PortalRequestError::Parameters)?;
        if !is_request_handle(&handle) {
            return Err(PortalRequestError::Handle);
        }
        let app_id = child_text(parameters, 1).ok_or(PortalRequestError::Parameters)?;
        if app_id.len() > APP_ID_CAPACITY
            || app_id.chars().any(|value| value.is_control())
            || (!app_id.is_empty()
                && !app_id.bytes().all(|value| {
                    value.is_ascii_alphanumeric() || matches!(value, b'.' | b'_' | b'-')
                }))
        {
            return Err(PortalRequestError::AppId);
        }
        let parent_window =
            ParentWindow::parse(&child_text(parameters, 2).ok_or(PortalRequestError::Parameters)?)?;
        if matches!(parent_window, ParentWindow::X11(_)) {
            return Err(PortalRequestError::Unsupported("x11 parent window"));
        }
        let title = sanitize_text(
            &child_text(parameters, 3).ok_or(PortalRequestError::Parameters)?,
            "title",
        )?;
        let options = parameters.child_value(4);
        if options.type_() != glib::VariantTy::VARDICT {
            return Err(PortalRequestError::Parameters);
        }
        let options = glib::VariantDict::new(Some(&options));
        let mut filters = lookup_filters(&options)?;
        let current_filter_value = lookup_current_filter(&options)?;
        let current_filter = if let Some(current) = current_filter_value {
            if filters.is_empty() {
                filters.push(current);
                Some(0)
            } else {
                Some(
                    filters
                        .iter()
                        .position(|filter| filter == &current)
                        .ok_or(PortalRequestError::Unsupported("current_filter"))?,
                )
            }
        } else {
            (!filters.is_empty()).then_some(0)
        };
        let choices = lookup_choices(&options)?;
        let chooser_options = SelectionChooserOptions {
            filters,
            current_filter,
            choices,
        };
        let accept_label = lookup::<String>(&options, "accept_label")?
            .map(|value| sanitize_text(&value.replace('_', ""), "accept label"))
            .transpose()?;
        let modal = lookup::<bool>(&options, "modal")?.unwrap_or(true);
        let current_folder = lookup_local_path(&options, "current_folder")?;
        let current_name = lookup::<String>(&options, "current_name")?
            .map(|value| validate_filename(value, "current_name"))
            .transpose()?;
        let kind = match method {
            "OpenFile" => {
                let multiple = lookup::<bool>(&options, "multiple")?.unwrap_or(false);
                let directory = lookup::<bool>(&options, "directory")?.unwrap_or(false);
                if directory {
                    if multiple {
                        return Err(PortalRequestError::Unsupported(
                            "multiple directory selection",
                        ));
                    }
                    PortalRequestKind::SelectFolder
                } else {
                    PortalRequestKind::OpenFile { multiple }
                }
            }
            "SaveFile" => {
                let current_file = lookup_local_path(&options, "current_file")?;
                let (current_folder, current_name) = if let Some(path) = current_file {
                    let name = path
                        .file_name()
                        .ok_or(PortalRequestError::LocalPath("current_file"))?
                        .to_str()
                        .ok_or(PortalRequestError::Filename("current_file"))?
                        .to_owned();
                    (
                        path.parent().map(Path::to_path_buf),
                        Some(validate_filename(name, "current_file")?),
                    )
                } else {
                    (current_folder, current_name)
                };
                return Ok(Self {
                    handle,
                    app_id,
                    parent_window,
                    title,
                    accept_label,
                    modal,
                    current_folder,
                    current_name,
                    kind: PortalRequestKind::SaveFile,
                    chooser_options,
                });
            }
            "SaveFiles" => {
                let names = lookup_filename_array(&options, "files")?
                    .ok_or(PortalRequestError::Filename("files"))?;
                if names.is_empty() || names.len() > SELECTION_PATH_CAPACITY {
                    return Err(PortalRequestError::Capacity);
                }
                PortalRequestKind::SaveFiles { names }
            }
            _ => return Err(PortalRequestError::Parameters),
        };
        Ok(Self {
            handle,
            app_id,
            parent_window,
            title,
            accept_label,
            modal,
            current_folder,
            current_name,
            kind,
            chooser_options,
        })
    }

    fn chooser_argv(&self, executable: &OsStr) -> Vec<OsString> {
        let mut arguments = vec![executable.to_os_string()];
        match &self.kind {
            PortalRequestKind::OpenFile { multiple } => {
                arguments.push("--choose-open".into());
                if *multiple {
                    arguments.push("--multiple".into());
                }
            }
            PortalRequestKind::SelectFolder | PortalRequestKind::SaveFiles { .. } => {
                arguments.push("--choose-folder".into());
            }
            PortalRequestKind::SaveFile => arguments.push("--choose-save".into()),
        }
        if let Some(path) = self.current_folder.as_ref() {
            arguments.push("--initial-directory".into());
            arguments.push(path.as_os_str().to_os_string());
        }
        if let Some(name) = self.current_name.as_ref() {
            arguments.push("--suggested-name".into());
            arguments.push(name.into());
        }
        arguments.push("--chooser-title".into());
        arguments.push(self.title.as_str().into());
        if let Some(label) = self.accept_label.as_ref() {
            arguments.push("--chooser-accept-label".into());
            arguments.push(label.as_str().into());
        }
        if let ParentWindow::Wayland(handle) = &self.parent_window {
            arguments.push("--chooser-parent".into());
            arguments.push(format!("wayland:{handle}").into());
        }
        if self.modal {
            arguments.push("--chooser-modal".into());
        }
        if !self.chooser_options.filters.is_empty() || !self.chooser_options.choices.is_empty() {
            if let Some(encoded) = encode_chooser_options(&self.chooser_options) {
                arguments.push("--chooser-options-v1".into());
                arguments.push(encoded.into());
            }
        }
        arguments
    }

    fn results_from_stdout(&self, stdout: &str) -> Result<PortalResults, PortalRequestError> {
        if stdout.len() > CHILD_STDOUT_CAPACITY {
            return Err(PortalRequestError::Capacity);
        }
        let mut lines = stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty());
        let options = match lines
            .clone()
            .next()
            .and_then(|line| line.strip_prefix("floe-chooser-options-v1:"))
        {
            Some(encoded) => {
                Some(decode_option_result(encoded).ok_or(PortalRequestError::Parameters)?)
            }
            None => None,
        };
        if options.is_some() {
            let _ = lines.next();
        }
        let uris = lines
            .map(str::trim)
            .map(normalize_local_uri)
            .collect::<Result<Vec<_>, _>>()?;
        let uris = match &self.kind {
            PortalRequestKind::OpenFile { multiple: false }
            | PortalRequestKind::SelectFolder
            | PortalRequestKind::SaveFile
                if uris.len() != 1 =>
            {
                return Err(PortalRequestError::Capacity);
            }
            PortalRequestKind::OpenFile { multiple: true }
                if uris.is_empty() || uris.len() > SELECTION_PATH_CAPACITY =>
            {
                return Err(PortalRequestError::Capacity);
            }
            PortalRequestKind::SaveFiles { names } if uris.len() == 1 => {
                let folder = gio::File::for_uri(&uris[0])
                    .path()
                    .ok_or(PortalRequestError::LocalPath("result"))?;
                names
                    .iter()
                    .map(|name| gio::File::for_path(folder.join(name)).uri().to_string())
                    .collect::<Vec<_>>()
            }
            PortalRequestKind::SaveFiles { .. } => return Err(PortalRequestError::Capacity),
            _ => uris,
        };
        let options = options.unwrap_or_default();
        if options
            .current_filter
            .is_some_and(|index| index >= self.chooser_options.filters.len())
            || options.choices.len() != self.chooser_options.choices.len()
            || options
                .choices
                .iter()
                .zip(&self.chooser_options.choices)
                .any(|((id, value), expected)| {
                    id != &expected.id
                        || if expected.options.is_empty() {
                            !matches!(value.as_str(), "true" | "false")
                        } else {
                            !expected
                                .options
                                .iter()
                                .any(|(candidate, _)| candidate == value)
                        }
                })
        {
            return Err(PortalRequestError::Parameters);
        }
        Ok(PortalResults { uris, options })
    }
}

#[derive(Debug, Eq, PartialEq)]
struct PortalResults {
    uris: Vec<String>,
    options: SelectionOptionResult,
}

impl ParentWindow {
    fn parse(value: &str) -> Result<Self, PortalRequestError> {
        if value.is_empty() {
            return Ok(Self::None);
        }
        if value.len() > PARENT_CAPACITY || value.chars().any(char::is_control) {
            return Err(PortalRequestError::Parent);
        }
        if let Some(handle) = value.strip_prefix("wayland:") {
            if !handle.is_empty() && handle.bytes().all(|byte| byte.is_ascii_graphic()) {
                return Ok(Self::Wayland(handle.to_owned()));
            }
        }
        if let Some(handle) = value.strip_prefix("x11:") {
            if !handle.is_empty() && handle.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                if let Ok(window) = u64::from_str_radix(handle, 16) {
                    return Ok(Self::X11(window));
                }
            }
        }
        Err(PortalRequestError::Parent)
    }
}

fn child_text(parameters: &glib::Variant, index: usize) -> Option<String> {
    parameters.child_value(index).str().map(ToOwned::to_owned)
}

fn sanitize_text(value: &str, field: &'static str) -> Result<String, PortalRequestError> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > TEXT_CAPACITY
        || value.chars().any(char::is_control)
    {
        return Err(PortalRequestError::Text(field));
    }
    Ok(value.to_owned())
}

fn lookup<T: glib::variant::FromVariant>(
    options: &glib::VariantDict,
    key: &'static str,
) -> Result<Option<T>, PortalRequestError> {
    options
        .lookup(key)
        .map_err(|_| PortalRequestError::OptionType(key))
}

fn lookup_local_path(
    options: &glib::VariantDict,
    key: &'static str,
) -> Result<Option<PathBuf>, PortalRequestError> {
    let Some(value) = options.lookup_value(key, None) else {
        return Ok(None);
    };
    let bytes = value
        .fixed_array::<u8>()
        .map_err(|_| PortalRequestError::OptionType(key))?;
    let raw = nul_terminated_bytes(bytes, key)?;
    let path = PathBuf::from(OsString::from_vec(raw.to_vec()));
    if !is_normalized_absolute(&path) {
        return Err(PortalRequestError::LocalPath(key));
    }
    Ok(Some(path))
}

fn lookup_filename_array(
    options: &glib::VariantDict,
    key: &'static str,
) -> Result<Option<Vec<OsString>>, PortalRequestError> {
    let Some(value) = options.lookup_value(key, None) else {
        return Ok(None);
    };
    if value.type_().as_str() != "aay" {
        return Err(PortalRequestError::OptionType(key));
    }
    if value.n_children() > SELECTION_PATH_CAPACITY {
        return Err(PortalRequestError::Capacity);
    }
    let mut names = Vec::with_capacity(value.n_children());
    for index in 0..value.n_children() {
        let child = value.child_value(index);
        let bytes = child
            .fixed_array::<u8>()
            .map_err(|_| PortalRequestError::OptionType(key))?;
        let raw = nul_terminated_bytes(bytes, key)?;
        let name = OsString::from_vec(raw.to_vec());
        if !valid_raw_filename(&name) {
            return Err(PortalRequestError::Filename(key));
        }
        names.push(name);
    }
    Ok(Some(names))
}

fn nul_terminated_bytes<'a>(
    bytes: &'a [u8],
    key: &'static str,
) -> Result<&'a [u8], PortalRequestError> {
    let Some((&0, raw)) = bytes.split_last() else {
        return Err(PortalRequestError::LocalPath(key));
    };
    if raw.is_empty() || raw.contains(&0) {
        return Err(PortalRequestError::LocalPath(key));
    }
    Ok(raw)
}

fn valid_raw_filename(name: &OsStr) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes != b"."
        && bytes != b".."
        && !bytes.contains(&b'/')
        && !bytes.contains(&0)
}

fn validate_filename(value: String, key: &'static str) -> Result<String, PortalRequestError> {
    if valid_raw_filename(OsStr::new(&value)) && !value.chars().any(char::is_control) {
        Ok(value)
    } else {
        Err(PortalRequestError::Filename(key))
    }
}

fn is_normalized_absolute(path: &Path) -> bool {
    path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::RootDir | Component::Normal(_)))
}

fn is_request_handle(value: &str) -> bool {
    value.starts_with('/')
        && value.len() <= 1024
        && value.split('/').skip(1).all(|part| {
            !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
}

fn lookup_filters(options: &glib::VariantDict) -> Result<Vec<SelectionFilter>, PortalRequestError> {
    let Some(value) = options.lookup_value("filters", None) else {
        return Ok(Vec::new());
    };
    if value.type_().as_str() != "a(sa(us))" {
        return Err(PortalRequestError::OptionType("filters"));
    }
    let raw = value
        .get::<Vec<(String, Vec<(u32, String)>)>>()
        .ok_or(PortalRequestError::OptionType("filters"))?;
    parse_filters(raw, "filters")
}

fn lookup_current_filter(
    options: &glib::VariantDict,
) -> Result<Option<SelectionFilter>, PortalRequestError> {
    let Some(value) = options.lookup_value("current_filter", None) else {
        return Ok(None);
    };
    if value.type_().as_str() != "(sa(us))" {
        return Err(PortalRequestError::OptionType("current_filter"));
    }
    let raw = value
        .get::<(String, Vec<(u32, String)>)>()
        .ok_or(PortalRequestError::OptionType("current_filter"))?;
    Ok(parse_filters(vec![raw], "current_filter")?.pop())
}

fn parse_filters(
    raw: Vec<(String, Vec<(u32, String)>)>,
    field: &'static str,
) -> Result<Vec<SelectionFilter>, PortalRequestError> {
    if raw.len() > 32 {
        return Err(PortalRequestError::Capacity);
    }
    raw.into_iter()
        .map(|(label, rules)| {
            let label = sanitize_text(&label, field)?;
            if rules.is_empty() || rules.len() > 32 {
                return Err(PortalRequestError::Capacity);
            }
            let rules = rules
                .into_iter()
                .map(|(kind, value)| {
                    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
                    {
                        return Err(PortalRequestError::Text(field));
                    }
                    match kind {
                        0 => Ok(SelectionFilterRule::Glob(value)),
                        1 => Ok(SelectionFilterRule::Mime(value)),
                        _ => Err(PortalRequestError::Unsupported(field)),
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(SelectionFilter { label, rules })
        })
        .collect()
}

fn lookup_choices(options: &glib::VariantDict) -> Result<Vec<SelectionChoice>, PortalRequestError> {
    let Some(value) = options.lookup_value("choices", None) else {
        return Ok(Vec::new());
    };
    if value.type_().as_str() != "a(ssa(ss)s)" {
        return Err(PortalRequestError::OptionType("choices"));
    }
    let raw = value
        .get::<Vec<(String, String, Vec<(String, String)>, String)>>()
        .ok_or(PortalRequestError::OptionType("choices"))?;
    if raw.len() > 32 {
        return Err(PortalRequestError::Capacity);
    }
    let mut ids = std::collections::HashSet::new();
    raw.into_iter()
        .map(|(id, label, choices, initial)| {
            let id = sanitize_text(&id, "choices")?;
            if !ids.insert(id.clone()) || choices.len() > 64 {
                return Err(PortalRequestError::Unsupported("choices"));
            }
            let label = sanitize_text(&label, "choices")?;
            let mut option_ids = std::collections::HashSet::new();
            let choices = choices
                .into_iter()
                .map(|(id, label)| {
                    let id = sanitize_text(&id, "choices")?;
                    if !option_ids.insert(id.clone()) {
                        return Err(PortalRequestError::Unsupported("choices"));
                    }
                    Ok((id, sanitize_text(&label, "choices")?))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let valid_initial = if choices.is_empty() {
                matches!(initial.as_str(), "" | "true" | "false")
            } else {
                initial.is_empty() || choices.iter().any(|(id, _)| id == &initial)
            };
            if !valid_initial {
                return Err(PortalRequestError::Unsupported("choices"));
            }
            Ok(SelectionChoice {
                id,
                label,
                options: choices,
                initial,
            })
        })
        .collect()
}

fn normalize_local_uri(value: &str) -> Result<String, PortalRequestError> {
    if !value.starts_with("file://") {
        return Err(PortalRequestError::LocalPath("result"));
    }
    let file = gio::File::for_uri(value);
    let path = file.path().ok_or(PortalRequestError::LocalPath("result"))?;
    if !is_normalized_absolute(&path) {
        return Err(PortalRequestError::LocalPath("result"));
    }
    Ok(gio::File::for_path(path).uri().to_string())
}

struct PendingRequest {
    invocation: gio::DBusMethodInvocation,
    process: gio::Subprocess,
    cancellable: gio::Cancellable,
    registration: gio::RegistrationId,
    request: PortalRequest,
    cancelled: bool,
}

#[derive(Default)]
struct PortalService {
    connection: RefCell<Option<gio::DBusConnection>>,
    root_registration: RefCell<Option<gio::RegistrationId>>,
    requests: RefCell<HashMap<String, PendingRequest>>,
}

impl PortalService {
    fn register(self: &Rc<Self>, connection: &gio::DBusConnection) -> Result<(), glib::Error> {
        let node = gio::DBusNodeInfo::for_xml(PORTAL_XML)?;
        let interface = node
            .lookup_interface(PORTAL_INTERFACE)
            .expect("portal interface is embedded");
        let service = Rc::downgrade(self);
        let registration = connection
            .register_object(PORTAL_OBJECT_PATH, &interface)
            .method_call(move |_, _, _, _, method, parameters, invocation| {
                if let Some(service) = service.upgrade() {
                    service.handle_method(method, &parameters, invocation);
                } else {
                    return_response(invocation, 2, &[]);
                }
            })
            .build()?;
        *self.connection.borrow_mut() = Some(connection.clone());
        *self.root_registration.borrow_mut() = Some(registration);
        Ok(())
    }

    fn handle_method(
        self: &Rc<Self>,
        method: &str,
        parameters: &glib::Variant,
        invocation: gio::DBusMethodInvocation,
    ) {
        let request = match PortalRequest::parse(method, parameters) {
            Ok(request) => request,
            Err(error) => {
                tracing::warn!(%error, method, "rejected FileChooser portal request");
                return_response(invocation, 2, &[]);
                return;
            }
        };
        if self.requests.borrow().len() >= REQUEST_CAPACITY
            || self.requests.borrow().contains_key(&request.handle)
        {
            return_response(invocation, 2, &[]);
            return;
        }
        let Some(connection) = self.connection.borrow().clone() else {
            return_response(invocation, 2, &[]);
            return;
        };
        let request_node = match gio::DBusNodeInfo::for_xml(REQUEST_XML) {
            Ok(node) => node,
            Err(_) => {
                return_response(invocation, 2, &[]);
                return;
            }
        };
        let request_interface = request_node
            .lookup_interface(REQUEST_INTERFACE)
            .expect("request interface is embedded");
        let service = Rc::downgrade(self);
        let registration = match connection
            .register_object(&request.handle, &request_interface)
            .method_call(move |_, _, object_path, _, method, _, close_invocation| {
                if method == "Close" {
                    if let Some(service) = service.upgrade() {
                        service.cancel(object_path);
                    }
                }
                close_invocation.return_value(Some(&().to_variant()));
            })
            .build()
        {
            Ok(registration) => registration,
            Err(error) => {
                tracing::warn!(%error, "could not export portal request handle");
                return_response(invocation, 2, &[]);
                return;
            }
        };

        let executable = match std::env::current_exe() {
            Ok(executable) => executable,
            Err(error) => {
                tracing::warn!(%error, "could not resolve Floe executable");
                let _ = connection.unregister_object(registration);
                return_response(invocation, 2, &[]);
                return;
            }
        };
        let arguments = request.chooser_argv(executable.as_os_str());
        let argument_refs = arguments
            .iter()
            .map(OsString::as_os_str)
            .collect::<Vec<_>>();
        let launcher = gio::SubprocessLauncher::new(
            gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE,
        );
        let process = match launcher.spawn(&argument_refs) {
            Ok(process) => process,
            Err(error) => {
                tracing::warn!(%error, "could not launch Floe Selection Mode");
                let _ = connection.unregister_object(registration);
                return_response(invocation, 2, &[]);
                return;
            }
        };
        let cancellable = gio::Cancellable::new();
        self.requests.borrow_mut().insert(
            request.handle.clone(),
            PendingRequest {
                invocation,
                process: process.clone(),
                cancellable: cancellable.clone(),
                registration,
                request: request.clone(),
                cancelled: false,
            },
        );
        let service = Rc::downgrade(self);
        let handle = request.handle.clone();
        process.communicate_utf8_async(None, Some(&cancellable), move |result| {
            if let Some(service) = service.upgrade() {
                service.finish(&handle, result);
            }
        });
    }

    fn cancel(&self, handle: &str) {
        if let Some(pending) = self.requests.borrow_mut().get_mut(handle) {
            pending.cancelled = true;
            pending.cancellable.cancel();
            pending.process.force_exit();
        }
    }

    fn finish(
        &self,
        handle: &str,
        result: Result<(Option<glib::GString>, Option<glib::GString>), glib::Error>,
    ) {
        let Some(pending) = self.requests.borrow_mut().remove(handle) else {
            return;
        };
        if let Some(connection) = self.connection.borrow().as_ref() {
            let _ = connection.unregister_object(pending.registration);
        }
        if pending.cancelled {
            return_response(pending.invocation, 1, &[]);
            return;
        }
        let response = result
            .ok()
            .filter(|_| pending.process.has_exited() && pending.process.exit_status() == 0)
            .and_then(|(stdout, _)| stdout)
            .and_then(|stdout| pending.request.results_from_stdout(&stdout).ok());
        match response {
            Some(results) => return_success(pending.invocation, &pending.request, results),
            None if pending.process.has_exited() && pending.process.exit_status() == 1 => {
                return_response(pending.invocation, 1, &[])
            }
            None => return_response(pending.invocation, 2, &[]),
        }
    }

    fn shutdown(&self) {
        for pending in self.requests.borrow_mut().values_mut() {
            pending.cancelled = true;
            pending.cancellable.cancel();
            pending.process.force_exit();
        }
    }
}

fn return_response(invocation: gio::DBusMethodInvocation, response: u32, uris: &[String]) {
    let results = glib::VariantDict::new(None);
    if !uris.is_empty() {
        results.insert_value("uris", &uris.to_variant());
    }
    let response = glib::Variant::tuple_from_iter([response.to_variant(), results.end()]);
    invocation.return_value(Some(&response));
}

fn return_success(
    invocation: gio::DBusMethodInvocation,
    request: &PortalRequest,
    result: PortalResults,
) {
    let results = glib::VariantDict::new(None);
    results.insert_value("uris", &result.uris.to_variant());
    if !request.chooser_options.choices.is_empty() {
        results.insert_value("choices", &result.options.choices.to_variant());
    }
    if let Some(index) = result.options.current_filter {
        if let Some(filter) = request.chooser_options.filters.get(index) {
            let rules = filter
                .rules
                .iter()
                .map(|rule| match rule {
                    SelectionFilterRule::Glob(value) => (0u32, value.clone()),
                    SelectionFilterRule::Mime(value) => (1u32, value.clone()),
                })
                .collect::<Vec<_>>();
            results.insert_value(
                "current_filter",
                &(filter.label.clone(), rules).to_variant(),
            );
        }
    }
    let response = glib::Variant::tuple_from_iter([0u32.to_variant(), results.end()]);
    invocation.return_value(Some(&response));
}

pub fn requested(arguments: impl IntoIterator<Item = OsString>) -> bool {
    let mut arguments = arguments.into_iter();
    let _program = arguments.next();
    matches!(
        arguments.collect::<Vec<_>>().as_slice(),
        [flag] if flag == OsStr::new(PORTAL_BACKEND_FLAG)
    )
}

pub fn run() -> glib::ExitCode {
    let main_loop = glib::MainLoop::new(None, false);
    let service = Rc::new(PortalService::default());
    let acquired = Rc::new(Cell::new(false));
    let service_for_bus = Rc::clone(&service);
    let loop_for_error = main_loop.clone();
    let acquired_for_name = Rc::clone(&acquired);
    let owner = gio::bus_own_name(
        gio::BusType::Session,
        PORTAL_BUS_NAME,
        gio::BusNameOwnerFlags::NONE,
        move |connection, _| {
            if let Err(error) = service_for_bus.register(&connection) {
                tracing::error!(%error, "could not register FileChooser portal backend");
                loop_for_error.quit();
            }
        },
        move |_, _| acquired_for_name.set(true),
        {
            let main_loop = main_loop.clone();
            move |_, _| main_loop.quit()
        },
    );
    main_loop.run();
    service.shutdown();
    gio::bus_unown_name(owner);
    if acquired.get() {
        glib::ExitCode::SUCCESS
    } else {
        glib::ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_options() -> glib::Variant {
        glib::VariantDict::new(None).end()
    }

    fn call(method: &str, options: glib::Variant) -> glib::Variant {
        glib::Variant::tuple_from_iter([
            "/org/freedesktop/portal/desktop/request/1_2/token".to_variant(),
            "org.example.App".to_variant(),
            "wayland:parent-handle".to_variant(),
            "Choose a file".to_variant(),
            options,
        ])
        .tap(|_| {
            let _ = method;
        })
    }

    trait Tap: Sized {
        fn tap(self, callback: impl FnOnce(&Self)) -> Self {
            callback(&self);
            self
        }
    }
    impl<T> Tap for T {}

    #[test]
    fn phase_22b_contract_parses_open_save_and_parent_identifiers() {
        let open = PortalRequest::parse("OpenFile", &call("OpenFile", empty_options()))
            .expect("open request");
        assert_eq!(open.kind, PortalRequestKind::OpenFile { multiple: false });
        assert_eq!(
            open.parent_window,
            ParentWindow::Wayland("parent-handle".to_owned())
        );

        let options = glib::VariantDict::new(None);
        options.insert_value("multiple", &true.to_variant());
        let multiple =
            PortalRequest::parse("OpenFile", &call("OpenFile", options.end())).expect("multiple");
        assert_eq!(
            multiple.kind,
            PortalRequestKind::OpenFile { multiple: true }
        );
        assert_eq!(ParentWindow::parse("x11:1a"), Ok(ParentWindow::X11(0x1a)));
        assert_eq!(ParentWindow::parse(""), Ok(ParentWindow::None));
        assert!(ParentWindow::parse("x11:not-hex").is_err());
    }

    #[test]
    fn phase_22b_contract_preserves_raw_paths_and_rejects_complex_unsupported_options() {
        let options = glib::VariantDict::new(None);
        let mut folder = b"/tmp/raw-\xff".to_vec();
        folder.push(0);
        options.insert_value(
            "current_folder",
            &glib::Variant::array_from_fixed_array(&folder),
        );
        let request =
            PortalRequest::parse("OpenFile", &call("OpenFile", options.end())).expect("raw path");
        assert_eq!(
            request.current_folder,
            Some(PathBuf::from(OsString::from_vec(b"/tmp/raw-\xff".to_vec())))
        );

        let options = glib::VariantDict::new(None);
        options.insert_value("choices", &vec!["unsupported"].to_variant());
        assert!(matches!(
            PortalRequest::parse("OpenFile", &call("OpenFile", options.end())),
            Err(PortalRequestError::OptionType("choices"))
        ));

        let options = glib::VariantDict::new(None);
        let names = vec![
            glib::FixedSizeVariantArray::from(b"first.txt\0".to_vec()),
            glib::FixedSizeVariantArray::from(b"raw-\xff.bin\0".to_vec()),
        ];
        options.insert_value("files", &names.to_variant());
        let save_files = PortalRequest::parse("SaveFiles", &call("SaveFiles", options.end()))
            .expect("SaveFiles request");
        assert_eq!(
            save_files.kind,
            PortalRequestKind::SaveFiles {
                names: vec![
                    OsString::from("first.txt"),
                    OsString::from_vec(b"raw-\xff.bin".to_vec()),
                ],
            }
        );
        assert_eq!(
            save_files
                .results_from_stdout("file:///tmp\n")
                .expect("SaveFiles results")
                .uris,
            vec!["file:///tmp/first.txt", "file:///tmp/raw-%FF.bin"]
        );
    }

    #[test]
    fn phase_22b_supervisor_builds_exact_no_shell_arguments_and_normalizes_results() {
        let request = PortalRequest {
            handle: "/org/freedesktop/portal/desktop/request/1_2/token".to_owned(),
            app_id: "org.example.App".to_owned(),
            parent_window: ParentWindow::None,
            title: "Open safely".to_owned(),
            accept_label: Some("Choose".to_owned()),
            modal: true,
            current_folder: Some(PathBuf::from(OsString::from_vec(b"/tmp/raw-\xff".to_vec()))),
            current_name: None,
            kind: PortalRequestKind::OpenFile { multiple: true },
            chooser_options: SelectionChooserOptions::default(),
        };
        let argv = request.chooser_argv(OsStr::new("/usr/bin/floe"));
        assert_eq!(argv[0], "/usr/bin/floe");
        assert!(argv.contains(&OsString::from("--multiple")));
        assert!(argv.contains(&OsString::from("--chooser-modal")));
        assert!(argv.contains(&OsString::from_vec(b"/tmp/raw-\xff".to_vec())));
        assert!(!argv.iter().any(|argument| argument == OsStr::new("sh")));
        assert_eq!(
            request
                .results_from_stdout("file:///tmp/a%0Ab\nfile:///tmp/b\n")
                .expect("results")
                .uris,
            vec!["file:///tmp/a%0Ab", "file:///tmp/b"]
        );
        assert!(
            request
                .results_from_stdout("https://example.invalid")
                .is_err()
        );
    }

    #[test]
    fn phase_22c_portal_model_round_trips_filters_current_filter_and_choices() {
        let filters = vec![(
            "PDF documents".to_owned(),
            vec![
                (0u32, "*.pdf".to_owned()),
                (1u32, "application/pdf".to_owned()),
            ],
        )];
        let choices = vec![
            (
                "preview".to_owned(),
                "Show preview".to_owned(),
                Vec::<(String, String)>::new(),
                "true".to_owned(),
            ),
            (
                "encoding".to_owned(),
                "Encoding".to_owned(),
                vec![
                    ("utf8".to_owned(), "UTF-8".to_owned()),
                    ("latin1".to_owned(), "Latin-1".to_owned()),
                ],
                "utf8".to_owned(),
            ),
        ];
        let options = glib::VariantDict::new(None);
        options.insert_value("filters", &filters.to_variant());
        options.insert_value("current_filter", &filters[0].to_variant());
        options.insert_value("choices", &choices.to_variant());
        let request = PortalRequest::parse("OpenFile", &call("OpenFile", options.end()))
            .expect("valid portal options");
        assert_eq!(request.chooser_options.current_filter, Some(0));
        assert_eq!(request.chooser_options.choices.len(), 2);
        let argv = request.chooser_argv(OsStr::new("/usr/bin/floe"));
        let encoded = argv
            .windows(2)
            .find(|pair| pair[0] == OsStr::new("--chooser-options-v1"))
            .and_then(|pair| pair[1].to_str())
            .expect("typed chooser payload");
        assert_eq!(
            crate::selection_mode::decode_chooser_options(encoded),
            Some(request.chooser_options.clone())
        );

        let option_result = SelectionOptionResult {
            current_filter: Some(0),
            choices: vec![
                ("preview".to_owned(), "true".to_owned()),
                ("encoding".to_owned(), "latin1".to_owned()),
            ],
        };
        let result = request
            .results_from_stdout(&format!(
                "floe-chooser-options-v1:{}\nfile:///tmp/report.pdf\n",
                crate::selection_mode::encode_option_result(&option_result)
                    .expect("result encoding")
            ))
            .expect("valid filtered result");
        assert_eq!(result.options, option_result);
        assert_eq!(result.uris, ["file:///tmp/report.pdf"]);
    }

    #[test]
    fn phase_23_reliability_portal_filter_is_advisory_for_explicit_selection() {
        let filters = vec![("PDF documents".to_owned(), vec![(0u32, "*.pdf".to_owned())])];
        let options = glib::VariantDict::new(None);
        options.insert_value("filters", &filters.to_variant());
        options.insert_value("current_filter", &filters[0].to_variant());
        let request = PortalRequest::parse("SaveFile", &call("SaveFile", options.end()))
            .expect("valid filtered SaveFile request");
        let option_result = SelectionOptionResult {
            current_filter: Some(0),
            choices: Vec::new(),
        };

        let result = request
            .results_from_stdout(&format!(
                "floe-chooser-options-v1:{}\nfile:///tmp/notes.txt\n",
                crate::selection_mode::encode_option_result(&option_result)
                    .expect("result encoding")
            ))
            .expect("portal filters are selection aids, not content enforcement");

        assert_eq!(result.uris, ["file:///tmp/notes.txt"]);
        assert_eq!(result.options.current_filter, Some(0));
    }

    #[test]
    fn phase_22b_dbus_contract_names_interfaces_and_response_types() {
        let node = gio::DBusNodeInfo::for_xml(PORTAL_XML).expect("portal XML");
        assert!(node.lookup_interface(PORTAL_INTERFACE).is_some());
        let request = gio::DBusNodeInfo::for_xml(REQUEST_XML).expect("request XML");
        assert!(request.lookup_interface(REQUEST_INTERFACE).is_some());
        assert_eq!(PORTAL_BUS_NAME, "org.freedesktop.impl.portal.desktop.floe");
        assert!(requested(["floe".into(), PORTAL_BACKEND_FLAG.into()]));
        assert!(!requested(["floe".into()]));
        let response =
            glib::Variant::tuple_from_iter([0u32.to_variant(), glib::VariantDict::new(None).end()]);
        assert_eq!(response.type_().as_str(), "(ua{sv})");
    }
}
