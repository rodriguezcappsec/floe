//! Application-owned GIO boundary for drives, volumes, and mounts.
//!
//! The browser and GTK widgets consume immutable [`DeviceSnapshot`] values. GIO
//! objects stay private to this module, and all storage mutations use GIO's
//! asynchronous APIs on the owning GLib main context.

use gtk::gio;
use gtk::gio::prelude::*;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

/// Opaque identity that remains stable while GIO reports the same backing
/// storage identity. Display names are deliberately not authoritative.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DeviceKind {
    Drive,
    Volume,
    Mount,
}

impl DeviceKind {
    fn identity_prefix(self) -> &'static str {
        match self {
            Self::Drive => "drive",
            Self::Volume => "volume",
            Self::Mount => "mount",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceMountState {
    Unmounted,
    Mounted,
}

/// Describes navigation without leaking a non-local URI as a filesystem path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceRootKind {
    None,
    Local,
    NonLocal,
    /// A drive can aggregate more than one mounted volume and therefore has no
    /// single safe navigation target.
    Multiple,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DeviceAction {
    Mount,
    Unmount,
    Eject,
}

impl DeviceAction {
    pub fn present_participle(self) -> &'static str {
        match self {
            Self::Mount => "Mounting",
            Self::Unmount => "Unmounting",
            Self::Eject => "Ejecting",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceActionUnavailable {
    WrongDeviceKind,
    AlreadyMounted,
    NotMounted,
    NotSupported,
}

impl DeviceActionUnavailable {
    pub fn message(self, action: DeviceAction) -> &'static str {
        match (action, self) {
            (DeviceAction::Mount, Self::AlreadyMounted) => "This device is already mounted.",
            (DeviceAction::Unmount, Self::NotMounted) => "This device is not mounted.",
            (_, Self::WrongDeviceKind) => "This action is not available for this device row.",
            (_, Self::NotSupported) => "The desktop storage service does not support this action.",
            (_, Self::AlreadyMounted) => "This device is already mounted.",
            (_, Self::NotMounted) => "This device is not mounted.",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceActionStatus {
    Available,
    Busy,
    Unavailable(DeviceActionUnavailable),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceActions {
    pub mount: DeviceActionStatus,
    pub unmount: DeviceActionStatus,
    pub eject: DeviceActionStatus,
}

impl DeviceActions {
    pub fn status(self, action: DeviceAction) -> DeviceActionStatus {
        match action {
            DeviceAction::Mount => self.mount,
            DeviceAction::Unmount => self.unmount,
            DeviceAction::Eject => self.eject,
        }
    }
}

/// GTK-independent input to the device action policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DevicePolicy {
    pub kind: DeviceKind,
    pub mount_state: DeviceMountState,
    pub removable: bool,
    pub can_mount: bool,
    pub can_unmount: bool,
    pub can_eject: bool,
    pub busy: bool,
}

impl DevicePolicy {
    pub fn actions(self) -> DeviceActions {
        DeviceActions {
            mount: mount_status(self),
            unmount: unmount_status(self),
            eject: eject_status(self),
        }
    }
}

fn mount_status(policy: DevicePolicy) -> DeviceActionStatus {
    if policy.kind != DeviceKind::Volume {
        return DeviceActionStatus::Unavailable(DeviceActionUnavailable::WrongDeviceKind);
    }
    if policy.mount_state == DeviceMountState::Mounted {
        return DeviceActionStatus::Unavailable(DeviceActionUnavailable::AlreadyMounted);
    }
    if !policy.can_mount {
        return DeviceActionStatus::Unavailable(DeviceActionUnavailable::NotSupported);
    }
    if policy.busy {
        DeviceActionStatus::Busy
    } else {
        DeviceActionStatus::Available
    }
}

fn unmount_status(policy: DevicePolicy) -> DeviceActionStatus {
    if !matches!(policy.kind, DeviceKind::Volume | DeviceKind::Mount) {
        return DeviceActionStatus::Unavailable(DeviceActionUnavailable::WrongDeviceKind);
    }
    if policy.mount_state == DeviceMountState::Unmounted {
        return DeviceActionStatus::Unavailable(DeviceActionUnavailable::NotMounted);
    }
    if !policy.can_unmount {
        return DeviceActionStatus::Unavailable(DeviceActionUnavailable::NotSupported);
    }
    if policy.busy {
        DeviceActionStatus::Busy
    } else {
        DeviceActionStatus::Available
    }
}

fn eject_status(policy: DevicePolicy) -> DeviceActionStatus {
    if !policy.can_eject {
        return DeviceActionStatus::Unavailable(DeviceActionUnavailable::NotSupported);
    }
    if policy.busy {
        DeviceActionStatus::Busy
    } else {
        DeviceActionStatus::Available
    }
}

/// GIO exposes a hierarchy, while a file-manager sidebar needs one actionable
/// row per storage target. Volumes carry the useful mounted/unmounted state, so
/// their parent drive is only shown when it has no volume (for example, an
/// empty card reader). A mount is shown separately only when no volume owns it
/// (for example, a remote or synthetic mount).
fn should_show_drive(volume_count: usize) -> bool {
    volume_count == 0
}

fn should_show_mount(has_associated_volume: bool) -> bool {
    !has_associated_volume
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceSnapshot {
    pub id: DeviceId,
    pub kind: DeviceKind,
    pub name: String,
    pub mount_state: DeviceMountState,
    pub root_kind: DeviceRootKind,
    pub removable: bool,
    pub actions: DeviceActions,
    local_root: Option<PathBuf>,
}

impl DeviceSnapshot {
    /// Returns an exact local filesystem path. Remote roots, unmounted devices,
    /// and drives with multiple roots intentionally return `None`.
    pub fn local_root(&self) -> Option<&Path> {
        self.local_root.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceActionFailureKind {
    Cancelled,
    Busy,
    PermissionDenied,
    NotMounted,
    AlreadyMounted,
    NotSupported,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceActionFailure {
    pub kind: DeviceActionFailureKind,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceActionOutcome {
    Completed {
        id: DeviceId,
        action: DeviceAction,
    },
    Failed {
        id: DeviceId,
        action: DeviceAction,
        failure: DeviceActionFailure,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DeviceActionStartError {
    #[error("The storage device is no longer available.")]
    UnknownDevice,
    #[error("Another storage action is already running for this device.")]
    Busy { action: DeviceAction },
    #[error("{message}")]
    Unavailable {
        action: DeviceAction,
        reason: DeviceActionUnavailable,
        message: &'static str,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DeviceSubscriptionId(u64);

#[derive(Clone)]
pub struct DeviceMonitor {
    shared: Rc<RefCell<DeviceMonitorInner>>,
}

type SnapshotListener = Rc<dyn Fn(&[DeviceSnapshot])>;

struct DeviceMonitorInner {
    monitor: gio::VolumeMonitor,
    objects: HashMap<DeviceId, DeviceObject>,
    snapshots: Vec<DeviceSnapshot>,
    in_flight: HashMap<DeviceId, InFlightAction>,
    listeners: BTreeMap<u64, SnapshotListener>,
    next_listener_id: u64,
}

#[derive(Clone)]
enum DeviceObject {
    Drive(gio::Drive),
    Volume(gio::Volume),
    Mount(gio::Mount),
}

struct InFlightAction {
    action: DeviceAction,
    _cancellable: gio::Cancellable,
}

impl DeviceMonitor {
    pub fn new() -> Self {
        let monitor = gio::VolumeMonitor::get();
        let shared = Rc::new(RefCell::new(DeviceMonitorInner {
            monitor: monitor.clone(),
            objects: HashMap::new(),
            snapshots: Vec::new(),
            in_flight: HashMap::new(),
            listeners: BTreeMap::new(),
            next_listener_id: 1,
        }));

        connect_topology_signals(&monitor, Rc::downgrade(&shared));
        refresh_shared(&shared);
        Self { shared }
    }

    pub fn snapshots(&self) -> Vec<DeviceSnapshot> {
        self.shared.borrow().snapshots.clone()
    }

    /// Registers a topology/action-state observer and immediately supplies the
    /// current immutable snapshot.
    pub fn connect_changed<F>(&self, listener: F) -> DeviceSubscriptionId
    where
        F: Fn(&[DeviceSnapshot]) + 'static,
    {
        let listener: SnapshotListener = Rc::new(listener);
        let (id, snapshot) = {
            let mut inner = self.shared.borrow_mut();
            let id = inner.next_listener_id;
            inner.next_listener_id = inner.next_listener_id.saturating_add(1);
            inner.listeners.insert(id, Rc::clone(&listener));
            (DeviceSubscriptionId(id), inner.snapshots.clone())
        };
        listener(&snapshot);
        id
    }

    pub fn disconnect_changed(&self, id: DeviceSubscriptionId) -> bool {
        self.shared.borrow_mut().listeners.remove(&id.0).is_some()
    }

    /// Rebuilds from GIO. This is useful after a browser recovery event; normal
    /// drive, volume, and mount changes refresh automatically through signals.
    pub fn refresh(&self) {
        refresh_shared(&self.shared);
    }

    pub fn mount<F>(
        &self,
        id: &DeviceId,
        mount_operation: Option<&gio::MountOperation>,
        completion: F,
    ) -> Result<(), DeviceActionStartError>
    where
        F: FnOnce(DeviceActionOutcome) + 'static,
    {
        self.start_action(id, DeviceAction::Mount, mount_operation, completion)
    }

    pub fn unmount<F>(
        &self,
        id: &DeviceId,
        mount_operation: Option<&gio::MountOperation>,
        completion: F,
    ) -> Result<(), DeviceActionStartError>
    where
        F: FnOnce(DeviceActionOutcome) + 'static,
    {
        self.start_action(id, DeviceAction::Unmount, mount_operation, completion)
    }

    pub fn eject<F>(
        &self,
        id: &DeviceId,
        mount_operation: Option<&gio::MountOperation>,
        completion: F,
    ) -> Result<(), DeviceActionStartError>
    where
        F: FnOnce(DeviceActionOutcome) + 'static,
    {
        self.start_action(id, DeviceAction::Eject, mount_operation, completion)
    }

    fn start_action<F>(
        &self,
        id: &DeviceId,
        action: DeviceAction,
        mount_operation: Option<&gio::MountOperation>,
        completion: F,
    ) -> Result<(), DeviceActionStartError>
    where
        F: FnOnce(DeviceActionOutcome) + 'static,
    {
        let (object, cancellable) = {
            let mut inner = self.shared.borrow_mut();
            if let Some(running) = inner.in_flight.get(id) {
                return Err(DeviceActionStartError::Busy {
                    action: running.action,
                });
            }
            let snapshot = inner
                .snapshots
                .iter()
                .find(|snapshot| snapshot.id == *id)
                .ok_or(DeviceActionStartError::UnknownDevice)?;
            match snapshot.actions.status(action) {
                DeviceActionStatus::Available => {}
                DeviceActionStatus::Busy => {
                    return Err(DeviceActionStartError::Busy { action });
                }
                DeviceActionStatus::Unavailable(reason) => {
                    return Err(DeviceActionStartError::Unavailable {
                        action,
                        reason,
                        message: reason.message(action),
                    });
                }
            }
            let object = inner
                .objects
                .get(id)
                .cloned()
                .ok_or(DeviceActionStartError::UnknownDevice)?;
            let cancellable = gio::Cancellable::new();
            reserve_action(&mut inner.in_flight, id, action, &cancellable)?;
            (object, cancellable)
        };

        refresh_shared(&self.shared);
        let id = id.clone();
        let shared = Rc::clone(&self.shared);
        let mount_operation = mount_operation.cloned();
        let finish = move |result: Result<(), glib::Error>| {
            let outcome = action_outcome(&id, action, result);
            shared.borrow_mut().in_flight.remove(&id);
            refresh_shared(&shared);
            completion(outcome);
        };

        match (object, action) {
            (DeviceObject::Volume(volume), DeviceAction::Mount) => volume.mount(
                gio::MountMountFlags::NONE,
                mount_operation.as_ref(),
                Some(&cancellable),
                finish,
            ),
            (DeviceObject::Volume(volume), DeviceAction::Unmount) => {
                let Some(mount) = volume.get_mount() else {
                    finish(Err(glib::Error::new(
                        gio::IOErrorEnum::NotMounted,
                        "The volume is no longer mounted.",
                    )));
                    return Ok(());
                };
                mount.unmount_with_operation(
                    gio::MountUnmountFlags::NONE,
                    mount_operation.as_ref(),
                    Some(&cancellable),
                    finish,
                );
            }
            (DeviceObject::Mount(mount), DeviceAction::Unmount) => mount.unmount_with_operation(
                gio::MountUnmountFlags::NONE,
                mount_operation.as_ref(),
                Some(&cancellable),
                finish,
            ),
            (DeviceObject::Drive(drive), DeviceAction::Eject) => drive.eject_with_operation(
                gio::MountUnmountFlags::NONE,
                mount_operation.as_ref(),
                Some(&cancellable),
                finish,
            ),
            (DeviceObject::Volume(volume), DeviceAction::Eject) => volume.eject_with_operation(
                gio::MountUnmountFlags::NONE,
                mount_operation.as_ref(),
                Some(&cancellable),
                finish,
            ),
            (DeviceObject::Mount(mount), DeviceAction::Eject) => mount.eject_with_operation(
                gio::MountUnmountFlags::NONE,
                mount_operation.as_ref(),
                Some(&cancellable),
                finish,
            ),
            _ => unreachable!("action availability and private object kind must agree"),
        }
        Ok(())
    }
}

impl Default for DeviceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

fn connect_topology_signals(monitor: &gio::VolumeMonitor, weak: Weak<RefCell<DeviceMonitorInner>>) {
    macro_rules! refresh_on {
        ($connect:ident) => {{
            let weak = weak.clone();
            monitor.$connect(move |_, _| {
                if let Some(shared) = weak.upgrade() {
                    refresh_shared(&shared);
                }
            });
        }};
    }

    refresh_on!(connect_drive_changed);
    refresh_on!(connect_drive_connected);
    refresh_on!(connect_drive_disconnected);
    refresh_on!(connect_mount_added);
    refresh_on!(connect_mount_changed);
    refresh_on!(connect_mount_pre_unmount);
    refresh_on!(connect_mount_removed);
    refresh_on!(connect_volume_added);
    refresh_on!(connect_volume_changed);
    refresh_on!(connect_volume_removed);
}

fn refresh_shared(shared: &Rc<RefCell<DeviceMonitorInner>>) {
    let (snapshot, listeners) = {
        let mut inner = shared.borrow_mut();
        let (snapshots, objects) = collect_snapshots(&inner.monitor, &inner.in_flight);
        inner.snapshots = snapshots;
        inner.objects = objects;
        (
            inner.snapshots.clone(),
            inner.listeners.values().cloned().collect::<Vec<_>>(),
        )
    };
    for listener in listeners {
        listener(&snapshot);
    }
}

fn reserve_action(
    in_flight: &mut HashMap<DeviceId, InFlightAction>,
    id: &DeviceId,
    action: DeviceAction,
    cancellable: &gio::Cancellable,
) -> Result<(), DeviceActionStartError> {
    if let Some(running) = in_flight.get(id) {
        return Err(DeviceActionStartError::Busy {
            action: running.action,
        });
    }
    in_flight.insert(
        id.clone(),
        InFlightAction {
            action,
            _cancellable: cancellable.clone(),
        },
    );
    Ok(())
}

fn collect_snapshots(
    monitor: &gio::VolumeMonitor,
    in_flight: &HashMap<DeviceId, InFlightAction>,
) -> (Vec<DeviceSnapshot>, HashMap<DeviceId, DeviceObject>) {
    let mut snapshots = Vec::new();
    let mut objects = HashMap::new();

    for drive in monitor.connected_drives() {
        let volumes = drive.volumes();
        if !should_show_drive(volumes.len()) {
            continue;
        }
        let identity = drive_identity(&drive);
        let id = unique_id(DeviceKind::Drive, &identity, &objects);
        let roots = volumes
            .into_iter()
            .filter_map(|volume| volume.get_mount())
            .map(|mount| mount.root())
            .collect::<Vec<_>>();
        let (mount_state, root_kind, local_root) = aggregate_roots(&roots);
        let policy = DevicePolicy {
            kind: DeviceKind::Drive,
            mount_state,
            removable: drive.is_removable(),
            can_mount: false,
            can_unmount: false,
            can_eject: drive.can_eject(),
            busy: in_flight.contains_key(&id),
        };
        snapshots.push(DeviceSnapshot {
            id: id.clone(),
            kind: DeviceKind::Drive,
            name: drive.name().to_string(),
            mount_state,
            root_kind,
            removable: policy.removable,
            actions: policy.actions(),
            local_root,
        });
        objects.insert(id, DeviceObject::Drive(drive));
    }

    for volume in monitor.volumes() {
        let identity = volume_identity(&volume);
        let id = unique_id(DeviceKind::Volume, &identity, &objects);
        let mount = volume.get_mount();
        let (mount_state, root_kind, local_root) = mount
            .as_ref()
            .map(|mount| one_root(&mount.root()))
            .unwrap_or((DeviceMountState::Unmounted, DeviceRootKind::None, None));
        let removable =
            volume.drive().is_some_and(|drive| drive.is_removable()) || volume.can_eject();
        let policy = DevicePolicy {
            kind: DeviceKind::Volume,
            mount_state,
            removable,
            can_mount: volume.can_mount(),
            can_unmount: mount.as_ref().is_some_and(|mount| mount.can_unmount()),
            can_eject: volume.can_eject(),
            busy: in_flight.contains_key(&id),
        };
        snapshots.push(DeviceSnapshot {
            id: id.clone(),
            kind: DeviceKind::Volume,
            name: volume.name().to_string(),
            mount_state,
            root_kind,
            removable,
            actions: policy.actions(),
            local_root,
        });
        objects.insert(id, DeviceObject::Volume(volume));
    }

    for mount in monitor.mounts() {
        if !should_show_mount(mount.volume().is_some()) {
            continue;
        }
        let identity = mount_identity(&mount);
        let id = unique_id(DeviceKind::Mount, &identity, &objects);
        let (mount_state, root_kind, local_root) = one_root(&mount.root());
        let removable =
            mount.drive().is_some_and(|drive| drive.is_removable()) || mount.can_eject();
        let policy = DevicePolicy {
            kind: DeviceKind::Mount,
            mount_state,
            removable,
            can_mount: false,
            can_unmount: mount.can_unmount(),
            can_eject: mount.can_eject(),
            busy: in_flight.contains_key(&id),
        };
        snapshots.push(DeviceSnapshot {
            id: id.clone(),
            kind: DeviceKind::Mount,
            name: mount.name().to_string(),
            mount_state,
            root_kind,
            removable,
            actions: policy.actions(),
            local_root,
        });
        objects.insert(id, DeviceObject::Mount(mount));
    }

    snapshots.sort_by(|left, right| {
        left.kind
            .identity_prefix()
            .cmp(right.kind.identity_prefix())
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });
    (snapshots, objects)
}

fn one_root(root: &gio::File) -> (DeviceMountState, DeviceRootKind, Option<PathBuf>) {
    match root.path() {
        Some(path) => (DeviceMountState::Mounted, DeviceRootKind::Local, Some(path)),
        None => (DeviceMountState::Mounted, DeviceRootKind::NonLocal, None),
    }
}

fn aggregate_roots(roots: &[gio::File]) -> (DeviceMountState, DeviceRootKind, Option<PathBuf>) {
    match roots {
        [] => (DeviceMountState::Unmounted, DeviceRootKind::None, None),
        [root] => one_root(root),
        _ => (DeviceMountState::Mounted, DeviceRootKind::Multiple, None),
    }
}

fn drive_identity(drive: &gio::Drive) -> Vec<Vec<u8>> {
    let mut identifiers = drive.enumerate_identifiers();
    identifiers.sort();
    let mut parts = identifiers
        .into_iter()
        .filter_map(|kind| {
            drive
                .identifier(kind.as_str())
                .map(|value| format!("{kind}={value}").into_bytes())
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        parts.push(format!("live-object:{:p}", drive.as_ptr()).into_bytes());
    }
    parts
}

fn volume_identity(volume: &gio::Volume) -> Vec<Vec<u8>> {
    if let Some(uuid) = volume.uuid() {
        return vec![uuid.as_bytes().to_vec()];
    }
    let mut identifiers = volume.enumerate_identifiers();
    identifiers.sort();
    let mut parts = identifiers
        .into_iter()
        .filter_map(|kind| {
            volume
                .identifier(kind.as_str())
                .map(|value| format!("{kind}={value}").into_bytes())
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        if let Some(root) = volume.activation_root() {
            parts.push(root.uri().as_bytes().to_vec());
        } else {
            parts.push(format!("live-object:{:p}", volume.as_ptr()).into_bytes());
        }
    }
    parts
}

fn mount_identity(mount: &gio::Mount) -> Vec<Vec<u8>> {
    if let Some(uuid) = mount.uuid() {
        return vec![uuid.as_bytes().to_vec()];
    }
    vec![mount.root().uri().as_bytes().to_vec()]
}

fn unique_id(
    kind: DeviceKind,
    identity: &[Vec<u8>],
    existing: &HashMap<DeviceId, DeviceObject>,
) -> DeviceId {
    let base = stable_device_id(kind, identity);
    if !existing.contains_key(&base) {
        return base;
    }
    let mut suffix = 2_u64;
    loop {
        let candidate = DeviceId(format!("{}-{suffix}", base.0));
        if !existing.contains_key(&candidate) {
            return candidate;
        }
        suffix = suffix.saturating_add(1);
    }
}

fn stable_device_id(kind: DeviceKind, identity: &[Vec<u8>]) -> DeviceId {
    // Stable FNV-1a keeps identifiers opaque without introducing a hashing
    // dependency or exposing remote root URIs through the public snapshot.
    let mut hash = 0xcbf29ce484222325_u64;
    for part in identity {
        for byte in part {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    DeviceId(format!("{}-{hash:016x}", kind.identity_prefix()))
}

fn classify_gio_failure(error: &glib::Error) -> DeviceActionFailure {
    let kind = if error.matches(gio::IOErrorEnum::Cancelled) {
        DeviceActionFailureKind::Cancelled
    } else if error.matches(gio::IOErrorEnum::Busy) {
        DeviceActionFailureKind::Busy
    } else if error.matches(gio::IOErrorEnum::PermissionDenied) {
        DeviceActionFailureKind::PermissionDenied
    } else if error.matches(gio::IOErrorEnum::NotMounted) {
        DeviceActionFailureKind::NotMounted
    } else if error.matches(gio::IOErrorEnum::AlreadyMounted) {
        DeviceActionFailureKind::AlreadyMounted
    } else if error.matches(gio::IOErrorEnum::NotSupported) {
        DeviceActionFailureKind::NotSupported
    } else {
        DeviceActionFailureKind::Other
    };
    let prefix = match kind {
        DeviceActionFailureKind::Cancelled => "The storage action was cancelled",
        DeviceActionFailureKind::Busy => "The device is busy",
        DeviceActionFailureKind::PermissionDenied => "Permission was denied",
        DeviceActionFailureKind::NotMounted => "The device is not mounted",
        DeviceActionFailureKind::AlreadyMounted => "The device is already mounted",
        DeviceActionFailureKind::NotSupported => "This storage action is not supported",
        DeviceActionFailureKind::Other => "The storage action failed",
    };
    DeviceActionFailure {
        kind,
        message: format!("{prefix}: {}", error.message()),
    }
}

fn action_outcome(
    id: &DeviceId,
    action: DeviceAction,
    result: Result<(), glib::Error>,
) -> DeviceActionOutcome {
    match result {
        Ok(()) => DeviceActionOutcome::Completed {
            id: id.clone(),
            action,
        },
        Err(error) => DeviceActionOutcome::Failed {
            id: id.clone(),
            action,
            failure: classify_gio_failure(&error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    fn policy(kind: DeviceKind) -> DevicePolicy {
        DevicePolicy {
            kind,
            mount_state: DeviceMountState::Unmounted,
            removable: false,
            can_mount: false,
            can_unmount: false,
            can_eject: false,
            busy: false,
        }
    }

    #[test]
    fn phase_6k_device_policy_distinguishes_rows_mount_state_and_actions() {
        let volume = DevicePolicy {
            can_mount: true,
            can_eject: true,
            removable: true,
            ..policy(DeviceKind::Volume)
        };
        assert!(volume.removable);
        assert_eq!(volume.actions().mount, DeviceActionStatus::Available);
        assert_eq!(volume.actions().eject, DeviceActionStatus::Available);
        assert_eq!(
            volume.actions().unmount,
            DeviceActionStatus::Unavailable(DeviceActionUnavailable::NotMounted)
        );

        let mount = DevicePolicy {
            mount_state: DeviceMountState::Mounted,
            can_unmount: true,
            ..policy(DeviceKind::Mount)
        };
        assert_eq!(mount.actions().unmount, DeviceActionStatus::Available);
        assert_eq!(
            mount.actions().mount,
            DeviceActionStatus::Unavailable(DeviceActionUnavailable::WrongDeviceKind)
        );

        let drive = DevicePolicy {
            can_eject: true,
            ..policy(DeviceKind::Drive)
        };
        assert_eq!(drive.actions().eject, DeviceActionStatus::Available);
        assert_eq!(
            drive.actions().unmount,
            DeviceActionStatus::Unavailable(DeviceActionUnavailable::WrongDeviceKind)
        );
    }

    #[test]
    fn phase_6k_device_policy_busy_only_replaces_supported_actions() {
        let volume = DevicePolicy {
            can_mount: true,
            can_eject: true,
            busy: true,
            ..policy(DeviceKind::Volume)
        };
        assert_eq!(volume.actions().mount, DeviceActionStatus::Busy);
        assert_eq!(volume.actions().eject, DeviceActionStatus::Busy);
        assert_eq!(
            volume.actions().unmount,
            DeviceActionStatus::Unavailable(DeviceActionUnavailable::NotMounted)
        );
    }

    #[test]
    fn phase_6k_device_policy_collapses_gio_hierarchy_to_one_sidebar_row() {
        assert!(should_show_drive(0), "empty media readers need a drive row");
        assert!(!should_show_drive(1), "a volume replaces its parent drive");
        assert!(
            !should_show_drive(3),
            "all child volumes replace the drive row"
        );
        assert!(
            should_show_mount(false),
            "standalone mounts need their own row"
        );
        assert!(
            !should_show_mount(true),
            "a volume-backed mount is represented by its volume row"
        );
    }

    #[test]
    fn phase_6k_device_identity_is_stable_opaque_and_kind_scoped() {
        let backing = vec![b"unix-device=/dev/disk/by-uuid/example".to_vec()];
        let first = stable_device_id(DeviceKind::Volume, &backing);
        let renamed = stable_device_id(DeviceKind::Volume, &backing);
        let mount = stable_device_id(DeviceKind::Mount, &backing);
        assert_eq!(first, renamed);
        assert_ne!(first, mount);
        assert!(!first.as_str().contains("/dev/"));
        assert!(first.as_str().starts_with("volume-"));
    }

    #[test]
    fn phase_6k_device_identity_preserves_exact_local_non_utf8_path_only() {
        let exact = PathBuf::from(OsString::from_vec(b"/media/floe/usb-\xff".to_vec()));
        let snapshot = DeviceSnapshot {
            id: stable_device_id(DeviceKind::Mount, &[b"local".to_vec()]),
            kind: DeviceKind::Mount,
            name: "USB".to_owned(),
            mount_state: DeviceMountState::Mounted,
            root_kind: DeviceRootKind::Local,
            removable: true,
            actions: policy(DeviceKind::Mount).actions(),
            local_root: Some(exact.clone()),
        };
        assert_eq!(snapshot.local_root(), Some(exact.as_path()));

        let remote = DeviceSnapshot {
            root_kind: DeviceRootKind::NonLocal,
            local_root: None,
            ..snapshot
        };
        assert_eq!(remote.local_root(), None);
    }

    #[test]
    fn phase_6k_device_monitor_refresh_notifies_and_disconnects_clients() {
        let monitor = DeviceMonitor::new();
        let notifications = Rc::new(Cell::new(0));
        let observed = Rc::clone(&notifications);
        let subscription = monitor.connect_changed(move |_| observed.set(observed.get() + 1));
        assert_eq!(
            notifications.get(),
            1,
            "subscription supplies initial state"
        );
        monitor.refresh();
        assert_eq!(notifications.get(), 2);
        assert!(monitor.disconnect_changed(subscription));
        monitor.refresh();
        assert_eq!(notifications.get(), 2);
    }

    #[test]
    fn phase_6k_device_monitor_snapshot_invariants_hold_for_live_read_only_state() {
        let monitor = DeviceMonitor::new();
        for snapshot in monitor.snapshots() {
            assert_eq!(
                snapshot.local_root().is_some(),
                snapshot.root_kind == DeviceRootKind::Local
            );
            if snapshot.mount_state == DeviceMountState::Unmounted {
                assert_eq!(snapshot.root_kind, DeviceRootKind::None);
                assert!(snapshot.local_root().is_none());
            }
        }
    }

    #[test]
    fn phase_6k_device_action_failure_mapping_is_structured_and_understandable() {
        let id = stable_device_id(DeviceKind::Mount, &[b"outcome".to_vec()]);
        assert_eq!(
            action_outcome(&id, DeviceAction::Unmount, Ok(())),
            DeviceActionOutcome::Completed {
                id: id.clone(),
                action: DeviceAction::Unmount,
            }
        );

        let error = glib::Error::new(gio::IOErrorEnum::Busy, "files are open");
        let outcome = action_outcome(&id, DeviceAction::Eject, Err(error));
        let DeviceActionOutcome::Failed { failure, .. } = outcome else {
            panic!("GIO errors must produce a structured failed outcome");
        };
        assert_eq!(failure.kind, DeviceActionFailureKind::Busy);
        assert_eq!(failure.message, "The device is busy: files are open");

        let cancelled = classify_gio_failure(&glib::Error::new(
            gio::IOErrorEnum::Cancelled,
            "cancelled by user",
        ));
        assert_eq!(cancelled.kind, DeviceActionFailureKind::Cancelled);
        assert!(
            cancelled
                .message
                .starts_with("The storage action was cancelled")
        );
    }

    #[test]
    fn phase_6k_device_action_unavailable_messages_name_recovery_state() {
        let already = DeviceActionUnavailable::AlreadyMounted.message(DeviceAction::Mount);
        let missing = DeviceActionUnavailable::NotMounted.message(DeviceAction::Unmount);
        let unsupported = DeviceActionUnavailable::NotSupported.message(DeviceAction::Eject);
        assert!(already.contains("already mounted"));
        assert!(missing.contains("not mounted"));
        assert!(unsupported.contains("does not support"));
    }

    #[test]
    fn phase_6k_device_action_reservation_rejects_duplicate_device_work() {
        let id = stable_device_id(DeviceKind::Volume, &[b"duplicate-test".to_vec()]);
        let mut in_flight = HashMap::new();
        let cancellable = gio::Cancellable::new();
        assert_eq!(
            reserve_action(&mut in_flight, &id, DeviceAction::Mount, &cancellable),
            Ok(())
        );
        assert_eq!(
            reserve_action(&mut in_flight, &id, DeviceAction::Eject, &cancellable),
            Err(DeviceActionStartError::Busy {
                action: DeviceAction::Mount
            })
        );
        assert_eq!(in_flight.len(), 1);
    }
}
