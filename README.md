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
- Application-owned transfer buffer with Ctrl+C copy, Ctrl+X move, and Ctrl+V
  paste
- Compact non-modal Operations Island with progress, cancellation, completion,
  conflict, and failure feedback
- GTK-independent exact-path move and same-directory rename models
- Atomic same-filesystem no-replace move/rename execution on a bounded worker
- F2 rename dialog with inline validation and visible file-actions menu
- Application-layer GIO trash request/executor foundation with cancellation,
  structured failures, and original-path preservation
- Explicit “Move to Trash” file-menu action and Delete shortcut routed through
  application state with Operations Island feedback
- Bounded terminal operation history and backend retry dispatch for copy, move,
  rename, and trash with stable logical operation identity
- Persistent accessible Retry control for failed or cancelled Operations Island
  jobs
- Native row context menu with Open, Copy, Cut, Rename, and Move to Trash

Phase 4D copy/move/paste and rename work within the running Floe application.
Phase 4F exposes the verified trash job as a single-selection, recoverable
desktop Trash action. Permanent delete, Shift+Delete, bulk trash, and built-in
restore/undo remain unavailable.
Phase 5B exposes that retry infrastructure through the Operations Island. Failed
or cancelled jobs remain visible with a Retry button; completed jobs dismiss
normally and cannot be retried. Overwrite and interactive conflict choices are
still unavailable.

Phase 5C adds secondary-click and Shift+F10/Menu-key access to a native context
menu. It selects the targeted virtualized row first and reuses the existing
selection-sensitive window actions. Open follows the current GIO default
application; Open With, association editing, and custom external tools remain
future work.
Cross-application clipboard formats, overwrite, cross-filesystem copy-delete
moves, trash restore/bulk UI, permanent deletion, previews, tabs, split view,
Miller columns, and environment-specific integrations remain deferred.

## Project documentation

- [`DESIGN.md`](DESIGN.md) — implemented and planned visual/interaction system
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — current code and data flow
- [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) — build, run, test, and troubleshoot
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — completed, next, and later milestones
