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

The sidebar contains Home and the available XDG Downloads, Documents, and
Pictures locations. It is visually separate from the directory surface and is
the start child of a native horizontal `GtkPaned` divider. The divider is
keyboard/pointer-native, user-resizable, and has a visible hover affordance.

The sidebar defaults are part of the appearance preset. They range from 152 to
176 pixels, with minimums from 124 to 136 pixels. The chosen width is not yet
persisted across launches.

### Directory surface

The directory surface is a virtualized `GtkListView` backed by
`GioListStore<glib::BoxedAnyObject>`. Phase 6A adds a compact header and aligned
Name, Type, Size, and Modified columns. Each row displays a symbolic file-kind
icon, a lossy display label, a textual kind that does not rely on icon or color,
an available regular-file size, and locale-aware modification time. The
underlying `DirectoryEntry` retains the original path and filename.

Directories sort before other entries. Hidden entries can be toggled. Rows are
inserted into the GTK model in batches so very large results do not arrive in
one main-loop update. Metadata strings are produced in the list factory bind
path for visible/reused rows instead of eagerly for the full directory result.

### Selection and activation

`GtkSingleSelection` provides the visible selected state. The application
controller mirrors the selected `DirectoryEntry`, so selection is not owned
only by a widget. Selection enables the Open action and changes the status text.

Enter/double-click activates a row. Directories navigate; regular files and
non-directory symbolic links are opened asynchronously through GIO's default
application. Paths are converted directly to GIO file URIs rather than rebuilt
from display text. Launch failures appear as toasts.

Phase 5C adds a native popover context menu to each virtualized row. A
secondary-click first selects the exact row under the pointer, then presents
Open, Copy, Cut, Rename, and Move to Trash using the same enabled state and
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

Phase 4D uses one application-owned internal transfer buffer. Ctrl+C stages a
copy, Ctrl+X replaces it with a move intent, and Ctrl+V submits an exact
destination beneath the current directory. F2 and the visible file-actions
menu open a focused rename dialog with inline validation. The editable name is
kept separate from the selected entry's original `PathBuf`; GTK callbacks do
not execute filesystem work. The buffer remains internal to Floe rather than
interoperable with other file managers.

Active work appears in a compact, bottom-end Operations Island. It uses visible
filename and state text, a stable progress bar that pulses before a total is
known, and a symbolic cancel button with an accessible label and tooltip.
Completion and cancellation remain visible briefly; conflicts and failures use
non-modal toasts with a concrete recovery action. The directory refreshes after
a successful copy, move, or rename affecting the visible location. Move and
rename remain same-filesystem only; overwrite is unavailable.

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
the recoverable desktop Trash rather than permanent deletion; permanent delete
and Shift+Delete remain unavailable.

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

### Motion and reduced motion

Current custom motion is limited to the path/location crossfade and native GTK
state transitions. Future motion must explain spatial change, remain
interruptible, and avoid blocking input. Floe does not yet expose an explicit
reduced-motion setting; new nonessential animation must account for GTK/system
animation preferences before it ships.

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
- **Grid and thumbnails:** lazy, bounded visual browsing that retains the same
  path-safe domain entries and selection semantics as list view.

None of these planned surfaces should move filesystem operation code into GTK.
