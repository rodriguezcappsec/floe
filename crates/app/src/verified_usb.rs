//! Explicit verified-removable-media state and flush policy.
//!
//! GTK/GIO removal remains in `devices`; this module is worker-safe and never
//! calls a device action itself.  A successful copy verification is deliberately
//! not a safe-removal result: only the caller may advance through a successful
//! GIO eject or unmount completion.

use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread::{self, JoinHandle},
};

use floe_core::VerifiedDestinationState;
use rustix::fs::{Mode, OFlags};
use thiserror::Error;

use crate::devices::{
    DeviceAction, DeviceRemovalTarget, DeviceSnapshot, revalidate_removal_target,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifiedUsbStage {
    Copying,
    Verifying,
    Flushing,
    Removing(DeviceAction),
    SafeToRemove,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VerifiedUsbTerminal {
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedUsbTransfer {
    target: DeviceRemovalTarget,
    stage: VerifiedUsbStage,
    last_completed: Option<VerifiedUsbStage>,
    destination_state: VerifiedDestinationState,
    terminal: Option<VerifiedUsbTerminal>,
}

/// Application-owned handoff state: a verified-copy job is observed exactly
/// once, then flush and GIO removal are separately driven by their owners.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedUsbWorkflow {
    child_job: floe_core::JobId,
    transfer: VerifiedUsbTransfer,
}

impl VerifiedUsbWorkflow {
    pub fn new(child_job: floe_core::JobId, target: DeviceRemovalTarget) -> Self {
        Self {
            child_job,
            transfer: VerifiedUsbTransfer::new(target),
        }
    }
    pub const fn child_job(&self) -> floe_core::JobId {
        self.child_job
    }
    pub fn transfer(&self) -> &VerifiedUsbTransfer {
        &self.transfer
    }
    pub fn copy_verified(&mut self) {
        self.transfer.copy_completed();
        self.transfer.verified();
    }
    pub fn flush_succeeded(&mut self) {
        self.transfer.flushed();
    }
    pub fn removal_succeeded(&mut self) {
        self.transfer.removed();
    }
    pub fn failed(&mut self) {
        self.transfer.fail();
    }
    pub fn cancelled(&mut self) {
        self.transfer.cancel();
    }
}

impl VerifiedUsbTransfer {
    pub fn new(target: DeviceRemovalTarget) -> Self {
        Self {
            target,
            stage: VerifiedUsbStage::Copying,
            last_completed: None,
            destination_state: VerifiedDestinationState::NotCreated,
            terminal: None,
        }
    }
    pub const fn stage(&self) -> VerifiedUsbStage {
        self.stage
    }
    pub const fn last_completed(&self) -> Option<VerifiedUsbStage> {
        self.last_completed
    }
    pub const fn destination_state(&self) -> VerifiedDestinationState {
        self.destination_state
    }
    pub fn target(&self) -> &DeviceRemovalTarget {
        &self.target
    }
    pub fn can_say_safe_to_remove(&self) -> bool {
        self.terminal.is_none() && self.stage == VerifiedUsbStage::SafeToRemove
    }
    pub fn stage_label(&self) -> &'static str {
        match self.stage {
            VerifiedUsbStage::Copying => "Copying to removable device",
            VerifiedUsbStage::Verifying => "Verifying copied bytes",
            VerifiedUsbStage::Flushing => "Flushing selected removable device",
            VerifiedUsbStage::Removing(DeviceAction::Eject) => "Ejecting removable device",
            VerifiedUsbStage::Removing(DeviceAction::Unmount) => "Unmounting removable device",
            VerifiedUsbStage::Removing(DeviceAction::Mount) => "Removing removable device",
            VerifiedUsbStage::SafeToRemove => "Safe to remove",
        }
    }
    pub fn safe_to_remove_notice(&self) -> Option<&'static str> {
        self.can_say_safe_to_remove().then_some(
            "The selected removable device was flushed and removed. It is safe to remove.",
        )
    }
    pub fn copy_completed(&mut self) -> bool {
        if self.terminal.is_some() || self.stage != VerifiedUsbStage::Copying {
            return false;
        }
        self.last_completed = Some(VerifiedUsbStage::Copying);
        self.destination_state = VerifiedDestinationState::CopiedUnverified;
        self.stage = VerifiedUsbStage::Verifying;
        true
    }
    pub fn verified(&mut self) -> bool {
        if self.terminal.is_some() || self.stage != VerifiedUsbStage::Verifying {
            return false;
        }
        self.last_completed = Some(VerifiedUsbStage::Verifying);
        self.destination_state = VerifiedDestinationState::Verified;
        self.stage = VerifiedUsbStage::Flushing;
        true
    }
    pub fn flushed(&mut self) -> bool {
        if self.terminal.is_some() || self.stage != VerifiedUsbStage::Flushing {
            return false;
        }
        self.last_completed = Some(VerifiedUsbStage::Flushing);
        self.stage = VerifiedUsbStage::Removing(self.target.action());
        true
    }
    pub fn removed(&mut self) -> bool {
        if self.terminal.is_some()
            || !matches!(
                self.stage,
                VerifiedUsbStage::Removing(DeviceAction::Eject | DeviceAction::Unmount)
            )
        {
            return false;
        }
        self.last_completed = Some(VerifiedUsbStage::Removing(self.target.action()));
        self.stage = VerifiedUsbStage::SafeToRemove;
        true
    }
    pub fn fail(&mut self) {
        self.terminal = Some(VerifiedUsbTerminal::Failed);
    }
    pub fn cancel(&mut self) {
        self.terminal = Some(VerifiedUsbTerminal::Cancelled);
    }
}

#[derive(Debug, Error)]
pub enum DeviceFlushError {
    #[error("verified destination is outside the selected mount")]
    OutsideMount,
    #[error("mount or destination identity changed before flush")]
    IdentityChanged,
    #[error("selected removable device changed before flush")]
    DeviceChanged,
    #[error("could not flush selected mount")]
    Io(#[source] std::io::Error),
}

const VERIFIED_USB_FLUSH_QUEUE_CAPACITY: usize = 1;

#[derive(Debug)]
struct DeviceFlushTask {
    child_job: floe_core::JobId,
    transfer: VerifiedUsbTransfer,
    snapshots: Vec<DeviceSnapshot>,
    destination: PathBuf,
}

#[derive(Debug)]
enum DeviceFlushCommand {
    Flush(DeviceFlushTask),
    Shutdown,
}

#[derive(Debug)]
pub struct DeviceFlushResult {
    child_job: floe_core::JobId,
    transfer: VerifiedUsbTransfer,
    result: Result<(), DeviceFlushError>,
}

impl DeviceFlushResult {
    pub const fn child_job(&self) -> floe_core::JobId {
        self.child_job
    }

    pub fn transfer(&self) -> &VerifiedUsbTransfer {
        &self.transfer
    }

    pub fn into_parts(self) -> (VerifiedUsbTransfer, Result<(), DeviceFlushError>) {
        (self.transfer, self.result)
    }
}

#[derive(Debug, Error)]
pub enum DeviceFlushSubmitError {
    #[error("another verified removable-device flush is already queued")]
    Busy,
    #[error("the verified removable-device flush worker has stopped")]
    Stopped,
}

/// One-worker, one-slot executor for the blocking `syncfs` boundary. It keeps
/// filesystem work off GTK while preventing unbounded flush threads or queues.
#[derive(Debug)]
pub struct DeviceFlushWorker {
    sender: Option<SyncSender<DeviceFlushCommand>>,
    results: Receiver<DeviceFlushResult>,
    worker: Option<JoinHandle<()>>,
}

impl DeviceFlushWorker {
    pub fn spawn() -> std::io::Result<Self> {
        let (sender, receiver) =
            mpsc::sync_channel::<DeviceFlushCommand>(VERIFIED_USB_FLUSH_QUEUE_CAPACITY);
        let (result_sender, results) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("floe-verified-usb-flush".to_owned())
            .spawn(move || {
                while let Ok(command) = receiver.recv() {
                    match command {
                        DeviceFlushCommand::Flush(mut task) => {
                            let result = revalidate_and_flush(
                                &mut task.transfer,
                                &task.snapshots,
                                &task.destination,
                            );
                            if result_sender
                                .send(DeviceFlushResult {
                                    child_job: task.child_job,
                                    transfer: task.transfer,
                                    result,
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                        DeviceFlushCommand::Shutdown => break,
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            results,
            worker: Some(worker),
        })
    }

    pub fn submit(
        &self,
        child_job: floe_core::JobId,
        transfer: VerifiedUsbTransfer,
        snapshots: Vec<DeviceSnapshot>,
        destination: PathBuf,
    ) -> Result<(), DeviceFlushSubmitError> {
        let command = DeviceFlushCommand::Flush(DeviceFlushTask {
            child_job,
            transfer,
            snapshots,
            destination,
        });
        match self
            .sender
            .as_ref()
            .ok_or(DeviceFlushSubmitError::Stopped)?
            .try_send(command)
        {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(DeviceFlushSubmitError::Busy),
            Err(TrySendError::Disconnected(_)) => Err(DeviceFlushSubmitError::Stopped),
        }
    }

    pub fn try_result(&self) -> Option<DeviceFlushResult> {
        self.results.try_recv().ok()
    }
}

impl Drop for DeviceFlushWorker {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(DeviceFlushCommand::Shutdown);
        }
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("verified removable-device flush worker panicked during shutdown");
        }
    }
}

/// Revalidates the GIO-derived target relationship, flushes only its exact
/// mount, then advances to the only state from which GIO removal may begin.
pub fn revalidate_and_flush(
    transfer: &mut VerifiedUsbTransfer,
    snapshots: &[DeviceSnapshot],
    destination: &Path,
) -> Result<(), DeviceFlushError> {
    if transfer.stage() != VerifiedUsbStage::Flushing
        || !revalidate_removal_target(transfer.target(), snapshots)
    {
        return Err(DeviceFlushError::DeviceChanged);
    }
    flush_verified_mount(transfer.target().mount_root(), destination)?;
    transfer.flushed();
    Ok(())
}

/// Opens exactly the selected local mount without following a symlink, checks
/// the destination is still on that mount, then `syncfs`s that mount only.
pub fn flush_verified_mount(mount_root: &Path, destination: &Path) -> Result<(), DeviceFlushError> {
    if !destination.starts_with(mount_root) {
        return Err(DeviceFlushError::OutsideMount);
    }
    let root = fs::symlink_metadata(mount_root).map_err(DeviceFlushError::Io)?;
    let destination = fs::symlink_metadata(destination).map_err(DeviceFlushError::Io)?;
    if root.file_type().is_symlink()
        || destination.file_type().is_symlink()
        || root.dev() != destination.dev()
    {
        return Err(DeviceFlushError::IdentityChanged);
    }
    let descriptor = rustix::fs::open(
        mount_root,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|source| {
        DeviceFlushError::Io(std::io::Error::from_raw_os_error(source.raw_os_error()))
    })?;
    let opened = rustix::fs::fstat(&descriptor).map_err(|source| {
        DeviceFlushError::Io(std::io::Error::from_raw_os_error(source.raw_os_error()))
    })?;
    if opened.st_dev != root.dev() {
        return Err(DeviceFlushError::IdentityChanged);
    }
    rustix::fs::syncfs(&descriptor).map_err(|source| {
        DeviceFlushError::Io(std::io::Error::from_raw_os_error(source.raw_os_error()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::devices::{DeviceId, DeviceRemovalTarget};

    fn target() -> DeviceRemovalTarget {
        DeviceRemovalTarget::new_for_test(
            DeviceId::new_for_test("device"),
            PathBuf::from("/tmp/mount"),
            DeviceAction::Eject,
        )
    }

    #[test]
    fn phase_18w_workflow_orders_states_and_never_claims_safe_before_removal() {
        let mut transfer = VerifiedUsbTransfer::new(target());
        assert!(!transfer.can_say_safe_to_remove());
        transfer.copy_completed();
        transfer.verified();
        transfer.flushed();
        assert_eq!(
            transfer.stage(),
            VerifiedUsbStage::Removing(DeviceAction::Eject)
        );
        assert!(!transfer.can_say_safe_to_remove());
        transfer.removed();
        assert!(transfer.can_say_safe_to_remove());
    }

    #[test]
    fn phase_18w_safety_rejects_out_of_order_or_terminal_transitions() {
        let mut transfer = VerifiedUsbTransfer::new(target());
        assert!(!transfer.verified());
        assert!(!transfer.flushed());
        assert!(!transfer.removed());
        assert!(!transfer.can_say_safe_to_remove());

        assert!(transfer.copy_completed());
        transfer.cancel();
        assert!(!transfer.verified());
        assert!(!transfer.flushed());
        assert!(!transfer.removed());
        assert!(!transfer.can_say_safe_to_remove());

        let mount_target = DeviceRemovalTarget::new_for_test(
            DeviceId::new_for_test("invalid-mount-action"),
            PathBuf::from("/tmp/mount"),
            DeviceAction::Mount,
        );
        let mut mount_transfer = VerifiedUsbTransfer::new(mount_target);
        assert!(mount_transfer.copy_completed());
        assert!(mount_transfer.verified());
        assert!(mount_transfer.flushed());
        assert!(!mount_transfer.removed());
        assert!(!mount_transfer.can_say_safe_to_remove());
    }

    #[test]
    fn phase_18w_workflow_failure_and_cancel_preserve_last_completed_stage() {
        let mut failed = VerifiedUsbTransfer::new(target());
        failed.copy_completed();
        failed.verified();
        failed.fail();
        assert_eq!(failed.last_completed(), Some(VerifiedUsbStage::Verifying));
        assert!(!failed.can_say_safe_to_remove());
        let mut cancelled = VerifiedUsbTransfer::new(target());
        cancelled.copy_completed();
        cancelled.cancel();
        assert_eq!(
            cancelled.destination_state(),
            VerifiedDestinationState::CopiedUnverified
        );
        assert!(!cancelled.can_say_safe_to_remove());
    }

    #[test]
    fn phase_18w_safety_rejects_destination_outside_exact_mount_before_flush() {
        let fixture = tempfile::tempdir().expect("fixture");
        let mount = fixture.path().join("mount");
        let outside = fixture.path().join("outside");
        fs::create_dir(&mount).expect("mount");
        fs::write(&outside, b"outside").expect("outside");
        assert!(matches!(
            flush_verified_mount(&mount, &outside),
            Err(DeviceFlushError::OutsideMount)
        ));
    }

    #[test]
    fn phase_18w_safety_rejects_replaced_destination_symlink_before_flush() {
        let fixture = tempfile::tempdir().expect("temporary fixture");
        let mount = fixture.path().join("mount");
        fs::create_dir(&mount).expect("mount fixture");
        let target = mount.join("target");
        fs::write(&target, b"target").expect("target fixture");
        let destination = mount.join("destination");
        std::os::unix::fs::symlink(&target, &destination).expect("destination symlink");
        assert!(matches!(
            flush_verified_mount(&mount, &destination),
            Err(DeviceFlushError::IdentityChanged)
        ));
    }

    #[test]
    fn phase_18w_ui_labels_never_expose_safe_removal_before_successful_eject() {
        let mut transfer = VerifiedUsbTransfer::new(target());
        for advance in [0, 1, 2, 3] {
            if advance == 1 {
                transfer.copy_completed();
            }
            if advance == 2 {
                transfer.verified();
            }
            if advance == 3 {
                transfer.flushed();
            }
            assert!(!transfer.stage_label().is_empty());
            assert!(transfer.safe_to_remove_notice().is_none());
        }
        transfer.removed();
        assert_eq!(transfer.stage_label(), "Safe to remove");
        assert!(transfer.safe_to_remove_notice().is_some());
    }

    #[test]
    fn phase_18w_workflow_child_job_handoff_requires_flush_and_removal_completion() {
        let job = floe_core::JobId::new(std::num::NonZeroU64::new(42).expect("job id"));
        let mut workflow = VerifiedUsbWorkflow::new(job, target());
        workflow.copy_verified();
        assert_eq!(workflow.transfer().stage(), VerifiedUsbStage::Flushing);
        workflow.flush_succeeded();
        assert!(!workflow.transfer().can_say_safe_to_remove());
        workflow.removal_succeeded();
        assert!(workflow.transfer().can_say_safe_to_remove());
    }
}
