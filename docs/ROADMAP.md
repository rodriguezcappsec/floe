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

### Phase 4E — Trash foundation

- Application-layer `TrashRequest` retaining the original `PathBuf`.
- Bounded GIO trash worker using `gio::File::trash` and `gio::Cancellable`.
- Structured missing-source, permission, unsupported, I/O, capacity, shutdown,
  cancellation, and completion lifecycle behavior.
- Shared application tracking and affected-parent refresh metadata.
- Injected-backend tests that never modify real user Trash.
- No Delete shortcut, restore UI, or permanent-delete action yet.

### Phase 4F — Trash interaction

- Explicitly labelled “Move to Trash” file-actions menu item.
- Conventional Delete shortcut with native selection-sensitive action state.
- Original selected `PathBuf` submitted through `ApplicationState` only.
- Trash-specific Operations Island progress, completion, and recovery wording.
- Affected-parent refresh after confirmed completion.
- No confirmation for the recoverable Trash action; permanent delete,
  Shift+Delete, bulk trash, restore UI, and undo remain unavailable.

### Phase 5A — Operation resilience foundation

- Retry dispatch for copy, move, rename, and trash through existing bounded
  executors.
- Stable logical `OperationId` and fresh `JobId` for every retry attempt.
- Original path-safe operation requests retained without display-text rebuilds.
- Sixty-four-entry terminal operation history with terminal registry pruning.
- Completed and evicted entries reject retry explicitly.
- No overwrite path or interactive conflict choice yet.

### Phase 5B — Retry interaction

- Accessible labelled Retry action for failed/cancelled Operations Island states.
- Retry submission through `ApplicationState::retry_operation`.
- Duplicate-click prevention while the new attempt is queued.
- Completed operations remain non-retryable.

### Phase 5C — Context menu

- Native list-row popover with Open, Copy, Cut, Rename, and Move to Trash.
- Secondary-click selects the exact pointer-targeted virtualized row first.
- Shift+F10 and Menu-key access scoped to the focused file list.
- Existing selection-sensitive `win.*` actions and original paths are reused.

### Phase 5D — Open With and file associations

- Asynchronous GIO content-type and compatible-application discovery.
- Current default shown separately and selected initially.
- Explicit Open and Set as Default actions with recoverable error feedback.
- Regular-file and non-directory-link eligibility; original paths retained.

### Phase 5E — Conflict interaction foundation

- Distinct conflict terminal outcome; generic Retry cannot repeat the same destination.
- Pending conflicts retain original paths plus job and operation IDs.
- Explicit `KeepExisting` and `RetryWithName(OsString)` decisions only.
- Revised copy/move/rename attempts remain fail-if-exists, retain logical operation ID, and receive a fresh job ID.
- Resolution is single-use and bookkeeping follows bounded terminal-history eviction.
- No overwrite, apply-to-all, trash-conflict, or GTK conflict-dialog path.

### Phase 5F — Conflict interaction

- Focused non-blocking conflict dialog with incoming/existing path context.
- Empty-by-default retry field with inline accessible single-name validation.
- Keep Existing submits no job; Retry with New Name returns to normal job feedback.
- Dismissal leaves the conflict pending behind an Operations Island Resolve Conflict action.
- Ordered pending conflicts and at most one active decision dialog.
- No overwrite, apply-to-all, or trash-conflict option.

### Phase 6A — List-view polish foundation

- Compact fixed header with Name, Type, Size, and Modified hierarchy.
- Textual file kinds distinguish folders, files, folder links, file links, and
  special entries without relying on icon or color alone.
- Decimal size formatting through exabytes and locale-aware modification time.
- Bind-time metadata presentation preserves `GtkListView` virtualization and
  bounded 256-entry main-loop insertion batches.
- Centralized dim metadata, tabular figures, and explicit keyboard focus styling.
- Original `PathBuf`/`OsString` ownership remains unchanged; thumbnails and a
  separate grid remain deferred.

### Phase 6B — List sorting

- Native keyboard/pointer Name, Type, Size, and Modified heading controls.
- Explicit ascending/descending arrows, accessible labels, and active pressed
  state; a newly selected column starts ascending.
- Navigable directories always remain first and missing optional metadata
  remains last in either direction.
- Comparisons run on the bounded directory worker using shared entries.
- Selection restores by exact original `PathBuf`; `GtkListView` virtualization
  and 256-entry main-loop insertion batches remain intact.
- Thumbnail generation and a separate grid remain deferred.

## Next

### Phase 6C — Thumbnail foundation

Create branch `phase-6c-thumbnail-foundation`. Define a bounded, asynchronous
thumbnail request/result boundary and safe cache identity without blocking GTK
or executing active content. Start with the smallest useful image-thumbnail
slice, preserve generic icons and readable fallback states, and keep a separate
grid view deferred until the shared thumbnail path is verified.

## Later

### Remaining Phase 4 and Phase 5 — Mutations and resilience

- Trash restore/bulk workflows and cross-filesystem move recovery.
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
