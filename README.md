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
- Explicit destination-conflict decisions to keep existing or retry with a
 validated sibling filename; generic Retry cannot repeat a conflict
- Focused non-blocking conflict dialog with inline filename validation and a
 persistent Operations Island Resolve Conflict action after dismissal
- Native row context menu with Open, Copy, Cut, Rename, and Move to Trash
- Asynchronous GIO Open With discovery, explicit app launching, and default
  association changes

Phase 4D copy/move/paste and rename work within the running Floe application.
Phase 4F exposes the verified trash job as a single-selection, recoverable
desktop Trash action. Permanent delete, Shift+Delete, bulk trash, and built-in
restore/undo remain unavailable.
Phase 5B exposes that retry infrastructure through the Operations Island. Failed
or cancelled jobs remain visible with a Retry button; completed jobs dismiss
normally and cannot be retried. Destination conflicts instead open a focused
decision dialog; dismissing it leaves an accessible Resolve Conflict action.

Phase 5C adds secondary-click and Shift+F10/Menu-key access to a native context
menu. It selects the targeted virtualized row first and reuses the existing
selection-sensitive window actions. Open follows the current GIO default
application. Phase 5D adds Open With for regular files and non-directory links:
it resolves the content type asynchronously, identifies the current default,
lists compatible desktop applications, and changes the default only through an
explicit button. Custom external tools remain future work. Phase 5E adds an
application-layer conflict contract retaining exact paths and stable logical
operation identity. Revised copy/move/rename attempts receive a fresh job ID
and remain fail-if-exists. Phase 5F presents those decisions without blocking
other file-manager work. The retry field starts empty and accepts one valid,
different filename; no lossy display path is submitted automatically.
Cross-application clipboard formats, overwrite, apply-to-all,
cross-filesystem copy-delete moves, trash restore/bulk UI, permanent
deletion, previews, tabs, split view, Miller columns, and environment-specific
integrations remain deferred.

## Project documentation

- [`DESIGN.md`](DESIGN.md) — implemented and planned visual/interaction system
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — current code and data flow
- [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) — build, run, test, and troubleshoot
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — completed, next, and later milestones
