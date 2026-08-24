# Floe

This file is the persistent project context and instruction set for Codex and other coding agents working on this repository.

Read this entire file before making changes.

The project is in early development. Missing features may be intentionally deferred.

---

# Project Mission

Build a fast, secure, polished Linux desktop file manager written in Rust for modern Wayland desktops.

Working project name:

**Floe**

Intended product description:

> Floe — a spatial file manager for Wayland.

First-class environments:

* Niri
* KDE Plasma

The application should also provide generic compatibility with other Wayland compositors and desktop environments.

The project should combine:

* Rust performance and safety
* native Linux/Wayland integration
* GTK4
* polished modern visuals
* spatial navigation
* excellent keyboard workflows
* strong asynchronous filesystem operations
* optional environment-specific integrations
* Linux power-user functionality

Floe is Wayland-first rather than Niri-exclusive.

Niri and KDE Plasma may expose additional capabilities, but the core filesystem engine, navigation system, job engine, and general UI must depend on neither.

---

# Product Identity

Floe should borrow useful ideas from:

* Niri
* macOS Finder
* modern GNOME/GTK applications
* KDE/Plasma desktop workflows
* modern Linux ricing
* keyboard-first power-user tools

Do not copy another file manager literally.

Floe is NOT intended to become:

* Nautilus with different CSS
* Dolphin rewritten in Rust
* Finder for Linux

Its intended identity is:

* spatial
* fast
* polished
* keyboard-friendly
* visually configurable
* native to modern Linux desktops

---

# Primary Technology

Language:

* Rust

UI:

* GTK4
* gtk-rs
* libadwaita where useful

Linux desktop integration:

* GIO
* GLib
* XDG standards
* freedesktop specifications
* XDG Desktop Portals
* Wayland

Potential supporting crates may include:

* `serde`
* `thiserror`
* `tracing`
* `notify`
* `ashpd`
* `tokio` or another appropriate task/worker system

Future Niri integration may use:

* `niri-ipc`

Dependencies must be added deliberately.

Do not add a crate merely because it appears in this document.

---

# Supported Environments

Primary platform:

* Linux
* Wayland

First-class environments:

* Niri
* KDE Plasma

Secondary compatibility target:

* other standards-compliant Wayland compositors/desktops

Possible environments may include:

* GNOME
* Hyprland
* Sway
* COSMIC
* River
* Wayfire
* others

Do not add compositor-specific code merely to claim compatibility.

Generic functionality should rely on standards such as:

* GIO
* XDG
* freedesktop specifications
* XDG Desktop Portals

Environment-specific functionality belongs behind explicit integration boundaries.

---

# Core Architectural Rule

THE GTK UI MUST NOT IMPLEMENT FILESYSTEM OPERATIONS DIRECTLY.

This is the project's most important technical rule.

Do not write filesystem operation logic inside GTK callbacks.

Target conceptual architecture:

```text
GTK UI
   |
   v
Application Commands / State
   |
   v
Job Manager
   |
   v
Filesystem Engine
```

Results and events flow back toward the UI.

The GTK main loop must remain responsive.

---

# Core / UI Separation

The filesystem/domain layer must not depend on GTK.

Preferred initial workspace shape:

```text
crates/
  core/
  app/
```

Where:

```text
core
```

owns concepts such as:

* filesystem models
* directory enumeration
* path handling
* navigation state
* future file-operation models
* future job state

And:

```text
app
```

owns:

* GTK application
* windows
* widgets
* application wiring
* appearance
* desktop integration
* user interaction

Additional crates may be created later when a real architectural boundary justifies them.

Do not create crates merely for appearance.

---

# Desktop Integration Architecture

Desktop/compositor integration must be isolated from the filesystem core.

Target conceptual architecture:

```text
                    Floe
                      |
             DesktopIntegration
                      |
       ┌──────────────┼──────────────┐
       │              │              │
     Niri           Plasma         Generic
  Integration     Integration      Wayland
```

A possible future implementation location is:

```text
crates/app/src/integration/
    mod.rs
    generic.rs
    niri.rs
    plasma.rs
```

Do not create every backend before it is needed.

The architectural boundary is more important than the exact module structure.

The core crate must never depend on:

* `niri-ipc`
* KDE Frameworks
* Plasma-specific APIs
* compositor-specific environment variables

Environment-specific types must not leak into:

* filesystem models
* directory entries
* navigation state
* file-operation models
* filesystem job events

The application should detect available integrations in the application layer.

If specialized integration is unavailable, Floe must gracefully fall back to generic Wayland behavior.

---

# Integration Priority

When implementing desktop functionality, prefer this order:

1. Rust/Linux standard APIs
2. GIO / GLib
3. XDG / freedesktop standards
4. XDG Desktop Portals
5. environment-specific integration

Do not implement separate Niri and KDE code paths if a standard desktop mechanism solves the problem adequately.

Avoid spreading code like:

```text
if niri
else if plasma
else if sway
```

throughout the application.

Compositor/desktop differences belong in the integration layer.

---

# Niri Integration

Niri is a first-class environment but not a dependency of the core application.

Potential future Niri functionality includes:

* Niri IPC
* focused output awareness
* workspace awareness
* spatial window launching
* compositor-aware actions
* Niri-specific workflow conveniences

Niri integration must fail gracefully when:

* Niri is not running
* `$NIRI_SOCKET` is missing
* IPC is unavailable
* IPC communication fails
* protocol behavior changes

The file manager must remain fully usable without Niri integration.

---

# KDE Plasma Integration

KDE Plasma is a first-class supported desktop environment.

Prefer standard:

* XDG
* GIO
* GLib
* portal

APIs whenever possible.

Potential future Plasma-specific functionality may include:

* environment awareness
* Plasma compositor capabilities
* KDE service integration where genuinely useful
* KWallet when credentials eventually require secure storage
* Plasma-specific workspace/window enhancements
* integration with KDE desktop facilities where standards do not provide equivalent functionality

Do not introduce KDE Framework dependencies merely for cosmetic integration.

The application must continue functioning if Plasma-specific integration is unavailable.

---

# Generic Wayland Mode

Floe should remain usable under Wayland environments where no specialized integration exists.

Generic mode should provide normal functionality such as:

* browsing
* opening files
* filesystem operations
* navigation
* previews
* mounts where supported
* trash
* portals
* appearance
* keyboard shortcuts

Compositor-specific features may simply be unavailable.

Missing compositor integration must not be considered an application error.

---

# Visual Design Principles

Floe should support a modern floating-panel visual style.

Desired characteristics include:

* rounded surfaces
* floating sidebar
* floating content panels
* intentional gaps between major surfaces
* compact navigation controls
* optional transparency
* optional frosted/glass appearance
* subtle borders
* subtle shadows
* restrained animations
* configurable density
* strong dark-mode appearance

The UI should have visual breathing room instead of every surface touching every neighboring surface.

Conceptually:

```text
╭──────────────────────────────────────────────────────────╮
│  ‹  ›    Home / Projects                     Search  ⋯  │
│                                                          │
│ ╭──────────────╮  ╭────────────────────────────────────╮ │
│ │ Home         │  │                                    │ │
│ │ Downloads    │  │                                    │ │
│ │ Documents    │  │          Directory View            │ │
│ │ Pictures     │  │                                    │ │
│ │              │  │                                    │ │
│ │ Devices      │  │                                    │ │
│ ╰──────────────╯  ╰────────────────────────────────────╯ │
│                                                          │
╰──────────────────────────────────────────────────────────╯
```

Functionality is more important than visual effects.

---

# Appearance Presets

Long-term appearance presets should include:

## Native

Closer to normal system GTK appearance.

## Glass

Floating translucent surfaces.

## Frosted

More opaque than Glass and capable of using compositor blur where available.

## Minimal

Low-decoration, simpler appearance.

## Compact

Reduced spacing and denser controls for keyboard-heavy workflows.

These presets should share widgets and design tokens rather than being separate UI implementations.

---

# Appearance Architecture

Do not scatter arbitrary visual constants throughout the code.

Centralize concepts such as:

* panel radius
* window radius
* panel spacing
* surface opacity
* content opacity
* border strength
* shadow strength
* animation preference
* density
* floating-panels preference

GTK CSS may consume these values.

The application should eventually support custom themes without requiring widget rewrites.

Transparency must be optional.

Actual background blur is compositor-dependent.

Do not implement expensive fake blur simply to mimic another operating system.

Glass mode must remain readable when actual blur is unavailable.

---

# Filesystem Path Safety

Linux filenames are not guaranteed to be valid UTF-8.

Never design filesystem paths around `String`.

Use:

* `Path`
* `PathBuf`
* `OsStr`
* `OsString`

where appropriate.

Display strings may use lossy conversion when necessary, but the original path representation must remain preserved.

Never reconstruct a filesystem path from lossy UI text.

---

# Security Principles

Treat filenames, metadata, and paths as untrusted input.

Do not construct shell commands by interpolating filenames.

Prefer native APIs instead of shelling out.

Avoid unnecessary `unsafe`.

Do not implicitly follow symbolic links when doing so could cause unsafe or surprising behavior.

Future destructive actions must:

* be explicit
* report failures
* avoid silent partial corruption
* handle conflicts deliberately
* avoid silent overwrites
* support cancellation where meaningful

Never silently overwrite user data unless the operation explicitly permits it.

---

# Async / Performance Principles

Filesystem activity must never freeze GTK.

Potentially slow operations include:

* directory enumeration
* metadata collection
* copy
* move
* delete
* hashing
* thumbnail generation
* archive operations
* search
* network filesystem operations

Use worker threads/tasks appropriately.

UI updates must safely return to GTK/GLib.

Avoid spawning unbounded numbers of tasks.

Design for directories containing tens or hundreds of thousands of entries.

Do not eagerly calculate expensive metadata when it is not required.

Prefer staged or lazy metadata loading where useful.

---

# Directory Entry Model

A directory entry should eventually be capable of representing:

* original path
* display name
* file type
* MIME type
* size
* modification time
* hidden state
* symbolic-link information
* permissions
* thumbnail state
* mount/device information where relevant

Not every field should necessarily be populated eagerly.

Preserve original filesystem path data separately from display text.

---

# File Operation Architecture

Future filesystem mutations should be represented as operations/jobs rather than ad-hoc functions owned by widgets.

Conceptual operations:

```text
FileOperation
├── Copy
├── Move
├── Rename
├── Trash
├── Restore
├── Delete
├── CreateDirectory
├── Compress
└── Extract
```

Future long-running jobs should emit structured events such as:

```text
Started
Progress
Conflict
Paused
Resumed
Completed
Cancelled
Failed
```

Do not implement the complete framework prematurely.

However, new filesystem features must not make this architecture difficult to introduce.

---

# Job Manager

Long-running filesystem jobs should eventually be managed centrally.

The future job system should support concepts such as:

* operation ID
* progress
* cancellation
* pause/resume where meaningful
* failures
* retry
* conflict resolution
* operation history

The UI should observe jobs rather than own them.

---

# Navigation Model

Navigation state should be application state, not merely widget state.

It should eventually support:

* current path
* back history
* forward history
* parent navigation
* tabs
* split views
* Miller columns

Replacing or changing the directory-view widget should not destroy navigation history.

---

# Major UX Features

These are long-term goals.

Do NOT implement them automatically simply because they are described here.

---

# Grid View

Visual browsing with icons and thumbnails.

Should remain responsive for large directories.

---

# List View

Efficient, dense file browsing suitable for:

* keyboard workflows
* large directories
* detailed metadata

---

# Miller / Column View

This is intended to become one of Floe's signature features.

Example:

```text
Projects
   →
Alongsia
   →
assets
   →
characters
```

Directory levels may appear as horizontally arranged floating panels.

This interaction is inspired by Miller columns and should complement Niri's horizontal spatial philosophy while remaining equally useful under KDE Plasma and other Wayland desktops.

Do not duplicate unnecessary filesystem state for every visible column.

---

# Quick Preview

The user should eventually be able to select a file and press:

```text
Space
```

to open a lightweight preview.

Potential preview types:

* image
* video
* audio
* PDF
* text
* Markdown
* source code
* JSON
* fonts
* archive contents

Preview generation must not block the UI.

Unsafe or active content must never execute merely because the user previews a file.

---

# Inspector Panel

A toggleable floating details panel may eventually contain:

* preview
* file type
* size
* image/video dimensions
* timestamps
* MIME type
* permissions
* metadata
* tags if tags are implemented

Possible shortcut:

```text
Ctrl+I
```

---

# Command Palette

Floe should eventually provide a keyboard-first command palette.

Potential actions:

```text
Open terminal here
Copy path
Copy relative path
Rename
Move to
Calculate checksum
Compress
Extract
Create symlink
Properties
```

Commands should use human-readable searchable names.

Do not make users memorize internal command identifiers.

---

# Operations Island

Long-running filesystem jobs should eventually appear in a compact floating surface rather than blocking the whole application.

Concept:

```text
╭──────────────────────────────╮
│ Copying Photos        72%    │
│ ███████████████░░░           │
│ 540 MB/s      Pause     ×    │
╰──────────────────────────────╯
```

Avoid modal progress windows that unnecessarily prevent other file-manager activity.

---

# Keyboard Philosophy

Keyboard workflows are a major project goal.

Normal desktop shortcuts should work first.

Potential defaults:

```text
Alt+Left        Back
Alt+Right       Forward
Alt+Up          Parent directory
Ctrl+L          Location
Ctrl+F          Search
Ctrl+H          Hidden files
Ctrl+T          New tab
Ctrl+W          Close tab
Ctrl+C          Copy
Ctrl+X          Cut
Ctrl+V          Paste
F2              Rename
Delete          Trash
Shift+Delete    Permanent delete
Space           Quick Preview
Ctrl+I          Inspector
```

A future optional Vim-style navigation mode is desirable.

Do not force Vim controls on normal users.

Input architecture should allow future keymaps without rewriting file-view widgets.

---

# Linux / XDG Integration

Prefer Linux desktop standards over hardcoded assumptions.

Use appropriate standards/APIs for:

* XDG user directories
* MIME types
* default applications
* trash
* mounted devices
* removable devices
* portals
* notifications
* desktop theme information where appropriate

GIO should be strongly considered when it already provides correct desktop integration.

`ashpd` may be used for portal access where justified.

Do not duplicate standard desktop functionality without a concrete reason.

---

# File Watching

Floe should eventually detect filesystem changes made by external programs.

Potential implementation:

* `notify`

File watching must account for:

* rapid event bursts
* duplicate notifications
* deleted directories
* renamed files
* inaccessible paths
* large event storms

Do not rebuild the entire directory UI for every low-level filesystem event if more efficient reconciliation is possible.

---

# Error Handling

Errors must not silently disappear into logs.

For recoverable errors:

* present understandable UI feedback
* preserve useful technical context in logs
* keep the rest of the application usable

Prefer structured Rust error types where appropriate.

Avoid `.unwrap()` and `.expect()` in normal recoverable runtime paths.

They may be used when an invariant is truly guaranteed and the reason is obvious.

---

# Logging

Use structured tracing/logging rather than random `println!()` statements.

Logs should help diagnose:

* filesystem failures
* background jobs
* UI/backend communication
* desktop integration failures
* file-watcher issues

Do not log file contents.

Paths may contain sensitive information, so avoid excessively verbose path logging at normal log levels.

---

# Testing Philosophy

Core filesystem/application logic must be testable without GTK.

Prioritize tests for:

* path handling
* navigation state
* directory enumeration
* conflict logic
* copy/move semantics
* cancellation state
* non-UTF-8 paths where practical
* symlink behavior
* operation errors

Use temporary directories for filesystem tests.

Tests must never operate on real user data.

Avoid brittle tests tied closely to widget layout unless there is a strong reason.

---

# Code Quality Rules

Before implementing a feature:

1. Inspect existing code.
2. Read this file.
3. Understand the current architecture.
4. Determine which layer owns the feature.
5. Implement the smallest coherent version.
6. Add tests where meaningful.
7. Run formatting/build/lint/tests.
8. Do not refactor unrelated code.

Avoid:

* giant files
* giant structs
* giant GTK callbacks
* speculative abstractions
* unnecessary macros
* unnecessary unsafe
* hidden mutable global state
* environment checks spread throughout unrelated code
* architecture rewrites without explicit justification

Prefer clarity over cleverness.

---

# Dependency Rules

Before adding a crate, consider:

* Is it maintained?
* Does Rust's standard library already solve this?
* Does GTK/GIO already provide it?
* Is there an XDG/portal solution?
* Does the dependency introduce significant complexity?
* Is the dependency needed now?
* Is it only for a hypothetical future feature?

Do not add dependencies for roadmap features before they are actually required.

---

# Scope Discipline

Do not implement unrelated roadmap items simply because they appear easy.

When asked to implement one feature:

* inspect related architecture
* implement that feature
* make only necessary supporting changes
* avoid unrelated cleanup unless correctness requires it

Large unrelated refactors require clear justification.

---

# Build Quality Gate

Before ending a meaningful coding session, run the relevant available checks.

Prefer:

```bash
cargo fmt --check
cargo check --workspace
cargo clippy --workspace --all-targets
cargo test --workspace
```

If a command cannot run because the machine lacks GTK or other system development packages, state exactly what is missing.

Do not hide failures.

Do not claim verification that was not actually performed.

---

# Agent Session Procedure

Every Codex session must begin by:

1. Reading this file.
2. Inspecting the repository.
3. Reading the `Current Project Status` section below.
4. Understanding what already works.
5. Continuing from the recommended next task unless the user requests something different.

Every meaningful coding session must end by updating:

```text
Current Project Status
```

Do not erase long-term architectural instructions while updating status.

Keep project status concise and factual.

Do not mark a feature complete unless it actually works or has been verified.

---

# Roadmap

The rough roadmap is:

```text
Phase 0
Project architecture / bootstrap

Phase 1
Read-only directory browsing

Phase 2
Selection + basic file interaction

Phase 3
Filesystem operation/job engine

Phase 4
Copy / Move / Rename / Trash

Phase 5
Progress + cancellation + conflict handling

Phase 6
Grid/List polish + thumbnails

Phase 7
Tabs + split view

Phase 8
Miller / column navigation

Phase 9
Quick Preview

Phase 10
Inspector + properties

Phase 11
Command palette

Phase 12
Archive operations

Phase 13
Advanced search

Phase 14
Desktop integration framework

Phase 15
Niri integration

Phase 16
KDE Plasma integration

Phase 17
Remote/network filesystem support as justified

Phase 18
Packaging, distribution, performance tuning, polish
```

The order may evolve.

Architecture and correctness are more important than rigid phase numbering.

---

# Current Project Status

Last updated:

`2026-08-24`

Current phase:

```text
Phase 6 — List/grid polish and thumbnails (Phase 6F complete)
```

Status:

```text
Phases 0-4 and Phase 5D are complete. One application-owned transfer buffer
supports Ctrl+C copy and Ctrl+X move followed by Ctrl+V paste. F2 and the
file-actions menu open a validated rename dialog. Copy, move, and rename use
bounded workers plus generic non-blocking Operations Island feedback; GTK
callbacks perform no filesystem work. A separate bounded GIO trash executor
provides path-safe, cancellable job infrastructure. The explicitly labelled
“Move to Trash” menu action and Delete shortcut submit through application
state; permanent deletion remains unavailable.
Backend retry dispatch now covers copy, move, rename, and trash with stable
logical operation identity, fresh job attempts, and bounded terminal history.
Failed and cancelled jobs expose a persistent accessible Retry control in the
Operations Island; completed jobs remain non-retryable.
The virtualized file list has a native selection-aware context menu for Open,
Copy, Cut, Rename, and Move to Trash, with secondary-click and focused-list
keyboard access. Menu commands reuse existing application actions.
Open With asynchronously resolves GIO content type and compatible applications,
shows the current default, launches the explicit choice, and changes default
associations only through a separate user action.
Destination conflicts are distinct terminal outcomes. Generic Retry cannot
blindly repeat them; application state exposes keep-existing or validated
retry-with-name decisions for copy, move, and rename while preserving the
logical operation ID and never overwriting an existing destination.
Phase 5F presents those decisions in a focused non-blocking dialog with exact
request paths as context, inline filename validation, and no overwrite option.
Dismissal leaves an accessible Resolve Conflict action in the Operations
Island; revised attempts return to normal progress handling.

Phase 6A gives the existing virtualized list a compact Name, Type, Size, and
Modified hierarchy using metadata already captured by directory enumeration.
Formatting occurs only for bound visible rows, 256-entry model insertion batches
remain intact, keyboard focus is explicit, and original `PathBuf`/`OsString`
values remain authoritative. Thumbnails and a separate grid are not implemented.

Phase 6B makes all four visible metadata headings native keyboard/pointer
controls with explicit ascending/descending arrows and accessible pressed state.
The GTK-independent policy keeps navigable directories first, unknown optional
metadata last, and raw path values as deterministic tie-breakers. Sorting runs
on the bounded directory worker using shared entries; model rebuilds retain
256-entry batches and restore selection by exact original `PathBuf`.

Phase 6C adds lazy 32-pixel PNG/JPEG thumbnails to bound virtualized list rows.
A fixed-capacity single worker opens regular files with no-follow semantics,
rejects stale or oversized sources, applies explicit decoder limits, and returns
owned RGBA bytes for main-thread GTK textures. Exact path, size, and modification
time form cache identity. Pending, failed, unsupported, and disabled cases keep
generic icons; completed presentation state is capped at 256 entries. Phase 6C
initially kept this cache in memory only.

Phase 6D adds a native virtualized grid with seven discrete thumbnail sizes
from 64 through 192 pixels. List and grid share one `GioListStore`, one
`GtkSingleSelection`, exact `PathBuf` identity, activation, navigation, sorting,
and file actions. Only bound grid cells request requested-size thumbnails; the
edge is part of bounded cache identity. Ctrl+1/Ctrl+2 switch views and
Ctrl+-/Ctrl++ adjust size. View mode and grid size load at startup and save
atomically through a fixed-capacity application worker, never GTK file I/O.

Phase 6E adds freedesktop-compatible persistent thumbnail reuse with standard
`normal` and `large` tiers. Canonical absolute file URI MD5 names plus
`Thumb::URI`, `Thumb::MTime`, and `Thumb::Size` validation reject stale or
mismatched entries. Floe-owned PNGs also verify `Floe::MTimeNsec` for exact
same-second invalidation. Source/cache no-follow reads, bounded decoding, private
0700 directories, 0600 atomic PNG writes, and nonfatal fallbacks preserve the
existing safety contract. Separate Floe ownership markers allow one global
2,048-entry, 256-MiB, 90-day cleanup policy to remove only shared-cache entries
still carrying `Software=Floe`. All cache lookup, writes, and cleanup stay on
the capacity-64 thumbnail worker.

Phase 6F applies decoder-provided EXIF/TIFF orientation before scaling and
persistent-cache storage. The reviewed raster policy now accepts PNG, JPEG,
WebP, GIF, BMP, TIFF, and ICO case-insensitively; animated containers contribute
one still frame. SVG, AVIF/HEIF, and unreviewed formats retain generic icons.
The worker explicitly checks decoded dimensions and total bytes before
allocation while preserving encoded limits, no-follow opening, exact source
revalidation, capacity 64, aspect ratio, cache reuse, and GTK-free execution.
```

Established product decisions:

* The working name is Floe.
* Floe is a Wayland-first Linux file manager.
* Rust is the primary implementation language.
* GTK4 + gtk-rs are the primary UI technologies.
* libadwaita may be used where useful.
* Niri is a first-class environment.
* KDE Plasma is a first-class environment.
* Other Wayland desktops should receive generic compatibility.
* Core filesystem functionality must depend on neither Niri nor KDE.
* Standard XDG/GIO/portal mechanisms should be preferred before environment-specific implementations.
* Environment-specific functionality must live behind an integration boundary.
* Filesystem work must not block GTK.
* GTK widgets must not directly implement filesystem operations.
* Linux paths must not be treated as guaranteed UTF-8.
* The visual direction includes floating panels, rounded surfaces, optional transparency, optional glass/frosted appearance, and subtle animations.
* Visual effects must be optional and degrade gracefully.
* Miller/column navigation is intended to become a major differentiating feature.
* Strong keyboard navigation is a major goal.
* Quick Preview is planned.
* Inspector is planned.
* Command palette is planned.
* A floating operations island is planned.
* Niri IPC integration is planned for a later phase.
* KDE Plasma-specific integration is planned for a later phase.

Currently working:

```text
Phase 6F is complete. The next coherent branch is
`phase-6g-iconography-polish`, replacing the weak generic folder/file
presentation with a cohesive, accessible Floe icon system across list and grid.
```

Verified:

```text
`cargo fmt --all -- --check`, `cargo check --workspace`, strict Clippy, and all
129 tests pass: thirty-three core and ninety-six application tests. Five focused
Phase 6F tests cover reviewed mixed-case format policy, WebP/GIF/BMP/TIFF/ICO
decoding, malformed input, aspect preservation, real JPEG EXIF orientation
before scaling/cache storage, and added-format cache reuse across worker
restarts. Eleven focused Phase 6E tests cover canonical non-UTF-8 URI/digest
identity, tier mapping,
required metadata and invalidation, corrupt/oversized/symlink rejection,
same-second subsecond invalidation, private atomic writes, global Floe-only
age/count/byte cleanup, nonfatal cache
failure, and persistent worker reuse. Eight focused
Phase 6D tests cover strict view modes, bounded zoom steps, persisted values,
stable action names, requested-edge cache identity, invalid edge rejection,
nonblocking queue capacity, latest shutdown submission, and atomic persistence.
Eight focused Phase 6C tests cover PNG/JPEG eligibility and decoding, bounded scaling,
exact non-UTF-8 identity, metadata invalidation, symlink-replacement no-follow
safety, source limits, stale generations, non-blocking queue capacity, pending
deduplication, and the 256-entry cache bound.
Eight focused Phase 6B tests cover direction cycling, directories-first ordering,
unknown metadata, raw non-UTF-8 identity, worker dispatch, visible arrows, and
exact-path selection restoration. Four focused Phase 6A tests cover stable column
semantics, text-only kind distinctions,
size-unit boundaries through exabytes, and signed modified-time formatting.
Four focused Phase 5F tests cover conflict-action priority, dismissal/reopen,
single-dialog state, exact filename validation, keep-existing and fresh retry
submission, retry fallback, and the absence of overwrite/apply-to-all decisions.
Six focused Phase 5E tests cover distinct conflict outcome, blind-retry
rejection, raw-path identity, validation, revised copy/move/rename attempts,
keep-existing, single-use resolution, trash rejection, no overwrite, and
history-eviction cleanup.
Three focused Phase 5D tests cover eligible file kinds, default-first
deduplicated application ordering, and chooser/default button sensitivity.
Three focused Phase 5C tests cover the complete action mapping, rejection of an
unbound virtualized row, and lock-state-safe keyboard shortcuts. Two focused
Phase 5B tests cover retryable outcomes and preservation of pending retry state
while another job completes. A native Wayland Phase 6F smoke used isolated
temporary home/cache/config roots and a real 96x24 WebP fixture. The live app
owned the expected D-Bus name, generated the corresponding freedesktop PNG and
Floe ownership marker, remained healthy, exited with status 0 through its Quit
action, released the name, and left no temporary artifacts. It emitted only the
known host RADV/Vulkan suboptimal-swapchain warnings. A two-run native Wayland
Phase 6E smoke used
temporary home/cache/config roots, created a private standard cache entry on
the first run, reused the same thumbnail inode and modification time on the
second while refreshing only its ownership marker, owned the expected D-Bus
name, remained healthy, and released the name after intentional shutdown. It
emitted only the known host RADV/Vulkan suboptimal-swapchain warning.
A native Wayland Phase 6D build registered the
expected D-Bus application owner, switched live List/Grid modes, persisted
112/128/160-pixel steps, loaded bound-cell thumbnails, preserved selection
across grid-factory rebinding, and remained healthy until stopped. It emitted
only the documented host libadwaita, RADV, and Vulkan suboptimal-swapchain
warnings. Ten focused Phase 4E
tests cover original
non-UTF-8 paths, GIO error mapping, success, cancellation, structured failures,
capacity, shutdown, state tracking, and recovery feedback without touching real
user Trash. Phase 4F has one focused interaction-wording test. Four focused
Phase 5A tests cover all operation kinds, stable/new identities, cancelled
retry, non-UTF-8 paths, completed rejection, bounded history, and safe registry
eviction. Formatting, strict Clippy, and native smoke status are in `GATES.md`.
```

Known issues:

```text
No application correctness failures are known. The copy interaction uses a
Floe-internal clipboard only; it does not interoperate with other applications.
Copy still does not preserve timestamps, ownership, ACLs, extended attributes,
sparse extents, or reflink state. Move/rename is currently same-filesystem only;
cross-filesystem copy-delete recovery is not implemented. Native smoke runs may emit host
GtkSettings/libadwaita and Vulkan suboptimal-swapchain warnings; neither
originates from Floe logic.
GIO trash cancellation is cooperative and cannot reverse a move after the
desktop service commits it. Floe does not yet expose trash restore/browsing UI.
Phase 6F does not thumbnail SVG, AVIF/HEIF, RAW camera formats, or animation
beyond the first still frame. Cache interoperability remains intentionally
limited to the freedesktop normal/large tiers needed by Floe's current 32-192
pixel requests. Generic folder/file icons are visually weak and are the next
explicit product-quality target.
```

Deferred:

```text
Cross-application clipboard support, overwrite and apply-to-all conflict policy,
cross-filesystem moves, trash restore/bulk UI, permanent delete,
metadata-complete copies, job
persistence/history UI, drag and drop, heavyweight/RAW/vector thumbnails,
tabs, split view, Miller columns, previews, archives, search, device
management, Niri IPC, KDE-specific APIs, and network filesystems.
```

Completed this session:

```text
* Added `DESIGN.md` as the implemented/planned visual and interaction source of
  truth, including the actual appearance preset values.
* Added `docs/ARCHITECTURE.md` for current crate/module responsibilities, data
  flow, desktop/job boundaries, and recorded architectural debt.
* Added `docs/DEVELOPMENT.md` with verified Rust/GTK/libadwaita requirements,
  Arch/CachyOS packages, commands, environment notes, and troubleshooting.
* Added `docs/ROADMAP.md` with Phases 0-2 completed, Phase 3 next, and later
  milestones clearly separated.
* Updated the root README with links to the persistent documentation set.
* Added GTK-independent operation/job IDs, validated progress, structured
  failures, lifecycle commands/states/events, and strict legal transitions.
* Added an application-owned job registry/event boundary with retry attempts
  preserving logical operation identity while receiving new job IDs.
* Kept the Phase 3 foundation non-mutating: no operation model, executor, or GTK
  mutation action was added.
* Added Phase 4A path-safe `CopyRequest`, `ConflictPolicy::FailIfExists`, and
  preserve-or-reject `SymlinkPolicy` types in `floe-core`.
* Added recursive file/directory copy with preflight validation, chunk-level
  cancellation, tracked cleanup, non-UTF-8 preservation, and structured errors.
* Added a fixed-capacity, single-worker application copy executor connected to
  job progress, completion, cancellation, failure, retry identity, and shutdown.
* Kept Phase 4A copy execution backend-only until its safety contract was
  verified.
* Added an application-owned, original-path `CopyBuffer` and exact-destination
  paste submission through shared `ApplicationState`.
* Added internal Ctrl+C/Ctrl+V actions without filesystem work in GTK callbacks.
* Added a separate non-blocking `OperationController` and compact Operations
  Island for progress, cancellation, terminal status, recovery feedback, and
  destination refresh.
* Kept overwrite, cross-application clipboard formats, move, rename, trash,
  and permanent delete deferred.
* Added GTK-independent exact-path `MoveRequest` and same-parent
  `RenameRequest` models with raw `PathBuf`/`OsString` preservation.
* Added atomic Linux `RENAME_NOREPLACE` execution for same-filesystem files,
  directories, and symlinks; conflicts never overwrite existing targets.
* Added explicit invalid-name, missing-source, self-nesting, cancellation, and
  cross-filesystem failure semantics.
* Added a fixed-capacity move/rename worker connected to application job
  lifecycle events, structured failures, cancellation, and clean shutdown.
* Kept GTK move/rename actions, overwrite, cross-filesystem fallback, trash,
  and permanent delete deferred.
* Replaced the copy-only staging model with one original-path transfer buffer
  supporting Ctrl+C copy and Ctrl+X move followed by Ctrl+V paste.
* Added F2 and visible-menu rename interaction with focused inline validation
  while preserving the selected source path separately from editable text.
* Generalized application request tracking, cancellation, Operations Island
  feedback, and affected-directory refresh across copy, move, and rename.
* Kept overwrite, cross-filesystem copy-delete fallback, trash, and permanent
  delete deferred.
* Added an application-layer original-path `TrashRequest` and fixed-capacity
  `TrashExecutor` backed by GIO/XDG trash behavior and `gio::Cancellable`.
* Added structured trash lifecycle, failure mapping, capacity, cancellation,
  shutdown, generic operation tracking, and affected-parent refresh metadata.
* Added injected-backend tests that never modify the user's real Trash.
* Kept Delete shortcuts, restore/bulk-trash UI, and permanent deletion deferred.
* Added a selection-sensitive “Move to Trash” menu action and conventional
  Delete shortcut, both routed through `ApplicationState` with original paths.
* Added explicit trash progress/completion/recovery wording and affected-parent
  refresh after completion without a modal confirmation dialog.
* Kept Shift+Delete, permanent deletion, restore/bulk UI, and undo deferred.
* Added backend retry submission for copy, move, rename, and trash using the
  preserved request, stable `OperationId`, and a fresh `JobId` per attempt.
* Added 64-entry terminal operation history and terminal-only job-record
  eviction while preserving all active records.
* Added an accessible Retry control for failed and cancelled Operations Island
  terminal states, routed through `ApplicationState::retry_operation`.
* Prevented duplicate retry submissions and kept completed jobs non-retryable.
* Added a native list-row context menu with Open, Copy, Cut, Rename, and Move to
  Trash, all routed through existing selection-sensitive `win.*` actions.
* Added exact pointer-target row selection plus Shift+F10/Menu-key access scoped
  to the focused list.
* Added asynchronous GIO Open With discovery, default-first compatible app
  ordering, explicit launching, and explicit Set as Default behavior.
* Kept original paths intact and added no shell-command execution.
* Added distinct destination-conflict terminal outcomes and blocked blind
  generic Retry for conflicts.
* Added original-path pending conflict data with stable operation identity.
* Added single-use keep-existing and validated retry-with-name decisions for
  copy, move, and rename; revised attempts remain fail-if-exists.
* Added focused non-blocking conflict dialog with source/destination context,
  empty-by-default retry name, inline accessible validation, and native focus.
* Added ordered pending-conflict interaction state and a persistent Operations
  Island Resolve Conflict action after dismissal.
* Routed Keep Existing and Retry with New Name through `ApplicationState` only;
  GTK callbacks perform no filesystem work.
* Added no overwrite, apply-to-all, or trash-conflict path.
* Added compact aligned Name, Type, Size, and Modified list columns using
  metadata already owned by `DirectoryEntry`.
* Kept metadata formatting inside virtualized row binding and retained bounded
  256-entry GTK model insertion batches.
* Added textual folder/file/link/special kinds, exabyte-safe decimal sizes,
  locale-aware modified times, stable tabular figures, and visible keyboard
  focus styling.
* Kept original `PathBuf`/`OsString` identity authoritative and deferred
  thumbnail generation and a separate grid.
* Added GTK-independent Name, Type, Size, and Modified sorting in both
  directions, with directories first and unknown metadata last.
* Added native operable headings with visible arrows, tooltips, accessible
  labels, and pressed state without relying on color alone.
* Ran comparisons on the bounded directory worker using shared entries while
  retaining virtualized 256-entry main-loop insertion batches.
* Preserved selection across reordered models by exact original `PathBuf`,
  including colliding lossy non-UTF-8 display names.
* Added exact-path, size, and modification-sensitive thumbnail keys for regular
  PNG/JPEG files, with no-follow opening and explicit encoded/decoded limits.
* Added a fixed-capacity single thumbnail worker with stale-generation skipping,
  non-blocking queue-full behavior, clean shutdown, and owned RGBA responses.
* Added bound-row-only list thumbnail requests, unbind-safe weak presentation
  bindings, stable generic fallbacks, main-thread `GdkMemoryTexture` creation,
  and a 256-entry in-memory presentation cache.
* Added focused Phase 6C coverage for non-UTF-8 paths, metadata invalidation,
  symlink replacement, PNG/JPEG decoding/scaling, limits, stale generations,
  queue capacity, deduplication, and cache bounds.
* Pinned the pure-Rust `image 0.25.9` decoder with only PNG/JPEG features to
  preserve Floe's declared Rust 1.85 minimum toolchain.
* Added GTK-independent List/Grid policy with seven bounded 64-192 pixel sizes
  and stable view/zoom action names.
* Added native virtualized `GtkGridView` and `GtkListView` presentations sharing
  one `GioListStore`, `GtkSingleSelection`, activation, navigation, and actions.
* Added bound-cell-only requested-size thumbnails with edge-sensitive bounded
  cache identity and stable generic fallbacks.
* Added accessible List/Grid toggles, zoom buttons, scale, keyboard shortcuts,
  focus-visible grid styling, and two-line tooltip-backed labels.
* Added startup loading and fixed-capacity, nonblocking, atomic persistence for
  view mode and grid size outside GTK callbacks.
* Added focused Phase 6D coverage for modes, zoom bounds, persisted values,
  action mapping, thumbnail-size identity, invalid sizes, queue capacity, and
  preference persistence.
* Added canonical non-UTF-8-safe file URI and MD5 cache identity using standard
  freedesktop `normal`/`large` tier paths and required source metadata.
* Added no-follow bounded cache reads, 8-bit RGBA PNG metadata validation, and
  private same-directory atomic 0600 writes under 0700 cache directories.
* Added separate Floe ownership markers and global age/count/byte cleanup that
  verifies `Software=Floe` before pruning any shared thumbnail.
* Integrated persistent lookup/write/cleanup into the existing capacity-64
  thumbnail worker with source revalidation and nonfatal cache fallbacks.
* Added focused Phase 6E coverage for URI/tier identity, metadata invalidation,
  corrupt/oversized/symlink rejection, permissions/atomicity, ownership-safe
  cleanup, cache-root failure, and reuse across worker restarts.
* Expanded the deliberate thumbnail policy to WebP, GIF, BMP, TIFF, and ICO
  alongside PNG/JPEG while continuing to reject SVG and unreviewed formats.
* Applied decoder-provided EXIF/TIFF orientation before aspect-preserving tier
  scaling and persistent cache storage.
* Kept added formats inside existing source/decoded/dimension limits, exact
  source revalidation, capacity-64 worker, stale-generation, and GTK-free
  response boundaries.
* Added focused Phase 6F coverage for mixed-case format policy, all five added
  decoders, malformed input, real JPEG orientation, oriented cache pixels,
  aspect ratio, and added-format cache reuse across worker restarts.
```

Recommended next task:

```text
Create `phase-6g-iconography-polish` and replace the weak generic folder/file
presentation with a cohesive Floe icon system. Audit theme versus app-owned
vector assets, then refine optical sizing, hierarchy, MIME-family distinction,
alignment, selected/focused contrast, and list/grid consistency while retaining
textual file kinds and exact paths as authoritative.
```

---

# Status Update Template

At the end of future coding sessions, update the status section using approximately this structure:

```text
Last updated:
YYYY-MM-DD

Current phase:
...

Completed this session:
- ...

Currently working:
- ...

Verified:
- ...

Important decisions:
- ...

Known issues:
- ...

Deferred:
- ...

Recommended next task:
...
```

Do not turn this section into a long historical changelog.

Git history should provide detailed history.

This section exists to answer:

* Where are we?
* What works?
* What is unfinished?
* What should the next Codex session do?

---

# Final Instruction to Coding Agents

Preserve what already works.

Read before rewriting.

Keep GTK responsive.

Keep filesystem logic out of widgets.

Treat Linux paths correctly.

Treat user data carefully.

Prefer desktop standards over environment-specific hacks.

Keep Niri integration optional.

Keep KDE integration optional.

Keep environment-specific APIs out of the filesystem core.

Prefer small, testable changes.

Do not prematurely implement the entire roadmap.

At the end of every meaningful session, leave this file updated so the next Codex session knows exactly where the project currently stands.
