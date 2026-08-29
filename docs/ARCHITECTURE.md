# Floe Architecture

This document describes the code that exists now. Future boundaries are called
out separately rather than presented as implemented.

## Workspace and dependency direction

Floe is a Cargo workspace with two crates:

```text
floe-app (GTK4, libadwaita, GIO, GLib, tracing)
   |
   v
floe-core (standard library, rustix, thiserror)
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
 |               |       |          +--> TransferBuffer / clipboard formats / tracked operations
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

`DirectorySort` now covers Name, Type, Size, Modified, Created, Accessed,
Extension, Rating, Tags, and Comment. Hidden-last partitioning is independent
from folder placement, grouping, and direction; unavailable facts stay last
and exact raw names/paths remain deterministic tie-breakers.

### `user_sort_metadata.rs`

Explicit Rating, Tags, and Comment sorts use a core-owned bounded enrichment
step on the application browser worker. KDE-compatible xattrs are read with
no-follow semantics, strict entry/value limits, and generation cancellation;
GTK receives only enriched immutable entries and never reads the filesystem.

`DirectorySort` is the GTK-independent ordering policy for Name, Type, Size,
Modified, and Extension. It owns direction cycling, first/last directory
placement, None/Type/Extension grouping, stable raw `OsStr`/`Path` tie-breaking,
and unknown-metadata-last behavior in both directions. Directory enumeration
applies the default policy; the application may submit another policy without
implementing comparisons in GTK callbacks.

### `navigation.rs`

`NavigationState` owns current, back, and forward `PathBuf` values. New
navigation clears forward history; back/forward exchange paths between stacks;
parent navigation stops at filesystem root. It is toolkit-independent.

### `view.rs` and `session.rs`

Phase 7A makes `floe-core` the canonical owner of GTK-independent view policy:
list/grid mode, bounded grid size, density, directory sort/group/placement, and
visible/clamped list-column layout. The application `view` module only re-exports
those types and maps existing view actions.

`BrowserSession` owns a stable nonzero ID and bounded complete location states.
Each current/back/forward location preserves its exact absolute `PathBuf`, exact
multi-selection, optional path/index scroll anchor, and complete view policy.
Navigation moves whole locations between stacks, clears forward history after a
new destination, and stops parent traversal at root. Session duplication clones
state under a caller-provided ID; no widget trees or workers are duplicated.

The version-1 in-memory codec is bounded and rejects bad headers/versions,
relative or oversized paths, invalid policy fields, duplicate selection paths,
truncation, trailing bytes, and oversized history/selection counts. On Unix it
encodes original path bytes through `OsStrExt`/`OsStringExt`, preserving invalid
UTF-8 exactly. Phase 7A does not call this codec from the application and does no
session filesystem I/O; persistence and privacy lifecycle remain Phase 7C.

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
`execute_move` and `execute_rename` first use
`renameat2(RENAME_NOREPLACE)` through `rustix`; the destination cannot be
silently replaced even under a race. Files, directories, and symlinks move
atomically on one filesystem, and symlinks are not followed.

Phase 6O handles `EXDEV` on the same bounded worker. It snapshots exact source
device/inode/type/mode/size/mtime identity, copies to a collision-safe hidden
sibling staging path, preserves supported basic metadata, revalidates the
complete source tree, atomically publishes with `RENAME_NOREPLACE`, synchronizes
the destination parent, then removes only matching source nodes without
following links. Cancellation before publication cleans staging and retains the
source. Cancellation or cleanup failure after publication is an explicit
non-retryable partial result with the complete destination and retained source
identified; it is not described as crash-recoverable journaling.

### `error.rs`

`DirectoryError` uses `thiserror` to distinguish open, entry-read, metadata, and
superseded/cancelled enumeration failures while retaining original `io::Error`
context.

## `floe-app`

### Live tab ownership

Phase 7B layers bounded `BrowserTabs` state over the Phase 7A `BrowserSession`
primitive. The core collection owns stable IDs, active index, and deterministic
new/duplicate/close/reorder transitions with a 64-tab cap. The GTK controller
captures exact selection, path/index scroll anchor, and complete folder view
policy before changing the active session, then submits the destination through
the existing superseding browser worker. All tabs share the same GTK model,
thumbnail/metadata workers, file watcher, application jobs, and Operations
Island. Dormant tabs retain data state only; they do not own hidden widgets or
background directory enumerations.

Phase 7C extends that collection with a 32-entry recently closed LIFO and a
versioned workspace envelope covering ordered live sessions, active stable ID,
and closed sessions. Reopened tabs receive fresh IDs. The application-owned
`SessionStoreWorker` performs bounded no-follow reads, codec work, private
0700/0600 same-directory atomic writes, directory synchronization, suppression,
and clean-shutdown flush outside GTK callbacks. Corruption and unsupported
versions return no workspace and the application constructs one normal tab.
One capacity-one worker exists per application window; no filesystem worker is
added per tab.

Phase 7D adds GTK-independent `split.rs`. `BrowserSplit` composes the existing
`BrowserSession` primitive into primary and optional secondary panes with
explicit active-side ownership and a bounded ratio. `BrowserTabs` stores one
split context per live or recently closed tab while preserving the primary pane
ID as stable tab identity. Workspace version 2 serializes both panes and safely
migrates version-1 unsplit records. No GTK model, widget, watcher, or worker is
created by this state layer.

Phase 7E keeps exactly one active virtualized list/grid, enumerator, thumbnail
worker, metadata worker, and active-directory monitor. The inactive split pane
is a bounded application-owned presentation snapshot keyed by stable tab ID and
side; activating it restores its authoritative `BrowserSession` through the
existing generation-safe pipeline. Opposite-pane transfers resolve exact paths
from `BrowserTabs` and submit typed requests to `ApplicationState`; GTK callbacks
do not enumerate or mutate the filesystem.

Phase 7F attaches the existing `drag_drop` destination adapter to the inactive
pane. Its resolver reads the current opposite `BrowserSession` path at event
time, rejects unsplit and Trash contexts, and emits the same typed
`DropRequest` used by list/grid/sidebar destinations. Copy, move, and symbolic
link operations therefore retain existing FIFO, conflict, cancellation, and
no-overwrite semantics. The inactive pane owns no enumerator, watcher, job
executor, or hover-navigation timer.

Phase 8A adds GTK-independent `miller.rs` in `floe-core`. `MillerColumnModel`
stores only a bounded `VecDeque` of exact directory paths, selected direct-child
paths, stable logical depths, and active depth. It validates path and chain
invariants and reconciles caller-supplied rename/delete events without touching
the filesystem. It deliberately owns no `DirectoryEntry`, listing cache,
enumerator, worker, widget, GIO, compositor, or persistence dependency.

Phase 8B adds application-owned `miller_view.rs`. Its active `GtkListView`
shares the existing `GtkMultiSelection` and `GioListStore`; it never owns a
second browser worker, watcher, or filesystem enumerator. Previously returned
columns are capped snapshots of shared `Arc<DirectoryEntry>` values (16 columns,
4,096 entries each), so recycling cannot grow without bound. GTK activation
returns the captured logical depth plus exact entry identity to
`BrowserController`; labels are display-only. The single global clamped column
width persists through the existing preference worker.

Phase 8C keeps key and scroll controllers inside `MillerView` but sends logical
parent/child commands—depth plus an exact selected `DirectoryEntry`—back to
`BrowserController`. The controller alone mutates `MillerColumnModel` and the
active `BrowserSession`; widgets never derive a path from labels. Up/Down and
Home/End operate on the current `GtkSelectionModel`. Dominant horizontal scroll
updates only the bounded outer adjustment, and GTK animation settings control
kinetic scrolling. No worker, enumerator, or persistence channel is added.


Phase 8D applies the same ownership boundary to actions. Each column menu emits
its exact logical depth, directory `PathBuf`, and bounded selected shared
entries. `BrowserController` revalidates the retained column and direct-child
identity before making it the action owner. Existing application actions and
bounded no-overwrite jobs remain the only mutation route. Retained-column
create, paste, and relative-path commands use the validated owner directory;
navigation-only background commands stay disabled when that directory is not
the active browser session.

Phase 8E extends the existing `drag_drop` adapter rather than adding an
operation path. Active and retained Miller selection models publish standard
GDK local-file lists; exact folder and column directories become typed
destinations. One application dispatcher resolves live tab/split/sidebar/device
state and submits the existing validated copy/move/link requests. Typed hover
targets distinguish directory, tab, opposite-pane, and Miller-child ownership;
the single cancellable timer revalidates state before navigation. Edge motion
walks ancestor scrollers and clamps both vertical and horizontal adjustments.

Phase 8F adds GTK-independent `miller_detail.rs`. It owns only bounded exact
handoff state: surface kind, monotonically revised generation, active logical
depth/directory, and at most 4,096 original paths. Hidden, empty, ready, and
unsupported states are explicit. `BrowserController` reconciles selection and
navigation, while `MillerView` renders an optional accessible final column.
There is no provider worker, decoding, metadata lookup, cache, persistence, or
sandbox claim; Phases 9 and 10 consume this contract.

Phase 9A adds public GTK-independent `preview.rs`. A deterministic registry of
at most 32 providers feeds one fixed-capacity 16-request worker. Requests carry
exact source path/size/modified identity, nonzero generation, explicit source,
output, text, archive, and deadline limits, and Disabled or MemoryOnly cache
policy. One atomic generation token provides cooperative cancellation and stale
rejection; provider support/load panics become failures. The GTK loop drains at
most eight responses per tick and applies only the current generation to the
Phase 8F lifecycle. The default registry is intentionally empty until 9B.

### `application.rs` and `main.rs`

`main.rs` declares modules and starts `application::run`. `application.rs`
creates application ID `io.github.floe.FileManager`, installs Ctrl+Q, selects
appearance and XDG locations, builds the window, starts `BrowserWorker` and the
optional thumbnail, lazy-metadata, and storage-facts workers, starts the
view-preference and bookmark workers,
creates the application-owned GIO `DeviceMonitor`, and surfaces worker start
failures.
Thumbnail-worker start failure is non-fatal and leaves generic icons. It also
creates the shared `ApplicationState`
that owns the job registry boundary. `tracing-subscriber` reads `RUST_LOG` and
defaults to `floe=info`.

### `view.rs` and `preferences.rs`

Phase 7A moves List/Grid, density, column, and complete folder-view policy to
`floe-core`; application `view.rs` remains a thin re-export/action mapping.
`PreferenceWorker` continues to load startup preferences and owns a
fixed-capacity channel plus atomic configuration-file writes. GTK actions submit
current values non-blockingly; they never read or write the configuration file
directly.

Phase 6T extends that policy with file-view density, optional clamped list
columns, sort/group settings, and at most 256 exact raw-path per-folder
overrides.

### `metadata.rs` and `storage.rs`

Phase 6T keeps expensive browser enrichment outside enumeration and GTK.
`MetadataWorker` has one thread, a capacity-64 request queue, and a 512-entry
exact path/size/mtime cache; only bound rows with enabled lazy columns request
MIME, Created, Accessed, and Unix mode. `StorageWorker` has one thread and a
capacity-32 queue for GIO filesystem size/free/read-only facts. Current-location
generation and device ID/path checks reject stale results. Neither worker owns
sorting or filesystem mutation.

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

Fifteen app-owned SVGs compile into one GResource at build time and register as
a non-symbolic icon-theme resource before list/grid factories bind. Both views
call the same policy and CSS-family mapping. Generic list icons use 28 pixels;
grid generic icons use bounded 48-88 pixel optical sizes independently of the
64-192 pixel thumbnail request. A ready thumbnail still replaces the icon, and
any unsupported/failed thumbnail returns to the same semantic fallback. File
icons are decorative GTK Presentation nodes beside authoritative filename and
text-kind labels.

The post-Phase-14 icon correction keeps this classification boundary and adds
an app-only `EntryIconStyle`. Floe Color resolves fifteen
non-symbolic resources, Phosphor Monochrome resolves a pinned local symbolic
resource subset, and System Theme resolves standard freedesktop icon names.
Plain text and office documents are separate from PDF. Every System Theme
family terminates in its distinct Floe Color resource, preventing missing
theme artwork from collapsing unrelated families onto one shared generic icon.
For regular files, a recognized extension family takes presentation precedence
over execute bits because FAT/exFAT and similar filesystems may synthesize
`0755` for every file. Only unknown or extensionless executable entries fall
back to executable artwork. This is app-only presentation policy and does not
change permission or launch decisions.
One shared `Rc<Cell<EntryIconStyle>>` is read only while virtualized list/grid
and search rows bind. A style change updates that cell and rebuilds the model
from already-loaded `Arc<DirectoryEntry>` values; it performs no enumeration,
MIME probe, path conversion, or filesystem operation. The existing thumbnail
presentation remains the final paintable authority. Style persistence stays in
the capacity-one application preference worker and does not enter `floe-core`.

### `state.rs` and `job_manager.rs`

`ApplicationState`, separate from `BrowserController`, owns the
`ApplicationJobManager`, bounded copy/create/move/trash/restore/permanent-delete executors, internal
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

Phase 6J extends `TransferBuffer` to an ordered, exact-path set and adds
application-owned batch queues for copy, move, and Trash. One request is
dispatched at a time; terminal handling pumps the next request through the
existing bounded executor. This prevents a large GTK selection from overflowing
an eight-item worker queue while retaining per-item job identity, cancellation,
failure, conflict, retry, and Operations Island events.

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

The exact-path `TransferBuffer` remains authoritative for in-process work.
Phase 6O adds `clipboard.rs` as an application/desktop boundary: copy/cut
publishes bounded `text/uri-list`, `x-special/gnome-copied-files`, and
`application/x-kde-cutselection` providers. Paste reads supported MIME streams
asynchronously with a 4 MiB/4096-item ceiling, accepts only local filename URIs,
deduplicates exact decoded `PathBuf` values, then stages through
`ApplicationState`. GTK callbacks perform no filesystem work. Clipboard
managers and ownership lifetimes remain external desktop behavior. Phase 6P
adds bounded memory-only operation history; persistent recovery and overwrite
policy are not implemented.

### `ui.rs`

Phase 6K2 keeps the sidebar widget tree shared while applying Compact,
Balanced, or Comfortable metric classes. `BrowserController` observes the
paned position only after the first allocated idle, clamps it to 128-480 pixels,
replaces a 320 ms debounce source, and submits complete `ViewPreferences`
through `PreferenceWorker`. The paned start child does not absorb window
allocation; labels ellipsize and the scroller does not propagate natural width,
so startup layout cannot overwrite an explicit preference. Resetting
stores no width override and reapplies the active appearance default.

The Operations Island uses a 340-pixel bounded surface with 12-pixel insets.
Title/cancel, detail, flexible progress, and recovery actions occupy separate
rows, removing the old fixed-progress/action overflow while preserving the same
application-owned operation commands.

`ui::build` constructs the header, `GtkPaned` sidebar/content layout, a compact
vertically scrollable Places/Bookmarks/Devices surface, `GtkListView`, empty
overlay, status strip, toast overlay, and compact
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

Phase 6T extends the same bind boundary with Extension, MIME, Created, Accessed,
and Permissions columns. Optional lazy details use weak recycled-row bindings
and never create a parallel directory model. The UI also owns grouping labels,
shared density classes, and pointer plus keyboard/menu column resize routes;
application actions remain the source of policy changes.

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
loop tick. Sort requests run on the existing single directory worker. Its
concurrency is one and stale generations are superseded, but its request channel
is unbounded. On
completion, `browser.rs` rebuilds the model and restores selection by exact
`PathBuf`; it never derives identity from lossy labels.

Phase 6C keeps thumbnail presentation in the virtualized factory bind/unbind
boundary. `ThumbnailPresentation` deduplicates pending exact keys, retains weak
bindings only for bound rows, and caps completed textures/fallbacks at 256
entries. The controller submits requests without blocking, retries a full queue
on a later main-loop tick, drops stale-generation responses, and creates
`GdkMemoryTexture` objects only on the GTK main thread.

Phase 6D adds a `GtkGridView` and a second popover presentation while reusing
the same `GioListStore`, `GtkMultiSelection`, action names, and exact-path
entries as the list. Replacing the grid factory at a discrete size change
rebinds visible cells only; no eager directory-wide thumbnail pass is created.

### `browser.rs`

`BrowserController` is the current application-state coordinator. It owns:

- `NavigationState`;
- every currently selected `DirectoryEntry` mirrored from GTK multi-selection;
- hidden-file preference for the current process;
- current List/Grid mode, bounded grid size, and pending nonblocking preference save;
- request generation and pending listing batches;
- enabled state for navigation and Open actions.

It also owns the list-focused Shift+F10/Menu-key route for the Phase 5C context
menu. Secondary-click selection flows through `GtkMultiSelection`, so the
controller retains every original `DirectoryEntry` and exact path before any
existing action is dispatched.

It also owns Ctrl+C/Ctrl+X/Ctrl+V/F2/Delete/Shift+Delete and file-menu action wiring because
those commands depend on the active selection and current destination. The
actions stage or submit through shared `ApplicationState`; they do not perform
filesystem work. “Move to Trash” remains the recoverable action.
“Delete Permanently…” opens a safe-focus irreversible confirmation containing
escaped exact target labels; its GTK callback submits preserved `PathBuf`
values and performs no filesystem work.

Phase 6N adds an explicit Trash browser mode without replacing local
`NavigationState` identity. A sidebar action requests supported home and
mounted-volume Trash roots from `BrowserWorker`; exact backing paths remain the
selection identity while decoded original paths and deletion dates are display
metadata only. Trash mode swaps context policies to Restore, Delete
Permanently, Empty Trash, and Refresh, and disables inapplicable mutations.

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
refreshes every affected visible directory for copy, move, rename, trash,
restore, or permanent deletion. Permanent-delete cancellation is pre-commit only; a
`Partial` failure refreshes affected directories and remains non-retryable.
Conflict, permission, cross-filesystem, unsupported-trash, and general failures
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
`MoveRequest` and `RenameRequest` values. It starts, progresses, completes,
cancels, or fails jobs through the shared `ApplicationJobManager`. Phase 6O
maps cross-filesystem copy bytes/entries into existing validated progress and
maps destination-committed/source-retained outcomes to `JobFailureKind::Partial`,
which existing application policy refuses to retry blindly. `ApplicationState`
owns the executor so its lifetime matches the application. GTK reaches it only
through application commands and tracked requests.
Move and rename retry methods allocate attempts through
`ApplicationJobManager::retry` before reusing the same queue path.

### `create_operation.rs` and `create_executor.rs`

Phase 6Q adds GTK-independent `CreateRequest` variants for directories, empty
files, template copies, symbolic links, and hard links. Exact source, target,
and destination values remain `PathBuf`; every destination uses no-overwrite
semantics. Template creation reuses the reviewed copy engine and preserves
symlinks without following them. Symbolic links preserve the stored target,
including raw non-UTF-8 and intentionally broken targets. Hard links accept only
regular non-symlink sources and classify cross-filesystem behavior explicitly.

`CreateExecutor` owns one fixed-capacity named worker and maps create progress,
cancellation, conflicts, unsupported cases, permission failures, outcomes, and
retries into the shared job manager. `ApplicationState` serializes duplicate
selections through the existing batch boundary, retains FIFO source identity,
and generates deterministic `(copy N)` retry names without filesystem work in
GTK callbacks. Native name dialogs submit validated typed requests; template
selection and reveal-target metadata use asynchronous GIO. Clipboard name/path
text is derived only from exact UTF-8 values, while local file URI encoding
preserves raw path bytes.

### `drag_drop.rs`

Phase 6R centralizes GDK drag payload conversion, exact-path destination policy,
action negotiation, accessible drop feedback, hover-open, and bounded edge
autoscroll. `gdk::FileList` is the external boundary: every `gio::File` must
resolve to a local `PathBuf`, and raw paths never pass through display text.
Recycled list/grid rows resolve their currently bound `DirectoryEntry` only at
interaction time. A shared application dispatcher sends typed `DropRequest`
values to `BrowserController`; `ApplicationState` converts them into existing
FIFO copy, move, symbolic-link, or Trash jobs with fail-if-exists semantics.
GTK owns only gesture presentation and submission, never filesystem mutation.

### `file_watcher.rs`

Phase 6S owns one non-recursive `gio::FileMonitor` for the active successful
local listing. A single 140 ms cancellable source normalizes and deduplicates
events into exact-path `WatchBatch` values with explicit caps of 16,384 events,
4,096 paths, and 1,024 rename pairs. Overflow requests one conservative reload.
The callback never enumerates or reads metadata; `BrowserController` accepts
only the current monitor generation/directory and submits one superseding
`BrowserWorker` enumeration per batch. Linear reconciliation translates rename
chains, preserves surviving exact selections, and restores a stable path/index
scroll anchor after existing 256-entry model insertion finishes. Monitors stop on
navigation, replacement, Trash mode, failed listing, and shutdown.

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

### `permanent_delete_executor.rs`

`PermanentDeleteExecutor` owns one named worker and a fixed-capacity queue of
validated core `PermanentDeleteRequest` batches. The core builds a complete
no-follow postorder plan before mutation, refuses filesystem roots, selected
mount roots, mounted subtrees, duplicate or nested selections, and paths whose
device, inode, or kind changes before removal. Symlinks are removed as links
and are never traversed.

Cancellation can produce `Cancelled` only before the first removal. After
commit, execution finishes or produces `JobFailureKind::Partial` with exact
removed/planned counts. Application retry rejects that partial outcome; safe
preflight failures and pre-commit cancellation retain normal retry identity.

### `trash_lifecycle.rs` and `restore_executor.rs`

`floe-core::trash_lifecycle` models supported local freedesktop Trash roots,
bounded no-follow `.trashinfo` parsing, exact payload/metadata/original paths,
and atomic no-replace restore. Home Trash and `.Trash/$uid` / `.Trash-$uid`
mounted-volume candidates are explicit. Symlinked roots, non-sticky shared
roots, oversized or malformed metadata, NUL, invalid percent encoding, and
non-normalized destinations do not become restore targets. Payloads with
missing metadata remain visible so users can still delete them.

`RestoreExecutor` is a fixed-capacity application worker using the shared job
registry. It removes matching `.trashinfo` only after the payload move succeeds.
Existing destinations become normal conflict outcomes; cleanup after a
committed move becomes an explicit non-retryable partial failure. GTK submits
`RestoreRequest` through `ApplicationState` and performs no filesystem work.
Trash permanent deletion and Empty Trash reuse Phase 6M for both payload and
companion metadata.

### `worker.rs`

`BrowserWorker` owns one named `std::thread` and two `std::mpsc` channels. An
atomic latest-generation value lets core enumeration cooperatively stop stale
requests. This is a browsing worker, not the future filesystem mutation job
engine. The request channel is currently unbounded; single-threaded execution
and generation supersession bound concurrency, not queued request count.

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

The app layer uses GLib home and all eight XDG user-special directory kinds:
Desktop, Documents, Downloads, Music, Pictures, Public Share, Templates, and
Videos. It includes only existing directories and removes exact duplicate paths,
while retaining each authoritative `PathBuf`.

### `bookmarks.rs`

The application-owned `BookmarkWorker` asynchronously loads and saves
`$XDG_CONFIG_HOME/floe/bookmarks.bin` (or GLib's equivalent user configuration
root) on a fixed-capacity channel. Bookmark submissions require absolute,
distinct existing directories. The versioned binary format round-trips exact
raw Unix path bytes and rejects relative, duplicate, and oversized encoded data;
it never reconstructs a path from display text. Persistence
creates a same-directory 0600 temporary file, synchronizes it, atomically renames
it into place, and synchronizes the 0700 parent directory. GTK callbacks only
submit requests and consume structured worker events.

### `devices.rs`

`DeviceMonitor` is the application-owned GIO boundary for drives, volumes, and
mounts. It converts the live `gio::VolumeMonitor` topology into immutable,
deduplicated `DeviceSnapshot` values and refreshes observers on drive, volume,
and mount signals. Stable opaque IDs are separate from user-facing labels.

The snapshot policy distinguishes mounted/unmounted, removable, local,
non-local, multiple-root, unavailable, and busy states. GIO mount, unmount, and
eject calls are asynchronous and accept a window-parented `GtkMountOperation`
for desktop-native authentication. Passwords and passphrases remain opaque to
Floe and belong to the desktop prompt. In-flight actions remain busy until their
callback resolves;
completion refreshes snapshots and structured failures become actionable UI
feedback. Only a single mounted local filesystem root becomes a `PathBuf`
navigation target. Remote/network roots remain typed as non-local and are
deferred rather than lossily converted into local paths.

### Privileged access boundary

Phase 14B implements an experimental read-only privileged provider without
changing local `PathBuf` navigation or job types. `privileged_access.rs` owns a
private `PrivilegedResourceId`, validates exact absolute local path → GIO file
URI → GLib-built `admin` URI identity, and rejects arbitrary administrator URIs.
The GIO/GVfs provider owns administrator GFiles, enumerators, and cancellables
on the GLib main context and emits only generation-bound typed pages/events.

Its virtualized Administrator-badged view has separate history, selection, and
folder-only activation. It never adapts an administrator resource into a local
`DirectoryEntry` or `PathBuf` executor, and has no mutation, preview, thumbnail,
terminal, launcher, archive, custom-action, clipboard, or plugin route. GVfs
and the polkit agent own authentication; Floe's process UID and credential
boundary do not change. Privileged mutations and removing the experimental
guard remain gated by `docs/PRIVILEGED_ACCESS.md`.

### `appearance.rs`

`Appearance` centralizes preset-level radius, gap, opacity, row padding, shadow,
floating-panel, and sidebar-width values. It generates GTK CSS using libadwaita
semantic colors. Phase 6A adds shared list-heading, secondary metadata, tabular
figure, and focus-visible rules without forking widget trees by preset. Phase
6K2 adds shared sidebar-density and Operations Island action-size rules without
forking the navigation widgets. Frosted
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
registry, all four executors, unified transfer buffer, and tracked
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
bits, file/directory access and modification timestamps, and link targets. It
synchronizes regular-file content and resulting metadata. Symlink metadata,
ownership, ACLs, extended attributes, security labels, sparse extents, and
reflink state are not claimed as preserved. Planned regular-file bytes are
checked against destination `statvfs` user-available bytes before output
creation; this is a point-in-time preflight, not a reservation. Phase 6P adds
bounded memory-only history and exact-identity move/rename Undo. There is no
persistent operation journal or overwrite path. The current
direction is:

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
bounded copy, move/rename, GIO trash, restore, and permanent-delete executors (implemented)
```

Phase 4D exposes the Phase 4C move/rename models through application-owned
commands, a file-actions menu, keyboard shortcuts, validated rename dialog,
and the generic Operations Island.
These operations use atomic same-filesystem no-replace rename and Phase 6O's
staged cross-filesystem fallback. Overwrite and operation persistence remain
unimplemented. Phase 6P adds deterministic Keep Both and batch-scoped Skip All
beside existing Keep Existing/Retry With New Name; Replace remains absent.

Phase 4E implements the separate XDG/GIO trash job boundary. Phase 4F exposes
it through one selection-sensitive “Move to Trash” action and Delete shortcut.
The callback submits through `ApplicationState`; no direct GIO work appears in
widgets and the read-only browser worker remains separate. The recoverable
Trash action has no confirmation dialog. Phase 6M later adds Shift+Delete and
permanent deletion; Phase 6N adds Trash browsing, restore, and Empty Trash.
Phase 6P offers Undo only for completed move/rename records whose captured
destination identity still matches; irreversible and incomplete work remains non-undoable.

Phase 5A generalizes retry dispatch and bounds application terminal history.
Phase 5B adds the Operations Island Retry control for failed and cancelled
attempts. Phase 5E prevents generic retries for destination conflicts and
establishes explicit keep-existing/retry-with-name decisions without enabling
overwrite. Phase 5F adds the focused, dismissible conflict interaction and
recoverable Operations Island action. Phase 6P adds stable serial batch records,
pause-after-current/resume, queued-item cancellation, item counts, byte-only
speed/ETA, terminal summaries, and a bounded memory-only history dialog.

Phase 6H adds `location_input.rs` as a GTK-independent input and recovery policy. GTK only captures explicit text and submits it to `BrowserController`; absolute-path syntax is checked immediately, directory enumeration remains on `BrowserWorker`, and failed submissions restore the exact previous navigation snapshot. Existing non-UTF-8 `PathBuf` state is used directly until the user explicitly submits edited UTF-8 entry text.

Phase 6I reuses the existing asynchronous GIO launcher/chooser boundary. `launcher::launch_default` now returns `DefaultLaunch::Launched` or `DefaultLaunch::NoDefault(OpenWithOptions)` after content-type/application resolution. `BrowserController` presents the existing chooser for the latter; the UI never infers or mutates a default association. Exact original paths continue through `gio::File` URIs.

Phase 6L adds `system_thumbnailer.rs` as an application-layer provider registry
and supervised process boundary. The capacity-64 thumbnail worker discovers
user/system freedesktop `.thumbnailer` definitions with deterministic
precedence, resolves allowed MIME types through GIO, parses reviewed `%i`, `%u`,
`%o`, `%s`, and `%%` codes into raw argv, and launches no shell. Native raster
decode remains preferred. Provider-backed requests validate support before
persistent-cache lookup, execute in a private temporary directory on cache miss,
terminate the process group on cancellation/timeout, accept only no-follow
regular output under 32 MiB, decode it as bounded passive PNG, revalidate the
source, and reuse the existing cache/pixel result boundary. GTK observes only
owned pixels or failure and retains generic icons. These helpers are supervised
but not sandboxed; Phase 18L owns isolation. Phase 6N adds standards-correct
local Trash browsing and restore. Phase 6O adds space-aware, metadata-aware
copy, staged cross-filesystem move, and bounded desktop clipboard
interoperability. Phase 6P adds operation control. Phase 6Q adds create,
duplicate, link, reveal, and explicit path-copy commands. Phase 6R drag and drop
is the sole recommended next phase.

## Phase 18A security architecture boundary

Phase 18A changes documentation and sequencing only. The authoritative
[threat model](security/THREAT_MODEL.md),
[decision record](security/PHASE_18A_DECISIONS.md), and
[test plan](security/PHASE_18_TEST_PLAN.md) constrain later security work while
preserving the existing GTK → application controller → bounded worker/core
boundaries. They select no runtime dependency, crypto library, credential
backend, vault backend, or sandbox mechanism.

Phases 18T–18X now implement the bounded integrity and data-loss-safety slice
defined by that architecture. Application-owned workers handle hashing,
baseline scanning, verified-copy verification, mount flushing, and guardrail
preflight; GTK only submits typed requests and observes events. Exact raw paths
remain authoritative throughout.

The integrity layer reuses reviewed Phase 10E SHA-256 and never presents hash
equality as authenticity or safety. Monitoring is explicit, local,
same-device, bounded, and rescan-aware rather than intrusion detection.
Verified Copy distinguishes no output, copied-but-unverified output, and
verified output. Verified removable transfer adds an exact revalidated mount
`syncfs` worker and returns to the GLib context for relationship-aware GIO
eject/unmount; only successful removal reaches SafeToRemove.

`ApplicationState` owns the Protected Folder authority. A strict private store,
policy generation, bounded preflight worker, and non-cloneable single-use
permits form the last application boundary before every destructive executor
dispatch, including queued work, undo/retry, and revised conflict destinations.
Corrupt policy state fails closed. This is accidental-loss prevention, not
encryption, access control, or privilege elevation. Phase 18Y is the next
bounded architecture leaf and owns privacy-aware operation recovery journals.

## Known architectural debt

- `BrowserController` already coordinates several concerns and should not absorb
  mutation execution, previews, tabs, and desktop integration.
- Normal entry/sidebar navigation still changes state before enumeration succeeds. Phase 6H
  direct location submissions use a generation-bound `PendingLocation` snapshot and commit or
restore the complete `NavigationState` after the single directory worker responds.
- Appearance values are partly centralized while local widget margins remain in
  `ui.rs`.
- Sidebar width and density are persisted. Appearance, hidden-file visibility,
  selection, and scroll position are not yet persisted.
- MIME and permissions data remain incomplete. Remote roots, device details
  beyond the current GIO snapshot remain deferred.
- There is no file-watching reconciliation for external changes.
