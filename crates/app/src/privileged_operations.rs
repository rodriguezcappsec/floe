//! Typed GIO/GVfs mutation boundary for administrator resources.
//!
//! Requests retain validated `admin://` identity and never enter ordinary local
//! `PathBuf` executors. GIO objects remain on their owning GLib main context.

use std::{
    cell::RefCell,
    ffi::OsStr,
    os::unix::ffi::OsStrExt,
    path::{Component, Path},
    rc::Rc,
};

use gtk::{gio, glib, prelude::*};
use thiserror::Error;

use crate::privileged_access::{PrivilegedEntry, PrivilegedEntryKind, PrivilegedResourceId};

const REVALIDATION_ATTRIBUTES: &str =
    "standard::type,standard::is-symlink,standard::size,time::modified,unix::device,unix::inode";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivilegedOperationKind {
    CreateDirectory,
    Rename,
    Copy,
    Move,
    Trash,
    DeletePermanently,
    SetPermissions,
}

impl PrivilegedOperationKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::CreateDirectory => "Create folder",
            Self::Rename => "Rename",
            Self::Copy => "Copy",
            Self::Move => "Move",
            Self::Trash => "Move to Trash",
            Self::DeletePermanently => "Delete permanently",
            Self::SetPermissions => "Change permissions",
        }
    }

    pub const fn irreversible(self) -> bool {
        matches!(self, Self::DeletePermanently)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivilegedFingerprint {
    kind: PrivilegedEntryKind,
    size: Option<u64>,
    modified: Option<u64>,
    device: Option<u64>,
    inode: Option<u64>,
}

impl PrivilegedFingerprint {
    pub fn from_entry(entry: &PrivilegedEntry) -> Self {
        Self {
            kind: entry.kind(),
            size: entry.size(),
            modified: entry.modified(),
            device: entry.device(),
            inode: entry.inode(),
        }
    }

    fn from_info(info: &gio::FileInfo) -> Self {
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
        let modified = info
            .has_attribute("time::modified")
            .then(|| info.attribute_uint64("time::modified"));
        let device = info
            .has_attribute("unix::device")
            .then(|| info.attribute_uint64("unix::device"));
        let inode = info
            .has_attribute("unix::inode")
            .then(|| info.attribute_uint64("unix::inode"));
        Self {
            kind,
            size,
            modified,
            device,
            inode,
        }
    }

    fn matches_observed(&self, observed: &Self) -> bool {
        self.kind == observed.kind
            && self.size == observed.size
            && optional_fact_matches(self.modified, observed.modified)
            && optional_fact_matches(self.device, observed.device)
            && optional_fact_matches(self.inode, observed.inode)
    }
}

fn optional_fact_matches(expected: Option<u64>, observed: Option<u64>) -> bool {
    expected.is_none() || observed.is_none() || expected == observed
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivilegedSubject {
    resource: PrivilegedResourceId,
    expected: PrivilegedFingerprint,
}

impl PrivilegedSubject {
    pub fn from_entry(entry: &PrivilegedEntry) -> Self {
        Self {
            resource: entry.resource().clone(),
            expected: PrivilegedFingerprint::from_entry(entry),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivilegedOperationRequest {
    id: u64,
    kind: PrivilegedOperationKind,
    source: Option<PrivilegedSubject>,
    destination: Option<PrivilegedResourceId>,
    parent: PrivilegedResourceId,
    mode: Option<u32>,
}

impl PrivilegedOperationRequest {
    pub fn create_directory(
        id: u64,
        parent: &PrivilegedResourceId,
        name: &OsStr,
    ) -> Result<Self, PrivilegedOperationValidationError> {
        validate_id(id)?;
        validate_name(name)?;
        let destination = child_resource(parent, name)?;
        Ok(Self {
            id,
            kind: PrivilegedOperationKind::CreateDirectory,
            source: None,
            destination: Some(destination),
            parent: parent.clone(),
            mode: None,
        })
    }

    pub fn rename(
        id: u64,
        source: &PrivilegedEntry,
        name: &OsStr,
    ) -> Result<Self, PrivilegedOperationValidationError> {
        validate_id(id)?;
        validate_mutable_entry(source)?;
        validate_name(name)?;
        let parent = source
            .resource()
            .parent()
            .ok_or(PrivilegedOperationValidationError::RootTarget)?;
        let destination = child_resource(&parent, name)?;
        if destination == *source.resource() {
            return Err(PrivilegedOperationValidationError::SameSourceDestination);
        }
        Ok(Self {
            id,
            kind: PrivilegedOperationKind::Rename,
            source: Some(PrivilegedSubject::from_entry(source)),
            destination: Some(destination),
            parent,
            mode: None,
        })
    }

    pub fn transfer(
        id: u64,
        kind: PrivilegedOperationKind,
        source: &PrivilegedEntry,
        destination: &Path,
    ) -> Result<Self, PrivilegedOperationValidationError> {
        validate_id(id)?;
        if !matches!(
            kind,
            PrivilegedOperationKind::Copy | PrivilegedOperationKind::Move
        ) {
            return Err(PrivilegedOperationValidationError::WrongConstructor);
        }
        validate_mutable_entry(source)?;
        if kind == PrivilegedOperationKind::Copy && source.kind() == PrivilegedEntryKind::Directory
        {
            return Err(PrivilegedOperationValidationError::RecursiveCopyUnsupported);
        }
        let destination = PrivilegedResourceId::from_local_path(destination)
            .map_err(|_| PrivilegedOperationValidationError::InvalidDestination)?;
        if destination.local_path().as_os_str().as_bytes().len() > 4_096 {
            return Err(PrivilegedOperationValidationError::InvalidDestination);
        }
        if destination == *source.resource() {
            return Err(PrivilegedOperationValidationError::SameSourceDestination);
        }
        let parent = destination
            .parent()
            .ok_or(PrivilegedOperationValidationError::InvalidDestination)?;
        Ok(Self {
            id,
            kind,
            source: Some(PrivilegedSubject::from_entry(source)),
            destination: Some(destination),
            parent,
            mode: None,
        })
    }

    pub fn selected(
        id: u64,
        kind: PrivilegedOperationKind,
        source: &PrivilegedEntry,
    ) -> Result<Self, PrivilegedOperationValidationError> {
        validate_id(id)?;
        if !matches!(
            kind,
            PrivilegedOperationKind::Trash | PrivilegedOperationKind::DeletePermanently
        ) {
            return Err(PrivilegedOperationValidationError::WrongConstructor);
        }
        validate_mutable_entry(source)?;
        let parent = source
            .resource()
            .parent()
            .ok_or(PrivilegedOperationValidationError::RootTarget)?;
        Ok(Self {
            id,
            kind,
            source: Some(PrivilegedSubject::from_entry(source)),
            destination: None,
            parent,
            mode: None,
        })
    }

    pub fn set_permissions(
        id: u64,
        source: &PrivilegedEntry,
        mode: u32,
    ) -> Result<Self, PrivilegedOperationValidationError> {
        validate_id(id)?;
        validate_mutable_entry(source)?;
        if mode > 0o7777 {
            return Err(PrivilegedOperationValidationError::InvalidMode);
        }
        let parent = source
            .resource()
            .parent()
            .ok_or(PrivilegedOperationValidationError::RootTarget)?;
        Ok(Self {
            id,
            kind: PrivilegedOperationKind::SetPermissions,
            source: Some(PrivilegedSubject::from_entry(source)),
            destination: None,
            parent,
            mode: Some(mode),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum PrivilegedOperationValidationError {
    #[error("administrator operation ID must be nonzero")]
    InvalidId,
    #[error("name must be one nonempty filesystem component")]
    InvalidName,
    #[error("administrator destination must be an exact absolute local path")]
    InvalidDestination,
    #[error("the filesystem root cannot be mutated from this surface")]
    RootTarget,
    #[error("source and destination must differ")]
    SameSourceDestination,
    #[error("this request used the wrong typed constructor")]
    WrongConstructor,
    #[error("administrator directory copy is not available until recursive policy is verified")]
    RecursiveCopyUnsupported,
    #[error("Unix mode must be between 0000 and 7777")]
    InvalidMode,
}

fn validate_id(id: u64) -> Result<(), PrivilegedOperationValidationError> {
    if id == 0 {
        Err(PrivilegedOperationValidationError::InvalidId)
    } else {
        Ok(())
    }
}

fn validate_name(name: &OsStr) -> Result<(), PrivilegedOperationValidationError> {
    let mut components = Path::new(name).components();
    if name.is_empty()
        || name.as_bytes().contains(&0)
        || name.as_bytes().len() > 255
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        Err(PrivilegedOperationValidationError::InvalidName)
    } else {
        Ok(())
    }
}

fn validate_mutable_entry(
    entry: &PrivilegedEntry,
) -> Result<(), PrivilegedOperationValidationError> {
    if entry.resource().parent().is_none() {
        Err(PrivilegedOperationValidationError::RootTarget)
    } else {
        Ok(())
    }
}

fn child_resource(
    parent: &PrivilegedResourceId,
    name: &OsStr,
) -> Result<PrivilegedResourceId, PrivilegedOperationValidationError> {
    PrivilegedResourceId::from_local_path(&parent.local_path().join(name))
        .map_err(|_| PrivilegedOperationValidationError::InvalidDestination)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivilegedOperationFailureKind {
    Busy,
    AuthorizationDenied,
    Cancelled,
    Changed,
    Conflict,
    Unsupported,
    Backend,
}

impl PrivilegedOperationFailureKind {
    pub const fn user_message(self) -> &'static str {
        match self {
            Self::Busy => "Another administrator operation is still active.",
            Self::AuthorizationDenied => "Administrator authorization was not granted.",
            Self::Cancelled => "Administrator operation cancelled.",
            Self::Changed => "The selected item changed; nothing else was attempted.",
            Self::Conflict => "The destination already exists; nothing was overwritten.",
            Self::Unsupported => "The administrator backend does not support this operation.",
            Self::Backend => "The administrator operation failed. Review the operation details.",
        }
    }
}

#[derive(Clone, Debug)]
pub enum PrivilegedOperationEvent {
    Started {
        id: u64,
        kind: PrivilegedOperationKind,
    },
    Progress {
        id: u64,
        current: u64,
        total: Option<u64>,
    },
    CancellationRequested {
        id: u64,
    },
    Completed {
        id: u64,
        kind: PrivilegedOperationKind,
        affected_parent: PrivilegedResourceId,
    },
    Failed {
        id: u64,
        kind: PrivilegedOperationKind,
        failure: PrivilegedOperationFailureKind,
        destination_may_exist: bool,
    },
}

struct ActiveOperation {
    request: PrivilegedOperationRequest,
    cancellable: gio::Cancellable,
}

pub struct GioPrivilegedOperationService {
    active: Rc<RefCell<Option<ActiveOperation>>>,
    callback: Rc<dyn Fn(PrivilegedOperationEvent)>,
}

impl GioPrivilegedOperationService {
    pub fn new(callback: Rc<dyn Fn(PrivilegedOperationEvent)>) -> Self {
        Self {
            active: Rc::new(RefCell::new(None)),
            callback,
        }
    }

    pub fn submit(
        &self,
        request: PrivilegedOperationRequest,
        mount_operation: gio::MountOperation,
    ) -> Result<(), PrivilegedOperationFailureKind> {
        if self.active.borrow().is_some() {
            return Err(PrivilegedOperationFailureKind::Busy);
        }
        let cancellable = gio::Cancellable::new();
        self.active.replace(Some(ActiveOperation {
            request: request.clone(),
            cancellable: cancellable.clone(),
        }));
        (self.callback)(PrivilegedOperationEvent::Started {
            id: request.id,
            kind: request.kind,
        });
        authorize_operation(
            request,
            mount_operation,
            cancellable,
            Rc::clone(&self.active),
            Rc::clone(&self.callback),
        );
        Ok(())
    }

    pub fn cancel(&self) -> bool {
        let active = self.active.borrow();
        let Some(active) = active.as_ref() else {
            return false;
        };
        if !active.cancellable.is_cancelled() {
            active.cancellable.cancel();
            (self.callback)(PrivilegedOperationEvent::CancellationRequested {
                id: active.request.id,
            });
        }
        true
    }

    pub fn is_active(&self) -> bool {
        self.active.borrow().is_some()
    }
}

impl Drop for GioPrivilegedOperationService {
    fn drop(&mut self) {
        if let Some(active) = self.active.borrow_mut().take() {
            active.cancellable.cancel();
        }
    }
}

fn authorize_operation(
    request: PrivilegedOperationRequest,
    mount_operation: gio::MountOperation,
    cancellable: gio::Cancellable,
    active: Rc<RefCell<Option<ActiveOperation>>>,
    callback: Rc<dyn Fn(PrivilegedOperationEvent)>,
) {
    let authority = request
        .source
        .as_ref()
        .map(|subject| subject.resource.file())
        .unwrap_or_else(|| request.parent.file());
    let request_for_callback = request.clone();
    let cancellable_for_callback = cancellable.clone();
    authority.mount_enclosing_volume(
        gio::MountMountFlags::NONE,
        Some(&mount_operation),
        Some(&cancellable),
        move |result| {
            let authorized = match result {
                Ok(()) => true,
                Err(error) if error.matches(gio::IOErrorEnum::AlreadyMounted) => true,
                Err(error) => {
                    finish_failure(
                        &active,
                        &callback,
                        &request_for_callback,
                        classify_failure(&error),
                        false,
                    );
                    false
                }
            };
            if authorized {
                revalidate_then_execute(
                    request_for_callback,
                    cancellable_for_callback,
                    active,
                    callback,
                );
            }
        },
    );
}

fn revalidate_then_execute(
    request: PrivilegedOperationRequest,
    cancellable: gio::Cancellable,
    active: Rc<RefCell<Option<ActiveOperation>>>,
    callback: Rc<dyn Fn(PrivilegedOperationEvent)>,
) {
    let Some(subject) = request.source.clone() else {
        execute_operation(request, cancellable, active, callback);
        return;
    };
    let file = subject.resource.file();
    let request_for_callback = request.clone();
    let cancellable_for_callback = cancellable.clone();
    file.query_info_async(
        REVALIDATION_ATTRIBUTES,
        gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
        glib::Priority::DEFAULT,
        Some(&cancellable),
        move |result| match result {
            Ok(info) => {
                let observed = PrivilegedFingerprint::from_info(&info);
                if !subject.expected.matches_observed(&observed) {
                    finish_failure(
                        &active,
                        &callback,
                        &request_for_callback,
                        PrivilegedOperationFailureKind::Changed,
                        false,
                    );
                    return;
                }
                execute_operation(
                    request_for_callback,
                    cancellable_for_callback,
                    active,
                    callback,
                );
            }
            Err(error) => finish_failure(
                &active,
                &callback,
                &request_for_callback,
                classify_failure(&error),
                false,
            ),
        },
    );
}

fn execute_operation(
    request: PrivilegedOperationRequest,
    cancellable: gio::Cancellable,
    active: Rc<RefCell<Option<ActiveOperation>>>,
    callback: Rc<dyn Fn(PrivilegedOperationEvent)>,
) {
    match request.kind {
        PrivilegedOperationKind::CreateDirectory => {
            let destination = request
                .destination
                .as_ref()
                .expect("validated destination")
                .file();
            let request_for_callback = request.clone();
            destination.make_directory_async(
                glib::Priority::DEFAULT,
                Some(&cancellable),
                move |result| finish_simple(result, active, callback, request_for_callback, false),
            );
        }
        PrivilegedOperationKind::Rename | PrivilegedOperationKind::Move => {
            let source = request
                .source
                .as_ref()
                .expect("validated source")
                .resource
                .file();
            let destination = request
                .destination
                .as_ref()
                .expect("validated destination")
                .file();
            let request_for_callback = request.clone();
            let progress_callback = progress_callback(request.id, Rc::clone(&callback));
            source.move_async(
                &destination,
                gio::FileCopyFlags::NOFOLLOW_SYMLINKS,
                glib::Priority::DEFAULT,
                Some(&cancellable),
                Some(progress_callback),
                move |result| finish_simple(result, active, callback, request_for_callback, true),
            );
        }
        PrivilegedOperationKind::Copy => {
            let source = request
                .source
                .as_ref()
                .expect("validated source")
                .resource
                .file();
            let destination = request
                .destination
                .as_ref()
                .expect("validated destination")
                .file();
            let request_for_callback = request.clone();
            let progress_callback = progress_callback(request.id, Rc::clone(&callback));
            source.copy_async(
                &destination,
                gio::FileCopyFlags::NOFOLLOW_SYMLINKS,
                glib::Priority::DEFAULT,
                Some(&cancellable),
                Some(progress_callback),
                move |result| finish_simple(result, active, callback, request_for_callback, true),
            );
        }
        PrivilegedOperationKind::Trash => {
            let source = request
                .source
                .as_ref()
                .expect("validated source")
                .resource
                .file();
            let request_for_callback = request.clone();
            source.trash_async(glib::Priority::DEFAULT, Some(&cancellable), move |result| {
                finish_simple(result, active, callback, request_for_callback, false)
            });
        }
        PrivilegedOperationKind::DeletePermanently => {
            let source = request
                .source
                .as_ref()
                .expect("validated source")
                .resource
                .file();
            let request_for_callback = request.clone();
            source.delete_async(glib::Priority::DEFAULT, Some(&cancellable), move |result| {
                finish_simple(result, active, callback, request_for_callback, false)
            });
        }
        PrivilegedOperationKind::SetPermissions => {
            let source = request
                .source
                .as_ref()
                .expect("validated source")
                .resource
                .file();
            let info = gio::FileInfo::new();
            info.set_attribute_uint32("unix::mode", request.mode.expect("validated mode"));
            let request_for_callback = request.clone();
            source.set_attributes_async(
                &info,
                gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                glib::Priority::DEFAULT,
                Some(&cancellable),
                move |result| {
                    finish_simple(
                        result.map(|_| ()),
                        active,
                        callback,
                        request_for_callback,
                        false,
                    )
                },
            );
        }
    }
}

fn progress_callback(
    id: u64,
    callback: Rc<dyn Fn(PrivilegedOperationEvent)>,
) -> Box<dyn FnMut(i64, i64)> {
    Box::new(move |current, total| {
        callback(PrivilegedOperationEvent::Progress {
            id,
            current: u64::try_from(current).unwrap_or(0),
            total: u64::try_from(total).ok().filter(|total| *total > 0),
        });
    })
}

fn finish_simple(
    result: Result<(), glib::Error>,
    active: Rc<RefCell<Option<ActiveOperation>>>,
    callback: Rc<dyn Fn(PrivilegedOperationEvent)>,
    request: PrivilegedOperationRequest,
    destination_may_exist: bool,
) {
    match result {
        Ok(()) => finish_success(&active, &callback, &request),
        Err(error) => finish_failure(
            &active,
            &callback,
            &request,
            classify_failure(&error),
            destination_may_exist,
        ),
    }
}

fn finish_success(
    active: &Rc<RefCell<Option<ActiveOperation>>>,
    callback: &Rc<dyn Fn(PrivilegedOperationEvent)>,
    request: &PrivilegedOperationRequest,
) {
    if !take_active(active, request.id) {
        return;
    }
    callback(PrivilegedOperationEvent::Completed {
        id: request.id,
        kind: request.kind,
        affected_parent: request.parent.clone(),
    });
}

fn finish_failure(
    active: &Rc<RefCell<Option<ActiveOperation>>>,
    callback: &Rc<dyn Fn(PrivilegedOperationEvent)>,
    request: &PrivilegedOperationRequest,
    failure: PrivilegedOperationFailureKind,
    destination_may_exist: bool,
) {
    if !take_active(active, request.id) {
        return;
    }
    callback(PrivilegedOperationEvent::Failed {
        id: request.id,
        kind: request.kind,
        failure,
        destination_may_exist,
    });
}

fn take_active(active: &Rc<RefCell<Option<ActiveOperation>>>, id: u64) -> bool {
    if active
        .borrow()
        .as_ref()
        .is_some_and(|active| active.request.id == id)
    {
        active.borrow_mut().take();
        true
    } else {
        false
    }
}

fn classify_failure(error: &glib::Error) -> PrivilegedOperationFailureKind {
    if error.matches(gio::IOErrorEnum::Cancelled) {
        PrivilegedOperationFailureKind::Cancelled
    } else if error.matches(gio::IOErrorEnum::PermissionDenied) {
        PrivilegedOperationFailureKind::AuthorizationDenied
    } else if error.matches(gio::IOErrorEnum::Exists) {
        PrivilegedOperationFailureKind::Conflict
    } else if error.matches(gio::IOErrorEnum::NotSupported) {
        PrivilegedOperationFailureKind::Unsupported
    } else {
        PrivilegedOperationFailureKind::Backend
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

    use super::*;
    use crate::privileged_access::test_entry;

    #[test]
    fn phase_14c_policy_preserves_raw_identity_and_rejects_unsafe_names() {
        let parent = PrivilegedResourceId::from_local_path(Path::new("/tmp"))
            .expect("absolute administrator parent");
        let raw = OsString::from_vec(vec![b'a', 0x80, b'b']);
        let request = PrivilegedOperationRequest::create_directory(1, &parent, &raw)
            .expect("raw name request");
        assert_eq!(
            request
                .destination
                .as_ref()
                .expect("create destination")
                .local_path(),
            PathBuf::from("/tmp").join(&raw)
        );
        for invalid in [
            OsStr::new(""),
            OsStr::new("."),
            OsStr::new(".."),
            OsStr::new("a/b"),
        ] {
            assert!(PrivilegedOperationRequest::create_directory(1, &parent, invalid).is_err());
        }
        assert!(PrivilegedOperationRequest::create_directory(0, &parent, OsStr::new("x")).is_err());
    }

    #[test]
    fn phase_14c_policy_is_no_overwrite_root_safe_and_bounds_modes_recursion() {
        let file = test_entry("/tmp/source", PrivilegedEntryKind::File, Some(3));
        let folder = test_entry("/tmp/folder", PrivilegedEntryKind::Directory, None);
        assert_eq!(
            PrivilegedOperationRequest::transfer(
                2,
                PrivilegedOperationKind::Copy,
                &file,
                Path::new("/tmp/source")
            ),
            Err(PrivilegedOperationValidationError::SameSourceDestination)
        );
        assert_eq!(
            PrivilegedOperationRequest::transfer(
                3,
                PrivilegedOperationKind::Copy,
                &folder,
                Path::new("/tmp/copy")
            ),
            Err(PrivilegedOperationValidationError::RecursiveCopyUnsupported)
        );
        assert_eq!(
            PrivilegedOperationRequest::set_permissions(4, &file, 0o10000),
            Err(PrivilegedOperationValidationError::InvalidMode)
        );
    }

    #[test]
    fn phase_14c_service_events_are_redacted_bounded_and_cancel_distinct() {
        assert_eq!(
            classify_failure(&glib::Error::new(
                gio::IOErrorEnum::Exists,
                "admin:///secret"
            )),
            PrivilegedOperationFailureKind::Conflict
        );
        assert!(
            !PrivilegedOperationFailureKind::Conflict
                .user_message()
                .contains("admin://")
        );
        let events = Rc::new(RefCell::new(Vec::new()));
        let events_for_callback = Rc::clone(&events);
        let service = GioPrivilegedOperationService::new(Rc::new(move |event| {
            events_for_callback.borrow_mut().push(event);
        }));
        assert!(!service.cancel());
        assert!(!service.is_active());
        let entry = test_entry("/tmp/source", PrivilegedEntryKind::File, Some(3));
        let request =
            PrivilegedOperationRequest::selected(7, PrivilegedOperationKind::Trash, &entry)
                .expect("valid Trash request");
        service.active.replace(Some(ActiveOperation {
            request,
            cancellable: gio::Cancellable::new(),
        }));
        assert!(service.cancel());
        assert!(
            service
                .active
                .borrow()
                .as_ref()
                .is_some_and(|active| active.cancellable.is_cancelled())
        );
        assert!(matches!(
            events.borrow().last(),
            Some(PrivilegedOperationEvent::CancellationRequested { id: 7 })
        ));
        assert!(PrivilegedOperationKind::DeletePermanently.irreversible());
        assert!(!PrivilegedOperationKind::Trash.irreversible());
    }

    #[test]
    fn phase_14c_service_terminal_failure_retains_partial_destination_evidence() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let events_for_callback = Rc::clone(&events);
        let callback: Rc<dyn Fn(PrivilegedOperationEvent)> = Rc::new(move |event| {
            events_for_callback.borrow_mut().push(event);
        });
        let entry = test_entry("/tmp/source", PrivilegedEntryKind::File, Some(3));
        let request = PrivilegedOperationRequest::transfer(
            9,
            PrivilegedOperationKind::Copy,
            &entry,
            Path::new("/tmp/destination"),
        )
        .expect("valid copy request");
        let active = Rc::new(RefCell::new(Some(ActiveOperation {
            request: request.clone(),
            cancellable: gio::Cancellable::new(),
        })));
        finish_failure(
            &active,
            &callback,
            &request,
            PrivilegedOperationFailureKind::Backend,
            true,
        );
        assert!(active.borrow().is_none());
        assert!(matches!(
            events.borrow().as_slice(),
            [PrivilegedOperationEvent::Failed {
                id: 9,
                kind: PrivilegedOperationKind::Copy,
                failure: PrivilegedOperationFailureKind::Backend,
                destination_may_exist: true,
            }]
        ));
    }

    #[test]
    fn phase_14c_service_never_reuses_local_jobs_or_shells() {
        let source = include_str!("privileged_operations.rs");
        let implementation = source
            .split("#[cfg(all(test, unix))]")
            .next()
            .unwrap_or(source);
        for forbidden in [
            "CopyRequest",
            "MoveRequest",
            "RenameRequest",
            "TrashRequest",
            "PermanentDeleteRequest",
            "Command::new",
            "pkexec",
            "sudo",
        ] {
            assert!(
                !implementation.contains(forbidden),
                "forbidden boundary: {forbidden}"
            );
        }
        assert!(implementation.contains("NOFOLLOW_SYMLINKS"));
        assert!(implementation.contains("FileCopyFlags::NOFOLLOW_SYMLINKS"));
    }
}
