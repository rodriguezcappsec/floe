//! Generic Linux desktop capability boundary.
//!
//! This module deliberately lives in `floe-app`: desktop services are an
//! application concern and must not leak into the filesystem core.

mod generic;

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use adw::prelude::*;
use gtk::glib;

pub use generic::{GenericDesktopFacts, GenericDesktopProbe, GioSessionBusProbe};

const REQUEST_CAPACITY: usize = 1;
const RESPONSE_CAPACITY: usize = 1;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DesktopCapabilityId {
    Launch,
    MountsAndVolumes,
    XdgUserDirectories,
    Portals,
    Notifications,
    Share,
    ThemeSignals,
    CredentialService,
    SessionLockSignals,
}

impl DesktopCapabilityId {
    pub const ALL: [Self; 9] = [
        Self::Launch,
        Self::MountsAndVolumes,
        Self::XdgUserDirectories,
        Self::Portals,
        Self::Notifications,
        Self::Share,
        Self::ThemeSignals,
        Self::CredentialService,
        Self::SessionLockSignals,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Launch => "Opening files and URIs",
            Self::MountsAndVolumes => "Drives and mounted volumes",
            Self::XdgUserDirectories => "Standard user folders",
            Self::Portals => "Desktop portals",
            Self::Notifications => "Desktop notifications",
            Self::Share => "Share",
            Self::ThemeSignals => "Desktop appearance signals",
            Self::CredentialService => "Credential service",
            Self::SessionLockSignals => "Session-lock signals",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopCapabilityStatus {
    Available,
    Degraded,
    Unavailable,
}

impl DesktopCapabilityStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Available => "Available",
            Self::Degraded => "Limited",
            Self::Unavailable => "Unavailable",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopCapability {
    pub id: DesktopCapabilityId,
    pub status: DesktopCapabilityStatus,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopIntegrationSnapshot {
    pub generation: u64,
    pub capabilities: Vec<DesktopCapability>,
}

impl DesktopIntegrationSnapshot {
    #[cfg(test)]
    pub fn capability(&self, id: DesktopCapabilityId) -> Option<&DesktopCapability> {
        self.capabilities
            .iter()
            .find(|capability| capability.id == id)
    }

    fn unavailable(generation: u64, reason: &str) -> Self {
        Self {
            generation,
            capabilities: DesktopCapabilityId::ALL
                .into_iter()
                .map(|id| DesktopCapability {
                    id,
                    status: DesktopCapabilityStatus::Unavailable,
                    reason: reason.to_owned(),
                })
                .collect(),
        }
    }
}

struct ProbeRequest {
    generation: u64,
    facts: GenericDesktopFacts,
}

pub struct DesktopIntegrationWorker {
    sender: Option<SyncSender<ProbeRequest>>,
    responses: Receiver<DesktopIntegrationSnapshot>,
    latest_generation: Arc<AtomicU64>,
    worker: Option<JoinHandle<()>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopIntegrationSubmitError {
    Busy,
    Stopped,
}

impl DesktopIntegrationWorker {
    pub fn spawn() -> std::io::Result<Self> {
        Self::spawn_with_probe(GioSessionBusProbe)
    }

    fn spawn_with_probe<P: GenericDesktopProbe>(probe: P) -> std::io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<ProbeRequest>(REQUEST_CAPACITY);
        let (response_sender, responses) =
            mpsc::sync_channel::<DesktopIntegrationSnapshot>(RESPONSE_CAPACITY);
        let latest_generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&latest_generation);
        let worker = thread::Builder::new()
            .name("floe-desktop-integration".to_owned())
            .spawn(move || {
                while let Ok(request) = requests.recv() {
                    if worker_generation.load(Ordering::Acquire) != request.generation {
                        continue;
                    }
                    let services = probe.probe();
                    if worker_generation.load(Ordering::Acquire) != request.generation {
                        continue;
                    }
                    let snapshot =
                        generic::build_snapshot(request.generation, request.facts, services);
                    match response_sender.try_send(snapshot) {
                        Ok(()) => {}
                        Err(TrySendError::Full(_)) => {}
                        Err(TrySendError::Disconnected(_)) => break,
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            responses,
            latest_generation,
            worker: Some(worker),
        })
    }

    pub fn submit(
        &self,
        generation: u64,
        facts: GenericDesktopFacts,
    ) -> Result<(), DesktopIntegrationSubmitError> {
        self.latest_generation.store(generation, Ordering::Release);
        let Some(sender) = self.sender.as_ref() else {
            return Err(DesktopIntegrationSubmitError::Stopped);
        };
        match sender.try_send(ProbeRequest { generation, facts }) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(DesktopIntegrationSubmitError::Busy),
            Err(TrySendError::Disconnected(_)) => Err(DesktopIntegrationSubmitError::Stopped),
        }
    }

    pub fn try_snapshot(&self) -> Option<DesktopIntegrationSnapshot> {
        self.responses.try_recv().ok()
    }
}

impl Drop for DesktopIntegrationWorker {
    fn drop(&mut self) {
        self.latest_generation.store(u64::MAX, Ordering::Release);
        self.sender.take();
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("desktop integration worker panicked during shutdown");
        }
    }
}

pub struct DesktopIntegrationController {
    window: adw::ApplicationWindow,
    dialog: adw::Dialog,
    list: gtk::ListBox,
    summary: gtk::Label,
    refresh: gtk::Button,
    snapshot: RefCell<DesktopIntegrationSnapshot>,
    worker: RefCell<Option<DesktopIntegrationWorker>>,
    generation: Cell<u64>,
    poll: RefCell<Option<glib::SourceId>>,
}

impl DesktopIntegrationController {
    pub fn new(window: &adw::ApplicationWindow) -> Rc<Self> {
        let dialog = adw::Dialog::builder()
            .title("Desktop Integration")
            .content_width(620)
            .content_height(560)
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(18);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);

        let heading = gtk::Label::builder()
            .label("Desktop Integration")
            .xalign(0.0)
            .css_classes(["title-2"])
            .build();
        let explanation = gtk::Label::builder()
            .label("Floe uses standard Linux desktop services when they are available. Missing optional services do not prevent normal local browsing.")
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build();
        let summary = gtk::Label::builder().xalign(0.0).wrap(true).build();
        summary.update_property(&[gtk::accessible::Property::Label(
            "Desktop integration probe status",
        )]);
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        list.update_property(&[
            gtk::accessible::Property::Label("Desktop integration capabilities"),
            gtk::accessible::Property::Description(
                "Availability and limitations for standard Linux desktop services",
            ),
        ]);
        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&list)
            .build();
        let refresh = gtk::Button::with_label("Refresh Status");
        refresh.update_property(&[gtk::accessible::Property::Description(
            "Check standard desktop services again without blocking the window",
        )]);

        content.append(&heading);
        content.append(&explanation);
        content.append(&summary);
        content.append(&scroll);
        content.append(&refresh);
        dialog.set_child(Some(&content));

        let worker = DesktopIntegrationWorker::spawn()
            .map_err(|error| {
                tracing::warn!(%error, "could not start desktop integration probe worker");
                error
            })
            .ok();
        let controller = Rc::new(Self {
            window: window.clone(),
            dialog,
            list,
            summary,
            refresh,
            snapshot: RefCell::new(DesktopIntegrationSnapshot::unavailable(
                0,
                "Capability status has not been checked yet.",
            )),
            worker: RefCell::new(worker),
            generation: Cell::new(0),
            poll: RefCell::new(None),
        });
        controller.render();
        let weak = Rc::downgrade(&controller);
        controller.refresh.connect_clicked(move |_| {
            if let Some(controller) = weak.upgrade() {
                controller.refresh_async();
            }
        });
        controller.refresh_async();
        controller
    }

    pub fn present(&self) {
        self.dialog.present(Some(&self.window));
    }

    fn refresh_async(self: &Rc<Self>) {
        if self.poll.borrow().is_some() {
            self.summary
                .set_label("A desktop integration check is already running.");
            return;
        }
        let generation = self.generation.get().wrapping_add(1).max(1);
        self.generation.set(generation);
        let worker = self.worker.borrow();
        let Some(worker) = worker.as_ref() else {
            self.summary.set_label("Desktop integration checks are unavailable; normal local browsing remains available.");
            return;
        };
        match worker.submit(generation, GenericDesktopFacts::compiled()) {
            Ok(()) => self
                .summary
                .set_label("Checking standard desktop services…"),
            Err(DesktopIntegrationSubmitError::Busy) => {
                self.summary
                    .set_label("A desktop integration check is already queued.");
                return;
            }
            Err(DesktopIntegrationSubmitError::Stopped) => {
                self.summary.set_label(
                    "Desktop integration checks stopped; normal local browsing remains available.",
                );
                return;
            }
        }

        let weak = Rc::downgrade(self);
        let source = glib::timeout_add_local(Duration::from_millis(25), move || {
            let Some(controller) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let snapshot = controller
                .worker
                .borrow()
                .as_ref()
                .and_then(DesktopIntegrationWorker::try_snapshot);
            let Some(snapshot) = snapshot else {
                return glib::ControlFlow::Continue;
            };
            if snapshot.generation == controller.generation.get() {
                *controller.snapshot.borrow_mut() = snapshot;
                controller.render();
            }
            controller.poll.borrow_mut().take();
            glib::ControlFlow::Break
        });
        *self.poll.borrow_mut() = Some(source);
    }

    fn render(&self) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let snapshot = self.snapshot.borrow();
        for capability in &snapshot.capabilities {
            self.list.append(&capability_row(capability));
        }
        let available = snapshot
            .capabilities
            .iter()
            .filter(|capability| capability.status == DesktopCapabilityStatus::Available)
            .count();
        let limited = snapshot
            .capabilities
            .iter()
            .filter(|capability| capability.status == DesktopCapabilityStatus::Degraded)
            .count();
        self.summary.set_label(&format!(
            "{available} available, {limited} limited, {} unavailable. Local browsing remains independent of optional desktop services.",
            snapshot.capabilities.len().saturating_sub(available + limited)
        ));
    }
}

impl Drop for DesktopIntegrationController {
    fn drop(&mut self) {
        if let Some(source) = self.poll.get_mut().take() {
            source.remove();
        }
    }
}

fn capability_row(capability: &DesktopCapability) -> gtk::ListBoxRow {
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    let title = gtk::Label::builder()
        .label(format!(
            "{} — {}",
            capability.id.label(),
            capability.status.label()
        ))
        .xalign(0.0)
        .build();
    let reason = gtk::Label::builder()
        .label(&capability.reason)
        .xalign(0.0)
        .wrap(true)
        .css_classes(["dim-label"])
        .build();
    text.append(&title);
    text.append(&reason);
    let row = gtk::ListBoxRow::builder()
        .selectable(false)
        .child(&text)
        .build();
    row.update_property(&[
        gtk::accessible::Property::Label(&format!(
            "{}, {}",
            capability.id.label(),
            capability.status.label()
        )),
        gtk::accessible::Property::Description(&capability.reason),
    ]);
    row
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use super::*;
    use crate::integration::generic::GenericServiceProbe;

    struct DelayedProbe(GenericServiceProbe);

    impl GenericDesktopProbe for DelayedProbe {
        fn probe(&self) -> GenericServiceProbe {
            thread::sleep(Duration::from_millis(20));
            self.0
        }
    }

    fn receive(worker: &DesktopIntegrationWorker) -> DesktopIntegrationSnapshot {
        for _ in 0..200 {
            if let Some(snapshot) = worker.try_snapshot() {
                return snapshot;
            }
            thread::sleep(Duration::from_millis(2));
        }
        panic!("desktop integration worker did not return a snapshot");
    }

    #[test]
    fn phase_14_integration_model_has_stable_complete_order_and_plain_reasons() {
        let snapshot = generic::build_snapshot(
            8,
            GenericDesktopFacts::compiled(),
            GenericServiceProbe::all_available(),
        );
        assert_eq!(snapshot.generation, 8);
        assert_eq!(snapshot.capabilities.len(), DesktopCapabilityId::ALL.len());
        assert_eq!(
            snapshot
                .capabilities
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            DesktopCapabilityId::ALL
        );
        assert!(
            snapshot
                .capabilities
                .iter()
                .all(|item| !item.reason.is_empty())
        );
        assert_eq!(
            snapshot
                .capability(DesktopCapabilityId::SessionLockSignals)
                .map(|item| item.status),
            Some(DesktopCapabilityStatus::Degraded)
        );
    }

    #[test]
    fn phase_14_integration_worker_is_bounded_generation_safe_and_stops_cleanly() {
        let worker = DesktopIntegrationWorker::spawn_with_probe(DelayedProbe(
            GenericServiceProbe::all_available(),
        ))
        .expect("worker");
        worker
            .submit(4, GenericDesktopFacts::compiled())
            .expect("submit");
        loop {
            match worker.submit(5, GenericDesktopFacts::compiled()) {
                Ok(()) => break,
                Err(DesktopIntegrationSubmitError::Busy) => {
                    thread::sleep(Duration::from_millis(1));
                }
                Err(DesktopIntegrationSubmitError::Stopped) => panic!("worker stopped"),
            }
        }
        let snapshot = receive(&worker);
        assert_eq!(snapshot.generation, 5);
        assert_eq!(snapshot.capabilities.len(), 9);
        assert!(std::mem::size_of::<DesktopIntegrationSnapshot>() < 128);
        drop(worker);
    }

    #[test]
    fn phase_14_generic_fallback_keeps_compiled_local_services_available() {
        let snapshot = generic::build_snapshot(
            1,
            GenericDesktopFacts::compiled(),
            GenericServiceProbe::unavailable(),
        );
        for id in [
            DesktopCapabilityId::Launch,
            DesktopCapabilityId::MountsAndVolumes,
            DesktopCapabilityId::XdgUserDirectories,
            DesktopCapabilityId::ThemeSignals,
        ] {
            assert_eq!(
                snapshot.capability(id).map(|item| item.status),
                Some(DesktopCapabilityStatus::Available)
            );
        }
        assert_eq!(
            snapshot
                .capability(DesktopCapabilityId::Portals)
                .map(|item| item.status),
            Some(DesktopCapabilityStatus::Unavailable)
        );
    }

    #[test]
    fn phase_14_integration_ui_rows_are_deterministic_and_non_color_only() {
        let snapshot = generic::build_snapshot(
            2,
            GenericDesktopFacts::compiled(),
            GenericServiceProbe::unavailable(),
        );
        let rows = snapshot
            .capabilities
            .iter()
            .map(|item| (item.id.label(), item.status.label(), item.reason.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 9);
        assert_eq!(rows[0].0, "Opening files and URIs");
        assert!(
            rows.iter()
                .all(|(_, status, reason)| !status.is_empty() && !reason.is_empty())
        );
    }

    #[test]
    #[ignore = "requires a real GTK display"]
    fn phase_14_integration_ui_real_gtk_dialog_is_accessible_and_refreshable() {
        gtk::init().expect("GTK component gate requires an available display");
        adw::init().expect("libadwaita must initialize in GTK component gate");
        let application = adw::Application::builder()
            .application_id("io.github.rodriguezcappsec.Floe.Phase14GtkTest")
            .build();
        application
            .register(None::<&gtk::gio::Cancellable>)
            .expect("component-test application must register before creating a window");
        let window = adw::ApplicationWindow::builder()
            .application(&application)
            .build();
        let controller = DesktopIntegrationController::new(&window);
        assert_eq!(controller.list.observe_children().n_items(), 9);
        assert_eq!(
            controller.refresh.label().as_deref(),
            Some("Refresh Status")
        );
        assert!(!controller.summary.label().is_empty());
        window.present();
        while gtk::glib::MainContext::default().iteration(false) {}
        controller.present();
        while gtk::glib::MainContext::default().iteration(false) {}
        assert!(controller.dialog.is_visible());
        controller.dialog.close();
    }
}
