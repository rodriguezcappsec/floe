# Floe Design

This document is Floe's persistent visual and interaction design source of
truth. It describes both the interface that exists today and the direction for
future work. `AGENTS.md` remains authoritative for architecture and scope.

## Product character

Floe is a modern, Wayland-first spatial file manager. It should feel native to
Linux while combining the calm polish of a focused desktop tool with the
horizontal, spatial thinking associated with Niri and Miller columns.

The visual language is built around:

- floating, rounded surfaces with intentional gaps;
- restrained borders, shadows, translucency, and motion;
- strong light and dark appearances inherited from the desktop theme;
- compact navigation and keyboard-first operation;
- density choices that do not require separate widget implementations;
- readable fallbacks whenever transparency or compositor blur is unavailable.

Function, legibility, and responsiveness take priority over visual effects.

## Currently implemented

### Window and navigation header

The application uses an `AdwApplicationWindow` with an `AdwHeaderBar`. The
header contains Back, Forward, Parent, Open, and hidden-file controls plus a
centered path representation. `Ctrl+L` replaces that representation with a
local-path entry; Escape returns focus to the file list.

Icon-only controls use GTK symbolic icons, tooltips, and explicit accessible
labels. Disabled navigation and Open actions use native GTK sensitivity.

### Places sidebar

The compact sidebar is a vertically scrollable surface with separate
Places, Bookmarks, and Devices sections. Places always starts with Home, then
includes each distinct XDG Desktop, Documents, Downloads, Music, Pictures, Public Share,
Templates, and Videos directory that currently exists. Duplicate XDG paths and
missing directories are omitted rather than presenting broken destinations.
Original `PathBuf` values remain authoritative.

The sidebar is the start child of a native horizontal `GtkPaned` divider. Its
pointer/keyboard-native handle makes the panel user-resizable while preserving a
128-pixel minimum. Floe restores the chosen width on launch, clamps persisted
and dragged values to 128-480 pixels, and debounces divider writes by 320 ms.
Reset Sidebar Width clears the override and reapplies the active appearance
preset's default.

Sidebar Density in the view-options menu provides Compact, Balanced, and
Comfortable choices without forking the widget tree. All modes keep related rows
at 2 pixels; section gaps are 4, 8, and 12 pixels, and outer margins are 6, 8,
and 12 pixels respectively. Compact is the default daily-driver rhythm. The
state is applied immediately and persisted with the view and grid preferences.

Bookmarks may add the current folder, navigate an existing bookmark, or remove
it with an explicit adjacent control. Loading and saving are asynchronous;
buttons expose loading/in-flight states and failures remain visible as toasts.
The stored format owns exact raw Linux path bytes, not lossy labels.

Devices are live GIO drive, volume, and mount snapshots. Rows distinguish
mounted, unmounted, remote, multiple-location, unavailable, and busy states.
Available Mount, Unmount, and Eject actions use native asynchronous GIO mount
operations, disable conflicting controls while busy, and surface failures in a
toast. Mount and unlock pass a window-parented `GtkMountOperation`; any password
or passphrase prompt belongs to the desktop, and Floe never receives, stores, or
logs credentials. A mounted local filesystem root navigates through normal Floe
navigation.
Remote/network roots remain explicitly unavailable instead of being converted
into a `PathBuf`; their browsing support is deferred.

Mounted local rows add capacity, free-space, and read-only detail only after a
bounded asynchronous GIO query returns for the current device identity. Unknown
facts are omitted rather than shown as zero or writable.

### Directory surface

The directory surface is a virtualized `GtkListView` backed by
`GioListStore<glib::BoxedAnyObject>`. Phase 6A adds a compact header and aligned
Name, Type, Size, and Modified columns. Each row displays a Floe-owned vector file-kind
icon or eligible image thumbnail, a lossy display label, a textual kind that
does not rely on icon or color,
an available regular-file size, and locale-aware modification time. The
underlying `DirectoryEntry` retains the original path and filename.

Phase 6B turns those headings into native flat buttons with visible arrows,
tooltips, accessible labels, and pressed state for the active column. Repeating
the active heading reverses direction; a different heading begins ascending.
Navigable directories remain grouped first, missing optional metadata remains
last, and the selected entry is restored using its exact original path. The
worker owns comparison work so pointer and keyboard callbacks stay responsive.

Directories sort before other entries. Hidden entries can be toggled. Rows are
inserted into the GTK model in batches so very large results do not arrive in
one main-loop update. Metadata strings are produced in the list factory bind
path for visible/reused rows instead of eagerly for the full directory result.

Phase 6T adds Extension, MIME Type, Created, Accessed, and Permissions to the
existing columns. Name remains mandatory; optional columns and clamped widths
persist globally or in an opt-in per-folder override. Directories may be first
or last, while None, Type, and raw-extension grouping remains independent of the
active sort column. MIME, Created, Accessed, and Permissions are requested only
for bound rows when needed; delayed enrichment never reorders the model or
paints a recycled row.

Phase 6C adds a 32-pixel thumbnail slot without changing row identity or
selection behavior. Only bound rows lazily request regular PNG/JPEG images.
Generic Floe icons remain stable while work is queued and whenever a source
is unsupported, stale, oversized, unreadable, or malformed. Completed textures
use a 5-pixel radius and a 256-entry in-memory cache. There is intentionally no
spinner per row.

Phase 6E keeps those same bound-row/cell presentation rules while adding
persistent freedesktop thumbnail reuse. The worker selects the standard
`normal` tier through 128 pixels and `large` above it, validates the
canonical file URI plus enumerated modification time and size, and returns to
the stable generic icon or source decode on any cache fault. Cache maintenance
has no visible modal state and never competes for GTK ownership.

Phase 6F corrects camera-authored orientation before any list/grid scaling and
adds reviewed static raster thumbnails for WebP, GIF, BMP, TIFF, and ICO.
Animated files show a stable first frame rather than motion in the directory
surface; vector/active and unreviewed content retains the generic icon.

Phase 6G introduces one bundled full-color SVG family with a blue dimensional
folder silhouette and a shared folded-page construction for files. Interior
glyphs distinguish links, documents, spreadsheets, presentations, images,
audio, video, archives, code, PDFs, and executables by both shape and restrained
category accents. Generic files remain deliberately quieter. List icons use a
28-pixel optical size; grid icons grow only from 48 to 88 pixels while image
thumbnails retain their independent 64-192 pixel scale. The visible name and
textual type remain authoritative, every decorative image exposes the GTK
Presentation role, and selected/focused state remains visible through semantic
GTK background, opacity, and focus-ring styling rather than color alone.

Phase 6D adds a native virtualized grid beside the list. Both views share one
`GioListStore` and one `GtkMultiSelection`; switching presentation never forks
directory state or reconstructs a path from a label. Grid cells use centered,
two-line ellipsized names and the same activation and context actions as list
rows. Native header toggles expose List/Grid mode, while a keyboard-operable
scale and zoom buttons select discrete 64, 80, 96, 112, 128, 160, or 192 pixel
thumbnail edges. Hover, selection, and focus-visible treatments are consistent
across both views. View mode and grid size persist across launches through an
application-layer worker rather than GTK file I/O.

### Selection and activation

`GtkMultiSelection` provides the visible selected state. The application
controller mirrors every selected `DirectoryEntry`, so selection is not owned
only by a widget. Exact policy enables single-target or batch actions and changes
status text for zero, one, or many targets.

Enter/double-click activates a row. Directories navigate; regular files and
non-directory symbolic links are opened asynchronously through GIO's default
application. Paths are converted directly to GIO file URIs rather than rebuilt
from display text. Launch failures appear as toasts.

Phase 5C adds a native popover context menu to each virtualized row. A
secondary-click first selects the exact row under the pointer, then presents
Open, Copy, Cut, Rename, Move to Trash, and Delete Permanently using the same enabled state and
application actions as the header and keyboard shortcuts. Shift+F10 and the
Menu key provide the focused-list keyboard route. Destructive-adjacent Trash is
separated from editing actions.

Phase 5D adds Open With as a distinct context/header action for launchable file
kinds. Discovery shows immediate loading status, then a focused native dialog
with the current default, one selectable application list, Cancel, Open, and an
explicit Set as Default action. Opening never silently changes associations;
association failures and launch failures remain recoverable toasts. Custom
external tools are not mixed into the application chooser.

### Status, loading, errors, and empty folders

The bottom status area shows a spinner during enumeration, incremental row-load
counts, total item counts, or the selected filename. An empty folder shows a
symbolic folder icon and plain-language message. Directory and launch failures
surface in an `AdwToastOverlay` while technical context is sent to tracing.

### Transfer, rename, and Operations Island

Phase 4D uses one application-owned exact-path transfer buffer. Phase 6O keeps
that buffer authoritative while Ctrl+C/Ctrl+X also publish bounded local-file
URI data in freedesktop, GNOME, and KDE-compatible copy/cut formats. Ctrl+V
asynchronously accepts only supported local file URIs, rejects malformed,
remote, oversized, or over-count payloads, stages decoded paths through
application state, then submits exact destinations beneath the current
directory. A failed desktop clipboard publication is stated as Floe-only
staging rather than silently claiming interoperability. F2 and the visible
file-actions menu open a focused rename dialog with inline validation. The
editable name stays separate from the selected entry's original `PathBuf`;
GTK callbacks do not execute filesystem work.

Active work appears in a compact, bottom-end Operations Island. It uses visible
filename and state text, a stable progress bar that pulses before a total is
known, and a symbolic cancel button with an accessible label and tooltip.
Completion and cancellation remain visible briefly; conflicts and failures use
non-modal toasts with a concrete recovery action. The directory refreshes after
a successful copy, move, or rename affecting the visible location.
Cross-device move progress uses the same island; a destination-committed,
source-retained outcome is explicitly partial and non-retryable. Overwrite
remains unavailable.

Phase 6K2 corrects the island's previous crowded single-row geometry. The
340-pixel surface uses 12-pixel insets, a title/cancel row, a separate detail
row, a flexible full-width progress row, and an end-aligned recovery-action row.
Retry and Resolve Conflict therefore remain reachable without overlapping or
forcing the progress bar beyond the surface.

Phase 5B keeps failed and cancelled terminal jobs visible and adds a labelled
Retry button in the Operations Island. The control uses native keyboard focus,
disables immediately after submission to prevent duplicate attempts, and hides
when the fresh job enters its running state. Completed operations remain
non-retryable and dismiss after the existing terminal delay.

Phase 5E separates destination conflicts from ordinary failures. Generic Retry
is intentionally unavailable for a conflict because it would resubmit the same
destination. Application state retains exact source/destination paths and
offers only two explicit decisions: keep the existing destination, or retry
with one validated sibling filename. Revised attempts retain the logical
operation ID and receive a fresh job ID. Overwrite and apply-to-all remain
deferred.

Phase 5F presents that contract in a focused non-blocking dialog. Incoming and
existing paths are visible context only; the retry field begins empty so a
lossy display name can never become an implicit destination. Inline validation
requires one different filename, keeps keyboard focus in the dialog, and
associates errors with the entry for assistive technology. Cancel or window
dismissal makes no decision and leaves a labelled Resolve Conflict action in
the Operations Island. Keep Existing acknowledges without a new job; Retry
with New Name returns to normal queued/progress feedback.

Phase 6P gives serial multi-item work a stable batch identity and truthful
item-boundary controls. “Pause after current” never implies an active syscall or
GIO request is suspended; Resume continues FIFO and Cancel stops the active
attempt where cooperative cancellation is still possible while removing queued
items. The Island distinguishes byte and item progress, shows smoothed speed and
ETA only for meaningful determinate byte samples, and summarizes completed,
skipped, failed, and cancelled batch items. Conflict UI adds deterministic Keep
Both and batch-only Skip All, but deliberately no Replace action. A recent-action
button opens bounded memory-only history. Clear Completed preserves actionable
failures and conflicts; Undo appears only for completed move/rename records and
submits an exact-identity, no-overwrite reverse move.

### Create, duplicate, links, and path identity

Phase 6Q adds New Folder, New Empty File, and New From Template to the header
and background context menu. Folder and file creation use a focused validated
name dialog. Template choice uses the native asynchronous file dialog, starts at
the XDG Templates location when available, then asks for the destination name;
selection never performs the copy itself.

Selection menus add Duplicate, Create Symbolic Link, Create Hard Link, Reveal
Link Target, and Copy Name/Path/Relative Path/URI. Duplicate supports stable
multi-selection FIFO batches and uses familiar `(copy)` / `(copy N)` siblings
without overwrite. Symbolic-link creation is available for one normal entry and
preserves a relative sibling target; hard-link creation is visibly enabled only
for one regular non-symbolic file. Reveal Link Target reads metadata
asynchronously, reports broken/inaccessible targets, and navigates to the exact
target without opening or executing it.

Copy Name/Path/Relative Path publishes newline-separated plain text only when
every selected identity is lossless UTF-8; it refuses lossy display text. Copy
URI instead preserves exact local path bytes with percent encoding. Ctrl+Shift+N
opens New Folder, Ctrl+D duplicates, and Ctrl+Shift+C copies the absolute path;
all commands remain available by pointer and keyboard-accessible menus.

Phase 6R adds standard local-file drag-and-drop to both list and grid views.
Folder rows, the directory background, Places, bookmarks, mounted navigable
devices, and Trash expose exact targets. Copy, move, and link negotiation follows
the desktop drag action; every operation remains no-overwrite and uses the same
Operations Island as its keyboard/menu alternative. A dashed outline, explicit
action-and-destination status text, and accessible descriptions communicate the
drop state without color alone. Folder hover-open waits 720 ms and cancels on
leave/drop; bounded edge autoscroll keeps virtualized views responsive.

Phase 6S keeps successful local directory views live without a visible polling
mode or disruptive per-event redraw. External changes are coalesced, then the
existing loading pipeline reconciles once. Surviving exact selections and a
stable scroll anchor remain in place; exact rename pairs follow the renamed item,
while deleted identities disappear normally. Manual Refresh and operation-driven
refresh use the same preservation policy. Monitor failure leaves browsing usable
and gives a clear “use Refresh” recovery message.

### Trash job foundation

Phase 4E adds the application-owned backend contract for moving one original
path to the desktop Trash through GIO. The bounded worker reports queued,
running, completed, cancelled, and failed states through the same job boundary.
Cancellation is cooperative: it can stop work while GIO still accepts
cancellation, but it cannot reverse a trash move after the desktop service has
committed it.

Phase 4F exposes this backend through an explicitly labelled “Move to Trash”
menu item and the conventional Delete shortcut. The action is available only
with one selected entry, keeps the browser responsive, refreshes the affected
parent after completion, and never implies that a failed request silently
deleted the item. Floe uses no confirmation dialog because this action targets
the recoverable desktop Trash.

Phase 6M adds a separate “Delete Permanently…” action and `Shift+Delete` for
multi-selection. It always opens a focused, non-blocking irreversible
confirmation showing the target count and a scrollable, copyable list of
escaped exact paths. Cancel is the default and initial focus; the confirm
button uses destructive styling. Confirm submits one application-owned batch,
never performs filesystem work in GTK, offers no undo, and never claims secure
erase. Operations Island distinguishes preparing, deleting, cancelled before
deletion, completed, and non-retryable partial failure.

### Trash lifecycle

Phase 6N adds Trash as a normal Place with an explicit special-location state.
List rows replace Type and Modified with Original and Deleted values only when
valid freedesktop metadata exists; exact backing/original paths remain tooltips
and operation identity. Malformed or orphaned payloads stay visible with
“Original location unavailable” so deletion remains possible without inventing
restore data. Direct activation can open files, while trashed folders must be
restored before browsing their contents.

Trash item menus contain Restore and Delete Permanently rather than normal
copy/cut/rename/trash actions. Restore is enabled only when every selected item
has matching original-path and `.trashinfo` metadata. Existing destinations
open the established non-modal conflict dialog; overwrite is never offered.
Empty Trash is a background/header action, requires aggregate safe-focus
irreversible confirmation, and remains available without selecting every item.
All destructive language says Delete Permanently or Empty Trash, never secure
erase. Cancel remains initial focus.

## Implemented appearance system

`crates/app/src/appearance.rs` defines `AppearancePreset` and one shared token
structure. The preset is selected through `FLOE_APPEARANCE`; Frosted is the
default. Colors come from libadwaita semantic colors rather than a hard-coded
light or dark palette.

| Preset | Radius | Gap | Opacity | Row padding | Shadow | Floating | Sidebar default/min |
| --- | ---: | ---: | ---: | ---: | ---: | --- | ---: |
| Native | 0 | 0 | 1.00 | 8 | 0.00 | No | 176 / 136 |
| Glass | 18 | 16 | 0.78 | 9 | 0.16 | Yes | 168 / 136 |
| Frosted | 16 | 14 | 0.94 | 9 | 0.12 | Yes | 168 / 136 |
| Minimal | 8 | 8 | 1.00 | 8 | 0.00 | Yes | 160 / 128 |
| Compact | 10 | 8 | 0.98 | 4 | 0.08 | Yes | 152 / 124 |

These are current code values, not universal design constants. The system still
contains local widget spacing and margin values; moving all of those into a
coherent token scale remains architectural work.

Native removes floating-panel borders and shadows. Glass lowers panel opacity
but does not simulate blur. Frosted is more opaque and remains readable without
compositor support. Minimal removes panel shadow. Compact reduces row and
sidebar density.

## Interaction principles

### Spacing and panel hierarchy

Major surfaces must remain visually distinct instead of merging into one
dashboard frame. Gaps communicate hierarchy; borders and shadows should remain
subtle. The directory surface is primary, Places is supporting navigation, and
status is subordinate feedback.

### Selection, hover, and focus

- Selection must remain visible without relying on color alone when more states
  are added. The current row selection uses the semantic accent at 20% alpha.
- Hover is a lightweight pointer affordance, currently 8% semantic accent.
- Keyboard focus must remain native, visible, and unobscured. Do not remove GTK
  focus indicators without an accessible replacement.
- Every pointer-only interaction requires a keyboard route. Drag and drop will
  require non-drag alternatives when introduced.

### Error, empty, and loading states

Errors must say what failed and keep the rest of the browser usable. Destructive
errors will need explicit recovery choices rather than transient feedback only.
Empty states should be calm and factual. Loading feedback must not flash for
trivial waits or hide a stalled operation; long jobs will belong in Operations
Island rather than the directory status strip.

### Typography and icons

Floe uses the system/libadwaita typography rather than bundled fonts. Current
file labels use medium weight, the path and list headings use semibold, and Type,
Size, Modified, and status text use the theme's dim-label color. Size and time
columns use tabular figures for steadier scanning. Filenames may be long or
non-UTF-8, so display text may ellipsize and provide a tooltip while the
original path remains untouched.

Use one family of themed GTK symbolic icons. Do not use emoji as structural
icons. Icon-only controls require an accessible name and a tooltip.

### Density

Density changes spacing and sizing, not information architecture. Compact mode
must remain comfortably keyboard-navigable and must not shrink focus or pointer
targets into precision controls. Future user settings should update shared
tokens rather than fork widget trees.

Phase 6T implements Compact, Comfortable, and Spacious file-view choices on the
shared list/grid widgets. Density-only changes retain the model and factory,
exact selection, visible focus, context actions, and drag/drop behavior.

### Motion and reduced motion

Current custom motion is limited to the path/location crossfade and native GTK
state transitions. Future motion must explain spatial change, remain
interruptible, and avoid blocking input. Floe does not yet expose an explicit
reduced-motion setting; new nonessential animation must account for GTK/system
animation preferences before it ships.

### Editable location

The header location is both orientation and navigation. Its resting state is a pointer-operable button with the current path, and Ctrl+L provides the keyboard route. Edit mode starts with the current displayed path selected for replacement; Enter submits an absolute local folder path and Escape returns to the prior browser state. Validation stays beside the field, keeps focus recoverable, and never relies on color alone. A submitted path is committed to navigation history only after its background directory enumeration succeeds.

Phase 6H implements this surface.

### Tabs

Phase 7B adds a compact horizontally scrollable native tab strip between the
header and browser workspace. Each tab has a path-derived display-only title,
full-path tooltip, explicit active toggle state, and separately labelled close
control. A bottom accent and weight change reinforce active state without color
alone. Raw `PathBuf` identity remains solely in application/core state.

Ctrl+T/Ctrl+W create and close, Ctrl+Tab/Ctrl+Shift+Tab switch, and
Ctrl+Shift+PageUp/PageDown reorder. Pointer drag has those keyboard alternatives;
middle-click closes a tab. Folder menus open foreground or background tabs and
middle-click opens a folder in the background without moving focus. Tabs reuse
one virtualized model and one bounded browser/thumbnail/metadata/watcher pipeline;
they never clone widget trees or filesystem workers.

Phase 7C adds Reopen Closed Tab (`Ctrl+Shift+T`) and tab-menu Close Left,
Close Right, and Close Others. Recently closed state is LIFO and bounded rather
than an unbounded history. A normal clean shutdown restores ordered live tabs,
active tab, complete navigation/view state, and bounded closed tabs on the next
launch. Missing or corrupt state quietly falls back to one normal tab. Optional
custom tab names and pins remain deferred.

Phase 7D makes each tab own a GTK-independent split context: a primary
`BrowserSession`, optional secondary session, explicit active side, and a
bounded 20–80% ratio. Each pane retains independent history, selection, scroll,
and view policy while the tab's primary identity remains stable through
close/swap transitions. Workspace version 2 persists this state and migrates
version-1 unsplit sessions. No split widget or shortcut is exposed until 7E.

Phase 7E presents that state as one horizontal native split. The active pane is
identified in text as left or right; color is only reinforcing feedback. The
inactive side is a bounded, explicitly stale snapshot with its exact display
path and an Activate Pane control. F3 toggles the split, F6 switches sides, and
Ctrl+Alt+Left/Right adjusts the primary ratio in 5% steps. Pointer resizing,
swap, close, Open Folder in Other Pane, and direct no-overwrite Copy/Move actions
all preserve one shared browser pipeline. Dragging files between panes remains
Phase 7F.

Phase 7F makes the inactive pane a native file drop destination without turning
its stale snapshot into a second browser. The destination resolves from the
authoritative opposite session on every enter, motion, and commit. Existing
desktop modifiers select copy, move, or symbolic link; dashed highlighting and
action/path/release text provide non-color feedback. Open, Copy, Move, and
Create Links in Other Pane remain explicit alternatives. The target does not
hover-activate the pane, and tab detachment and Miller-column drag stay deferred.

Phase 8A defines the exact Miller navigation model. A column is an
exact directory path plus at most one exact selected direct child. Logical
depths remain stable while only the newest 16 locations are retained, so deep
navigation is bounded and stale UI requests can be rejected explicitly. Leaf
selection, directory descent, same-parent rename, deletion truncation, root
invalidation, and reset are deterministic. Phase 8B presents that model as
horizontally scrolling floating columns with virtualized rows. The active
column shares the browser model; prior columns are visibly retained snapshots,
never new enumerators. Text identifies the active column without relying on
color. One global width is adjustable from 180–520 pixels and persists without
creating per-folder width state. Keyboard/trackpad navigation remains Phase 8C.
Phase 8C makes that surface keyboard-native: Up/Down/Home/End move bounded item
focus, while logical parent/child movement maps to Left/Right in LTR and reverses
in RTL. Modified selection chords remain GTK-owned. Dominant horizontal
trackpad/wheel motion scrolls the column strip without stealing ordinary
vertical column scrolling. Active columns expose text descriptions, and GTK's
disabled-animation setting turns off kinetic scrolling rather than making
navigation unavailable.

Phase 8D gives every active or retained column the same native file and
background menus as list/grid. Secondary click and Shift+F10/Menu establish a
textually announced action owner before dispatch. Menu targets come from exact
column identity, never the visible title. Unsupported retained-column
navigation commands are disabled instead of silently acting on another column.

### Open without a default application

Phase 6I removes a dead end from normal Open. Floe first resolves the selected file's GIO content type and registered applications off the direct interaction callback. A known default launches normally; without one, the existing Open With chooser appears with compatible applications. Choosing Open is a one-time decision. Association changes remain visually and behaviorally separate behind the explicit Set as Default action. Empty chooser results provide a recovery message instead of a blank dialog.

Phase 6K completes the first Places/bookmarks/devices navigation surface. Phase
6L adds installed freedesktop system thumbnailers for video frames, PDF pages,
office documents including DOCX, fonts, text/code, embedded audio artwork, and
archive previews through the bounded thumbnail boundary. Floe does not invoke a
shell or intentionally execute document active content, but the helpers retain
the user's normal authority until the Phase 18L sandbox boundary.

### Privileged access

**Open as Administrator...** is security-designed but intentionally not
exposed. Floe's current browser and operation identities are local `PathBuf`
values and cannot truthfully preserve GFile authority. The action may ship only
after the GFile/GVfs `admin://` provider, polkit authentication, visible
Administrator state, downgrade/cancellation behavior, and documented test and
rollout gates pass. Floe must never elevate the whole GTK process, capture a
password, or construct `sudo`, `pkexec`, or shell commands from a path.

### Multi-selection and context surfaces

Phase 6J uses the platform-native multi-selection contract in list and grid:
Ctrl toggles, Shift extends a range, Ctrl+A selects all, Ctrl+Shift+A clears,
and Escape clears when the file view owns focus. Selection remains visible and
is reported as an exact count; Open, Open With, and Rename stay single-target.

Secondary-clicking any already-selected item preserves the whole selection.
Secondary-clicking an unselected item selects only that target before showing
file actions. Secondary-clicking directory background clears file selection and
shows a deliberately separate menu with Paste, Select All, Refresh, and Edit
Location. Keyboard context-menu access exposes the same active surface.

Copy, move, and Trash submit original paths through application state. A serial
batch dispatcher feeds existing bounded workers one job at a time, preventing
large selections from becoming silent partial operations.

Permanent deletion is implemented by Phase 6M as “Delete Permanently,” with
explicit irreversible confirmation and escaped exact target context. Floe does
not call this secure erase because modern storage and filesystem layers cannot
guarantee it.

### Transparency and accessibility

Transparency is optional. Text, icons, selection, borders, and focus must remain
legible without compositor blur and against both light and dark backgrounds.
Do not implement expensive fake blur. Glass/Frosted should fall back toward an
opaque readable surface when stronger composition guarantees are unavailable.

## Planned surfaces

The following are direction, not current functionality:

- **Miller/Column navigation:** horizontally arranged directory levels that
  preserve one navigation model and complement spatial Wayland workflows.
- **Quick Preview:** a safe Space-key preview surface for supported passive
  formats; preview must never execute active content.
- **Inspector:** a toggleable metadata/details surface for type, size,
  timestamps, dimensions, MIME data, permissions, and future tags.
- **Command palette:** a searchable keyboard-first action surface using human
  names rather than internal identifiers.
- **Tabs and split view:** multiple browser contexts with independent history
  and clear focus ownership.
- **Grid polish:** continue refining large-directory layout and persistent
  thumbnail-cache behavior without changing the shared model/selection contract.
- **Customization:** user-facing theme presets and theme tokens, font family and
  scale controls, and a full MIME/file-association manager built on explicit
  Open With and Set as Default behavior.

None of these planned surfaces should move filesystem operation code into GTK.
## Privacy, security, and integrity states

These surfaces are planned; they are not evidence that Floe currently provides
encryption, sandboxing, malware detection, or integrity monitoring. Security
language is part of correctness and must match the mechanism actually active.

- **Encrypted Vault** is reserved for real encrypted storage. A vault surface
  must show a text label and accessible state for Locked, Unlocking, Unlocked,
  Locking, Lock delayed by open files, and Recovery required. Unlocked state is
  not a promise of protection from applications or malware running as the user.
- **Sensitive Folder** means reduced Floe-owned traces and caches, not
  encryption. **Private Mode** means a non-persistent browsing session, not
  cryptographic privacy. **Protected Folder** adds mistake-prevention friction,
  not attacker resistance. These terms must never be substituted for one
  another.
- **Open Safely** may appear only when an actual reviewed restriction policy is
  active. The surface must name the restricted application/session, show a
  persistent text-and-icon sandbox indicator, and explain important limits.
  Sandbox setup failure returns to an explicit unsupported/error state; it must
  never silently launch normally under the Open Safely label.
- Suspicious-file presentation is an evidence-based inspection mode, not an
  accusation or antivirus verdict. It states the signal that triggered it (for
  example executable metadata, a MIME/extension mismatch, or bidirectional
  controls), offers an escaped filename view, and retains normal inspection and
  recovery routes.
- **Integrity verified** may be shown only after verification completes against
  a named digest or manifest. Changed, missing, new, interrupted, stale, and
  unverified are separate text states; a hash alone is not authenticity or a
  signature.
- Security, privacy, warning, and recovery states use visible wording,
  accessible names, icon or shape, and hierarchy. Color may reinforce meaning
  but cannot carry it alone. High-contrast, screen-reader, reduced-motion,
  keyboard, and pointer behavior are acceptance requirements.
- Recovery is conservative. Interrupted encryption, sanitization, transfer,
  vault, or destructive work shows what is known about source and destination
  and never silently deletes uncertain data. Destructive choices state scope
  and reversibility instead of relying on a generic warning.
- Password and recovery-material UX supports reveal/hide, confirmation when
  creating or changing credentials, Caps Lock indication where the platform
  exposes it, careful focus behavior, and conservative wrong-password errors.
  Secrets never appear in notifications, logs, command arguments, or ordinary
  preference storage.

Detailed mechanism, threat-boundary, cache, and claim rules live in
`docs/PRIVACY_SECURITY.md`. That document, not visual styling, decides whether a
security label is truthful.
