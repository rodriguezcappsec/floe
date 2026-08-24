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
- Home plus every existing, distinct XDG Desktop, Documents, Downloads, Music,
  Pictures, Public Share, Templates, and Videos location
- Switchable virtualized list/grid views sharing one multi-selection model and original `PathBuf` values
- Fixed Name, Type, Size, and Modified columns using already-loaded metadata
- Compact, vertically scrollable sidebar with separate Places, Bookmarks, and
  Devices sections; Compact, Balanced, and Comfortable density choices apply
  immediately, and the resizable divider width is restored on the next launch
- Add/remove folder bookmarks loaded and saved asynchronously with exact Linux
  path identity and private atomic persistence
- Live GIO drive, volume, and mount rows with asynchronous mount, unmount, and
  eject actions; a window-parented native `GtkMountOperation` presents any
  desktop password prompt without Floe receiving credentials, and mounted local
  filesystem roots navigate directly
- Floe-owned scalable folder/file icon family with executable, link, document,
  media, archive, code, PDF, spreadsheet, and presentation distinctions
- Lazy PNG/JPEG/WebP/GIF/BMP/TIFF/ICO thumbnails decoded on a bounded worker
  at list or selected grid size, with embedded orientation applied
- Freedesktop-compatible persistent `normal`/`large` thumbnail cache with
  strict source invalidation and Floe-owned bounded cleanup
- Discrete 64-192 pixel grid sizing with pointer/keyboard controls and persisted view preferences
- Explicit selection with Enter/double-click activation
- Asynchronous regular-file opening through GIO's default application
- GTK-independent path-safe copy requests with explicit fail-on-conflict and
  preserve-or-reject symlink policies
- Fixed-capacity background copy execution connected to application-owned job
  lifecycle events, cancellation, failure mapping, and retry identity
- Application-owned multi-path transfer buffer for Ctrl+C copy, Ctrl+X move, and Ctrl+V paste
- Compact non-modal Operations Island with progress, cancellation, completion,
  conflict, and failure feedback; separate title/cancel, detail, flexible
  progress, and recovery rows prevent action/progress collisions
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
Phase 4F exposes the verified trash job as a recoverable desktop Trash action.
Phase 6J extends it to bounded multi-selection batches. Permanent delete,
Shift+Delete, and built-in restore/undo remain unavailable.
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

Phase 6A gives the virtualized list a stable desktop-style information
hierarchy. A compact header and aligned Type, Size, and locale-aware Modified
columns improve scanning while metadata is still formatted only for bound
visible rows. Full filenames remain available by tooltip, keyboard focus has an
explicit visible treatment, and original paths remain untouched.

Phase 6B makes all four headings native keyboard/pointer controls. Activating
the current heading reverses its explicit ascending/descending order; choosing
a different heading starts ascending. Directories remain first and unavailable
size or modification metadata remains last in either direction. Sorting runs on
the directory worker, reuses shared entries, preserves selection by exact
original `PathBuf`, and retains virtualized 256-row insertion batches.

Phase 6C requests thumbnails only when virtualized rows are bound. A dedicated
fixed-capacity worker opens regular PNG/JPEG sources without following symlinks,
enforces encoded and decoded size limits, and returns owned RGBA pixels for GTK
to present on the main thread. Exact path, size, and modification time form the
cache identity. Unsupported, stale, failed, and queued requests keep their
stable generic icons; the in-memory presentation cache is capped at 256 entries.

Phase 6D adds a native `GtkGridView` without creating a second filesystem model.
List and grid share one `GioListStore`; Phase 6J upgrades selection to
`GtkMultiSelection`, so mode changes
retain selection, activation, navigation, sorting, and file actions. The header
exposes List/Grid controls plus seven bounded grid sizes. Ctrl+1/Ctrl+2 change
view and Ctrl+-/Ctrl++ adjust the grid. View preferences are loaded at startup
and atomically saved by a fixed-capacity application worker; GTK callbacks never
perform configuration-file I/O.

Phase 6E adds persistent image-thumbnail reuse under
`$XDG_CACHE_HOME/thumbnails` (or `$HOME/.cache/thumbnails`). Cache filenames
are the MD5 of the canonical absolute file URI and PNG metadata verifies URI,
source modification time, and byte size before reuse. Private 0700 directories,
0600 atomic files, no-follow reads, decoder limits, and exact source
revalidation keep malformed or stale entries as safe misses. Floe tracks its
own shared-cache entries separately and prunes only entries still marked
`Software=Floe`, with global limits of 2,048 entries, 256 MiB, and 90 days.
Lookup, writes, cleanup, source decoding, and scaling all remain on the bounded
thumbnail worker; cache failures retain the normal generic-icon/source-decode
fallback.

Phase 6F applies decoder-provided EXIF/TIFF orientation before scaling and
persistent-cache storage. The reviewed raster set now includes WebP, GIF, BMP,
TIFF, and ICO alongside PNG/JPEG; animated containers contribute only their
first still frame. SVG, AVIF, HEIF, and unreviewed extensions keep the generic
icon. The same 32-MiB encoded, 128-MiB decoded, 65,535-pixel dimension,
no-follow, exact-source, capacity-64 worker limits remain in force.

Phase 6G replaces theme-dependent generic glyphs with a bundled 14-icon SVG
family. Exact enumerated kind and executable metadata take precedence, then a
case-insensitive extension policy selects readable file-family marks. List
icons use a compact 28-pixel optical size; grid icons scale from 48 to 88 pixels
independently of 64-192 pixel thumbnail edges. File names and textual kinds
remain authoritative, while thumbnails continue to replace eligible image
icons only after the bounded worker returns pixels.

Cross-application clipboard formats, overwrite, apply-to-all,
cross-filesystem copy-delete moves, trash restore/bulk UI, permanent
deletion, previews, tabs, split view, Miller columns, additional heavyweight
thumbnail codecs, and environment-specific integrations remain deferred.

Phase 6H turns the header path into an editable location control. Click the displayed path or press Ctrl+L to edit the current absolute path; Enter validates and opens it on the bounded directory worker, while Escape cancels. Empty, relative, missing, unreadable, and non-directory locations remain in edit mode with recovery guidance. Existing navigation continues to own the original `PathBuf`; the lossy display string becomes a new path only when the user explicitly edits and submits it.

Phase 6I makes normal Open resolve GIO applications asynchronously before launch. A registered default opens immediately; when no default exists, Floe automatically presents the same compatible-application chooser used by Open With. One-time Open never changes associations, and Set as Default remains a separate explicit button.

Phase 6J adds Ctrl/Shift multi-selection in both views, Ctrl+A and clear-selection
keyboard routes, exact-path restoration after sorting, and accurate zero/one/many
action states. Secondary-click preserves an existing multi-selection or retargets
to one unselected entry. Directory-background secondary-click clears file selection
and offers Paste, Select All, Refresh, and Edit Location. Copy, move, and Trash use
an application-owned serial batch dispatcher, so selections larger than worker
queue capacity are not silently dropped.

Phase 6K completes the first Places and storage-device pass. The sidebar shows
all distinct existing XDG user folders, persists user-added folder bookmarks
without reducing raw paths to display text, and observes GIO `VolumeMonitor`
drive/volume/mount changes. Device rows expose honest mounted, unmounted, busy,
unavailable, and failed-action feedback. Floe navigates mounted local filesystem
roots; remote/network roots remain unavailable until the dedicated remote
filesystem phase. Phase 6K2 adds a compact default rhythm plus Balanced and
Comfortable sidebar density choices. Divider changes are clamped to 128-480
pixels, saved after a 320 ms debounce, restored on launch, and can be reset to
the active appearance preset's default width.

Encrypted or password-protected mounts use the desktop-native,
window-parented `GtkMountOperation`; Floe does not ask for, store, or log the
password. A future **Open as Administrator...** action is security-designed but
intentionally not exposed yet. It requires the documented GFile/GVfs
`admin://` provider and rollout gates. Floe will never elevate the whole GTK
application or interpolate paths into `sudo`, `pkexec`, or shell commands.

Phase 6K2 is complete. The immediate next branch is
`phase-6l-system-thumbnailers`, consuming reviewed system thumbnailers for video
frames, PDF pages, office documents including DOCX, fonts, text/code, embedded
audio artwork, and archive previews without executing active content. It is
followed by `phase-6m-permanent-delete`. Phase 6M will use
the truthful label “Delete Permanently,” require explicit confirmation, and avoid
claiming secure erase where storage cannot guarantee overwriting.

Later customization milestones include first-class theme and font controls plus
a full file-association manager. The existing Open With and explicit Set as
Default flows remain the safe foundation for association management.

## Project documentation

- [`DESIGN.md`](DESIGN.md) — implemented and planned visual/interaction system
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — current code and data flow
- [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) — build, run, test, and troubleshoot
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — completed, next, and later milestones
- [`docs/PRIVILEGED_ACCESS.md`](docs/PRIVILEGED_ACCESS.md) — security design and
  rollout gates for future administrator-scoped browsing
