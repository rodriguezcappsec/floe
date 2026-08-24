# Floe Roadmap

This roadmap summarizes implementation status. `AGENTS.md` remains the source
of truth for sequencing and current-session handoff.

## Completed

### Phase 0 — Workspace and application bootstrap

- Cargo workspace with `floe-core` and `floe-app`.
- GTK4/libadwaita application shell, tracing, appearance presets, and XDG Places.
- GTK-independent paths, models, errors, navigation, and tests.

### Phase 1 — Read-only directory browsing

- Non-recursive, cancellable background enumeration.
- Back, forward, parent, location, and hidden-file navigation.
- Virtualized list with bounded GTK insertion batches.
- Resizable Places/content split pane and basic status/error/empty feedback.

### Phase 2 — Selection and basic file interaction

- Controller-owned single selection mirrored from GTK.
- Enter/double-click and visible Open activation.
- Asynchronous default-application launch through GIO.
- Non-UTF-8 path preservation through the launch URI.

### Phase 3 — Job lifecycle foundation

- Strong logical-operation and execution-attempt identifiers.
- Validated progress, structured failures, lifecycle commands, and events.
- Tested legal queued/running/paused/terminal transitions.
- Application-owned registry, event queue, and retry-attempt identity.
- No mutation UI.

### Phase 4A — Safe copy engine

- Path-safe exact-source/exact-destination `CopyRequest` values.
- Explicit `ConflictPolicy::FailIfExists`; overwrite is unavailable.
- Explicit `SymlinkPolicy::Preserve` or `Reject`; links are never followed.
- Recursive file/directory copy with chunk-level cancellation and cleanup.
- Fixed-capacity, single-worker application executor connected to job events.
- Temporary-directory tests for success, conflict, cancellation, capacity,
  retry identity, symlinks, self-copy rejection, and non-UTF-8 names.

### Phase 4B — Copy interaction and operation observation

- Initial Floe-internal copy-only transfer buffer retaining original paths.
- Ctrl+C/Ctrl+V staging and paste submission through application state.
- Non-blocking structured job observation in a separate GTK controller.
- Compact Operations Island with progress, cancellation, terminal feedback,
  and destination refresh after completion.
- Explicit fail-if-exists behavior; no silent overwrite.
- Cross-application clipboard formats remain deferred.

### Phase 4C — Move and rename foundation

- Exact-path `MoveRequest` and same-parent `RenameRequest` models.
- Raw `PathBuf`/`OsString` preservation, including non-UTF-8 names.
- Linux atomic no-replace execution for files, directories, and symlinks.
- Explicit cancellation boundary and structured cross-filesystem failure.
- Fixed-capacity application executor with lifecycle events and shutdown.
- Temporary-directory core and executor tests; no GTK controls yet.

### Phase 4D — Move and rename interaction

- Unified internal copy/move transfer buffer preserving original paths.
- Ctrl+X/Ctrl+V move workflow and F2 rename command.
- Visible file-actions menu as a pointer alternative to shortcuts.
- Focused rename dialog with inline validation and retained focus on errors.
- Generic copy/move/rename Operations Island feedback and cancellation.
- Affected-directory refresh and recovery-oriented conflict/cross-device text.

## Next

### Phase 4E — Trash foundation

Create branch `phase-4e-trash-foundation`. Design and implement an
application-layer, standards-based trash job using GIO/XDG behavior, with
original paths, structured failures, cancellation boundaries, and tests. Keep
permanent delete and any GTK Delete shortcut deferred until trash recovery and
confirmation semantics are verified.

## Later

### Remaining Phase 4 and Phase 5 — Mutations and resilience

- Trash operation model/executor and cross-filesystem move recovery.
- Pause where meaningful, richer retry, and conflict resolution.
- Explicit overwrite policy and understandable partial-failure reporting.

### Phase 6 — List/grid polish and thumbnails

- Lazy bounded thumbnail generation and cache policy.
- Denser metadata presentation and large-directory performance work.
- Grid view sharing the same domain entries and selection model.

### Phase 7 — Tabs and split view

Multiple browser contexts with clear active-view focus and independent history.

### Phase 8 — Miller/column navigation

Floe's signature spatial, horizontal directory navigation for Niri, Plasma, and
generic Wayland environments.

### Phases 9–11 — Preview and keyboard tools

- Safe Quick Preview.
- Inspector/properties.
- Searchable command palette.

### Phases 12–13 — Archives and advanced search

Job-backed archive operations plus cancellable, filterable filesystem search.

### Phase 14 — Desktop integration abstraction

Introduce the application-layer capability boundary after generic XDG/GIO/
portal behavior is understood. Core types must remain desktop-neutral.

### Phases 15–16 — Optional Niri and KDE Plasma integration

- Niri IPC and spatial workflow enhancements that fail gracefully.
- Plasma-specific facilities only where standards provide no equivalent.

### Phase 17 — Remote/network filesystems

Support justified GIO-backed or standards-based remote locations without making
the local core depend on one desktop environment.

### Phase 18 — Packaging and polish

Distribution packaging, accessibility audit, profiling, performance tuning,
documentation, recovery hardening, and release-quality visual polish.
