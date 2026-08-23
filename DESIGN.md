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
`GioListStore<glib::BoxedAnyObject>`. Each row displays a symbolic file-kind
icon, a lossy display label, and a compact kind or size detail. The underlying
`DirectoryEntry` retains the original path and filename.

Directories sort before other entries. Hidden entries can be toggled. Rows are
inserted into the GTK model in batches so very large results do not arrive in
one main-loop update.

### Selection and activation

`GtkSingleSelection` provides the visible selected state. The application
controller mirrors the selected `DirectoryEntry`, so selection is not owned
only by a widget. Selection enables the Open action and changes the status text.

Enter/double-click activates a row. Directories navigate; regular files and
non-directory symbolic links are opened asynchronously through GIO's default
application. Paths are converted directly to GIO file URIs rather than rebuilt
from display text. Launch failures appear as toasts.

### Status, loading, errors, and empty folders

The bottom status area shows a spinner during enumeration, incremental row-load
counts, total item counts, or the selected filename. An empty folder shows a
symbolic folder icon and plain-language message. Directory and launch failures
surface in an `AdwToastOverlay` while technical context is sent to tracing.

### Nonvisual copy-job foundation

Phase 4A adds a nonvisual, fixed-capacity copy executor to the existing
operation/job lifecycle. Copy requests preserve original `PathBuf` values,
never follow symlinks, fail when the destination exists, and report structured
progress, cancellation, completion, and failure events. It does not yet expose
copy/paste controls or render the Operations Island. GTK will observe and
submit through this boundary rather than own copy correctness.

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
file labels use medium weight, the path uses semibold, and secondary details use
the theme's dim-label color. Filenames may be long or non-UTF-8, so display text
may ellipsize and provide a tooltip while the original path remains untouched.

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

- **Operations Island:** a compact, non-modal observer for queued/running file
  jobs, progress, cancellation, pause/resume where valid, and failures.
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
