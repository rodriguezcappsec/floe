//! Typed GIO/GVfs administrator browsing and explicit operation presentation.
//!
//! This module deliberately does not expose administrator resources as local
//! `PathBuf` jobs. The local path is retained only as the exact origin needed
//! to build and present a validated resource; all I/O uses the owned GFile URI.

use std::{
    cell::{Cell, RefCell},
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStrExt,
    path::{Component, Path, PathBuf},
    rc::{Rc, Weak},
    time::Duration,
};

use adw::prelude::*;
use gtk::{gio, glib};
use thiserror::Error;

use crate::privileged_operations::{
    GioPrivilegedOperationService, PrivilegedOperationEvent, PrivilegedOperationKind,
    PrivilegedOperationRequest, administrator_durable_undo_unavailable,
};

type OperationRequestBuilder =
    dyn Fn(&PrivilegedAccessController, u64, &str) -> Result<PrivilegedOperationRequest, String>;

const ENUMERATION_ATTRIBUTES: &str = "standard::name,standard::display-name,standard::type,standard::is-symlink,standard::size,standard::is-hidden,time::modified,unix::device,unix::inode";
const ENUMERATION_PAGE_SIZE: i32 = 128;
const ENUMERATION_ENTRY_CAPACITY: usize = 4_096;
const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(120);
const PAGE_TIMEOUT: Duration = Duration::from_secs(30);
const OPERATION_NO_PROGRESS_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum PrivilegedWatchdogPhase {
    #[default]
    Idle,
    Waiting,
    NoProgress,
    CancellationRequested {
        escape_allowed: bool,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct PrivilegedOperationWatchdog {
    operation_id: Option<u64>,
    generation: u64,
    phase: PrivilegedWatchdogPhase,
}

impl PrivilegedOperationWatchdog {
    fn arm(&mut self, operation_id: u64) -> u64 {
        self.operation_id = Some(operation_id);
        self.generation = self.generation.wrapping_add(1).max(1);
        self.phase = PrivilegedWatchdogPhase::Waiting;
        self.generation
    }

    fn expire(&mut self, operation_id: u64, generation: u64) -> bool {
        if self.operation_id == Some(operation_id)
            && self.generation == generation
            && self.phase == PrivilegedWatchdogPhase::Waiting
        {
            self.phase = PrivilegedWatchdogPhase::NoProgress;
            true
        } else {
            false
        }
    }

    fn progress(&mut self, operation_id: u64) -> Option<u64> {
        if self.operation_id != Some(operation_id)
            || matches!(
                self.phase,
                PrivilegedWatchdogPhase::CancellationRequested { .. }
            )
        {
            return None;
        }
        Some(self.arm(operation_id))
    }

    fn continue_waiting(&mut self) -> Option<(u64, u64)> {
        let operation_id = self.operation_id?;
        (self.phase == PrivilegedWatchdogPhase::NoProgress)
            .then(|| (operation_id, self.arm(operation_id)))
    }

    fn cancellation_requested(&mut self, operation_id: u64) -> bool {
        if self.operation_id != Some(operation_id) {
            return false;
        }
        let escape_allowed = self.phase == PrivilegedWatchdogPhase::NoProgress;
        self.phase = PrivilegedWatchdogPhase::CancellationRequested { escape_allowed };
        escape_allowed
    }

    fn escape_allowed(self) -> bool {
        matches!(
            self.phase,
            PrivilegedWatchdogPhase::CancellationRequested {
                escape_allowed: true
            }
        )
    }

    fn finish(&mut self, operation_id: u64) {
        if self.operation_id == Some(operation_id) {
            self.operation_id = None;
            self.phase = PrivilegedWatchdogPhase::Idle;
            self.generation = self.generation.wrapping_add(1).max(1);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessLevel {
    Standard,
    Administrator,
}

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum PrivilegedIdentityError {
    #[error("administrator access requires an absolute local path")]
    RelativePath,
    #[error("the local file URI could not be parsed safely")]
    InvalidFileUri,
    #[error("the local file URI contains authority or extra components")]
    UnsafeFileUri,
    #[error("the local path did not round-trip through GIO exactly")]
    FileUriRoundTrip,
    #[error("the administrator URI could not be validated")]
    InvalidAdministratorUri,
    #[error("the provider returned an unexpected child identity")]
    UnexpectedChild,
}

/// An administrator resource can only be created from an exact absolute local
/// path. There is intentionally no constructor accepting an arbitrary URI.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PrivilegedResourceId {
    local_path: PathBuf,
    admin_uri: Box<str>,
}

impl PrivilegedResourceId {
    pub fn from_local_path(path: &Path) -> Result<Self, PrivilegedIdentityError> {
        if !path.is_absolute() {
            return Err(PrivilegedIdentityError::RelativePath);
        }

        let local_file = gio::File::for_path(path);
        let file_uri = local_file.uri();
        let parsed = glib::Uri::parse(file_uri.as_str(), glib::UriFlags::ENCODED)
            .map_err(|_| PrivilegedIdentityError::InvalidFileUri)?;
        validate_file_uri(&parsed)?;

        if gio::File::for_uri(file_uri.as_str()).path().as_deref() != Some(path) {
            return Err(PrivilegedIdentityError::FileUriRoundTrip);
        }

        let built_admin_uri = glib::Uri::build(
            glib::UriFlags::ENCODED_PATH,
            "admin",
            None,
            None,
            -1,
            parsed.path().as_str(),
            None,
            None,
        )
        .to_str()
        .to_string();
        let admin_uri = gio::File::for_uri(&built_admin_uri)
            .uri()
            .to_string()
            .into_boxed_str();
        validate_admin_uri(&admin_uri, path)?;

        Ok(Self {
            local_path: path.to_path_buf(),
            admin_uri,
        })
    }

    pub fn local_path(&self) -> &Path {
        &self.local_path
    }

    pub const fn access_level(&self) -> AccessLevel {
        AccessLevel::Administrator
    }

    pub fn parent(&self) -> Option<Self> {
        self.local_path
            .parent()
            .and_then(|path| Self::from_local_path(path).ok())
    }

    pub(crate) fn file(&self) -> gio::File {
        gio::File::for_uri(&self.admin_uri)
    }

    #[cfg(test)]
    fn admin_uri(&self) -> &str {
        &self.admin_uri
    }
}

fn validate_file_uri(uri: &glib::Uri) -> Result<(), PrivilegedIdentityError> {
    if uri.scheme() != "file"
        || uri.userinfo().is_some()
        || uri.host().is_some_and(|host| !host.is_empty())
        || uri.port() != -1
        || uri.query().is_some()
        || uri.fragment().is_some()
        || !uri.path().starts_with('/')
    {
        return Err(PrivilegedIdentityError::UnsafeFileUri);
    }
    Ok(())
}

fn validate_admin_uri(uri: &str, expected_path: &Path) -> Result<(), PrivilegedIdentityError> {
    let parsed = glib::Uri::parse(uri, glib::UriFlags::ENCODED)
        .map_err(|_| PrivilegedIdentityError::InvalidAdministratorUri)?;
    if parsed.scheme() != "admin"
        || parsed.userinfo().is_some()
        || parsed.host().is_some_and(|host| !host.is_empty())
        || parsed.port() != -1
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed.path().starts_with('/')
    {
        return Err(PrivilegedIdentityError::InvalidAdministratorUri);
    }

    let local_uri = glib::Uri::build(
        glib::UriFlags::ENCODED_PATH,
        "file",
        None,
        None,
        -1,
        parsed.path().as_str(),
        None,
        None,
    )
    .to_str();
    if gio::File::for_uri(local_uri.as_str()).path().as_deref() != Some(expected_path) {
        return Err(PrivilegedIdentityError::FileUriRoundTrip);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivilegedEntryKind {
    Directory,
    File,
    SymbolicLink,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivilegedEntry {
    resource: PrivilegedResourceId,
    exact_name: OsString,
    display_name: String,
    kind: PrivilegedEntryKind,
    size: Option<u64>,
    hidden: bool,
    modified: Option<u64>,
    device: Option<u64>,
    inode: Option<u64>,
}

impl PrivilegedEntry {
    pub fn resource(&self) -> &PrivilegedResourceId {
        &self.resource
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn kind(&self) -> PrivilegedEntryKind {
        self.kind
    }

    pub fn size(&self) -> Option<u64> {
        self.size
    }

    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    pub(crate) fn exact_name(&self) -> &OsStr {
        &self.exact_name
    }

    pub(crate) fn modified(&self) -> Option<u64> {
        self.modified
    }

    pub(crate) fn device(&self) -> Option<u64> {
        self.device
    }

    pub(crate) fn inode(&self) -> Option<u64> {
        self.inode
    }
}

#[cfg(test)]
pub(crate) fn test_entry(
    path: &str,
    kind: PrivilegedEntryKind,
    size: Option<u64>,
) -> PrivilegedEntry {
    let local_path = PathBuf::from(path);
    let exact_name = local_path
        .file_name()
        .unwrap_or(OsStr::new("/"))
        .to_os_string();
    PrivilegedEntry {
        resource: PrivilegedResourceId::from_local_path(&local_path)
            .unwrap_or_else(|error| panic!("test administrator identity: {error}")),
        display_name: exact_name.to_string_lossy().into_owned(),
        exact_name,
        kind,
        size,
        hidden: false,
        modified: None,
        device: None,
        inode: None,
    }
}

fn entry_from_info(
    parent: &PrivilegedResourceId,
    enumerator: &gio::FileEnumerator,
    info: &gio::FileInfo,
) -> Result<PrivilegedEntry, PrivilegedIdentityError> {
    let name = info.name();
    let mut components = name.components();
    let valid_name = matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
        && !name.as_os_str().as_bytes().contains(&0);
    if !valid_name {
        return Err(PrivilegedIdentityError::UnexpectedChild);
    }

    let local_path = parent.local_path.join(&name);
    let resource = PrivilegedResourceId::from_local_path(&local_path)?;
    if enumerator.child(info).uri().as_str() != resource.admin_uri.as_ref() {
        return Err(PrivilegedIdentityError::UnexpectedChild);
    }

    let kind = if info.is_symlink() {
        PrivilegedEntryKind::SymbolicLink
    } else {
        match info.file_type() {
            gio::FileType::Directory => PrivilegedEntryKind::Directory,
            gio::FileType::Regular => PrivilegedEntryKind::File,
            _ => PrivilegedEntryKind::Other,
        }
    };
    let size = (kind == PrivilegedEntryKind::File)
        .then(|| u64::try_from(info.size()).ok())
        .flatten();
    let exact_name = name.into_os_string();
    let hidden = info.is_hidden() || exact_name.as_bytes().first() == Some(&b'.');

    Ok(PrivilegedEntry {
        resource,
        exact_name,
        display_name: info.display_name().to_string(),
        kind,
        size,
        hidden,
        modified: info
            .has_attribute("time::modified")
            .then(|| info.attribute_uint64("time::modified")),
        device: info
            .has_attribute("unix::device")
            .then(|| info.attribute_uint64("unix::device")),
        inode: info
            .has_attribute("unix::inode")
            .then(|| info.attribute_uint64("unix::inode")),
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivilegedFailureKind {
    Unsupported,
    Denied,
    NoAuthenticationAgent,
    Unavailable,
    Cancelled,
    TimedOut,
    InvalidIdentity,
    Backend,
}

impl PrivilegedFailureKind {
    pub const fn user_message(self) -> &'static str {
        match self {
            Self::Unsupported => "The desktop does not advertise a GVfs administrator backend.",
            Self::Denied => "Administrator access was denied. No files were opened.",
            Self::NoAuthenticationAgent => {
                "No desktop authentication agent answered the administrator request."
            }
            Self::Unavailable => "The desktop administrator service is unavailable.",
            Self::Cancelled => "Administrator access was cancelled.",
            Self::TimedOut => "Administrator access timed out and was cancelled.",
            Self::InvalidIdentity => "The administrator resource identity was rejected safely.",
            Self::Backend => "The desktop administrator service could not open this folder.",
        }
    }
}

fn failure_can_retry(kind: PrivilegedFailureKind) -> bool {
    matches!(
        kind,
        PrivilegedFailureKind::Denied
            | PrivilegedFailureKind::NoAuthenticationAgent
            | PrivilegedFailureKind::Unavailable
            | PrivilegedFailureKind::TimedOut
            | PrivilegedFailureKind::Backend
    )
}

#[derive(Clone, Debug)]
pub enum PrivilegedProviderEvent {
    Page {
        generation: u64,
        location: PrivilegedResourceId,
        entries: Vec<PrivilegedEntry>,
    },
    Finished {
        generation: u64,
        location: PrivilegedResourceId,
        truncated: bool,
    },
    Failed {
        generation: u64,
        location: PrivilegedResourceId,
        kind: PrivilegedFailureKind,
    },
}

impl PrivilegedProviderEvent {
    fn generation(&self) -> u64 {
        match self {
            Self::Page { generation, .. }
            | Self::Finished { generation, .. }
            | Self::Failed { generation, .. } => *generation,
        }
    }
}

pub fn admin_scheme_supported() -> bool {
    gio::Vfs::default()
        .supported_uri_schemes()
        .iter()
        .any(|scheme| scheme.as_str() == "admin")
}

struct ActiveRequest {
    generation: u64,
    cancellable: gio::Cancellable,
}

/// GIO objects are created, used, and released on the owning GLib main context.
/// Callbacks emit typed events; they never hand administrator URIs to widgets.
pub struct GioPrivilegedProvider {
    active: Rc<RefCell<Option<ActiveRequest>>>,
    callback: Rc<dyn Fn(PrivilegedProviderEvent)>,
}

impl GioPrivilegedProvider {
    pub fn new(callback: Rc<dyn Fn(PrivilegedProviderEvent)>) -> Self {
        Self {
            active: Rc::new(RefCell::new(None)),
            callback,
        }
    }

    pub fn start(
        &self,
        generation: u64,
        location: PrivilegedResourceId,
        mount_operation: gio::MountOperation,
    ) {
        self.cancel();
        if !admin_scheme_supported() {
            (self.callback)(PrivilegedProviderEvent::Failed {
                generation,
                location,
                kind: PrivilegedFailureKind::Unsupported,
            });
            return;
        }

        let cancellable = gio::Cancellable::new();
        self.active.replace(Some(ActiveRequest {
            generation,
            cancellable: cancellable.clone(),
        }));
        request_enumerator(
            generation,
            location,
            mount_operation,
            cancellable,
            Rc::clone(&self.callback),
            Rc::clone(&self.active),
            false,
        );
    }

    pub fn cancel(&self) {
        if let Some(active) = self.active.borrow_mut().take() {
            tracing::debug!(
                request_id = active.generation,
                "cancelling privileged browse request"
            );
            active.cancellable.cancel();
        }
    }

    #[cfg(test)]
    fn active_generation(&self) -> Option<u64> {
        self.active
            .borrow()
            .as_ref()
            .map(|active| active.generation)
    }
}

impl Drop for GioPrivilegedProvider {
    fn drop(&mut self) {
        if let Some(active) = self.active.borrow_mut().take() {
            active.cancellable.cancel();
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EnumerationFailureAction {
    Mount,
    Fail(PrivilegedFailureKind),
}

fn enumeration_failure_action(
    error: &glib::Error,
    timed_out: bool,
    mount_attempted: bool,
) -> EnumerationFailureAction {
    if !timed_out && !mount_attempted && error.matches(gio::IOErrorEnum::NotMounted) {
        EnumerationFailureAction::Mount
    } else if mount_attempted && error.matches(gio::IOErrorEnum::NotMounted) {
        EnumerationFailureAction::Fail(PrivilegedFailureKind::Backend)
    } else {
        EnumerationFailureAction::Fail(classify_gio_error(error, timed_out))
    }
}

fn mount_failure_kind(error: &glib::Error, timed_out: bool) -> Option<PrivilegedFailureKind> {
    if !timed_out && error.matches(gio::IOErrorEnum::AlreadyMounted) {
        None
    } else {
        Some(classify_gio_error(error, timed_out))
    }
}

fn request_enumerator(
    generation: u64,
    location: PrivilegedResourceId,
    mount_operation: gio::MountOperation,
    cancellable: gio::Cancellable,
    callback: Rc<dyn Fn(PrivilegedProviderEvent)>,
    active: Rc<RefCell<Option<ActiveRequest>>>,
    mount_attempted: bool,
) {
    let file = location.file();
    let timed_out = Rc::new(Cell::new(false));
    let timeout = install_timeout(&cancellable, Rc::clone(&timed_out), AUTHORIZATION_TIMEOUT);
    let cancellable_for_callback = cancellable.clone();
    file.enumerate_children_async(
        ENUMERATION_ATTRIBUTES,
        gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
        glib::Priority::DEFAULT,
        Some(&cancellable),
        move |result| {
            cancel_timeout(&timeout);
            if request_was_superseded(&active, generation) {
                return;
            }
            match result {
                Ok(enumerator) => request_page(
                    generation,
                    location,
                    enumerator,
                    cancellable_for_callback,
                    callback,
                    active,
                    0,
                ),
                Err(error) => {
                    match enumeration_failure_action(&error, timed_out.get(), mount_attempted) {
                        EnumerationFailureAction::Mount => request_mount(
                            generation,
                            location,
                            mount_operation,
                            cancellable_for_callback,
                            callback,
                            active,
                        ),
                        EnumerationFailureAction::Fail(kind) => {
                            emit_failure(generation, location, kind, callback, active);
                        }
                    }
                }
            }
        },
    );
}

fn request_mount(
    generation: u64,
    location: PrivilegedResourceId,
    mount_operation: gio::MountOperation,
    cancellable: gio::Cancellable,
    callback: Rc<dyn Fn(PrivilegedProviderEvent)>,
    active: Rc<RefCell<Option<ActiveRequest>>>,
) {
    let file = location.file();
    let timed_out = Rc::new(Cell::new(false));
    let timeout = install_timeout(&cancellable, Rc::clone(&timed_out), AUTHORIZATION_TIMEOUT);
    let mount_operation_for_callback = mount_operation.clone();
    let cancellable_for_callback = cancellable.clone();
    file.mount_enclosing_volume(
        gio::MountMountFlags::NONE,
        Some(&mount_operation),
        Some(&cancellable),
        move |result| {
            cancel_timeout(&timeout);
            if request_was_superseded(&active, generation) {
                return;
            }
            if cancellable_for_callback.is_cancelled() {
                let kind = if timed_out.get() {
                    PrivilegedFailureKind::TimedOut
                } else {
                    PrivilegedFailureKind::Cancelled
                };
                emit_failure(generation, location, kind, callback, active);
                return;
            }
            match result {
                Ok(()) => request_enumerator(
                    generation,
                    location,
                    mount_operation_for_callback,
                    cancellable_for_callback,
                    callback,
                    active,
                    true,
                ),
                Err(error) => {
                    if let Some(kind) = mount_failure_kind(&error, timed_out.get()) {
                        emit_failure(generation, location, kind, callback, active);
                    } else {
                        request_enumerator(
                            generation,
                            location,
                            mount_operation_for_callback,
                            cancellable_for_callback,
                            callback,
                            active,
                            true,
                        );
                    }
                }
            }
        },
    );
}

fn request_was_superseded(active: &Rc<RefCell<Option<ActiveRequest>>>, generation: u64) -> bool {
    active
        .borrow()
        .as_ref()
        .is_some_and(|request| request.generation != generation)
}

fn emit_failure(
    generation: u64,
    location: PrivilegedResourceId,
    kind: PrivilegedFailureKind,
    callback: Rc<dyn Fn(PrivilegedProviderEvent)>,
    active: Rc<RefCell<Option<ActiveRequest>>>,
) {
    clear_active(&active, generation);
    callback(PrivilegedProviderEvent::Failed {
        generation,
        location,
        kind,
    });
}

fn install_timeout(
    cancellable: &gio::Cancellable,
    timed_out: Rc<Cell<bool>>,
    duration: Duration,
) -> Rc<RefCell<Option<glib::SourceId>>> {
    let cancellable = cancellable.clone();
    let source = Rc::new(RefCell::new(None));
    let source_for_timeout = Rc::clone(&source);
    let source_id = glib::timeout_add_local_once(duration, move || {
        source_for_timeout.borrow_mut().take();
        timed_out.set(true);
        cancellable.cancel();
    });
    source.replace(Some(source_id));
    source
}

fn cancel_timeout(source: &Rc<RefCell<Option<glib::SourceId>>>) {
    if let Some(source) = source.borrow_mut().take() {
        source.remove();
    }
}

fn request_page(
    generation: u64,
    location: PrivilegedResourceId,
    enumerator: gio::FileEnumerator,
    cancellable: gio::Cancellable,
    callback: Rc<dyn Fn(PrivilegedProviderEvent)>,
    active: Rc<RefCell<Option<ActiveRequest>>>,
    count: usize,
) {
    let timed_out = Rc::new(Cell::new(false));
    let timeout = install_timeout(&cancellable, Rc::clone(&timed_out), PAGE_TIMEOUT);
    let enumerator_for_callback = enumerator.clone();
    let cancellable_for_callback = cancellable.clone();
    enumerator.next_files_async(
        ENUMERATION_PAGE_SIZE,
        glib::Priority::DEFAULT,
        Some(&cancellable),
        move |result| {
            cancel_timeout(&timeout);
            let infos = match result {
                Ok(infos) => infos,
                Err(error) => {
                    clear_active(&active, generation);
                    callback(PrivilegedProviderEvent::Failed {
                        generation,
                        location,
                        kind: classify_gio_error(&error, timed_out.get()),
                    });
                    return;
                }
            };

            if infos.is_empty() {
                clear_active(&active, generation);
                callback(PrivilegedProviderEvent::Page {
                    generation,
                    location: location.clone(),
                    entries: Vec::new(),
                });
                callback(PrivilegedProviderEvent::Finished {
                    generation,
                    location,
                    truncated: false,
                });
                return;
            }

            let remaining = ENUMERATION_ENTRY_CAPACITY.saturating_sub(count);
            let mut entries = Vec::with_capacity(infos.len().min(remaining));
            for info in infos.iter().take(remaining) {
                match entry_from_info(&location, &enumerator_for_callback, info) {
                    Ok(entry) => entries.push(entry),
                    Err(_) => {
                        clear_active(&active, generation);
                        callback(PrivilegedProviderEvent::Failed {
                            generation,
                            location,
                            kind: PrivilegedFailureKind::InvalidIdentity,
                        });
                        return;
                    }
                }
            }
            let new_count = count.saturating_add(entries.len());
            callback(PrivilegedProviderEvent::Page {
                generation,
                location: location.clone(),
                entries,
            });
            if new_count >= ENUMERATION_ENTRY_CAPACITY {
                clear_active(&active, generation);
                callback(PrivilegedProviderEvent::Finished {
                    generation,
                    location,
                    truncated: true,
                });
                return;
            }
            request_page(
                generation,
                location,
                enumerator_for_callback,
                cancellable_for_callback,
                callback,
                active,
                new_count,
            );
        },
    );
}

fn clear_active(active: &Rc<RefCell<Option<ActiveRequest>>>, generation: u64) {
    if active
        .borrow()
        .as_ref()
        .is_some_and(|active| active.generation == generation)
    {
        active.borrow_mut().take();
    }
}

fn classify_gio_error(error: &glib::Error, timed_out: bool) -> PrivilegedFailureKind {
    if timed_out {
        PrivilegedFailureKind::TimedOut
    } else if error.matches(gio::IOErrorEnum::Cancelled) {
        PrivilegedFailureKind::Cancelled
    } else if error.matches(gio::IOErrorEnum::PermissionDenied) {
        PrivilegedFailureKind::Denied
    } else if error
        .message()
        .to_ascii_lowercase()
        .contains("authentication agent")
    {
        PrivilegedFailureKind::NoAuthenticationAgent
    } else if error.matches(gio::IOErrorEnum::NotSupported) {
        PrivilegedFailureKind::Unsupported
    } else if error.matches(gio::IOErrorEnum::NotMounted)
        || error.matches(gio::IOErrorEnum::HostNotFound)
        || error.matches(gio::IOErrorEnum::ConnectionRefused)
    {
        PrivilegedFailureKind::Unavailable
    } else {
        PrivilegedFailureKind::Backend
    }
}

#[derive(Clone, Debug)]
struct PrivilegedNavigation {
    current: PrivilegedResourceId,
    back: Vec<PrivilegedResourceId>,
    forward: Vec<PrivilegedResourceId>,
}

impl PrivilegedNavigation {
    fn new(current: PrivilegedResourceId) -> Self {
        Self {
            current,
            back: Vec::new(),
            forward: Vec::new(),
        }
    }

    fn navigate_to(&mut self, destination: PrivilegedResourceId) -> bool {
        if self.current == destination {
            return false;
        }
        self.back
            .push(std::mem::replace(&mut self.current, destination));
        self.forward.clear();
        true
    }

    fn go_back(&mut self) -> bool {
        let Some(destination) = self.back.pop() else {
            return false;
        };
        self.forward
            .push(std::mem::replace(&mut self.current, destination));
        true
    }

    fn go_forward(&mut self) -> bool {
        let Some(destination) = self.forward.pop() else {
            return false;
        };
        self.back
            .push(std::mem::replace(&mut self.current, destination));
        true
    }

    fn go_parent(&mut self) -> bool {
        let Some(parent) = self.current.parent() else {
            return false;
        };
        self.navigate_to(parent)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionPhase {
    Standard,
    Authorizing,
    Privileged,
    Failed(PrivilegedFailureKind),
}

#[derive(Clone, Debug)]
struct PrivilegedSession {
    next_generation: u64,
    active_generation: u64,
    return_location: Option<PathBuf>,
    navigation: Option<PrivilegedNavigation>,
    rollback: Option<PrivilegedNavigation>,
    phase: SessionPhase,
}

impl Default for PrivilegedSession {
    fn default() -> Self {
        Self {
            next_generation: 0,
            active_generation: 0,
            return_location: None,
            navigation: None,
            rollback: None,
            phase: SessionPhase::Standard,
        }
    }
}

impl PrivilegedSession {
    fn begin(
        &mut self,
        local_path: &Path,
    ) -> Result<(u64, PrivilegedResourceId), PrivilegedIdentityError> {
        let resource = PrivilegedResourceId::from_local_path(local_path)?;
        self.return_location = Some(local_path.to_path_buf());
        self.navigation = Some(PrivilegedNavigation::new(resource.clone()));
        self.start_current()
            .map(|generation| (generation, resource))
    }

    fn start_current(&mut self) -> Result<u64, PrivilegedIdentityError> {
        if self.navigation.is_none() {
            return Err(PrivilegedIdentityError::InvalidAdministratorUri);
        }
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        self.active_generation = self.next_generation;
        self.phase = SessionPhase::Authorizing;
        Ok(self.active_generation)
    }

    fn navigate(
        &mut self,
        destination: PrivilegedResourceId,
    ) -> Option<(u64, PrivilegedResourceId)> {
        let previous = self.navigation.clone()?;
        if !self.navigation.as_mut()?.navigate_to(destination.clone()) {
            return None;
        }
        self.rollback = Some(previous);
        self.start_current()
            .ok()
            .map(|generation| (generation, destination))
    }

    fn go_back(&mut self) -> Option<(u64, PrivilegedResourceId)> {
        self.change_history(PrivilegedNavigation::go_back)
    }

    fn go_forward(&mut self) -> Option<(u64, PrivilegedResourceId)> {
        self.change_history(PrivilegedNavigation::go_forward)
    }

    fn go_parent(&mut self) -> Option<(u64, PrivilegedResourceId)> {
        self.change_history(PrivilegedNavigation::go_parent)
    }

    fn change_history(
        &mut self,
        change: impl FnOnce(&mut PrivilegedNavigation) -> bool,
    ) -> Option<(u64, PrivilegedResourceId)> {
        let previous = self.navigation.clone()?;
        if !change(self.navigation.as_mut()?) {
            return None;
        }
        let destination = self.navigation.as_ref()?.current.clone();
        self.rollback = Some(previous);
        self.start_current()
            .ok()
            .map(|generation| (generation, destination))
    }

    fn accept_page(&mut self, generation: u64) -> bool {
        if generation != self.active_generation {
            return false;
        }
        match self.phase {
            SessionPhase::Authorizing => {
                self.phase = SessionPhase::Privileged;
                self.rollback = None;
                true
            }
            SessionPhase::Privileged => true,
            SessionPhase::Standard | SessionPhase::Failed(_) => false,
        }
    }

    fn accept_failure(&mut self, generation: u64, kind: PrivilegedFailureKind) -> bool {
        if generation != self.active_generation {
            return false;
        }
        if let Some(previous) = self.rollback.take() {
            self.navigation = Some(previous);
            self.phase = SessionPhase::Privileged;
        } else if self.phase == SessionPhase::Privileged {
            // A later page failed after this location had already entered the
            // privileged state. Keep the authority explicit while the UI
            // reports that its retained result set is incomplete.
        } else {
            self.phase = SessionPhase::Failed(kind);
        }
        true
    }

    fn leave(&mut self) -> Option<PathBuf> {
        let return_location = self.return_location.take();
        self.navigation = None;
        self.rollback = None;
        self.phase = SessionPhase::Standard;
        self.active_generation = self.active_generation.wrapping_add(1).max(1);
        return_location
    }

    fn current(&self) -> Option<&PrivilegedResourceId> {
        self.navigation
            .as_ref()
            .map(|navigation| &navigation.current)
    }

    fn access_level(&self) -> AccessLevel {
        if self.phase == SessionPhase::Privileged {
            AccessLevel::Administrator
        } else {
            AccessLevel::Standard
        }
    }
}

pub struct PrivilegedViewWidgets {
    pub dialog: adw::Dialog,
    pub badge: gtk::Label,
    pub location: gtk::Label,
    pub status: gtk::Label,
    pub list: gtk::ListView,
    pub back: gtk::Button,
    pub forward: gtk::Button,
    pub parent: gtk::Button,
    pub cancel: gtk::Button,
    pub continue_waiting: gtk::Button,
    pub retry: gtk::Button,
    pub return_standard: gtk::Button,
    pub new_folder: gtk::Button,
    pub rename: gtk::Button,
    pub copy: gtk::Button,
    pub move_item: gtk::Button,
    pub trash: gtk::Button,
    pub delete: gtk::Button,
    pub permissions: gtk::Button,
    selection: gtk::SingleSelection,
    model: gio::ListStore,
}

pub fn build_view() -> PrivilegedViewWidgets {
    let dialog = adw::Dialog::builder()
        .title("Administrator File Operations")
        .content_width(820)
        .content_height(620)
        .build();
    dialog.update_property(&[
        gtk::accessible::Property::Label("Administrator file operations view"),
        gtk::accessible::Property::Description(
            "Authenticated GVfs administrator browsing with explicit bounded file operations",
        ),
    ]);

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let title = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let badge = gtk::Label::new(Some("Administrator"));
    badge.add_css_class("error");
    badge.add_css_class("heading");
    badge.set_tooltip_text(Some(
        "This view uses desktop-authorized administrator access",
    ));
    badge.update_property(&[
        gtk::accessible::Property::Label("Administrator access active"),
        gtk::accessible::Property::Description(
            "Privileged content; only the explicit administrator controls below are available",
        ),
    ]);
    let read_only = gtk::Label::new(Some("Explicit operations"));
    read_only.add_css_class("dim-label");
    title.append(&badge);
    title.append(&read_only);
    header.set_title_widget(Some(&title));

    let back = icon_button("go-previous-symbolic", "Back in administrator history");
    let forward = icon_button("go-next-symbolic", "Forward in administrator history");
    let parent = icon_button("go-up-symbolic", "Parent administrator folder");
    header.pack_start(&back);
    header.pack_start(&forward);
    header.pack_start(&parent);
    let return_standard = gtk::Button::with_label("Return to Standard Access");
    return_standard.update_property(&[
        gtk::accessible::Property::Label("Return to standard access"),
        gtk::accessible::Property::Description(
            "Cancel administrator requests and close this privileged view",
        ),
    ]);
    header.pack_end(&return_standard);
    toolbar.add_top_bar(&header);

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    let location = gtk::Label::builder()
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .selectable(false)
        .build();
    location.update_property(&[
        gtk::accessible::Property::Label("Administrator folder location"),
        gtk::accessible::Property::Description(
            "Presentation only; the exact typed resource remains authoritative",
        ),
    ]);
    content.append(&location);

    let model = gio::ListStore::new::<glib::BoxedAnyObject>();
    let selection = gtk::SingleSelection::new(Some(model.clone()));
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, object| {
        let Some(item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.set_margin_top(7);
        row.set_margin_bottom(7);
        row.set_margin_start(10);
        row.set_margin_end(10);
        let icon = gtk::Image::new();
        icon.set_pixel_size(24);
        let label = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        let detail = gtk::Label::new(None);
        detail.add_css_class("dim-label");
        row.append(&icon);
        row.append(&label);
        row.append(&detail);
        item.set_child(Some(&row));
    });
    factory.connect_bind(|_, object| {
        let Some(item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(entry) = item.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let Some(row) = item.child().and_downcast::<gtk::Box>() else {
            return;
        };
        let Some(icon) = row.first_child().and_downcast::<gtk::Image>() else {
            return;
        };
        let Some(label) = icon.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(detail) = label.next_sibling().and_downcast::<gtk::Label>() else {
            return;
        };
        let entry = entry.borrow::<PrivilegedEntry>();
        icon.set_icon_name(Some(match entry.kind() {
            PrivilegedEntryKind::Directory => "folder-symbolic",
            PrivilegedEntryKind::SymbolicLink => "emblem-symbolic-link-symbolic",
            _ => "text-x-generic-symbolic",
        }));
        label.set_label(entry.display_name());
        let mut detail_text = match entry.kind() {
            PrivilegedEntryKind::Directory => "Folder".to_owned(),
            PrivilegedEntryKind::SymbolicLink => "Symbolic link".to_owned(),
            PrivilegedEntryKind::File => entry
                .size()
                .map_or_else(|| "File".to_owned(), |size| format!("File · {size} bytes")),
            PrivilegedEntryKind::Other => "Special".to_owned(),
        };
        if entry.is_hidden() {
            detail_text.push_str(" · Hidden");
        }
        detail.set_label(&detail_text);
        let exact_name = entry.exact_name.to_string_lossy();
        let tooltip = if exact_name == entry.display_name() {
            entry.display_name().to_owned()
        } else {
            format!("{} — filesystem name: {exact_name}", entry.display_name())
        };
        row.set_tooltip_text(Some(&tooltip));
    });
    let list = gtk::ListView::new(Some(selection.clone()), Some(factory));
    list.set_single_click_activate(false);
    list.update_property(&[
        gtk::accessible::Property::Label("Administrator folder contents"),
        gtk::accessible::Property::Description(
            "Administrator entries; activate folders to navigate without following symbolic links",
        ),
    ]);
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&list)
        .build();
    content.append(&scroller);

    let operations = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let new_folder = operation_button("New Folder", "Create a folder as administrator");
    let rename = operation_button("Rename", "Rename the selected item as administrator");
    let copy = operation_button("Copy To…", "Copy the selected item as administrator");
    let move_item = operation_button("Move To…", "Move the selected item as administrator");
    let trash = operation_button("Trash", "Move the selected item to Trash as administrator");
    let permissions = operation_button(
        "Permissions",
        "Change the selected Unix mode as administrator",
    );
    let delete = operation_button(
        "Delete Permanently",
        "Permanently delete the selected item as administrator",
    );
    delete.add_css_class("destructive-action");
    for button in [
        &new_folder,
        &rename,
        &copy,
        &move_item,
        &trash,
        &permissions,
        &delete,
    ] {
        operations.append(button);
    }
    content.append(&operations);

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let status = gtk::Label::builder()
        .xalign(0.0)
        .hexpand(true)
        .wrap(true)
        .build();
    status.set_accessible_role(gtk::AccessibleRole::Status);
    let retry = gtk::Button::with_label("Retry");
    retry.set_visible(false);
    let continue_waiting = gtk::Button::with_label("Continue Waiting");
    continue_waiting.set_visible(false);
    continue_waiting.update_property(&[gtk::accessible::Property::Description(
        "Wait another 30 seconds for progress from the administrator backend",
    )]);
    let cancel = gtk::Button::with_label("Cancel");
    cancel.set_visible(false);
    footer.append(&status);
    footer.append(&retry);
    footer.append(&continue_waiting);
    footer.append(&cancel);
    content.append(&footer);
    toolbar.set_content(Some(&content));
    dialog.set_child(Some(&toolbar));

    PrivilegedViewWidgets {
        dialog,
        badge,
        location,
        status,
        list,
        back,
        forward,
        parent,
        cancel,
        continue_waiting,
        retry,
        return_standard,
        new_folder,
        rename,
        copy,
        move_item,
        trash,
        delete,
        permissions,
        selection,
        model,
    }
}

fn icon_button(icon_name: &str, accessible_label: &str) -> gtk::Button {
    let button = gtk::Button::builder().icon_name(icon_name).build();
    button.add_css_class("flat");
    button.set_tooltip_text(Some(accessible_label));
    button.update_property(&[gtk::accessible::Property::Label(accessible_label)]);
    button
}

fn operation_button(label: &str, description: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.set_sensitive(false);
    button.set_tooltip_text(Some(description));
    button.update_property(&[
        gtk::accessible::Property::Label(label),
        gtk::accessible::Property::Description(description),
    ]);
    button
}

pub struct PrivilegedAccessController {
    pub widgets: PrivilegedViewWidgets,
    provider: GioPrivilegedProvider,
    operations: GioPrivilegedOperationService,
    parent_window: glib::WeakRef<gtk::Window>,
    session: RefCell<PrivilegedSession>,
    accepted_entries: RefCell<Vec<PrivilegedEntry>>,
    rollback_entries: RefCell<Option<Vec<PrivilegedEntry>>>,
    next_operation_id: Cell<u64>,
    self_weak: Weak<PrivilegedAccessController>,
    operation_watchdog: RefCell<PrivilegedOperationWatchdog>,
    operation_watchdog_source: RefCell<Option<glib::SourceId>>,
}

impl PrivilegedAccessController {
    pub fn new() -> Rc<Self> {
        let controller = Rc::new_cyclic(|weak: &std::rc::Weak<Self>| {
            let callback_weak = weak.clone();
            let callback = Rc::new(move |event| {
                if let Some(controller) = callback_weak.upgrade() {
                    controller.handle_event(event);
                }
            });
            let operation_weak = weak.clone();
            let operation_callback = Rc::new(move |event| {
                if let Some(controller) = operation_weak.upgrade() {
                    controller.handle_operation_event(event);
                }
            });
            Self {
                widgets: build_view(),
                provider: GioPrivilegedProvider::new(callback),
                operations: GioPrivilegedOperationService::new(operation_callback),
                parent_window: glib::WeakRef::new(),
                session: RefCell::new(PrivilegedSession::default()),
                accepted_entries: RefCell::new(Vec::new()),
                rollback_entries: RefCell::new(None),
                next_operation_id: Cell::new(1),
                self_weak: weak.clone(),
                operation_watchdog: RefCell::new(PrivilegedOperationWatchdog::default()),
                operation_watchdog_source: RefCell::new(None),
            }
        });
        controller.install_callbacks();
        controller.install_operation_callbacks();
        controller
    }

    pub fn present<P>(&self, parent: &P, local_path: &Path)
    where
        P: IsA<gtk::Window> + IsA<gtk::Widget>,
    {
        self.parent_window
            .set(Some(parent.upcast_ref::<gtk::Window>()));
        if matches!(
            self.session.borrow().phase,
            SessionPhase::Authorizing | SessionPhase::Privileged
        ) {
            self.widgets.dialog.present(Some(parent));
            return;
        }
        let request = { self.session.borrow_mut().begin(local_path) };
        match request {
            Ok((generation, location)) => {
                self.prepare_request(&location);
                self.widgets.dialog.present(Some(parent));
                self.start_provider(generation, location);
            }
            Err(_) => {
                self.widgets
                    .status
                    .set_label(PrivilegedFailureKind::InvalidIdentity.user_message());
                self.widgets.retry.set_visible(false);
                self.widgets.dialog.present(Some(parent));
            }
        }
    }

    pub fn cancel_and_close(&self) {
        if self.operations.is_active() {
            self.operations.cancel();
            if !self.operation_watchdog.borrow().escape_allowed() {
                self.widgets.status.set_label(
                    "Cancellation requested; keep this Administrator view open until the backend confirms a terminal result.",
                );
                return;
            }
        }
        self.provider.cancel();
        self.session.borrow_mut().leave();
        self.accepted_entries.borrow_mut().clear();
        self.rollback_entries.borrow_mut().take();
        self.widgets.model.remove_all();
        self.widgets.dialog.close();
    }

    fn install_callbacks(self: &Rc<Self>) {
        let controller = Rc::downgrade(self);
        self.widgets.dialog.connect_closed(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.provider.cancel();
                controller.session.borrow_mut().leave();
                controller.accepted_entries.borrow_mut().clear();
                controller.rollback_entries.borrow_mut().take();
                controller.widgets.model.remove_all();
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.return_standard.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.cancel_and_close();
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.cancel.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                if controller.operations.is_active() {
                    controller.operations.cancel();
                    controller.widgets.continue_waiting.set_visible(false);
                    if controller.operation_watchdog.borrow().escape_allowed() {
                        controller.cancel_and_close();
                        return;
                    }
                    controller
                        .widgets
                        .status
                        .set_label("Cancellation requested; waiting for administrator backend…");
                } else {
                    controller.provider.cancel();
                    controller
                        .widgets
                        .status
                        .set_label("Cancellation requested…");
                }
                controller.widgets.cancel.set_sensitive(false);
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.continue_waiting.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.continue_waiting_for_operation();
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.retry.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.retry();
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.back.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.navigate_history(|session| session.go_back());
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.forward.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.navigate_history(|session| session.go_forward());
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.parent.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.navigate_history(|session| session.go_parent());
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.list.connect_activate(move |_, position| {
            let Some(controller) = controller.upgrade() else {
                return;
            };
            let Some(entry) = controller
                .widgets
                .model
                .item(position)
                .and_downcast::<glib::BoxedAnyObject>()
            else {
                return;
            };
            let entry = entry.borrow::<PrivilegedEntry>();
            if entry.kind() == PrivilegedEntryKind::Directory {
                let destination = entry.resource().clone();
                drop(entry);
                let request = controller.session.borrow_mut().navigate(destination);
                if let Some((generation, location)) = request {
                    controller.start_request(generation, location);
                }
            } else {
                controller
                    .widgets
                    .status
                    .set_label("Only folders can be opened; use the explicit administrator controls for file operations.");
            }
        });
    }

    fn install_operation_callbacks(self: &Rc<Self>) {
        let controller = Rc::downgrade(self);
        self.widgets.selection.connect_selected_notify(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.update_operation_controls();
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.new_folder.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.prompt_new_folder();
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.rename.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.prompt_rename();
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.copy.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.prompt_transfer(PrivilegedOperationKind::Copy);
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.move_item.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.prompt_transfer(PrivilegedOperationKind::Move);
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.trash.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.confirm_selected(PrivilegedOperationKind::Trash);
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.delete.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.confirm_selected(PrivilegedOperationKind::DeletePermanently);
            }
        });
        let controller = Rc::downgrade(self);
        self.widgets.permissions.connect_clicked(move |_| {
            if let Some(controller) = controller.upgrade() {
                controller.prompt_permissions();
            }
        });
    }

    fn selected_entry(&self) -> Option<PrivilegedEntry> {
        self.widgets
            .selection
            .selected_item()
            .and_downcast::<glib::BoxedAnyObject>()
            .map(|item| item.borrow::<PrivilegedEntry>().clone())
    }

    fn allocate_operation_id(&self) -> u64 {
        let id = self.next_operation_id.get().max(1);
        self.next_operation_id.set(id.wrapping_add(1).max(1));
        id
    }

    fn prompt_new_folder(self: &Rc<Self>) {
        let Some(parent) = self.session.borrow().current().cloned() else {
            return;
        };
        self.present_text_request(
            "Create Folder as Administrator",
            "Enter one folder name. Floe will not overwrite an existing item.",
            "",
            "Create Folder",
            false,
            Rc::new(move |_controller, id, text| {
                PrivilegedOperationRequest::create_directory(id, &parent, OsStr::new(text))
                    .map_err(|error| error.to_string())
            }),
        );
    }

    fn prompt_rename(self: &Rc<Self>) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let initial = entry.exact_name().to_str().unwrap_or("").to_owned();
        let body = if initial.is_empty() {
            "This filename is not valid UTF-8, so it cannot be prefilled. Enter one new name; the original raw identity remains authoritative."
        } else {
            "Enter one new name. Floe will fail if that name already exists."
        };
        self.present_text_request(
            "Rename as Administrator",
            body,
            &initial,
            "Rename",
            false,
            Rc::new(move |_controller, id, text| {
                PrivilegedOperationRequest::rename(id, &entry, OsStr::new(text))
                    .map_err(|error| error.to_string())
            }),
        );
    }

    fn prompt_transfer(self: &Rc<Self>, kind: PrivilegedOperationKind) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let (heading, action) = if kind == PrivilegedOperationKind::Copy {
            ("Copy as Administrator", "Copy")
        } else {
            ("Move as Administrator", "Move")
        };
        self.present_text_request(
            heading,
            "Enter the complete absolute destination path, including the new filename. Existing destinations are never overwritten. Folder copy remains unavailable until recursive policy is verified.",
            "",
            action,
            false,
            Rc::new(move |_controller, id, text| {
                PrivilegedOperationRequest::transfer(id, kind, &entry, Path::new(text))
                    .map_err(|error| error.to_string())
            }),
        );
    }

    fn prompt_permissions(self: &Rc<Self>) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        self.present_text_request(
            "Change Permissions as Administrator",
            "Enter an octal Unix mode from 0000 to 7777. This changes mode bits only; ownership, ACLs, xattrs, capabilities, and immutable flags are not changed.",
            "",
            "Apply Mode",
            false,
            Rc::new(move |_controller, id, text| {
                let mode = u32::from_str_radix(text.trim_start_matches("0o"), 8)
                    .map_err(|_| "Enter an octal mode such as 0755".to_owned())?;
                PrivilegedOperationRequest::set_permissions(id, &entry, mode)
                    .map_err(|error| error.to_string())
            }),
        );
    }

    fn present_text_request(
        self: &Rc<Self>,
        heading: &str,
        body: &str,
        initial: &str,
        action_label: &str,
        destructive: bool,
        build: Rc<OperationRequestBuilder>,
    ) {
        if self.operations.is_active() {
            return;
        }
        let entry = gtk::Entry::builder()
            .text(initial)
            .max_length(4_096)
            .activates_default(true)
            .build();
        entry.update_property(&[gtk::accessible::Property::Label(
            "Administrator operation value",
        )]);
        let body = format!(
            "{body}\n\nAdministrator changes are not yet stored in durable Undo/Redo history because the desktop service does not return enough exact post-operation identity evidence."
        );
        let dialog = adw::AlertDialog::builder()
            .heading(heading)
            .body(body)
            .extra_child(&entry)
            .default_response("apply")
            .close_response("cancel")
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("apply", action_label);
        dialog.set_response_appearance(
            "apply",
            if destructive {
                adw::ResponseAppearance::Destructive
            } else {
                adw::ResponseAppearance::Suggested
            },
        );
        let controller = Rc::downgrade(self);
        let entry_for_response = entry.clone();
        dialog.connect_response(None, move |_, response| {
            if response != "apply" {
                return;
            }
            let Some(controller) = controller.upgrade() else {
                return;
            };
            let id = controller.allocate_operation_id();
            match build(&controller, id, entry_for_response.text().as_str()) {
                Ok(request) => controller.submit_operation(request),
                Err(error) => controller.widgets.status.set_label(&error),
            }
        });
        if let Some(parent) = self.parent_window.upgrade() {
            dialog.present(Some(&parent));
            entry.grab_focus();
        }
    }

    fn confirm_selected(self: &Rc<Self>, kind: PrivilegedOperationKind) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let (heading, body, action) = match kind {
            PrivilegedOperationKind::Trash => (
                "Move to Trash as Administrator?",
                "The desktop administrator backend must support Trash. Floe will not fall back to permanent deletion.",
                "Move to Trash",
            ),
            PrivilegedOperationKind::DeletePermanently => (
                "Delete Permanently as Administrator?",
                "This cannot be undone and is not secure erase. Non-empty folders are refused by the backend; symbolic links are not followed.",
                "Delete Permanently",
            ),
            _ => return,
        };
        let dialog = adw::AlertDialog::builder()
            .heading(heading)
            .body(format!(
                "{}\n\nSelected item: {}\n\nDurable Undo/Redo unavailable: {}.",
                body,
                entry.display_name(),
                administrator_durable_undo_unavailable(kind)
                    .unwrap_or("the desktop service returned insufficient inverse evidence")
            ))
            .default_response("cancel")
            .close_response("cancel")
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("apply", action);
        dialog.set_response_appearance(
            "apply",
            if kind.irreversible() {
                adw::ResponseAppearance::Destructive
            } else {
                adw::ResponseAppearance::Suggested
            },
        );
        let controller = Rc::downgrade(self);
        dialog.connect_response(None, move |_, response| {
            if response != "apply" {
                return;
            }
            let Some(controller) = controller.upgrade() else {
                return;
            };
            let id = controller.allocate_operation_id();
            match PrivilegedOperationRequest::selected(id, kind, &entry) {
                Ok(request) => controller.submit_operation(request),
                Err(error) => controller.widgets.status.set_label(&error.to_string()),
            }
        });
        if let Some(parent) = self.parent_window.upgrade() {
            dialog.present(Some(&parent));
        }
    }

    fn cancel_operation_watchdog_source(&self) {
        if let Some(source) = self.operation_watchdog_source.borrow_mut().take() {
            source.remove();
        }
    }

    fn schedule_operation_watchdog(&self, operation_id: u64, generation: u64) {
        self.cancel_operation_watchdog_source();
        let controller = self.self_weak.clone();
        let source = glib::timeout_add_local_once(OPERATION_NO_PROGRESS_TIMEOUT, move || {
            let Some(controller) = controller.upgrade() else {
                return;
            };
            controller.operation_watchdog_source.borrow_mut().take();
            if !controller
                .operation_watchdog
                .borrow_mut()
                .expire(operation_id, generation)
            {
                return;
            }
            controller.widgets.status.set_label(
                "Still waiting… No administrator progress for 30 seconds. Continue Waiting or Cancel.",
            );
            controller.widgets.continue_waiting.set_visible(true);
            controller.widgets.cancel.set_visible(true);
            controller.widgets.cancel.set_sensitive(true);
        });
        self.operation_watchdog_source.replace(Some(source));
    }

    fn arm_operation_watchdog(&self, operation_id: u64) {
        let generation = self.operation_watchdog.borrow_mut().arm(operation_id);
        self.schedule_operation_watchdog(operation_id, generation);
    }

    fn record_operation_progress(&self, operation_id: u64) -> bool {
        let Some(generation) = self.operation_watchdog.borrow_mut().progress(operation_id) else {
            return false;
        };
        self.schedule_operation_watchdog(operation_id, generation);
        self.widgets.continue_waiting.set_visible(false);
        true
    }

    fn continue_waiting_for_operation(&self) {
        let Some((operation_id, generation)) =
            self.operation_watchdog.borrow_mut().continue_waiting()
        else {
            return;
        };
        self.widgets.continue_waiting.set_visible(false);
        self.widgets.cancel.set_sensitive(true);
        self.widgets.status.set_label(&format!(
            "Still waiting for administrator operation {operation_id}…"
        ));
        self.schedule_operation_watchdog(operation_id, generation);
    }

    fn submit_operation(&self, request: PrivilegedOperationRequest) {
        let parent = self.parent_window.upgrade();
        let mount_operation = gtk::MountOperation::new(parent.as_ref());
        self.widgets.dialog.set_can_close(false);
        self.widgets.return_standard.set_sensitive(false);
        self.widgets.cancel.set_visible(true);
        self.widgets.cancel.set_sensitive(true);
        self.update_operation_controls();
        if let Err(failure) = self.operations.submit(request, mount_operation.upcast()) {
            self.widgets.dialog.set_can_close(true);
            self.widgets.return_standard.set_sensitive(true);
            self.widgets.cancel.set_visible(false);
            self.widgets.status.set_label(failure.user_message());
            self.update_operation_controls();
        }
    }

    fn handle_operation_event(&self, event: PrivilegedOperationEvent) {
        match event {
            PrivilegedOperationEvent::Started { id, kind } => {
                self.arm_operation_watchdog(id);
                self.widgets.status.set_label(&format!(
                    "{} as Administrator… Operation {id}",
                    kind.label()
                ));
            }
            PrivilegedOperationEvent::Progress { id, current, total } => {
                if !self.record_operation_progress(id) {
                    return;
                }
                self.widgets.status.set_label(&match total {
                    Some(total) => {
                        format!("Administrator operation {id}: {current} of {total} bytes")
                    }
                    None => format!("Administrator operation {id}: {current} bytes"),
                });
            }
            PrivilegedOperationEvent::CancellationRequested { id } => {
                self.cancel_operation_watchdog_source();
                let escape_allowed = self
                    .operation_watchdog
                    .borrow_mut()
                    .cancellation_requested(id);
                self.widgets.continue_waiting.set_visible(false);
                if escape_allowed {
                    self.widgets.dialog.set_can_close(true);
                    self.widgets.return_standard.set_sensitive(true);
                }
                self.widgets.status.set_label(
                    &format!("Cancellation requested for operation {id}; it is not cancelled until the backend confirms it."),
                );
            }
            PrivilegedOperationEvent::Completed {
                id,
                kind,
                affected_parent,
            } => {
                self.cancel_operation_watchdog_source();
                self.operation_watchdog.borrow_mut().finish(id);
                self.finish_operation_surface();
                let refreshes_current = self.session.borrow().current() == Some(&affected_parent);
                self.widgets.status.set_label(&format!(
                    "{} completed (operation {id}).{}",
                    kind.label(),
                    if refreshes_current {
                        " Refreshing…"
                    } else {
                        ""
                    }
                ));
                self.retry();
            }
            PrivilegedOperationEvent::Failed {
                id,
                kind,
                failure,
                destination_may_exist,
            } => {
                self.cancel_operation_watchdog_source();
                self.operation_watchdog.borrow_mut().finish(id);
                self.finish_operation_surface();
                let suffix = if destination_may_exist {
                    " A partial destination may remain; Floe did not remove it automatically."
                } else {
                    ""
                };
                self.widgets.status.set_label(&format!(
                    "{} failed (operation {id}): {}{suffix}",
                    kind.label(),
                    failure.user_message()
                ));
            }
        }
        self.update_operation_controls();
    }

    fn finish_operation_surface(&self) {
        self.widgets.dialog.set_can_close(true);
        self.widgets.return_standard.set_sensitive(true);
        self.widgets.cancel.set_visible(false);
        self.widgets.cancel.set_sensitive(true);
        self.widgets.continue_waiting.set_visible(false);
    }

    fn update_operation_controls(&self) {
        let privileged = self.session.borrow().phase == SessionPhase::Privileged;
        let available = privileged && !self.operations.is_active();
        let selected = available && self.selected_entry().is_some();
        self.widgets.new_folder.set_sensitive(available);
        for button in [
            &self.widgets.rename,
            &self.widgets.copy,
            &self.widgets.move_item,
            &self.widgets.trash,
            &self.widgets.permissions,
            &self.widgets.delete,
        ] {
            button.set_sensitive(selected);
        }
    }

    fn navigate_history(
        &self,
        change: impl FnOnce(&mut PrivilegedSession) -> Option<(u64, PrivilegedResourceId)>,
    ) {
        let request = change(&mut self.session.borrow_mut());
        if let Some((generation, location)) = request {
            self.start_request(generation, location);
        }
    }

    fn retry(&self) {
        let request = {
            let mut session = self.session.borrow_mut();
            let Some(location) = session.current().cloned() else {
                return;
            };
            session
                .start_current()
                .ok()
                .map(|generation| (generation, location))
        };
        if let Some((generation, location)) = request {
            self.start_request(generation, location);
        }
    }

    fn start_request(&self, generation: u64, location: PrivilegedResourceId) {
        self.prepare_request(&location);
        self.start_provider(generation, location);
    }

    fn start_provider(&self, generation: u64, location: PrivilegedResourceId) {
        let parent = self.parent_window.upgrade();
        let mount_operation = gtk::MountOperation::new(parent.as_ref());
        self.provider
            .start(generation, location, mount_operation.upcast());
    }

    fn prepare_request(&self, location: &PrivilegedResourceId) {
        debug_assert_eq!(location.access_level(), AccessLevel::Administrator);
        debug_assert_eq!(self.session.borrow().access_level(), AccessLevel::Standard);
        if self.session.borrow().rollback.is_some() {
            self.rollback_entries
                .replace(Some(self.accepted_entries.borrow().clone()));
        } else {
            self.rollback_entries.borrow_mut().take();
        }
        self.accepted_entries.borrow_mut().clear();
        self.widgets.model.remove_all();
        self.widgets.badge.set_visible(false);
        self.widgets
            .location
            .set_label(&location.local_path().to_string_lossy());
        self.widgets
            .location
            .set_tooltip_text(Some(&location.local_path().to_string_lossy()));
        self.widgets
            .status
            .set_label("Waiting for desktop administrator authorization…");
        self.widgets.cancel.set_sensitive(true);
        self.widgets.cancel.set_visible(true);
        self.widgets.retry.set_visible(false);
        self.update_navigation_buttons();
        self.update_operation_controls();
    }

    fn handle_event(&self, event: PrivilegedProviderEvent) {
        if event.generation() != self.session.borrow().active_generation {
            return;
        }
        match event {
            PrivilegedProviderEvent::Page {
                generation,
                location,
                entries,
            } => {
                if self.session.borrow().current() != Some(&location)
                    || !self.session.borrow_mut().accept_page(generation)
                {
                    return;
                }
                self.rollback_entries.borrow_mut().take();
                self.widgets.cancel.set_visible(false);
                self.widgets.badge.set_visible(true);
                for entry in entries {
                    self.accepted_entries.borrow_mut().push(entry.clone());
                    self.widgets.model.append(&glib::BoxedAnyObject::new(entry));
                }
                self.widgets.status.set_label(&format!(
                    "Administrator view — {}",
                    item_count_text(self.accepted_entries.borrow().len())
                ));
                self.update_navigation_buttons();
            }
            PrivilegedProviderEvent::Finished {
                generation: _,
                location,
                truncated,
            } => {
                if self.session.borrow().current() != Some(&location) {
                    return;
                }
                self.widgets.cancel.set_visible(false);
                self.widgets.status.set_label(if truncated {
                    "Showing the first 4,096 entries. Refine this folder outside administrator mode."
                } else if self.accepted_entries.borrow().is_empty() {
                        "Administrator view — Empty folder"
                    } else {
                        "Administrator view — use only the explicit controls below for elevated operations"
                });
            }
            PrivilegedProviderEvent::Failed {
                generation,
                location,
                kind,
            } => {
                if self.session.borrow().current() != Some(&location) {
                    return;
                }
                let restoring_previous = self.rollback_entries.borrow().is_some();
                if !self.session.borrow_mut().accept_failure(generation, kind) {
                    return;
                }
                let retains_privileged_view =
                    self.session.borrow().phase == SessionPhase::Privileged;
                self.widgets.cancel.set_visible(false);

                if retains_privileged_view {
                    if let Some(entries) = self.rollback_entries.borrow_mut().take() {
                        self.accepted_entries.replace(entries.clone());
                        self.widgets.model.remove_all();
                        for entry in entries {
                            self.widgets.model.append(&glib::BoxedAnyObject::new(entry));
                        }
                    }

                    if let Some(location) = self.session.borrow().current() {
                        let display = location.local_path().to_string_lossy();
                        self.widgets.location.set_label(&display);
                        self.widgets.location.set_tooltip_text(Some(&display));
                    }
                    self.widgets.badge.set_visible(true);
                    self.widgets
                        .retry
                        .set_visible(!restoring_previous && failure_can_retry(kind));
                    self.widgets.status.set_label(&format!(
                        "{} {}",
                        if restoring_previous {
                            "Previous administrator folder restored."
                        } else {
                            "Administrator results are incomplete."
                        },
                        kind.user_message()
                    ));
                } else {
                    self.rollback_entries.borrow_mut().take();
                    self.widgets.badge.set_visible(false);
                    self.widgets.retry.set_visible(failure_can_retry(kind));
                    self.widgets.status.set_label(kind.user_message());
                }
                self.update_navigation_buttons();
            }
        }
        self.update_operation_controls();
    }

    fn update_navigation_buttons(&self) {
        let session = self.session.borrow();
        let navigation = session.navigation.as_ref();
        self.widgets
            .back
            .set_sensitive(navigation.is_some_and(|navigation| !navigation.back.is_empty()));
        self.widgets
            .forward
            .set_sensitive(navigation.is_some_and(|navigation| !navigation.forward.is_empty()));
        self.widgets.parent.set_sensitive(
            navigation.is_some_and(|navigation| navigation.current.parent().is_some()),
        );
    }
}

impl Drop for PrivilegedAccessController {
    fn drop(&mut self) {
        self.provider.cancel();
    }
}

fn item_count_text(count: usize) -> String {
    if count == 1 {
        "1 item".to_owned()
    } else {
        format!("{count} items")
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    use super::*;

    #[test]
    fn phase_14b_identity_preserves_non_utf8_and_rejects_relative_input() {
        let raw = PathBuf::from("/tmp").join(OsString::from_vec(vec![b'a', 0x80, b'b']));
        let identity = PrivilegedResourceId::from_local_path(&raw).expect("raw absolute path");
        assert_eq!(identity.local_path(), raw);
        let parsed = glib::Uri::parse(identity.admin_uri(), glib::UriFlags::ENCODED)
            .expect("canonical administrator URI");
        assert_eq!(parsed.scheme(), "admin");
        assert!(parsed.path().starts_with("/tmp/"));
        assert!(identity.admin_uri().contains("%80"));
        assert_eq!(
            PrivilegedResourceId::from_local_path(Path::new("relative")),
            Err(PrivilegedIdentityError::RelativePath)
        );
    }

    #[test]
    fn phase_14b_identity_keeps_colliding_lossy_names_distinct() {
        let first = PathBuf::from("/tmp").join(OsString::from_vec(vec![0x80]));
        let second = PathBuf::from("/tmp").join(OsString::from_vec(vec![0x81]));
        assert_eq!(first.to_string_lossy(), second.to_string_lossy());
        let first = PrivilegedResourceId::from_local_path(&first).expect("first identity");
        let second = PrivilegedResourceId::from_local_path(&second).expect("second identity");
        assert_ne!(first, second);
        assert_ne!(first.admin_uri(), second.admin_uri());
    }

    #[test]
    fn phase_14b_identity_rejects_untrusted_uri_components() {
        for uri in [
            "admin://host/tmp",
            "admin://user@/tmp",
            "admin:///tmp?query=1",
            "admin:///tmp#fragment",
            "sftp:///tmp",
            "file://host/tmp",
        ] {
            assert_eq!(
                validate_admin_uri(uri, Path::new("/tmp")),
                Err(PrivilegedIdentityError::InvalidAdministratorUri),
                "accepted hostile URI: {uri}"
            );
        }
    }

    #[test]
    fn phase_14b_state_rejects_stale_events_and_restores_failed_navigation() {
        let mut session = PrivilegedSession::default();
        let (first_generation, root) = session.begin(Path::new("/tmp")).expect("initial");
        assert!(session.accept_page(first_generation));
        assert_eq!(session.phase, SessionPhase::Privileged);

        let child =
            PrivilegedResourceId::from_local_path(Path::new("/tmp/child")).expect("child identity");
        let (child_generation, _) = session.navigate(child).expect("navigate child");
        assert!(!session.accept_page(first_generation));
        assert!(session.accept_failure(child_generation, PrivilegedFailureKind::Denied));
        assert_eq!(session.current(), Some(&root));
        assert_eq!(session.phase, SessionPhase::Privileged);
    }

    #[test]
    fn phase_14b_state_accepts_multiple_bounded_pages_for_only_active_generation() {
        let mut session = PrivilegedSession::default();
        let (generation, _) = session.begin(Path::new("/tmp")).expect("initial");
        assert!(session.accept_page(generation));
        assert!(session.accept_page(generation));
        assert!(session.accept_failure(generation, PrivilegedFailureKind::Backend));
        assert_eq!(session.phase, SessionPhase::Privileged);
        assert!(!session.accept_page(generation.wrapping_add(1)));
        assert_eq!(session.phase, SessionPhase::Privileged);
    }

    #[test]
    fn phase_14b_state_history_keeps_administrator_identity_and_exact_return() {
        let mut session = PrivilegedSession::default();
        let (generation, _) = session.begin(Path::new("/var/lib")).expect("initial");
        assert!(session.accept_page(generation));
        let child = PrivilegedResourceId::from_local_path(Path::new("/var/lib/floe"))
            .expect("child identity");
        let (generation, _) = session.navigate(child.clone()).expect("child navigation");
        assert!(session.accept_page(generation));
        let (generation, parent) = session.go_back().expect("back");
        assert_eq!(parent.local_path(), Path::new("/var/lib"));
        assert!(session.accept_page(generation));
        let (generation, forward) = session.go_forward().expect("forward");
        assert_eq!(forward, child);
        assert!(session.accept_page(generation));
        assert_eq!(session.leave(), Some(PathBuf::from("/var/lib")));
        assert_eq!(session.phase, SessionPhase::Standard);
    }

    #[test]
    fn phase_14b_provider_classifies_failures_without_exposing_backend_text() {
        for (error, expected) in [
            (
                glib::Error::new(gio::IOErrorEnum::Cancelled, "secret admin URI"),
                PrivilegedFailureKind::Cancelled,
            ),
            (
                glib::Error::new(gio::IOErrorEnum::PermissionDenied, "secret path"),
                PrivilegedFailureKind::Denied,
            ),
            (
                glib::Error::new(gio::IOErrorEnum::NotSupported, "details"),
                PrivilegedFailureKind::Unsupported,
            ),
        ] {
            let kind = classify_gio_error(&error, false);
            assert_eq!(kind, expected);
            assert!(!kind.user_message().contains("secret"));
            assert!(!kind.user_message().contains("admin://"));
        }
        assert_eq!(
            classify_gio_error(
                &glib::Error::new(gio::IOErrorEnum::Cancelled, "timeout"),
                true
            ),
            PrivilegedFailureKind::TimedOut
        );
    }

    #[test]
    fn phase_14b_provider_mounts_once_for_a_fresh_not_mounted_location() {
        let not_mounted = glib::Error::new(
            gio::IOErrorEnum::NotMounted,
            "administrator location is not mounted",
        );

        assert_eq!(
            enumeration_failure_action(&not_mounted, false, false),
            EnumerationFailureAction::Mount
        );
        assert_eq!(
            enumeration_failure_action(&not_mounted, false, true),
            EnumerationFailureAction::Fail(PrivilegedFailureKind::Backend)
        );
        assert_eq!(
            enumeration_failure_action(&not_mounted, true, false),
            EnumerationFailureAction::Fail(PrivilegedFailureKind::TimedOut)
        );
    }

    #[test]
    fn phase_14b_provider_accepts_already_mounted_but_preserves_mount_failures() {
        assert_eq!(
            mount_failure_kind(
                &glib::Error::new(gio::IOErrorEnum::AlreadyMounted, "already mounted"),
                false
            ),
            None
        );
        assert_eq!(
            mount_failure_kind(
                &glib::Error::new(gio::IOErrorEnum::PermissionDenied, "denied"),
                false
            ),
            Some(PrivilegedFailureKind::Denied)
        );
        assert_eq!(
            mount_failure_kind(
                &glib::Error::new(gio::IOErrorEnum::Cancelled, "cancelled"),
                true
            ),
            Some(PrivilegedFailureKind::TimedOut)
        );
    }

    #[test]
    fn phase_14b_state_fake_provider_covers_terminal_outcomes_without_io() {
        let outcomes = [
            PrivilegedFailureKind::Denied,
            PrivilegedFailureKind::NoAuthenticationAgent,
            PrivilegedFailureKind::Unsupported,
            PrivilegedFailureKind::Unavailable,
            PrivilegedFailureKind::Cancelled,
            PrivilegedFailureKind::TimedOut,
            PrivilegedFailureKind::Backend,
        ];
        for outcome in outcomes {
            let mut session = PrivilegedSession::default();
            let (generation, _) = session.begin(Path::new("/tmp")).expect("fake request");
            assert!(session.accept_failure(generation, outcome));
            assert_eq!(session.phase, SessionPhase::Failed(outcome));
            assert_eq!(session.leave(), Some(PathBuf::from("/tmp")));
        }

        let mut success = PrivilegedSession::default();
        let (generation, _) = success.begin(Path::new("/tmp")).expect("fake success");
        assert!(success.accept_page(generation));
        assert_eq!(success.phase, SessionPhase::Privileged);
    }

    #[test]
    fn phase_14b_retry_policy_excludes_terminal_and_identity_failures() {
        for kind in [
            PrivilegedFailureKind::Denied,
            PrivilegedFailureKind::NoAuthenticationAgent,
            PrivilegedFailureKind::Unavailable,
            PrivilegedFailureKind::TimedOut,
            PrivilegedFailureKind::Backend,
        ] {
            assert!(failure_can_retry(kind), "{kind:?} should be retryable");
        }

        for kind in [
            PrivilegedFailureKind::Cancelled,
            PrivilegedFailureKind::Unsupported,
            PrivilegedFailureKind::InvalidIdentity,
        ] {
            assert!(!failure_can_retry(kind), "{kind:?} should be terminal");
        }
    }

    #[test]
    fn phase_14b_provider_bounds_pages_and_tracks_one_active_request() {
        assert_eq!(ENUMERATION_PAGE_SIZE, 128);
        assert_eq!(ENUMERATION_ENTRY_CAPACITY, 4_096);
        let provider = GioPrivilegedProvider::new(Rc::new(|_| {}));
        assert_eq!(provider.active_generation(), None);
    }

    #[test]
    fn phase_14c_ui_keeps_privileged_boundary_free_of_shell_and_local_jobs() {
        let source = include_str!("privileged_access.rs");
        let implementation = source
            .split("#[cfg(all(test, unix))]")
            .next()
            .expect("implementation section");
        for forbidden in ["pkexec", "sudo", "sh -c", "Command::new", "set_attribute"] {
            assert!(
                !implementation.contains(forbidden),
                "forbidden privileged boundary: {forbidden}"
            );
        }
        assert!(implementation.contains("NOFOLLOW_SYMLINKS"));
        assert!(implementation.contains("mount_enclosing_volume"));
        assert!(implementation.contains("gtk::MountOperation::new"));
        assert!(implementation.contains("Administrator File Operations"));
    }

    #[test]
    fn phase_14c_ui_exposes_only_explicit_confirmed_administrator_operations() {
        let source = include_str!("privileged_access.rs");
        let implementation = source
            .split("#[cfg(all(test, unix))]")
            .next()
            .expect("implementation section");
        for label in [
            "New Folder",
            "Rename",
            "Copy To…",
            "Move To…",
            "Trash",
            "Permissions",
            "Delete Permanently",
        ] {
            assert!(implementation.contains(label), "missing operation: {label}");
        }
        assert!(implementation.contains("set_can_close(false)"));
        assert!(
            implementation.contains("fresh visible confirmation")
                || implementation.contains("AlertDialog")
        );
        assert!(implementation.contains("will not overwrite"));
        assert!(implementation.contains("will not fall back to permanent deletion"));
    }

    fn assert_failed_child_restores_visible_parent() {
        let controller = PrivilegedAccessController::new();
        let (parent_generation, parent) = controller
            .session
            .borrow_mut()
            .begin(Path::new("/tmp"))
            .expect("parent request");
        controller.prepare_request(&parent);

        let prior_entry = PrivilegedEntry {
            resource: PrivilegedResourceId::from_local_path(Path::new("/tmp/visible.txt"))
                .expect("prior entry identity"),
            exact_name: OsString::from("visible.txt"),
            display_name: "visible.txt".to_owned(),
            kind: PrivilegedEntryKind::File,
            size: Some(7),
            hidden: false,
            modified: None,
            device: None,
            inode: None,
        };
        controller.handle_event(PrivilegedProviderEvent::Page {
            generation: parent_generation,
            location: parent.clone(),
            entries: vec![prior_entry.clone()],
        });

        let child = PrivilegedResourceId::from_local_path(Path::new("/tmp/protected"))
            .expect("child identity");
        let (child_generation, _) = controller
            .session
            .borrow_mut()
            .navigate(child.clone())
            .expect("child request");
        controller.prepare_request(&child);
        assert_eq!(controller.widgets.model.n_items(), 0);
        assert_eq!(
            controller.widgets.location.label().as_str(),
            "/tmp/protected"
        );

        controller.handle_event(PrivilegedProviderEvent::Failed {
            generation: child_generation,
            location: child.clone(),
            kind: PrivilegedFailureKind::Denied,
        });

        assert_eq!(controller.session.borrow().current(), Some(&parent));
        assert_eq!(controller.session.borrow().phase, SessionPhase::Privileged);
        assert_eq!(controller.widgets.location.label().as_str(), "/tmp");
        assert_eq!(controller.widgets.model.n_items(), 1);
        assert!(controller.widgets.badge.is_visible());
        assert!(!controller.widgets.retry.is_visible());

        controller.handle_event(PrivilegedProviderEvent::Page {
            generation: child_generation,
            location: child,
            entries: Vec::new(),
        });
        assert_eq!(controller.widgets.location.label().as_str(), "/tmp");
        assert_eq!(controller.widgets.model.n_items(), 1);
        let restored = controller
            .widgets
            .model
            .item(0)
            .and_downcast::<glib::BoxedAnyObject>()
            .expect("restored entry");
        assert_eq!(&*restored.borrow::<PrivilegedEntry>(), &prior_entry);

        let partial_generation = controller
            .session
            .borrow_mut()
            .start_current()
            .expect("reload parent");
        controller.prepare_request(&parent);
        controller.handle_event(PrivilegedProviderEvent::Page {
            generation: partial_generation,
            location: parent.clone(),
            entries: vec![prior_entry],
        });
        controller.handle_event(PrivilegedProviderEvent::Failed {
            generation: partial_generation,
            location: parent,
            kind: PrivilegedFailureKind::Backend,
        });
        assert_eq!(controller.session.borrow().phase, SessionPhase::Privileged);
        assert!(controller.widgets.badge.is_visible());
        assert!(controller.widgets.retry.is_visible());
        assert_eq!(controller.widgets.model.n_items(), 1);
        assert!(controller.widgets.status.label().contains("incomplete"));
    }

    #[test]
    fn adversarial_privileged_watchdog() {
        assert_eq!(OPERATION_NO_PROGRESS_TIMEOUT, Duration::from_secs(30));
        let mut watchdog = PrivilegedOperationWatchdog::default();

        let started = watchdog.arm(41);
        let progress = watchdog.arm(41);
        assert!(
            !watchdog.expire(41, started),
            "stale timers must be ignored"
        );
        assert!(watchdog.expire(41, progress));
        assert_eq!(watchdog.phase, PrivilegedWatchdogPhase::NoProgress);

        let (operation_id, continued) = watchdog
            .continue_waiting()
            .expect("Continue Waiting must re-arm the bounded watchdog");
        assert_eq!(operation_id, 41);
        assert!(watchdog.expire(operation_id, continued));
        assert!(
            watchdog.cancellation_requested(operation_id),
            "Cancel from no-progress state must release the modal close guard"
        );
        assert_eq!(
            watchdog.phase,
            PrivilegedWatchdogPhase::CancellationRequested {
                escape_allowed: true
            }
        );
        assert!(watchdog.escape_allowed());
        assert_eq!(
            watchdog.progress(operation_id),
            None,
            "late progress must not re-lock a cancellation escape"
        );

        watchdog.finish(operation_id);
        assert_eq!(watchdog.phase, PrivilegedWatchdogPhase::Idle);
        assert_eq!(watchdog.operation_id, None);
    }

    #[test]
    #[ignore = "requires graphical GTK session; run documented GTK component gate"]
    fn phase_testing_gtk_phase_14c_ui_accessibility() {
        gtk::init().expect("GTK component gate requires a display");
        adw::init().expect("libadwaita component gate");
        assert_failed_child_restores_visible_parent();
        let widgets = build_view();
        assert_eq!(
            widgets.dialog.accessible_role(),
            gtk::AccessibleRole::Dialog
        );
        assert_eq!(widgets.badge.label().as_str(), "Administrator");
        assert_eq!(
            widgets.status.accessible_role(),
            gtk::AccessibleRole::Status
        );
        assert_eq!(widgets.list.accessible_role(), gtk::AccessibleRole::List);
        assert_eq!(
            widgets.return_standard.accessible_role(),
            gtk::AccessibleRole::Button
        );
        assert!(
            widgets
                .cancel
                .label()
                .is_some_and(|label| label == "Cancel")
        );
        assert!(
            widgets
                .continue_waiting
                .label()
                .is_some_and(|label| label == "Continue Waiting")
        );
        for (button, expected) in [
            (&widgets.new_folder, "New Folder"),
            (&widgets.rename, "Rename"),
            (&widgets.copy, "Copy To…"),
            (&widgets.move_item, "Move To…"),
            (&widgets.trash, "Trash"),
            (&widgets.permissions, "Permissions"),
            (&widgets.delete, "Delete Permanently"),
        ] {
            assert_eq!(button.accessible_role(), gtk::AccessibleRole::Button);
            assert_eq!(button.label().as_deref(), Some(expected));
            assert!(!button.is_sensitive());
        }
    }
}
