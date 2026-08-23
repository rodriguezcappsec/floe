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

`2026-08-23`

Current phase:

```text
Phase 4 — Filesystem operations (Phase 4A copy engine complete)
```

Status:

```text
Phases 0-3 are complete. Phase 4A's backend-only copy slice is implemented:
path-safe requests, explicit conflict/symlink policies, recursive copy,
cooperative cancellation, cleanup, a fixed-capacity application executor, and
job lifecycle integration. No GTK copy/paste control exists yet. The current
design, architecture, development workflow, and roadmap are documented in
`DESIGN.md` and `docs/`.
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
Phase 4A copy execution is complete. Phase 4B should add application commands
and non-blocking GTK job observation for copy/paste, progress, cancellation,
and conflict feedback without placing filesystem work in widgets.
```

Verified:

```text
`cargo fmt --all -- --check`, `cargo check --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`, and
`cargo test --workspace` pass. Thirty tests pass: twenty core tests and ten
application tests. Nine focused core copy tests and six executor tests
cover byte-preserving recursion, symlinks, conflicts, self-copy rejection,
non-UTF-8 names, cancellation/cleanup, capacity, lifecycle failure mapping,
retry identity, and shutdown. A native Niri/Wayland smoke launch emitted
`Floe application started`; the window remained healthy until timeout.
```

Known issues:

```text
No application correctness failures are known. The copy backend has no GTK
observer or mutation controls yet and does not preserve timestamps, ownership,
ACLs, extended attributes, sparse extents, or reflink state. Native smoke runs
emit host GtkSettings/libadwaita and Vulkan suboptimal-swapchain warnings;
neither originates from Floe logic.
```

Deferred:

```text
GTK copy/paste controls, overwrite/conflict resolution, move, rename, trash,
permanent delete, metadata-complete copies, job persistence/history UI, drag
and drop, thumbnails, tabs, split view, Miller columns, previews, archives,
search, device management, Niri IPC, KDE-specific APIs, and network filesystems.
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
* Kept copy execution backend-only; GTK mutation controls remain deferred.
```

Recommended next task:

```text
Begin Phase 4B with a copy-only GTK interaction slice: application-owned
copy/paste commands, non-blocking job event observation, and compact progress,
cancellation, conflict, and failure feedback. Use the existing executor; do not
place filesystem work in callbacks or introduce overwrite, move, rename, trash,
or permanent delete until the copy interaction is verified.
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
