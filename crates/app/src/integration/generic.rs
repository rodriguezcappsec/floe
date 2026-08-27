use std::{sync::mpsc, thread, time::Duration};

use gtk::{gio, glib, prelude::*};

use super::{
    DesktopCapability, DesktopCapabilityId, DesktopCapabilityStatus, DesktopIntegrationSnapshot,
};

const PROBE_TIMEOUT_MSEC: i32 = 1_500;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenericDesktopFacts {
    pub gio_launch: bool,
    pub gio_mounts: bool,
    pub xdg_user_directories: bool,
    pub theme_signals: bool,
}

impl GenericDesktopFacts {
    pub const fn compiled() -> Self {
        Self {
            gio_launch: true,
            gio_mounts: true,
            xdg_user_directories: true,
            theme_signals: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenericServiceProbe {
    pub session_bus: bool,
    pub portals: bool,
    pub notifications: bool,
    pub credential_service: bool,
}

impl GenericServiceProbe {
    #[cfg(test)]
    pub const fn all_available() -> Self {
        Self {
            session_bus: true,
            portals: true,
            notifications: true,
            credential_service: true,
        }
    }

    #[cfg(test)]
    pub const fn unavailable() -> Self {
        Self {
            session_bus: false,
            portals: false,
            notifications: false,
            credential_service: false,
        }
    }
}

pub trait GenericDesktopProbe: Send + 'static {
    fn probe(&self) -> GenericServiceProbe;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GioSessionBusProbe;

impl GenericDesktopProbe for GioSessionBusProbe {
    fn probe(&self) -> GenericServiceProbe {
        let Ok(connection) = bounded_session_bus() else {
            return GenericServiceProbe {
                session_bus: false,
                portals: false,
                notifications: false,
                credential_service: false,
            };
        };
        GenericServiceProbe {
            session_bus: true,
            portals: name_has_owner(&connection, "org.freedesktop.portal.Desktop"),
            notifications: name_has_owner(&connection, "org.freedesktop.Notifications"),
            credential_service: name_has_owner(&connection, "org.freedesktop.secrets"),
        }
    }
}

fn bounded_session_bus() -> Result<gio::DBusConnection, glib::Error> {
    let cancellable = gio::Cancellable::new();
    let timer_cancellable = cancellable.clone();
    let (finished, wait) = mpsc::sync_channel(1);
    let timer = thread::Builder::new()
        .name("floe-desktop-probe-timeout".to_owned())
        .spawn(move || {
            if wait
                .recv_timeout(Duration::from_millis(PROBE_TIMEOUT_MSEC as u64))
                .is_err()
            {
                timer_cancellable.cancel();
            }
        });
    let Ok(timer) = timer else {
        cancellable.cancel();
        return gio::bus_get_sync(gio::BusType::Session, Some(&cancellable));
    };
    let result = gio::bus_get_sync(gio::BusType::Session, Some(&cancellable));
    let _ = finished.try_send(());
    let _ = timer.join();
    result
}

fn name_has_owner(connection: &gio::DBusConnection, name: &str) -> bool {
    let parameters = glib::Variant::tuple_from_iter([name.to_variant()]);
    connection
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameHasOwner",
            Some(&parameters),
            None::<&glib::VariantTy>,
            gio::DBusCallFlags::NONE,
            PROBE_TIMEOUT_MSEC,
            None::<&gio::Cancellable>,
        )
        .ok()
        .and_then(|reply| reply.child_value(0).get::<bool>())
        .unwrap_or(false)
}

pub(super) fn build_snapshot(
    generation: u64,
    facts: GenericDesktopFacts,
    services: GenericServiceProbe,
) -> DesktopIntegrationSnapshot {
    use DesktopCapabilityId as Id;
    use DesktopCapabilityStatus as Status;

    let compiled = |id, available, yes: &str, no: &str| DesktopCapability {
        id,
        status: if available {
            Status::Available
        } else {
            Status::Unavailable
        },
        reason: if available { yes } else { no }.to_owned(),
    };
    let service = |id, available, yes: &str, no: &str| DesktopCapability {
        id,
        status: if available {
            Status::Available
        } else {
            Status::Unavailable
        },
        reason: if available { yes } else { no }.to_owned(),
    };

    DesktopIntegrationSnapshot {
        generation,
        capabilities: vec![
            compiled(
                Id::Launch,
                facts.gio_launch,
                "GIO provides local-file, URI, default-application, and Open With launching.",
                "The generic GIO launch boundary is unavailable.",
            ),
            compiled(
                Id::MountsAndVolumes,
                facts.gio_mounts,
                "GIO volume monitoring and desktop-owned mount prompts are active.",
                "Drive and volume monitoring is unavailable; local browsing still works.",
            ),
            compiled(
                Id::XdgUserDirectories,
                facts.xdg_user_directories,
                "GLib resolves XDG standard folders; individual folders may be unset.",
                "XDG standard-folder resolution is unavailable; Home remains usable.",
            ),
            service(
                Id::Portals,
                services.portals,
                "The generic XDG desktop portal service owns its session-bus name.",
                if services.session_bus {
                    "No XDG desktop portal service is currently available."
                } else {
                    "No session bus is available, so desktop portals cannot be detected."
                },
            ),
            service(
                Id::Notifications,
                services.notifications,
                "A freedesktop notification service is available; Floe has not sent a notification.",
                if services.session_bus {
                    "No freedesktop notification service is currently available."
                } else {
                    "No session bus is available, so notification service detection is unavailable."
                },
            ),
            DesktopCapability {
                id: Id::Share,
                status: if services.portals { Status::Degraded } else { Status::Unavailable },
                reason: if services.portals {
                    "Desktop portals are available, but Floe does not expose a generic Share transfer yet."
                } else {
                    "No compatible generic Share service is available; no data was transmitted."
                }
                .to_owned(),
            },
            compiled(
                Id::ThemeSignals,
                facts.theme_signals,
                "GTK and libadwaita appearance signals are active.",
                "Desktop appearance signals are unavailable; Floe keeps its readable fallback styling.",
            ),
            service(
                Id::CredentialService,
                services.credential_service,
                "A freedesktop Secret Service is available; Floe did not read or store a secret.",
                if services.session_bus {
                    "No freedesktop Secret Service is currently available."
                } else {
                    "No session bus is available, so credential-service detection is unavailable."
                },
            ),
            DesktopCapability {
                id: Id::SessionLockSignals,
                status: if services.session_bus { Status::Degraded } else { Status::Unavailable },
                reason: if services.session_bus {
                    "A session bus is available, but Floe has not established a reliable cross-desktop lock signal."
                } else {
                    "No session bus is available and no reliable session-lock signal is established."
                }
                .to_owned(),
            },
        ],
    }
}
