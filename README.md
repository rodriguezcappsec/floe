# Floe

Floe is an early, Wayland-first spatial file manager for Linux. This bootstrap
contains a read-only local directory browser with a GTK-independent core and a
GTK4/libadwaita application shell.

## Build and run

The development packages for GTK 4 and libadwaita are required.

```bash
cargo run -p floe-app
```

Floe follows the system color scheme. The initial appearance architecture can
be exercised without a settings UI by setting `FLOE_APPEARANCE` to `native`,
`glass`, `frosted`, `minimal`, or `compact` before launch. Glass and frosted
presets use readable translucent surfaces; actual compositor blur is neither
required nor simulated.

## Current scope

- Local read-only directory enumeration on a background worker
- Back, forward, parent, location, and hidden-file navigation
- XDG user locations
- Virtualized list rows backed by original `PathBuf` values
- Compact, user-resizable Places sidebar
- Explicit selection with Enter/double-click activation
- Asynchronous regular-file opening through GIO's default application
- GTK-independent path-safe copy requests with explicit fail-on-conflict and
  preserve-or-reject symlink policies
- Fixed-capacity background copy execution connected to application-owned job
  lifecycle events, cancellation, failure mapping, and retry identity
- Application-owned internal copy buffer with Ctrl+C/Ctrl+V copy-only workflow
- Compact non-modal Operations Island with progress, cancellation, completion,
  conflict, and failure feedback

Phase 4B copy/paste works within the running Floe application. Cross-application
clipboard formats and overwrite are not implemented. Move, rename, trash,
previews, tabs, split view, Miller columns, and environment-specific
integrations remain deferred.

## Project documentation

- [`DESIGN.md`](DESIGN.md) — implemented and planned visual/interaction system
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — current code and data flow
- [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) — build, run, test, and troubleshoot
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — completed, next, and later milestones
