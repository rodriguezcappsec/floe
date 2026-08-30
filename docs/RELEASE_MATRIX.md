# Release Candidate Environment Matrix

This matrix records what the Phase 21D candidate actually verified. Floe's
portable contract is GTK4/GIO/XDG on Wayland; a compositor name does not unlock
hidden filesystem behavior. Niri-specific and Plasma-specific integrations are
still deferred by product decision.

| Environment | Candidate evidence | Result and limits |
| --- | --- | --- |
| Generic Wayland | Isolated private HOME/XDG launch, application D-Bus `Peer.Ping`, exported Quit action, and exit status 0 | Verified through the portable contract on the current Wayland host; native semantic input remains unavailable because Dogtail/pyatspi is not installed. |
| Niri | Same generic GTK/GIO application contract; no Niri IPC dependency | Prior active-Niri lifecycle evidence remains recorded in development gates; Niri was not verified on this host during Phase 21D. Niri-specific integration is intentionally deferred. |
| KDE Plasma Wayland | Isolated session bus and private HOME/XDG roots on `XDG_CURRENT_DESKTOP=KDE`, release binary `Peer.Ping`, `org.gtk.Actions` Quit, and exit status 0 | Verified on the Phase 21D host. Plasma-specific integration remains intentionally deferred. |

The release candidate is blocked by a failed launch, failed `Peer.Ping`, unclean
Quit, data-loss/security-critical defect, unresolved supported dependency
advisory, or irreproducible source archive. Missing compositor-specific features
are documented limitations, not false generic-Wayland failures.

The environment matrix does not replace the Rust, GTK component, native E2E,
packaging, recovery, or documentation gates. See [Development](DEVELOPMENT.md),
[Recovery](RECOVERY.md), and [Security and Privacy](PRIVACY_SECURITY.md).
