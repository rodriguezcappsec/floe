# Floe Architecture

This document describes the code that exists now. Future boundaries are called
out separately rather than presented as implemented.

## Workspace and dependency direction

Floe is a Cargo workspace with two crates:

```text
floe-app (GTK4, libadwaita, GIO, GLib, tracing)
   |
   v
floe-core (standard library, thiserror)
```

`crates/core` never depends on GTK, GIO, GLib, a compositor, or a desktop
environment. `crates/app` depends on `floe-core` and owns all desktop/UI wiring.

## Current runtime architecture

```text
AdwApplication / GTK widgets
       |                       |
       v                       v
BrowserController      OperationController
 |       |               |             |
 |       +--> launcher   |             +--> Operations Island / toasts
 |                       v
 |              shared ApplicationState
 |               |       |          |
 |               |       |          +--> TransferBuffer / tracked operations
 |               |       +--> ApplicationJobManager / structured events
 |               +--> CopyExecutor / MoveExecutor / TrashExecutor
 |                    (bounded workers)
 v
BrowserWorker (one std thread)
       |
       v
floe-core directory enumeration
       |
       +---- std::mpsc Response ----> 16 ms GTK poll
                                      |
                                      v
                           batched virtualized list model
```

In parallel, Phase 6C's fixed-capacity `ThumbnailWorker` accepts lazy requests
from bound rows and returns owned RGBA buffers to the same 16 ms GTK poll. GTK
constructs `GdkMemoryTexture` objects only on the main thread.

GTK callbacks submit navigation, activation, copy, paste, or cancellation
intent to controllers and application state. They do not call `std::fs`
operations. `BrowserController` owns navigation, selection, loading
generations, and GTK-model delivery state; `OperationController` observes job
events and owns operation feedback without executing copy work.

## `floe-core`

### `directory.rs`

`enumerate_directory` and `enumerate_directory_with_cancel` perform one-level,
non-recursive `std::fs::read_dir` enumeration. They use `symlink_metadata`,
classify directory/file/symlink/other entries, mark dot-prefixed hidden names,
capture inexpensive size and modification metadata, and sort directories first.

The cancellation callback is checked between entries. No directory is traversed
recursively, so a symlink cannot create an implicit recursive walk.

### `model.rs`

`DirectoryEntry` preserves `PathBuf` and `OsString` values independently of
lossy display text. It currently contains kind, optional size and modification
time, optional MIME placeholder, hidden state, and `ThumbnailState`. MIME is not
populated and thumbnail state remains `NotRequested`.

`EntryKind::SymbolicLink` records whether the resolved target is a directory.
`DirectoryListing` couples the enumerated directory path to its entries.

### `sorting.rs`

`DirectorySort` is the GTK-independent ordering policy for Name, Type, Size,
and Modified. It owns direction cycling, directories-first grouping, stable raw
`OsStr`/`Path` tie-breaking, and unknown-metadata-last behavior in both
directions. Directory enumeration applies the default policy; the application
may submit another policy without implementing comparisons in GTK callbacks.

### `navigation.rs`

`NavigationState` owns current, back, and forward `PathBuf` values. New
navigation clears forward history; back/forward exchange paths between stacks;
parent navigation stops at filesystem root. It is toolkit-independent.

### `jobs.rs`

The Phase 3 foundation defines non-zero `OperationId` and `JobId` types,
validated determinate/indeterminate `JobProgress`, structured failures,
lifecycle commands/states/events, and `JobRecord::apply` as the legal-transition
authority. A logical operation retains its ID across retry attempts while each
attempt receives a new job ID. Terminal states reject further commands.

### `move_operation.rs`

`MoveRequest` retains exact source and destination `PathBuf` values.
`RenameRequest` retains the original source plus one raw `OsString` filename
component, so rename never reconstructs a path from UI text. On Linux,
`execute_move` and `execute_rename` use
`renameat2(RENAME_NOREPLACE)` through `rustix`; the destination cannot be
silently replaced even under a race. Files, directories, and symlinks move
atomically on one filesystem, and symlinks are not followed.

Cancellation is checked before inspection and again immediately before the
irreversible rename syscall. Cross-filesystem moves return a structured
unsupported error; a copy-delete fallback is intentionally deferred until
partial-failure recovery is designed.

### `error.rs`

`DirectoryError` uses `thiserror` to distinguish open, entry-read, metadata, and
superseded/cancelled enumeration failures while retaining original `io::Error`
context.

## `floe-app`

### `application.rs` and `main.rs`

`main.rs` declares modules and starts `application::run`. `application.rs`
creates application ID `io.github.floe.FileManager`, installs Ctrl+Q, selects
appearance and XDG locations, builds the window, starts `BrowserWorker` and the
optional `ThumbnailWorker`, starts the view-preference worker, and surfaces
browser-worker start failure.
Thumbnail-worker start failure is non-fatal and leaves generic icons. It also
creates the shared `ApplicationState`
that owns the job registry boundary. `tracing-subscriber` reads `RUST_LOG` and
defaults to `floe=info`.

### `view.rs` and `preferences.rs`

Phase 6D keeps List/Grid policy independent of GTK. `ViewMode` and `GridSize`
define strict persisted values and seven bounded zoom steps. `PreferenceWorker`
loads startup preferences and owns a fixed-capacity channel plus atomic
configuration-file writes. GTK actions submit current values non-blockingly;
they never read or write the configuration file directly.

### `thumbnail.rs` and `thumbnail_cache.rs`

Phase 6C owns a fixed-capacity, single-thread thumbnail request/result boundary
separate from GTK and the filesystem mutation executors. `ThumbnailKey` retains
the exact `PathBuf`, enumerated byte size, modification time, and whitelisted
raster format. Only regular files qualify. The worker opens with `O_NOFOLLOW`,
rejects sources larger than 32 MiB or changed since enumeration, applies
explicit decoder dimension/allocation limits, and scales to the requested
32-192 pixel edge with the pure-Rust `image` decoder. The requested edge is part
of cache identity. Old navigation generations are skipped.
Only owned RGBA bytes cross back to the GTK thread; the worker creates or uses
no GTK object.

Phase 6E gives that same worker optional persistent-cache state resolved once
from the XDG cache environment. It verifies the source as an unchanged regular
file before cache access, maps requested edges to freedesktop `normal`
(128-pixel) or `large` (256-pixel) tiers, and validates `Thumb::URI`,
`Thumb::MTime`, and `Thumb::Size` before decoding a cached PNG. Misses,
corruption, symlinks, oversized entries, and cache I/O failures fall through to
the existing bounded source decoder. Floe-owned entries additionally carry
`Floe::MTimeNsec`, preventing a same-size source change within one Unix second
from reusing Floe's older pixels while remaining compatible with foreign
standard cache entries.

Successful source decodes are written as 8-bit RGBA, non-interlaced PNGs using
0600 same-directory temporary files and atomic rename beneath private 0700
directories. Standard cache entries live in `thumbnails/{normal,large}`;
separate Floe ownership markers live in
`floe/thumbnail-ownership/{normal,large}`. Startup and periodic cleanup apply
one global 2,048-entry, 256-MiB, 90-day budget and remove a shared thumbnail
only when its marker exists and PNG metadata still says `Software=Floe`.
Thus GTK performs no cache I/O and another application's cache ownership is not
inferred from a filename alone.

Phase 6F expands the explicit extension policy to PNG, JPEG, WebP, GIF, BMP,
TIFF, and ICO while continuing to reject SVG and unreviewed formats before
request submission. The worker obtains decoder metadata through
`ImageDecoder::orientation`, decodes one still frame, and applies orientation
before tier scaling and cache storage. Animated GIF/WebP inputs therefore
produce one stable first-frame thumbnail. Decoder dimensions and total output
bytes are checked before allocation in addition to the existing encoded-source
limit; oriented pixels continue through the unchanged freedesktop cache and
owned-RGBA response boundaries.

### `iconography.rs` and embedded resources

Phase 6G centralizes generic presentation in a GTK-independent `EntryIcon`
policy. Enumerated `EntryKind` and executable permission metadata take
precedence; a case-insensitive extension policy then selects document, media,
archive, code, PDF, spreadsheet, presentation, or stable generic families.
Classification uses the original `Path` extension and never reconstructs a path
from lossy display text. Directory enumeration captures only an inexpensive
executable bit; icon selection performs no filesystem I/O.

Fourteen app-owned SVGs compile into one GResource at build time and register as
a non-symbolic icon-theme resource before list/grid factories bind. Both views
call the same policy and CSS-family mapping. Generic list icons use 28 pixels;
grid generic icons use bounded 48-88 pixel optical sizes independently of the
64-192 pixel thumbnail request. A ready thumbnail still replaces the icon, and
any unsupported/failed thumbnail returns to the same semantic fallback. File
icons are decorative GTK Presentation nodes beside authoritative filename and
text-kind labels.

### `state.rs` and `job_manager.rs`

`ApplicationState`, separate from `BrowserController`, owns the
`ApplicationJobManager`, bounded copy/move/trash executors, internal
`TransferBuffer`, and `TrackedOperation` values keyed by `JobId`. The manager
allocates operation/job IDs, stores core `JobRecord` values, applies core lifecycle
commands, queues observable events, and creates retries under the original
operation ID. `stage_copy` and `stage_move` replace one transfer intent while
retaining the selected original `PathBuf`; `submit_paste` derives the exact
destination from the source `OsStr` filename and dispatches to the matching
executor. `submit_rename` retains the source path separately from the new
`OsString` filename. No path is reconstructed from display text.

`submit_trash` validates and tracks one original `PathBuf`, dispatches through
the GIO-backed trash executor, and reports the source parent for refresh after
completion. GTK interaction reaches it only through the browser command and
never calls GIO directly.

Phase 5A adds a 64-entry terminal operation history containing the terminal
job/operation IDs, outcome, and original `TrackedOperation`. Failed or cancelled
entries can be retried through the matching bounded executor. Each retry keeps
the logical `OperationId`, receives a new `JobId`, and tracks the same raw
request. Eviction also forgets only the corresponding terminal job record;
active records are never pruned.

Phase 5B exposes retry through `OperationController`: only failed and cancelled
terminal outcomes retain a retryable `JobId`, the button submits through
`ApplicationState::retry_operation`, and fresh structured events replace the
terminal presentation. GTK still observes and submits commands only.

The transfer buffer is Floe-internal only. Cross-application clipboard formats,
operation persistence, history UI, and overwrite policy are not implemented.

### `ui.rs`

`ui::build` constructs the header, `GtkPaned` Places/content layout,
`GtkListView`, empty overlay, status strip, toast overlay, and compact
Operations Island in a non-modal `GtkOverlay`. A
`GtkSignalListItemFactory` binds boxed `DirectoryEntry` values to virtualized
rows. Phase 6A keeps presentation inside that bind boundary and exposes aligned
Name, Type, Size, and Modified columns from metadata already owned by directory
enumeration;
it does not create a parallel presentation model or eagerly format a full
directory result. The module also builds the visible file-actions menu and
focused rename dialog with inline error text. Symbolic icon buttons have
tooltips and accessible labels, including the generic operation-cancellation
control.

Phase 5C also constructs one native `GtkPopoverMenu` parented to the list view.
Virtualized row setup adds only a secondary-click selection/presentation
gesture; every menu item targets an existing `win.*` action, so the UI layer
does not acquire filesystem or launch execution logic.

Phase 5F adds the conflict dialog widget tree: source/destination context,
labelled filename input, associated inline error, Cancel, Keep Existing, and
Retry with New Name controls. It contains no filesystem implementation and no
overwrite/apply-to-all control.

Phase 6B stores `Arc<DirectoryEntry>` in the boxed list model and exposes each
column heading as a native action-backed button with explicit direction and
accessible pressed state. Shared entries let the controller retain one complete
filtered set while the GTK model continues receiving at most 256 rows per main
loop tick. Sort requests run on the existing bounded directory worker. On
completion, `browser.rs` rebuilds the model and restores selection by exact
`PathBuf`; it never derives identity from lossy labels.

Phase 6C keeps thumbnail presentation in the virtualized factory bind/unbind
boundary. `ThumbnailPresentation` deduplicates pending exact keys, retains weak
bindings only for bound rows, and caps completed textures/fallbacks at 256
entries. The controller submits requests without blocking, retries a full queue
on a later main-loop tick, drops stale-generation responses, and creates
`GdkMemoryTexture` objects only on the GTK main thread.

Phase 6D adds a `GtkGridView` and a second popover presentation while reusing
the same `GioListStore`, `GtkSingleSelection`, action names, and exact-path
entries as the list. Replacing the grid factory at a discrete size change
rebinds visible cells only; no eager directory-wide thumbnail pass is created.

### `browser.rs`

`BrowserController` is the current application-state coordinator. It owns:

- `NavigationState`;
- the currently selected `DirectoryEntry` mirrored from GTK selection;
- hidden-file preference for the current process;
- current List/Grid mode, bounded grid size, and pending nonblocking preference save;
- request generation and pending listing batches;
- enabled state for navigation and Open actions.

It also owns the list-focused Shift+F10/Menu-key route for the Phase 5C context
menu. Secondary-click selection still flows through `GtkSingleSelection`, so
the controller retains the original `DirectoryEntry` and exact path before any
existing action is dispatched.

It also owns Ctrl+C/Ctrl+X/Ctrl+V/F2/Delete and file-menu action wiring because
those commands depend on the active selection and current destination. The
actions stage or submit through shared `ApplicationState`; they do not perform
filesystem work. The destructive-adjacent menu label is explicitly “Move to
Trash”; no permanent-delete command exists.

Navigation queues a worker request, disables stale interaction, and ignores
responses whose generation is no longer active. Results are filtered for hidden
entries and fed to a new `GioListStore` in batches of 256.

The controller currently polls worker responses from a GLib timeout every 16ms.
This polling is bounded and simple, but a future event/channel integration may
remove periodic polling.

### `operations.rs`

`OperationController` polls structured application job events every 50 ms,
maps queued/running/progress/terminal states into the Operations Island, and
submits generic cancellation intent through `ApplicationState`. Completion
refreshes every affected visible directory for copy, move, rename, or trash;
conflict, permission, cross-filesystem, unsupported-trash, and general failures
produce recovery-oriented toasts. Terminal status remains
visible for three seconds. The controller observes jobs but never executes
filesystem operations.

Phase 5E maps destination conflicts to a distinct non-retryable terminal
outcome, so the generic Retry control cannot blindly repeat the same
destination.

Phase 5F queues unresolved conflict IDs in `OperationController`, presents the
application-state decision contract through a non-blocking `AdwDialog`, and
reuses the Operations Island action slot as Resolve Conflict after dismissal.
GTK callbacks submit only `ConflictDecision` commands. Revised submissions
return to normal queued/progress handling; keep-existing submits no job.
Pending conflicts stay ordered and only one conflict dialog can be active.

### `move_executor.rs`

`MoveExecutor` owns one named worker and a fixed-capacity queue for core
`MoveRequest` and `RenameRequest` values. It starts, completes, cancels, or
fails jobs through the shared `ApplicationJobManager`, maps core conflicts and
unsupported cross-filesystem results into structured failure kinds, and
cancels queued work during shutdown. `ApplicationState` owns the executor so
its lifetime matches the application. GTK reaches it only through application
commands and tracked requests.
Move and rename retry methods allocate attempts through
`ApplicationJobManager::retry` before reusing the same queue path.

### `trash_executor.rs`

`TrashExecutor` owns one named worker and fixed-capacity queue for application-
layer `TrashRequest` values. Production execution creates a `gio::File` from
the original `Path` and calls GIO's synchronous trash API on that worker with a
`gio::Cancellable`; it does not shell out or implement the XDG Trash layout
itself. GIO cancellation is cooperative and cannot undo a move after the
desktop service commits it.

The executor maps missing sources, permission denial, unsupported locations,
other I/O errors, cancellation, queue capacity, and shutdown into the shared
job lifecycle. Tests inject a backend and use virtual or temporary paths, so
the suite never modifies the user's real Trash.
Trash retries likewise retain the original `TrashRequest` and delegate attempt
identity allocation to the shared job manager.

### `worker.rs`

`BrowserWorker` owns one named `std::thread` and two `std::mpsc` channels. An
atomic latest-generation value lets core enumeration cooperatively stop stale
requests. This is a browsing worker, not the future filesystem mutation job
engine.

### `launcher.rs`

`launch_default` converts the original `Path` directly to `gio::File::uri` and
uses asynchronous `GAppInfo` default-app launching. A Unix test verifies that a
non-UTF-8 path round-trips through the GIO URI without replacement characters.

Phase 5D adds asynchronous `standard::content-type` discovery, combines GIO's
default, recommended, and type-capable `AppInfo` results, removes duplicates,
and sorts the current default first. Specific-app launches use asynchronous
`launch_uris`; an explicit chooser action delegates default changes to
`set_as_default_for_type`. No shell command or lossy path reconstruction is
introduced.

### `locations.rs`

The app layer uses GLib home and XDG user-special directories for Home,
Downloads, Documents, and Pictures, removing duplicate paths. Trash, devices,
mounts, and network locations are not implemented.

### `appearance.rs`

`Appearance` centralizes preset-level radius, gap, opacity, row padding, shadow,
floating-panel, and sidebar-width values. It generates GTK CSS using libadwaita
semantic colors. Phase 6A adds shared list-heading, secondary metadata, tabular
figure, and focus-visible rules without forking widget trees by preset. Frosted
is the default; `FLOE_APPEARANCE` selects Native, Glass, Frosted, Minimal, or
Compact. Blur and settings persistence do not exist.

## Keyboard input

Current application/window actions are:

| Input | Action |
| --- | --- |
| Alt+Left | Back |
| Alt+Right | Forward |
| Alt+Up | Parent |
| Ctrl+L | Show local path entry |
| Ctrl+H | Toggle hidden entries |
| Ctrl+1 | List view |
| Ctrl+2 | Grid view |
| Ctrl+- | Decrease grid thumbnail size |
| Ctrl++ | Increase grid thumbnail size |
| Ctrl+C | Stage selected entry for copying in Floe's internal buffer |
| Ctrl+X | Stage selected entry for moving in Floe's internal buffer |
| Ctrl+V | Paste staged entry into the current directory |
| F2 | Rename selected entry through a validated dialog |
| Delete | Move selected entry to the desktop Trash through GIO |
| Shift+F10 / Menu | Open the selected row's native context menu |
| Escape | Leave path entry |
| Ctrl+Q | Quit |
| Enter / double-click | Activate selected list or grid item |

Open is also a visible header action. There is no configurable keymap or Vim
mode yet.

## Error handling and tracing

Core failures are structured. Recoverable browsing and launch errors produce
toasts/status feedback; worker-channel failures and technical context use
`tracing`. Paths may be sensitive, so normal logs should avoid adding verbose
path reporting. No file contents are logged.

## Boundaries not implemented yet

### Desktop integration

No desktop integration trait or Niri/Plasma backend exists. The app uses generic
GTK/GIO/GLib behavior and displays a "Generic Wayland" label. Environment
detection and compositor APIs must eventually stay under `crates/app`.

### Filesystem jobs through Phase 5F

Phase 5E adds an application-layer conflict resolver contract. Pending
conflicts retain exact source/destination paths and stable operation identity;
callers must keep the existing item or retry copy/move/rename with one
validated sibling `OsString`. Revised attempts remain fail-if-exists and
receive fresh job IDs. Resolution is single-use and pruned with bounded
terminal history. Phase 5F now presents that contract in GTK while retaining
original request paths in application state. Trash conflicts, apply-to-all,
and overwrite are not supported.

Identity, lifecycle, progress, failure, retry-attempt, registry, and event
foundations now exist. `floe-core::copy` adds the first path-safe operation
model and synchronous engine; `floe-app::copy_executor` runs it on one named
worker behind a fixed-capacity queue. `ApplicationState` owns the shared
registry, all three executors, unified transfer buffer, and tracked
copy/move/rename/trash requests.
`OperationController` is the GTK observer and feedback boundary.

`ConflictPolicy::FailIfExists` is the only Phase 4A conflict behavior, so an
existing target is never overwritten. `SymlinkPolicy::Preserve` recreates a
link using its stored target and never follows it; `Reject` fails during the
preflight scan before the destination is created. The destination is an exact
path and its parent must already exist. Files and directories are copied in
chunks/entries with cooperative cancellation. Created paths are tracked and
removed in reverse order after failure without recursively deleting unknown
content.

The engine preserves regular-file bytes, directory structure, Unix permission
bits, and link targets. It does not yet preserve timestamps, ownership, ACLs,
extended attributes, sparse extents, or reflink state. There is no persistence,
history UI, cross-application clipboard format, overwrite path, or interactive
conflict resolver. The current direction is:

```text
GTK observers/actions (implemented for copy, move, and rename)
       |
       v
ApplicationState / ApplicationJobManager (implemented)
       |
       v
floe-core legal state transitions (implemented)
       |
       v
floe-core path-safe copy model and engine (implemented)
       |
       v
bounded copy, move/rename, and GIO trash executors (implemented)
```

Phase 4D exposes the Phase 4C move/rename models through application-owned
commands, a file-actions menu, keyboard shortcuts, validated rename dialog,
and the generic Operations Island.
These operations currently support only atomic same-filesystem renames with
fail-if-exists behavior. Cross-filesystem move, overwrite, operation
persistence, and interactive conflict resolution remain unimplemented.

Phase 4E implements the separate XDG/GIO trash job boundary. Phase 4F exposes
it through one selection-sensitive “Move to Trash” action and Delete shortcut.
The callback submits through `ApplicationState`; no direct GIO work appears in
widgets and the read-only browser worker remains separate. The recoverable
Trash action has no confirmation dialog; restore/bulk UI, Shift+Delete, undo,
and permanent deletion remain deferred.

Phase 5A generalizes retry dispatch and bounds application terminal history.
Phase 5B adds the Operations Island Retry control for failed and cancelled
attempts. Phase 5E prevents generic retries for destination conflicts and
establishes explicit keep-existing/retry-with-name decisions without enabling
overwrite. Phase 5F adds the focused, dismissible conflict interaction and
recoverable Operations Island action. Overwrite, pause/resume controls, and
permanent deletion remain deferred.

Phase 6H adds `location_input.rs` as a GTK-independent input and recovery policy. GTK only captures explicit text and submits it to `BrowserController`; absolute-path syntax is checked immediately, directory enumeration remains on `BrowserWorker`, and failed submissions restore the exact previous navigation snapshot. Existing non-UTF-8 `PathBuf` state is used directly until the user explicitly submits edited UTF-8 entry text.

The next branch, `phase-6i-open-with-fallback`, reuses the existing asynchronous GIO launcher/chooser boundary and does not add filesystem work to GTK.

## Known architectural debt

- `BrowserController` already coordinates several concerns and should not absorb
  mutation execution, previews, tabs, and desktop integration.
- Normal entry/sidebar navigation still changes state before enumeration succeeds. Phase 6H
  direct location submissions use a generation-bound `PendingLocation` snapshot and commit or
  restore the complete `NavigationState` after the bounded directory worker responds.
- Appearance values are partly centralized while local widget margins remain in
  `ui.rs`.
- Sidebar width, appearance, hidden-file visibility, selection, and scroll
  position are not persisted.
- MIME, permissions, thumbnail, device, and mount data are incomplete or absent.
- There is no file-watching reconciliation for external changes.
