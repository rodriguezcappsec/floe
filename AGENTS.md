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

# Duplicate Finder

Phase 13G should expose “Check for Duplicates…” for explicitly selected files or
roots. Candidate discovery must be local, bounded, cancellable, and application
owned: group by exact size, stream a reviewed Phase 10E hash, then confirm equal
content byte-for-byte before calling files identical. Revalidate identities when
files change during scanning.

Hard-link aliases must be distinguished from independent copies and symbolic
links must not be followed by default. Results should support review and reveal
before reusing ordinary explicit Trash or delete actions. Never delete a copy
automatically, count hard-link aliases as reclaimable bytes, upload hashes or
content, or treat a digest match alone as proof.

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

## Permanent testing layers

Floe uses distinct testing layers. Do not call one layer by another name and do
not use a higher-cost graphical layer when a deterministic lower layer proves
the same invariant more clearly.

### Rust unit tests

Use Rust's built-in `#[test]` framework and keep deterministic tests close to
the owning code. This is the default layer for path handling, navigation,
sorting/filtering, conflict naming, metadata and selection policy, job state
transitions, command policy, parsers/codecs, preferences, operation decisions,
and security-sensitive validation.

### Filesystem integration tests

Exercise real filesystem behavior only below a `tempfile` root. This includes
create, copy, move, rename, Trash/restore through an injected test Trash,
delete, duplicate, links, permissions, conflicts, cancellation/recovery,
non-UTF-8 names, and source/destination races. Tests must never inspect or
modify the real HOME, Trash, XDG config/data/cache/state, user folders, mounts,
or user data. Cross-device tests require deliberate disposable test filesystems;
never borrow a user's mounted volume merely because it is available.

### Property-based tests

`proptest` is a `floe-core` dev dependency. Use it selectively when generated
inputs explore a materially larger state space: raw Linux filename bytes,
non-UTF-8 identity, sort/set invariants, navigation history, conflict names,
parser round trips, archive/path boundaries, state transitions, or bounded
collections. Keep ordinary examples as deterministic tests. A property failure
must retain/report the reproducible proptest seed or regression case.

Useful properties include deterministic ordering, multiset preservation, no
panic on arbitrary filenames, no lossy display-to-path reconstruction, no path
escape, no silent overwrite, defined cancellation states, and bounded resource
use.

### GTK component and accessibility tests

GTK tests belong in `floe-app` and test presentation/application behavior, not
filesystem implementation. Prefer small real-widget contracts for controls,
dialogs, sidebar/header state, tabs, breadcrumbs/location entry, list/grid,
Operations Island, action sensitivity, keyboard wiring, roles, states, labels,
and descriptions. Graphical tests are explicitly ignored by the ordinary
headless suite and run with:

```bash
cargo test -p floe-app phase_testing_gtk -- --ignored --nocapture
```

They require a real disposable GTK display. Do not move filesystem work into
GTK or add hidden testing-only widgets, test IDs, or fragile CSS selectors.

Accessibility metadata is both a user requirement and automation contract.
Interactive controls require stable meaningful roles, names/labels,
descriptions where useful, states, and relations. Prefer semantic nodes and
actions over widget position or screen coordinates.

### Native GTK end-to-end tests

`e2e/` is the opt-in Dogtail/AT-SPI layer. It launches the actual `floe-app`
executable and interacts through native accessibility/actions and keyboard
workflows. It is not part of `cargo test`, is not `proptest`, and must never be
implemented with Playwright, Selenium, browser DOM, Tauri, or other web-testing
tools.

Every E2E launch must use temporary private HOME and XDG config/cache/data/state
and runtime roots plus an isolated freedesktop Trash. Tests must use bounded
condition waits on accessibility, process, application, or filesystem state;
fixed sleeps and coordinate clicks are not acceptable synchronization.

The initial scenario registry is:

1. E2E-01 launch, responsiveness, clean quit.
2. E2E-02 child navigation, Back, Forward, Parent.
3. E2E-03 create and rename with filesystem verification.
4. E2E-04 copy and move with source/destination verification.
5. E2E-05 current implemented search/filter workflow.
6. E2E-06 Trash and restore using only isolated test Trash.
7. E2E-07 multi-selection and a batch operation.
8. E2E-08 implemented keyboard workflows including Ctrl+L, Ctrl+F, Ctrl+T,
   Ctrl+W, Alt+Left, Alt+Right, Alt+Up, and F2.

Run harness contract/preflight with:

```bash
python3 -m unittest discover -s e2e -p 'test_*.py' -v
```

Dogtail/AT-SPI and an appropriate graphical session are external system
dependencies. If unavailable, report the skipped native layer and exact reason;
never claim those workflows ran merely because discovery or Rust tests passed.

### Wayland and compositor gates

Keep these separate:

1. compositor-independent native GTK E2E in an isolated Mutter/GNOME Wayland
   environment where AT-SPI and semantic input are supported;
2. Niri native smoke for launch, actions/D-Bus, liveness, clean quit, and only
   the user-input behavior the compositor permits;
3. KDE Plasma Wayland smoke for the same generic GTK/GIO contract plus any
   future explicitly isolated Plasma integration.

The full E2E suite must not depend on Niri input injection or one compositor.
Existing native Wayland/Niri smoke evidence must be preserved unless a concrete
replacement is verified.

### Security and privacy testing

Every future encryption, password, vault, Sensitive Folder, Private Mode,
restricted-preview, or integrity feature requires dedicated correct-round-trip,
wrong-password, corrupted/truncated/tampered input, authentication failure,
interruption, temporary cleanup, atomic replacement, permissions, symlink/path
traversal, and sensitive-log tests as applicable. Verify source data is not
destroyed before successful commit and encrypted Floe files remain compatible
after moving. Never invent custom cryptography to simplify a test.

### Regression test policy and future phase requirement

Every reproduced bug fix must add a regression test at the lowest layer that
would have caught it. If no automated test is practical, record why and the
exact native/manual gate used.

For every roadmap leaf, future agents must:

1. identify the applicable testing layers before coding;
2. add or update tests during implementation, including failure and edge cases;
3. use isolated temporary filesystem roots and add regression coverage for bugs;
4. run focused tests while developing;
5. run `cargo fmt --all -- --check`, `cargo check --workspace`,
   `cargo clippy --workspace --all-targets -- -D warnings`, and
   `cargo test --workspace` before completion;
6. run applicable GTK, E2E, Niri, Plasma, or native-smoke gates when behavior is
   user-visible or environment-specific;
7. report exact commands, results, skips, and environment limitations.

Deterministic Rust gates belong in ordinary CI when CI is present. Graphical
GTK/E2E/compositor gates remain separate opt-in jobs with explicit environments;
do not make ordinary CI fail merely because it has no compositor or display.

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

`docs/ROADMAP.md` is the authoritative bounded sequence. The synopsis below is
historical orientation only and is superseded whenever the roadmap differs.

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
Privacy, security, and data integrity

Phase 19
Extensibility and developer features

Phase 20
Settings, visual, accessibility, and quality-of-life audit

Phase 21
Performance, packaging, and release hardening
```

The order may evolve.

Architecture and correctness are more important than rigid phase numbering.

Use persistent planning documents by responsibility:

* `docs/ROADMAP.md` owns phase sequencing and marks exactly one next phase.
* `docs/FEATURE_MATRIX.md` is the exhaustive capability/status ledger. Code and
  tests, not prose alone, determine `COMPLETE`.
* `docs/PRIVACY_SECURITY.md` owns the threat model, security architecture, and
  prohibited claims.
* `DESIGN.md` owns visual and interaction language.
* `PLAN.md` and `GATES.md` belong to one active implementation phase. Do not
  repurpose them as the long-term roadmap or erase completed evidence.

Future sessions read the current roadmap entry and matching matrix/security
requirements, implement only that bounded phase, verify it, update persistent
status, name exactly one next phase, and stop.

Security terms are non-interchangeable. Use **Encrypted Vault** only for real
encrypted storage, **Sensitive Folder** for reduced Floe-owned traces,
**Protected Folder** for accidental-change guardrails, **Private Mode** for
history/cache minimization, **Open Safely** only while a real restriction policy
is active, and **Integrity verified** only after verification completes.

---

# Current Project Status

## Active status

Last updated: `2026-08-30`

Current phase: **USB device discovery maintenance fix (complete); Phase 6V remains complete**

Completed this session:

- Fixed removable volumes disappearing as blank-looking rows when GIO exposes
  an empty or whitespace-only `GVolume::name()`. Snapshot presentation now uses
  a bounded deterministic filesystem-label, associated-drive/partition, or
  generic fallback while preserving the original opaque `DeviceId`, live GIO
  object, exact local root, deduplication, and device-action routing.
- Added regression coverage for real-name priority, empty/whitespace/control
  input, sibling partition distinction, bounded output, and ultimate fallbacks.
  Focused and full verification evidence is recorded in
  `gates/fix-usb-device-discovery.md`. The accompanying logical edge review was
  read-only; its unrelated findings were not implemented. Phase 6W — Undo Trash
  remains the sole recommended next phase.

- Added typed exact completion-result paths for successful local Copy, Move,
  Rename, Create, Duplicate, and Replace. Result reveal batches are capped at
  4,096 exact `PathBuf` values and bound to one job/batch, visible directory,
  and browser generation; failed, partial, stale, inactive, hidden, filtered,
  missing, and collapsed results cannot select a substitute or navigate away.
- The active List, grouped Grid/Search/Trash, and Miller view now selects and
  scrolls successful results without moving keyboard focus. Batch results
  accumulate into one selection and receive a 1.8-second view-level emphasis
  that clears safely without leaking through virtualized row/tile reuse.
- Selected list rows, grid/search/trash tiles, and Miller entries now include
  redundant border and label-weight cues beyond color while retaining GTK's
  authoritative multi-selection and native accessibility semantics.
- Focused exact-path, operation-result, presentation, and real-GTK tests pass,
  as do full workspace tests. Phase 6W — Undo Trash is the sole recommended
  next phase and is not implemented here.

- Added safe Replace for local Copy, Move, and Rename conflicts. Floe captures
  exact no-follow incoming and existing identities, stages on the destination
  filesystem inside owner-only capacity-bounded `.floe-replace-backups`,
  revalidates before Linux atomic exchange, rolls back before commit, and
  classifies rollback/post-commit uncertainty as partial instead of silently
  overwriting or cleaning it.
- Added durable replacement Undo/Redo with both version identities, restart
  review, and identity-owned backup cleanup on expiry or explicit resolution.
  Changed or insecure backup occupants are retained and marked review-required.
- Conflict UI now compares bounded incoming/existing metadata, exposes distinct
  destructive Replace and batch-only Replace All actions, and requires a second
  confirmation explaining retained contents, capacity, identity checks, and
  batch scope. Replace All captures fresh identities for every later compatible
  conflict, does not leak to another batch, and pauses at Protected Folder
  intersections for fresh review.
- Deterministic Phase 6U engine, batch, recovery, and UI contract tests pass,
  including cancellation, collision, changed-identity, restart, expiry, and
  cross-batch isolation. Phase 6V builds on this verified completion routing.

- Added a separate private durable Undo/Redo store at the XDG state path for
  completed local Copy, Move, Rename, and Create work. It preserves exact raw
  paths and no-follow identities, is capped at 256 records/4 MiB, uses `0700`
  parent and atomic `0600` files, expires completed history after 30 days, and
  keeps interrupted/uncertain action states for explicit review.
- Added a capacity-four asynchronous Undo/Redo worker. Move/Rename use exact
  identity-checked no-overwrite inverse moves; Copy/Create Undo uses ordinary
  recoverable GIO Trash after identity revalidation, with non-empty created
  directories refused. Redo republishes without overwrite. Failures restore
  the prior action state when no mutation occurred; partial or uncertain work
  remains reviewable and is never cleaned automatically.
- Operation History now exposes durable Applied/Undone records, expiry, Undo,
  and Redo. Recovery Center includes interrupted/uncertain Undo/Redo records.
  Administrator operation dialogs explicitly state durable Undo is unavailable:
  current GVfs completion events do not provide enough exact post-operation
  fingerprint/Trash metadata to prove a safe fresh-authorized inverse.
- Focused Phase 18Y2 store/local/administrator/UI tests and full workspace tests passed before Phase 6U. That durable-history foundation now supports replacement Undo/Redo and the later Phase 6V reveal path.

- Replaced the 128px/34px tab presentation with 72px-minimum adaptive tabs,
  18-character centered single-line end ellipsis, 24px label-strip height,
  flatter nested targets, hover/focus 20px separately labelled close controls,
  and one underline-only active marker without an active fill.
  Full path tooltips, stable-ID actions, keyboard, middle-click, context menu,
  drag/drop, and horizontal overflow remain unchanged.
- Replaced the blue active-pane frame with a quiet one-pixel neutral boundary.
  Existing active-pane text and focus semantics remain. Device rows retain the
  name as the primary line and concise free space as the secondary line; both
  are single-line, end-ellipsized, and expose complete text in tooltips when the
  sidebar is narrow.
- Focused deterministic and real-GTK tests, full workspace format/check/strict
  Clippy/tests, strict docs/render/package/E2E contracts, and isolated native
  Wayland CSS/Ping/Quit pass. No multi-window or session architecture work was
  included.

Prior administrator-operations evidence:

- Added a separately typed capacity-one GIO/GVfs administrator mutation
  service for New Folder, Rename, single-file Copy/Move, Trash, permanent
  delete, and Unix-mode changes. Floe remains UID-stable and never receives
  credentials or strips administrator identities into ordinary local jobs.
- Added exact no-follow fingerprint revalidation, no-overwrite flags, explicit
  accessible confirmation, structured cancellation/partial-output evidence,
  and active-mutation close blocking. Recursive copy, ownership, ACL/xattr,
  and cross-restart privileged recovery remain unavailable.
- Focused policy/service/UI tests, real-GTK accessibility, workspace format,
  check, strict Clippy and tests, documentation/package/release/E2E contracts,
  and isolated KDE Wayland Ping/Quit pass. The native process remained UID
  1000. A disposable root-owned mutation fixture was unavailable and is not
  claimed.

Prior release-candidate evidence:

- Updated `tar` from 0.4.44 to 0.4.46, resolving the three current medium
  Dependabot advisories. Added a deterministic offline policy over 204 resolved
  packages, ten allowed SPDX license identifiers, registry/path sources, and
  the recorded patched advisory floors.
- Added a reproducible candidate gate. Two 245-file source archives built with
  the frozen epoch were byte-identical and received a verified SHA-256 manifest.
- Re-ran seven fail-closed interrupted-operation recovery tests covering private
  atomic storage, corrupt/symlink/insecure stores, stale prior-process records,
  changed sources, uncertain output, retry, and explicit reset.
- Added and installed `docs/RELEASE_MATRIX.md`. An isolated-session-bus release
  binary with private HOME/XDG roots answered D-Bus `Peer.Ping`, exported Quit,
  and exited 0 under KDE Plasma Wayland.
- Release documentation, package layout/migrations/source, E2E harness, format,
  workspace check, strict all-target/all-feature Clippy, frozen release build,
  and workspace tests pass.

Important decisions:

- The release audit is not a future crypto/vault/sandbox hostile-input audit.
- Niri and semantic Dogtail/pyatspi were unavailable on this Phase 21D host and
  are not claimed. Generic GTK/GIO behavior remains compositor-independent.
- Floe currently has one GApplication process and one window; proper multi-window
  support requires shared application services rather than unsafe independent
  processes racing settings and recovery stores.

Recommended next task: create `phase-6w-undo-trash` and implement only durable,
standards-correct Undo for Floe-owned local Trash operations. Reuse Phase
6N/6P/18Y2 exact metadata and recovery foundations; require no-overwrite
restore, identity revalidation, bounded expiry/restart review, and explicit
conflicts. Keep unsupported Trash backends, permanent deletion, and
administrator Trash outside the claim unless complete reversible evidence is
available.

## Prior active status (Phase 21C)

Last updated: `2026-08-29`

Current phase: **Phase 21C — Release Documentation (complete); focused Phase 14B administrator-mount regression fixed and user-facing feature rationale added**

Completed this session:

- Added `docs/PHILOSOPHY.md` as an installed, rendered, strictly validated
  user-facing contract. It connects Floe's ownership, exact-identity,
  responsiveness, narrow-authority, local-first, standards, spatial, visual,
  and honest-security principles to concrete feature behavior and tradeoffs.
  README, Getting Started, User Guide, Administration, and the administrator
  view link or repeat the relevant rationale.
- Fixed fresh-session `admin://` browsing: the provider now handles the first
  `G_IO_ERROR_NOT_MOUNTED` with one cancellable, time-bounded,
  window-parented GIO mount/authorization request and retries enumeration once.
  Denial, cancellation, timeout, stale generations, and repeated `NotMounted`
  results fail closed; the view remains read-only and UID-stable.
- Added reciprocal release manuals and reconciled installation, migration,
  privacy, architecture, roadmap, and feature-matrix claims.
- Added strict documentation/link/table/terminology/render checks, installed
  public manuals with their relative-link layout, and added fresh-user release
  walkthrough contracts.
- Hardened deterministic source exclusions, updated the Arch checksum, and
  verified a real clean package build plus its staged native lifecycle.

Verified:

- Focused and complete Phase 14B tests, the real-GTK administrator-view
  accessibility contract, formatting, workspace check, strict Clippy, workspace
  tests, E2E preflight, strict docs/render, deterministic release-source hash,
  and diff hygiene pass. KDE exposes GVfs and an active polkit agent; successful
  administrator enumeration remains a manual password-authenticated gate.
- Strict documentation checks pass all 20 release-facing files and two focused
  checker regressions; render, layout, migration, deterministic-source, and
  diff-hygiene gates pass.
- Formatting, workspace check, strict Clippy, 554 app tests (14 graphical
  ignores), 21 app integration tests, 162 core tests, six duplicate workflows,
  and the frozen release build pass.
- `makepkg --cleanbuild --nodeps` passes. The staged binary answered D-Bus Ping,
  exported and accepted Quit, and exited 0.
- Two walkthrough contracts pass. Dogtail and pyatspi are unavailable, so the
  semantic installed-artifact walkthrough remains truthfully skipped.

Important decisions:

- Floe remains English-only with partial RTL foundations.
- Logs and technical details may contain sensitive paths and require review and
  redaction before sharing.
- Flatpak, compositor-specific features, remote/MTP, cryptography, vaults,
  sandboxing, Open Safely, and Secure Share remain unavailable or deferred.

Known issues:

- Native semantic Dogtail/AT-SPI walkthrough evidence is unavailable because
  Python Dogtail and pyatspi are not installed. The host AT-SPI socket refuses
  connections; staged native lifecycle otherwise passed with the known RADV
  warning only.

Recommended next task: create `phase-21d-release-candidate` and perform only
the bounded dependency, license, advisory, security, recovery,
environment-matrix, and reproducible release-candidate audit in the roadmap.

## Prior active status (Phase 21B)

Last updated: `2026-08-29`

Current phase: **Phase 21B — Packaging and migrations (complete)**

Completed this session:

- Froze stable `io.github.rodriguezcappsec.Floe` application identity,
  installed `floe` command, validated desktop/AppStream metadata, reviewed
  512×512 hicolor icon, manifest-driven source installation, and Arch package.
- Hardened version-18 preference migration with bounded no-follow reads,
  private atomic writes, supported legacy/corrupt backups, and explicit
  future/symlink/oversize refusal. Package operations never set MIME defaults
  or migrate user XDG state.
- Verified metadata, layout, migration, frozen release, real `makepkg`, and
  staged native directory/Ping/Quit lifecycle gates. Flatpak, clean-chroot,
  `namcap`, Dogtail, and pyatspi remain unavailable and are not claimed.

- Added one opt-in release-only, serial, tempfile-isolated performance harness
  covering real 100,000-entry enumeration, metadata sort, quick filter and
  filename search plus bounded thumbnail, content-search, copy/checksum,
  duplicate, integrity, and advanced-metadata workloads.
- Recorded the host, toolchain, exact fixture sizes, elapsed budgets, baseline
  and current measurements, and Linux `VmHWM` peak-memory procedure in
  `docs/PERFORMANCE.md`. The recorded run peaked at 119,876 KiB.
- Replaced two full ASCII metadata text passes with one pass. The 16,652,288-byte
  CPU workload improved from 64,108 us to 32,212 us (49.8%) with deterministic
  parity coverage for ASCII controls/whitespace, line endings, empty text, and
  Unicode fallback.

Verified:

- Release performance harness, formatting, workspace check, strict
  all-target/all-feature Clippy, 549 app tests (14 graphical ignores), 21 app
  integration tests, 162 core tests, six duplicate workflows, native build,
  11 focused real-GTK contracts in separate processes, E2E preflight, and diff
  hygiene pass.
- Disposable native 100k Wayland app answered D-Bus Ping, quit with exit 0,
  and peaked at 114,296 KiB. Critical-free logging is explicitly unavailable:
  the host AT-SPI socket refused connections even with a private address.

Known limitations:

- Recorded filesystem timings use host `/tmp` tmpfs and warm page cache; they
  are not cold-disk promises. Dogtail/pyatspi remains unavailable, so no native
  semantic E2E claim is made.

Recommended next task: create `phase-21c-release-documentation` and implement
only Phase 21C user, administrator, security, accessibility, recovery, and
debugging documentation with a fresh-user and consistency audit.

## Prior active status (Phase 20B2)

Last updated: `2026-08-29`

Current phase: **Phase 20B2 — Visual, Accessibility, and QoL Completeness Audit (complete)**

Completed this session:

- Corrected grouped Grid View's presentation: group headings now span the
  content width above virtualized group bodies instead of occupying the first
  file tile. Bounded selection-slice models delegate to Floe's one authoritative
  `GtkMultiSelection`, so exact paths, Ctrl-selection, activation, context
  actions, collapse persistence, click policy, and grid sizing stay coherent.
- Date and size grouping now have deterministic list/grid boundaries and
  focusable, expanded-state-aware collapsible buttons with bounded private
  persistence. Invert Selection is available from Ctrl+Shift+I, command
  registry, and the background context menu.
- Split ratios retain typed per-tab persistence. One version-18 preference
  controls list/grid/search/Miller single versus double click while Enter stays
  immediate. Complete column order and bounded content autosize persist
  globally, per folder, and through session codec v3.
- Settings add libadwaita System/Light/Dark, validated font-family name,
  75–200% scale, reduced motion, and reset. Escape closes the innermost browser
  surface and restores active-view focus; panes/groups have redundant non-color
  cues.
- Each failure toast carries its own bounded memory-only details target.
  Inactive-window completion notifications use generic path-free text. New
  feedback strings use a translation-ready message-ID boundary and displayed
  paths use Unicode first-strong isolation without changing exact identity.
- Focused selection-slice and grouping tests plus the real-GTK spanning-section
  regression pass. An isolated `view=grid`, `grouping=size` Wayland launch
  answers D-Bus Ping and quits cleanly without GTK/libadwaita criticals. Full
  workspace gate totals are recorded in `GATES.md`.

Known limitations: Python Dogtail/pyatspi is unavailable, so no native semantic
E2E/Orca claim is made. Translation catalogs and a physical multi-monitor
fractional-scale matrix remain Phase 21 work. Wayland position and compositor
workspace placement remain deliberately unpersisted.

Recommended next task: create `phase-21a-performance` and establish reproducible
100k-directory, thumbnail, search, operation, metadata, and integrity baselines
before optimizing.

## Prior active status (superseded)

Last updated: `2026-08-28`

Current phase: **Phase 20B1A — Advanced Metadata Sort Index (complete)**

Completed this session:

- All formerly disabled Document/Image/Audio/Video/Other Sort By rows are real
  persisted criteria backed by an explicit current-local-folder scan.
- One bounded cancellable application worker reuses Phase 10F image/EXIF/audio,
  bounded UTF-8 text, optional timed exact-argv `ffprobe`, and no-follow Unix/
  link facts. GTK performs no filesystem work and exact paths stay authoritative.
- A private versioned 32,768-entry/32-MiB fingerprint cache uses atomic `0600`
  persistence below `0700`, watcher invalidation, corruption/insecure-storage
  rejection, Settings reuse toggle and clearing. Preferences migrate to v16.
- Focused/core/app tests, strict Clippy, serial workspace suite, two focused
  real-GTK component gates, E2E preflight and native Wayland Ping/Quit pass.

Known limitations: unsupported providers/formats remain unknown-last; PDF/office
word counts are not faked. Optional `ffprobe` runs with normal user authority and
is not claimed sandboxed. Dogtail/pyatspi remains unavailable on this host.

Recommended next task: create `phase-20b2-completeness-audit` and continue the
bounded visual, accessibility, localization, scaling, and daily-driver QoL audit.

## Earlier active status (superseded)

Last updated:

`2026-08-28`

Current phase:

```text
Phase 20B1 — Sort By Completeness (complete)
```

Completed this session:

- Added a dedicated native Sort By menu beside the view controls and under
  Browser View with real radio/check state for criteria, direction, folders
  first, and hidden files last.
- Added worker-owned Created, Accessed, Rating, Tags, and Comment ordering;
  KDE-compatible user metadata is explicit-demand, no-follow, cancellable,
  bounded, memory-only, and unknown-last.
- Persisted hidden-last and new sort criteria through preference version 15 and
  session codec version 2 with backward migration.
- Added a Phosphor sort glyph, regressions, README/User Guide, architecture,
  design, roadmap, matrix, and privacy/security documentation.

Currently working:

- None; Phase 20B1 is complete and verified.

Important decisions:

- Advanced Document/Image/Audio/Video metadata fields are visible but disabled
  until a bounded metadata index can sort them truthfully.
- Sorting never reconstructs exact paths from display text and GTK performs no
  filesystem work.

Known issues:

- Running all ignored GTK tests in one process can still abort before `gtk_init`
  because of existing cross-test initialization order. The applicable focused
  real-GTK component test passes independently. Native Dogtail/pyatspi remains
  unavailable on this host.

Deferred:

- Natural-name, MIME, owner/permission, dimensions/duration/audio sorting and
  rating/tag/comment editing remain separate roadmap work.

Recommended next task:

```text
Create `phase-20b2-completeness-audit` and continue the bounded visual,
accessibility, localization, scaling, and daily-driver QoL audit.
```

## Prior phase status (archived)

Last updated:

`2026-08-28`

Current phase:

```text
Phase 14B — Privileged Local Browsing (complete, experimental read-only)
```

Completed this session:

* Added private validated administrator resource identity from exact absolute
  local paths through GIO/GLib URI APIs; arbitrary `admin://`, remote,
  credential-bearing, query/fragment, relative, and round-trip-drift input fails.
* Added application-owned async GIO/GVfs no-follow enumeration with 128-entry
  pages, 4,096-entry cap, 120-second authorization/30-second page deadlines,
  cancellables, typed failures, and generation rejection on the GLib context.
* Added version-14 opt-in Settings control, folder/background/Command Palette
  action, and a virtualized read-only Administrator view with accessible
  persistent authority, folder history, Cancel/Retry/Return controls.
* Kept privileged identity outside every local mutation, Preview/thumbnail,
  terminal, launcher/Open With, archive, custom action, clipboard, and plugin
  route. Native smoke found and fixed a RefCell activation crash; failed child
  navigation now restores the exact prior view and rejects stale responses.
* Expanded the bounded shortcut registry/list capacities from 96 to 128 after
  the new commands crossed the old integration-test ceiling.
* Updated README, User Guide, privileged-access/security/architecture guidance,
  roadmap, matrix, plan, gates, and project status.

Verified:

* Formatting, workspace check, strict all-target/all-feature Clippy, workspace
  tests, native build, diff hygiene, focused Phase 14B tests, and the real-GTK
  accessibility/component rollback gate pass.
* The application binary has 535 tests: 526 passed and nine intentional
  graphical ignores; 21 app integration, 150 core, and six duplicate-workflow
  tests pass. E2E harness checks report three passed and the native Dogtail suite
  is skipped because Python Dogtail/pyatspi are unavailable.
* Isolated native Wayland/D-Bus Open/Return/Ping/Quit passes. Real, effective,
  saved, and filesystem UIDs remained 1000 before and after activation. Only
  known external RADV and unavailable AT-SPI warnings remained.

Important decisions:

* Privileged browsing remains experimental, opt-in, read-only, application-layer
  GIO/GVfs access. It never elevates the GTK process or receives credentials.
* Privileged resources remain unavailable to previews, terminals, launchers,
  archives, custom actions, plugins, clipboard, and local `PathBuf` jobs.

Known issues:

* Full Dogtail/AT-SPI native workflows could not run in this environment because
  their Python bindings are unavailable; the real-GTK component gate did run.

Deferred:

* Niri, KDE-specific, Android/MTP, remote browsing, general plugin runtime, and
  general scripting remain deferred according to `docs/ROADMAP.md`.

Recommended next task:

Create `phase-20b-completeness-audit` and implement only the measured visual,
accessibility, and daily-driver quality-of-life audit in `docs/ROADMAP.md`.
Preserve the experimental administrator guard until separate Niri, Plasma,
root-owned-fixture, and repeated-lifecycle stable-release gates are available.

---

# Archived Project Status (through Phase 7G)

Last updated:

`2026-08-28`

Current phase:

```text
Phase 7G — Navigation Upgrades (complete)
```

Previous Phase 20A added one searchable native Settings Center with eight plain-language
sections, `Ctrl+,`, case-insensitive multi-term search, and a clear no-results
state. Appearance, icon style, default view, per-folder view memory, Vim
navigation, grid size, file/sidebar density, and the optional private filename
index apply live through existing BrowserController and `ViewPreferences`
boundaries. Keyboard Shortcuts, Context Menu Contents, terminal selection,
operation history, Recovery Center, Protected Folders, command palette, Preview
cache clearing, and desktop integration reuse their existing actions rather
than duplicate detailed editors. System accessibility and reduced-motion rows
describe GTK/desktop ownership without claiming the broader Phase 20B audit.
Irreversible confirmations remain fixed.

Focused Phase 20A tests, formatting, workspace check, strict all-target/all-
feature Clippy, 500 application tests (493 passed, seven intentional graphical
ignores), 21 application integration tests, 147 core tests, six duplicate
workflow tests, native build, clean real-GTK Settings component gate, E2E
harness preflight, and diff hygiene pass. Native Dogtail workflows remain
skipped because Python Dogtail and pyatspi/AT-SPI are unavailable.

The sole recommended next phase is **19B — Associations and Custom Actions** on
`phase-19b-associations-custom-actions`: complete inspect/set/clear XDG MIME
defaults and safe argv-based user actions without shell interpolation.

Phase 7G replaces the passive header path with exact raw-path breadcrumb
segments in a responsive horizontal surface, keeps Trash a truthful virtual
label, and exposes `Alt+Down` Recent Locations over the existing bounded
current/back/forward session history. `Ctrl+L` now offers local-folder
completion from a capacity-one superseding worker capped at 4,096 scanned
entries and 64 results. Suggestions retain exact `PathBuf` identity separately
from lossy text and are never persisted.

GApplication now accepts exactly one local command-line target. A folder
navigates directly; a regular file routes through a bounded validation worker
to its exact parent and existing reveal pipeline. Missing, inaccessible,
oversized, unsupported, relative, multiple, and non-local targets receive
deterministic truthful feedback. No filesystem probing runs in GTK callbacks,
no second history store was added, and existing Private/Sensitive workspace
suppression remains authoritative.

Focused core/application/raw non-UTF-8, completion, recent-session, and CLI
tests pass. The real-GTK header accessibility component gate passes. Full
workspace formatting, check, strict all-target/all-feature Clippy, tests, native
build, E2E preflight, native CLI smoke, and diff hygiene are completion gates
and must be recorded in `GATES.md` before this phase is published.

Previous Phase 18Y adds a private bounded XDG-state journal written by copy, move,
rename, and create workers before filesystem mutation. Successful or safely
absent failed outputs clear their records. Corrupt, symlinked, insecure, or
unavailable storage leaves browsing usable but blocks journaled mutations until
the user explicitly resets only the journal. Startup Operation Recovery lists
current exact-path presence and offers Reveal, record-only resolution, and
conservative retry for prior-process copy/move/rename only when source is
present and destination absent; uncertain output is never deleted.

Completed Create Undo now captures the no-follow destination identity and uses
ordinary recoverable Trash. Identity changes, replacement, or a non-empty
created directory fail before the GIO Trash backend runs. Replace/Replace All
and Undo Trash remain unavailable until safe backup/rollback and reversible
standards semantics exist.

Deterministic Phase 18Y tests, complete `floe-app` tests, formatting, workspace
check, strict Clippy, workspace tests, native build, E2E preflight, and diff
hygiene are the required completion evidence. The graphical GTK component gate
must be recorded as skipped when no usable display exists; never claim it ran.
That Phase 18Y checkpoint handed off to Phase 20A, which is now complete as
recorded above.

## Historical checkpoints (non-authoritative)

The material below preserves earlier session evidence. Its old “next phase”
statements are historical only; `docs/ROADMAP.md` and the current status above
are authoritative.

post-Phase-18X header-options follow-up reorganizes the three-dot Main menu
through task-based progressive disclosure: Create, Open & Inspect, File
Operations, View & Layout, Tools & Safety, then a compact utility section.
File Operations is split into Transfer, Rename & Duplicate, Links, Copy
Details, and Trash. Every prior `win.*` action remains present exactly once,
keyboard shortcuts and live GAction sensitivity are unchanged, selection-aware
context menus retain their separate customization model, and the Main menu
button has explicit accessible name and description. Phase 18Y remains the sole
recommended next phase.

Phase 13G3 follow-up accelerates cold and warm exact-duplicate scans without
changing their proof boundary. Same-size identities receive at most 64 KiB from
the first and last file regions as a non-authoritative quick filter; remaining
candidates use the reviewed SHA-256 path with at most four workers and two
active reads per filesystem device, followed by unchanged byte-for-byte
confirmation. A versioned private cache under the XDG cache root stores only
exact raw path, dev/inode/size/mtime/ctime, SHA-256 digest, and bounded recency,
with 200,000-entry/64-MiB limits, exact-fingerprint reuse, watcher/subtree
invalidation, pre/post-hash mutation rejection, corrupt/insecure/symlink
rejection, and atomic `0600` persistence below a `0700` directory. Duplicate
groups, review choices, and deletion choices remain memory-only; hashes and
content are not uploaded. This cache is not subject to future Private Mode or
Sensitive Folder policy yet, which is documented explicitly. Native progress
names discovery, quick filtering, hashing, cache reuse, and byte confirmation
without fake percentages. Formatting, workspace check, strict all-target/all-
feature Clippy, 480 application tests (475 passed and five intentional GTK
ignores), 21 application integration tests, 147 core unit tests, six duplicate
workflow integration tests, native build, diff hygiene, and the focused real GTK
setup contract pass. Historical checkpoint: Phase 18Y was then the recommended next phase.

Phase 13G2 follow-up matures duplicate discovery without changing roadmap
sequencing. **Check for Duplicates…** now opens a native setup window with
three exact-byte workflows: scan one chosen local folder and all subfolders,
find copies of one selected regular file within a chosen folder tree, or scan
the explicit selected files/folders. Contextual defaults cover no selection,
one file, one folder, and multiple supported items. The core reference mode
retains only the selected file's size class and result group, preserves exact
raw paths, and avoids double-counting when the reference already lies below the
chosen root. Existing Phase 13G same-device/no-follow traversal, reviewed
SHA-256, byte confirmation, mutation checks, cancellation, hard-link accounting,
memory-only review, Reveal, and explicit recoverable Trash remain intact. Exact
duplicates are not visually similar media. Formatting, workspace check, strict
all-target/all-feature Clippy, 474 app tests, 21 Phase 18X integration tests,
146 core tests, four new duplicate workflow integration tests, native build,
diff hygiene, and the focused real GTK setup-window gate pass. Phase 18Y remains
the sole recommended next phase.

Phases 18T–18X add saved SHA-256 fingerprints, strict path-safe portable
`SHA256SUMS`, explicit local integrity baselines, optional Copy and Verify,
verified removable-device transfer, and Protected Folder guardrails. Work is
bounded and cancellable off GTK; exact `PathBuf`/`OsString` identity is
preserved; no shell or silent overwrite path was added. Hashes are not
authenticity or safety, monitoring is not intrusion detection, copied output
can remain explicitly unverified, and “safe to remove” appears only after
successful revalidated flush and GIO eject/unmount. Protected Folders prevent
mistakes; they are not encryption or access control. All destructive backend
dispatches require fresh exact-scope generation-bound single-use permits, and
corrupt policy storage fails closed. Real USB validation remains skipped
without disposable lab media.

Historical checkpoint: Phase 18Y was then the recommended next phase. Niri,
Plasma-specific, remote, and Android/MTP integrations remain user-deferred.

A post-Phase-18X Operations Island regression is fixed: retryable integrity
failures now retain their explicit Retry action while still receiving a fresh
three-second auto-hide deadline. A retained retry can no longer suppress the
hide deadline of later operation results; unresolved destination conflicts
remain visible because they still require an explicit decision. Focused
failure-then-success policy coverage, formatting, workspace check, strict
all-target/all-feature Clippy, 467 application tests, 21 Phase 18X integration
tests, 146 core tests, native build, and the real GTK Operations Island
component gate pass.

`docs/USER_GUIDE.md` is the living current-feature manual linked from the
README. It explains navigation, views, selection, operations, search, preview,
archives, devices, customization, integrity tooling, guardrails, shortcuts,
troubleshooting, and truthful limitations without replacing the planned Phase
21C release-documentation audit.

The historical implementation evidence below remains valid unless a newer
entry above explicitly supersedes its sequencing text.

A verified post-Phase-14 appearance correction vendors 44 pinned Phosphor Core
2.1.1 Regular symbolic SVGs for Floe-owned interface chrome with bundled MIT
attribution and no runtime download. File and folder entries expose live,
persistent Floe Color, Phosphor Monochrome, and System Theme choices through
version-12 preferences. Switching rebinds already-loaded virtualized entries;
thumbnails, exact paths, MIME policy, and filesystem architecture are unchanged.
Semantic color tokens remain backed by shape, filename/type text, and accessible
labels. A shared toast-markup escape regression prevents user-visible labels
containing `&`, `<`, or `>` from producing GTK warnings. Strict workspace tests,
real GTK, and two-launch isolated Plasma Wayland action/state/persistence/clean-
Quit gates pass. Phase 15 Niri integration remains the sole recommended next
phase.

A follow-up file-type icon regression separates plain text from office
documents and PDF, gives all fifteen semantic entry families a distinct
app-owned fallback, and prevents System Theme fallback chains from collapsing
unavailable MIME artwork onto one generic glyph. Live style changes clear stale
`GtkImage` storage before rebinding. Focused policy/resource tests, the full
workspace suite, and real GTK resolved-paintable checks pass; Phase 15 remains
the sole recommended next phase.

A second follow-up regression corrects synthetic execute-bit handling on exFAT
and similar mounts. Known extensions now retain their semantic file icon even
when metadata reports `0755`; executable presentation is the fallback only for
unknown or extensionless executable files such as AppImages and ELF binaries.
This changes presentation only, performs no content/MIME/filesystem probe, and
does not grant or revoke execution permission. Focused regression, 397 app
tests, 132 core tests, strict Clippy, real GTK, and native build pass.

Phase 14 adds an app-only generic `DesktopIntegration` capability boundary for
GIO launch, mounts/volumes, XDG user folders, portals, notifications, Share
availability, GTK/libadwaita theme signals, Secret Service presence, and
reliable session-lock signals. One capacity-one generation-safe worker performs
bounded session-bus ownership probes and returns path/content-free memory-only
snapshots. A native refreshable Desktop Integration command/dialog reports
Available, Limited, or Unavailable with explicit text reasons. Missing optional
services preserve local browsing and existing GIO/device/location/appearance
behavior. No compositor branch, desktop type in core, secret read,
notification/share transmission, or lock control was added. Strict workspace,
real GTK, and isolated Wayland lifecycle gates pass. Phase 15 Niri integration
is the sole recommended next phase.

Phase 13G adds explicit Check for Duplicates over selected local files/folders.
The GTK-independent engine preserves exact raw paths, groups by exact size,
hashes only candidate identities through the existing reviewed Phase 10E
SHA-256 implementation, then confirms independent copies byte-for-byte before
calling them equal. No-follow same-device traversal, root/file/directory/depth/
group/result/byte bounds, cancellation, identity revalidation, collision tests,
and hard-link alias/reclaimable accounting are verified. One capacity-one
application worker retains progress/results in memory only. Default file-tools
context and command discovery expose the action; a native progress/review window
defaults every Trash checkbox off, labels aliases, reveals exact paths, and
hands only explicitly checked paths to ordinary recoverable Trash. Strict
workspace, real GTK, and isolated Wayland action/liveness gates pass. No index
dependency, remote/Trash roots, persistence, upload, automatic/permanent
deletion, or secure-erasure claim was added. Phase 14 generic desktop integration
is the sole recommended next phase.

Phase 13F adds one explicit optional filename/metadata index for one local root.
The GTK-independent build/codec/query boundary preserves exact raw paths,
excludes hidden trees and contents, never follows links or crosses the root
device, and enforces entry/directory/depth/path/64-MiB encoded bounds. One
application worker writes a versioned mode-`0600` atomic cache and validates
directory plus matching-entry fingerprints before presentation. Version-11
preferences persist only the off-by-default Use Index boolean. A compact native
Index menu exposes truthful capability text, Build/Rebuild, and Clear. Missing,
stale, corrupt, ineligible, or unavailable indexes automatically replay the
unchanged request through complete live Search Files; Search Contents never uses
the index. Strict workspace, real GTK, and isolated Wayland cache/action/liveness
gates pass. No content, hidden/sensitive override, remote/global, background
watcher rebuild, or Phase 18 privacy claim was added.

Phase 13E adds a validated GTK-independent saved-query catalog and session
recent-search model. At most 64 explicitly named definitions persist through
the current version-12 private preferences on the existing capacity-one worker; raw roots,
Search Files/Search Contents kind, scope, Text/Glob/Regex, hidden inclusion,
and every advanced predicate round-trip exactly. Invalid, corrupt, duplicate,
or over-capacity records are skipped independently. At most 32 recent executed
queries remain deduplicated and memory-only with visible Clear Recent and a
tested suppression boundary. Native controls save, list, exact-root replay,
delete, and clear without reconstructing paths from labels. Dedicated results
can order by Name, Modified newest, or Size largest while content matches for
one file retain line order. Group headings, indexing, tags, remote/global roots,
Sensitive Folder, Private Mode, and duplicate finding remain excluded. Strict
workspace, real GTK, and isolated Wayland lifecycle evidence is in `GATES.md`.

Phase 13D adds explicit Search Contents to the unified search surface. One
capacity-one application worker submits typed requests to a GTK-independent
engine that searches only local no-follow regular files under This Folder or
Include Subfolders without crossing the root device. Phase 13C predicates run
before bounded reads. UTF-8 and BOM-declared UTF-16LE/BE, Text/Glob/Regex, Match
Case, 64-result batches, 50,000-result/100,000-entry/depth-128/16-MiB-file/512-MiB
total limits, cancellation, post-read identity revalidation, and truthful
binary/encoding/change/limit counters are verified. Results preserve exact
entry identity and visibly show filename, line-number/snippet, and containing
folder while reusing ordinary Open/Open With/Properties/Reveal actions.
Queries, paths, snippets, results, and counters remain memory-only; no Trash,
remote roots, symlink targets, mount crossing, document/archive extraction,
helpers, uploads, saved history, or index was added. Strict workspace gates,
real GTK accessibility, and isolated native Wayland lifecycle are recorded in
`GATES.md`. Phase 13E saved searches is the sole recommended next phase.

Phase 13C adds one structured bounded advanced-filter model shared by Quick
Filter and Search Files. Entry type, extension, MIME, size, modification date,
Unix owner, hidden policy, and explicit Match Case combine with Text/Glob/Regex
filename matching; an empty filename is valid only with a real predicate.
Cheap facts run first. Capacity-one application workers resolve only requested
lazy facts: owner uses no-follow metadata and GIO guesses MIME from the filename
without content bytes. Unknown requested metadata excludes the candidate and
recursive feedback counts it. Quick Filter can temporarily include or isolate
hidden loaded entries without changing global Show Hidden. Exact paths, queries,
results, and counters remain memory-only. Accessible wrapping native controls,
raw-name/case/validation tests, strict workspace gates, real GTK accessibility,
and isolated native Wayland D-Bus/lifecycle smoke are recorded in `GATES.md`.
Tags, persistence/history, indexing, remote roots, and search-result ordering
remain excluded from Phase 13C; Phase 13D followed this work.

Phase 13B adds case-insensitive filename-only search rooted at the active local
folder with explicit This Folder and Include Subfolders scopes. One capacity-1
application worker streams batches of at most 128 exact `PathBuf` results while
generation cancellation rejects stale work. Traversal uses no-follow metadata,
does not descend symbolic links or cross the root filesystem device, and caps
results at 100,000, entries at 1,000,000, directories at 100,000, and depth at
128. Skipped, stopped, truncated, empty, and failed states remain explicit.

Quick Filter and Search Files now share one native surface: `Ctrl+F` opens Quick
Filter, `Ctrl+Shift+F` opens Search Files, and a visible mode selector explains
the distinction. The dedicated result list shows filename and containing folder,
reuses exact-path selection and ordinary file actions, and adds Reveal in Folder.
Queries, roots, results, counters, and use remain memory-only. File contents,
remote roots, history, persistence, indexing, advanced predicates, and
search-result ordering remain excluded. The supplied `icon_floe.png` is embedded
under the stable application ID and used as the GTK application/window icon.

Focused core/application/UI/icon tests and strict workspace gates are recorded
in `GATES.md`. Phase 13C advanced filters followed this work.

Phase 13A adds a current-folder-only Text/Glob/Regex filter over entries already
returned by the directory worker. Queries are capped at 256 Unicode scalar
values; valid names use Unicode semantics and non-UTF-8 Linux names retain raw
identity with documented ASCII-insensitive text and raw-byte pattern fallback.
One capacity-1 application worker compiles each pattern once and a one-result
latest-generation mailbox prevents stale replacement without blocking GTK.
List, grid, and Miller share the filtered backing entries; exact selection is
restored only for still-visible paths, refresh/sort/hidden/watcher changes
reapply the query, and location changes clear it. Header/Ctrl+F discovery,
visible modes, count/error alert, Escape/close, 100,000-entry tests, strict
workspace gates, and native Wayland lifecycle are verified. Query, mode,
results, and usage are memory-only; recursion, content/metadata reads, indexing,
history, and persistence remain excluded. Phase 13B filename search followed
this work.

A testing-foundation pass on `testing-foundation` preserves the Phase 13B next
recommendation while formalizing permanent test layers. The baseline 469 Rust
tests remain intact; four selective `floe-core` proptest invariants cover raw
filename identity, deterministic sort/set preservation, non-UTF-8 filter
behavior, and navigation history. One opt-in real-widget GTK/accessibility
contract covers header, filter, and Operations Island controls. `e2e/` registers
eight isolated Dogtail/AT-SPI workflows with temporary HOME/XDG/Trash roots.
Harness isolation/discovery is verified; native E2E execution is not claimed on
the current host because Dogtail and `pyatspi` are unavailable.

A post-13A keyboard correction removes bare Space from application-wide GTK
accelerators and handles it only in list, grid, and Miller file views. The
filter `SearchEntry` and every other editable control therefore receive Space
directly, while custom non-typing Quick Preview accelerators remain application
scoped. Focused regression, Phase 13A, workspace check, strict Clippy, and
native verification gates are recorded in `GATES.md`.

A post-13A filter discoverability correction gives Text, Glob, and Regex popup
rows concise plain-language summaries plus mode-specific hover and accessible
descriptions. Glob explains `*` and `?` with filename examples and Regex is
identified as advanced; the selected collapsed control exposes the same help.

Phase 12F exposes selection-aware Extract Here, Extract To, and Compress in a
default Archives submenu shared by list, grid, and Miller context surfaces.
Seven reviewed optional groups have deterministic order/defaults and a native
keyboard-accessible editor; fixed Open/edit/Trash/Delete/Properties/customize
actions cannot be hidden. Only stable group IDs persist in preferences through
the existing worker. Archive, batch rename, and customization commands
are now registered for header, palette, and shortcut discovery; live GAction
eligibility remains authoritative. Arbitrary commands, plugins, per-MIME rules,
and later privacy actions remain excluded. Phase 13A followed this work.

Post-12E split-pane maintenance preserves the focused active `GtkPaned` child
when opening or closing the other pane. Real side swaps hand focus to a stable
header control before reparenting and restore it to the active file view
afterward.

A post-12F appearance correction makes Glass and Frosted use real top-level
Wayland alpha composition instead of blending only against Floe's opaque
libadwaita background. The application removes the default `background` class
only for those two presets, then applies distinct semantic-color opacity to the
window, header, panels, and list/grid/Miller views. Native, Minimal, and Compact
remain opaque. Focused CSS tests and native dark/bright-backdrop captures verify
that wallpaper pixels affect Glass while text, selection, and focus treatment
remain visible. Floe still makes no generic compositor-blur claim.

A post-12F appearance chooser exposes all five presets as a radio-style
Appearance submenu in the three-dot header menu. Selection updates the shared
CSS provider immediately and persists one stable preset ID through the bounded
preference worker; version-9 migration defaults legacy or invalid values to
Frosted. `FLOE_APPEARANCE` remains a non-mutating launch override. Broader
light/dark/system, token editing, font, and reset controls remain Phase 20.
Native Wayland smoke verified live Frosted-to-Glass action state, a clean
two-launch Glass restore, D-Bus health and clean Quit, plus a Frosted environment
override over stored Glass without rewriting the stored preference.

Status:

```text
Phase 12E separates duplicate operations from hardened templates and adds explicit relative/absolute symbolic-link planning. Relative targets are calculated lexically from exact absolute source/destination paths without canonicalization or following; absolute targets store the exact source path, and native guidance states either can become broken. Hard links preflight regular no-follow source and destination-parent device, preserve no-overwrite semantics, and revalidate linked inode identity after commit. Duplicate naming preserves extensions/non-UTF-8 bytes and advances existing `(copy)`/`(copy N)` suffixes within the 10,000-attempt bound. All work reuses the bounded create executor and shared Operations UI. Phase 12F action integration is the sole next phase.

Phase 12D replaces arbitrary template picking with one bounded application worker that examines at most 4,096 direct XDG Templates entries and returns at most 256 no-follow regular files in stable raw-byte order. The native chooser exposes loaded, empty, truncated, and error states plus exact in-app Templates-folder management. Exact source paths feed the existing create executor; core creation rejects symbolic/non-regular sources, publishes without overwrite, revalidates the created no-follow descriptor, and removes every execute bit without modifying the source. Exact new folder/template destinations stay memory-only, are selected after refresh, and enter the existing rename workflow. No template contents, catalog, history, or lossy path reconstruction is persisted. Phase 12E links/duplicate polish is the sole next phase.

Phase 12C adds preview-first batch rename for 2–4,096 selected local entries.
Literal/regex find-replace, prefix/suffix, deterministic numbering/padding,
case, metadata-date/name/extension templates, and explicit preserve-extension
policy generate a bounded accessible native preview. Unicode transforms reject
non-UTF-8 names explicitly; original paths are never reconstructed from lossy
text. Whole-batch validation rejects unsafe/duplicate destinations before one
application job. Core execution uses same-directory hidden staging and Linux
no-replace renames so swaps/cycles work without overwrite, checks cancellation
before commit, attempts rollback, and reports incomplete rollback as partial.
One exact inverse mapping remains memory-only for Undo Last Batch Rename and is
revalidated through the same engine. Phase 12D create/templates polish is the
sole next phase.
```

```text
Phase 12B adds native Extract Here, Extract To, and Compress workflows over the
Phase 12A executor. Exact selected paths remain application state; raw archive
stems and default compression names are preserved independently from lossy
destination previews. One compact Archives submenu exposes live selection-aware
GActions. The accessible Compress dialog uses visible labels, reviewed formats,
editable explicit naming, destination preview, and clear no-overwrite/password
limitations; Extract To uses the native local-folder chooser. Archive jobs reuse
the shared Operations Island for byte/item progress and cancellation, refresh
affected directories, and present truthful conflict/password failures. No
password field, secret handoff, overwrite, helper process, or persisted archive
history was added. Phase 12C batch rename is the sole next phase.
```

```text
Phase 12A adds a GTK-independent bounded archive engine for ZIP, TAR, TAR.GZ,
TAR.XZ, and reviewed pure-Rust 7z listing, extraction, and compression. Typed
requests preserve exact `PathBuf` identities. Member plans cap archive bytes,
entries, member/total expanded bytes, expansion ratio, raw path bytes/depth,
and reject traversal/absolute paths, duplicates, file-directory nesting
conflicts, links, special entries, symbolic sources, and unsupported exact-name
conversions. Extraction/compression use private hidden sibling staging and
Linux no-replace publication; sources are opened no-follow and revalidated.
The application-owned capacity-4 worker reports shared job progress and
cancellation and retains at most 16 memory-only outcomes. Passwords, overwrite,
external helpers, and GTK workflows are excluded. Phase 12B archive UX is the
sole next phase.
```

```text
Phase 11E adds safe preferred-terminal integration. Nine reviewed providers are
discovered from at most 256 absolute PATH components on one capacity-4
application worker; resolved executable paths remain memory-only. Open Terminal
Here targets one selected navigable local directory or otherwise the active
local directory, and rejects Trash, relative, over-limit, missing, and
non-directory targets. Launch uses `std::process::Command` with the resolved
absolute executable, no additional arguments, and the exact raw
`PathBuf` as child working directory; no shell command, lossy display-path
reconstruction, password argv/environment injection, or GTK filesystem work is
used. Up to 32 child handles are reaped by the same worker. Automatic selection
is deterministic; an unavailable explicit preference falls back with named
feedback. The native chooser persists only one reviewed provider ID in
version-7 preferences and exposes available/unavailable state. File,
background, header, palette, and chooser actions reuse live GAction eligibility.
Embedded terminals, arbitrary commands, repository detection, and terminal
history remain deferred. A post-11E Phase 10C parity correction places the
existing selection-aware Properties action in a separated final section of the
concrete list, grid, Miller, and Trash context menus; the GTK popover builders
still delegate to the existing application action and perform no filesystem
work. Phase 12A archive engine is the sole next phase.
```

```text
Phase 11D adds an optional Vim-style browser navigation layer that is disabled
by default and persisted as one version-6 boolean preference. When explicitly
enabled, h/j/k/l move parent/next/previous/child, g/G select first/last, and o
opens through existing list/grid/Miller selection, navigation, and activation
paths. Controllers are attached only to file views; command/location/shortcut
entries, search entries, spin buttons, text views, passive preview text, and
dialogs retain native input. Control, Alt, Super, and unreviewed shifted chords
fall through. A stateful `win.vim-mode` GAction is searchable in the command
palette and available in the header menu; a persistent `Vim On`/`Vim Off`
header control provides non-color and accessible state. Preference writes remain
on the capacity-one application worker and no key or navigation history is
stored. Phase 11E terminal integration is not included.
```

```text
Phase 11C adds bounded persistent shortcut customization over the central
command registry. Version-5 view preferences migrate legacy files with defaults,
store at most 64 overrides and four canonical GTK accelerators per command, and
continue writing only through the existing capacity-one application worker.
Effective-binding conflicts, malformed input, duplicate bindings, unknown
commands, and over-capacity records are rejected deterministically. Normal and
recoverable commands may be changed or disabled; confirmation-required and
irreversible commands keep their reviewed defaults. The native Keyboard
Shortcuts dialog is reachable from Ctrl+?, the header menu, and command palette;
it searches at most 128 characters across all 60 registered command metadata
records and exposes category, description, availability, current binding, and
custom/default state. Individual reset, reset-all, and successful edits reinstall
accelerators immediately and queue asynchronous persistence. Only action IDs and
accelerator text persist; search and usage history remain memory-only. Phase 11D
Vim mode and Phase 11E terminal integration are not included.
```

```text
Phase 11B adds a native Ctrl+Shift+P command palette over the Phase 11A registry and existing live GActions. Search is limited to 128 Unicode scalar values and static command names, actions, descriptions, categories, and search terms; results are deterministically ranked and capped at 64. Disabled and missing commands remain visible with explicit context and cannot activate. Enter runs only an enabled existing GAction, while Down transfers focus to the result list and Escape uses native dialog dismissal. Rows expose accessible names/descriptions, shortcut/category context, focus state, and confirmation-required warnings. At most 16 successful command action IDs are deduplicated in memory only; queries and recents are never persisted or logged. No paths, filenames, contents, or clipboard data are searched. Phase 11C shortcut customization is not included.
```

```text
Phase 11A adds one bounded application-layer command registry for 59 normal user-invokable commands. Each definition has a unique existing `win.*` GAction, human name, description, deterministic category, at most eight search terms, default accelerators, menu placements, risk level, and palette eligibility. The registry owns no callbacks or filesystem logic: live GAction presence/enabled state remains the sole execution and context-eligibility authority. Existing window accelerators now install from registry metadata without changing bindings; background context labels resolve through the registry while preserving established order. File, Trash, and background context actions have parity tests. Parameterized tab IDs, stateful view/column controls, and widget plumbing are explicitly excluded from the normal command surface. Startup audits registry/action parity without logging command names or paths. Phase 11B command-palette UI is not included.
```

```text
Phase 10F adds lazy bounded advanced metadata to Inspector, Properties, and five opt-in list columns: Dimensions, Duration, Artist, Album, and Track. The existing capacity-64 metadata worker performs exact no-follow local regular-file parsing only on explicit Inspector demand or for bound visible rows when an advanced column is enabled. It revalidates device/inode/size/mtime/ctime, caps cumulative reads at 16 MiB and text at 1,024 characters, limits EXIF to ten reviewed presentation fields, uses strict media parsing with cover art disabled, and keeps results in a 512-entry memory-only cache. Unsupported, absent, malformed, changed, inaccessible, and limit states remain explicit. Asynchronous enrichment is non-sortable, so it cannot reorder rows; legacy nine-column preferences migrate through stable textual names. GPS/privacy findings remain Phase 18O, and no safety, authenticity, or sanitization claim is made.

Phase 10E adds explicit checksum calculation for exact selected local regular files. Typed requests accept SHA-256, SHA-512, and clearly legacy-labelled MD5, with one optional strict expected hexadecimal digest for a single target. A capacity-4 application-owned executor streams 1 MiB chunks, reports determinate byte progress, supports cancellation, opens no-follow, and revalidates device/inode/size/mtime/ctime before accepting results. Standard vectors and explicit match/mismatch are covered. Native selection-aware dialogs expose selectable results and digest-only clipboard text; GTK submits jobs and never reads file contents. Results remain memory-only. A checksum match compares bytes at calculation time and does not prove authenticity, authorship, freshness, safety, signature validity, or future integrity.

Phase 10D adds explicit Unix permission editing from Properties for exact selected local entries. Typed requests accept separate file/directory octal modes, executable-file enable/disable, numeric UID/GID values, and validated local owner/group names. A capacity-4 application-owned executor resolves local names off GTK, performs whole-request no-follow preflight capped at 250,000 entries and depth 1,024, refuses roots and mount crossings, revalidates device/inode identity, reports pre-commit cancellation separately from committed partial failure, and refreshes affected parents through Operations feedback. Recursive and ownership changes require explicit acknowledgement. Symbolic links are never followed; ACLs, xattrs, capabilities, immutable flags, checksums, password collection, and whole-process elevation remain out of scope.

Phase 10C adds native read-only Properties through Alt+Enter, file/Trash/header
menus, and an accessible dialog. One capacity-8 generation-superseding worker
reuses Phase 10B exact metadata, queries containing GIO filesystem/mount facts,
and calculates explicitly requested recursive selected-folder totals through
descriptor-relative no-follow traversal capped at 250,000 entries and depth
1,024. Multi-selection shows shared MIME only when identical and never merges
differing/unknown values. Open With delegates to the existing explicit chooser
and default-association boundary. No permission/owner edit, root-process
elevation, persistent property history, checksum, or advanced metadata is added.

Phase 10B extends the read-only Inspector with a fixed-capacity, generation-
superseding metadata provider. Exact selected local identities receive no-follow
MIME, created/modified/accessed timestamps, Unix UID/GID/mode, raw symlink target
and target-entry status, safely limited raster dimensions, and explicit per-item
missing/changed/inaccessible results. Selected folders share a 16,384 immediate-
child budget and report only known non-recursive bytes with truthful truncation.
Requests and results are both capped at 16; facts remain memory-only, source
identity is revalidated, GTK performs no filesystem work, and no property edit,
recursive size, persistent metadata cache, checksum, EXIF, or privacy claim is
made.

Phase 10A adds a read-only Inspector final column backed by a fixed-capacity
GTK-independent worker. It aggregates only exact raw selected paths and listing
facts already in memory: entry-kind counts, known bytes, unknown sizes, overflow,
and common parent. Requests are capped at 4,096 selections in a 16-slot queue;
generation checks discard stale responses. Ctrl+I toggles the accessible Miller
surface and closing restores active-column focus. Inspector width is independent
from normal Miller columns, clamped to 180–520 pixels, and saved asynchronously in
version-4 view preferences with version-3 migration. No filesystem reads,
recursion, rich metadata providers, property edits, history, or selected paths are
added to persistence.

Phase 9F completes Preview interaction polish. A stable Space action yields to
Entry/SearchEntry/SpinButton/TextView focus; open Preview follows exact
selection/navigation generations and restores active-column focus on close.
Image/document/font surfaces expose bounded 50–400% zoom/reset and fullscreen
without modifying sources, while GTK paintables remain monitor-scale aware.
An explicit control cancels active work, advances the cache epoch, clears the
bounded memory-only Preview cache, closes detail, and leaves persistent
thumbnail storage untouched. Media streams still pause/clear on retirement.

Phase 9E adds reviewed-provider passive font specimens and built-in archive
listings. Font sources are exact/no-follow and only bounded PNG becomes owned
RGBA; Floe never installs the font. ZIP central-directory and uncompressed TAR
headers are parsed with source, entry, name, output, cancellation, checksum, and
truncation limits. Raw member bytes remain separate from lossy labels, and
absolute/traversal-like names are visibly flagged. No member is extracted and
no archive command runs. Compressed TAR and unsupported formats fail honestly.

Phase 9D adds exact local audio/video Preview through main-thread GTK
MediaFile/Video/MediaControls. A worker validates no-follow extension and GIO
MIME identity and may request one bounded passive video poster through the
existing supervised Phase 6L provider boundary. Native controls provide
play/pause and seeking; audio has an honest icon fallback. Detail rerender,
selection change, navigation, or closure explicitly pauses and clears the
retired stream. Floe neither shells out for playback nor installs codecs.

Phase 9C adds passive PDF and reviewed office/document previews by reusing the
Phase 6L freedesktop thumbnailer registry and supervised argv-only process
boundary. Exact sources are opened no-follow before dispatch and reopened after
provider completion. Only bounded PNG output becomes owned RGBA for a labelled
first-page/document rendition. Unsupported providers, cancellation, malformed
output, source changes, and symlinks fail explicitly. Macro-enabled formats are
not selected. Helpers retain normal user authority and are not sandboxed;
Phase 18L owns isolation.

Phase 9B adds deterministic built-in raster and passive-text Preview providers.
Exact source path/size/modified identity is opened no-follow and revalidated;
explicit source/output/text limits bound first-frame RGBA and UTF-8/BOM UTF-16
payloads. Markdown, code, JSON, and XML remain inert selectable source. HTML,
SVG, binary content, malformed encodings, oversized input, and stale results are
rejected. GTK textures and text widgets are created only on the main thread;
retired payloads remain memory-only and bounded. Providers are in-process and
are not called sandboxed; Phase 18L still owns renderer isolation.

Phase 9A adds a GTK-independent Preview provider registry and fixed-capacity
single worker. At most 32 deterministic providers feed a 16-request queue;
requests retain exact path/size/modified identity, nonzero generation, explicit
source/output/text/archive/deadline limits, and Disabled or default MemoryOnly
cache policy. Cooperative atomic generation cancellation, stale submission and
response rejection, queue pressure, provider failure/panic containment, and
clean shutdown are verified. GTK drains at most eight responses per tick into
truthful Loading/unsupported/failed Phase 8F states. The default registry is
empty: no renderer, persistent cache, network, shell, active content, unrelated
file access, or sandbox claim was added.

Phase 8F completes the bounded Miller sequence with optional Preview and
Inspector final-column hooks. An application-owned GTK-independent lifecycle
preserves exact generation, logical depth, directory, and at most 4,096 raw
selected paths. Hidden, empty, ready-for-provider, and unsupported states are
explicit. Accessible active-column controls open a focusable truthful surface;
closing returns focus and leaving Miller hides it. No decoding, metadata,
provider worker, cache, persistence, active-content execution, or sandbox claim
was added; Phases 9 and 10 consume the handoff contract.

Phase 8E makes active and retained Miller columns standard local-file drag
sources and exact folder/background destinations. It reuses existing
copy/move/link negotiation, request validation, bounded FIFO jobs, and
no-overwrite behavior. Live tab sessions, the opposite split pane, Places,
bookmarks, and mounted devices share typed destination/hover ownership. One
cancellable 720 ms timer revalidates directory, tab, pane, or Miller-child
targets; edge motion clamps vertical and horizontal ancestor scrollers. GTK
performs no filesystem mutation, no drag history/path persistence was added,
and Phase 8F detail content remains absent.

Phase 8D adds native file/background context menus to every active and retained
Miller column. Pointer and Shift+F10/Menu paths emit bounded exact depth,
directory, and shared-entry identities; controller revalidation rejects stale,
overflowed, wrong-parent, and evicted contexts. Existing application actions,
no-overwrite jobs, and GTK responsiveness remain unchanged. Retained-column
paste/create/relative-path commands use the validated owner directory, while
navigation-only background actions are disabled when ownership is not active.
No drag/drop, preview provider, new filesystem worker, or persisted path state
was added.

Phase 8C adds bounded Up/Down/Home/End item movement, logical parent/child
directory movement with LTR/RTL reversal, and dominant-horizontal trackpad
scrolling that leaves vertical column scrolling intact. Modified key chords
fall through to native GTK selection behavior. Recycled active lists retain
focus-visible exact selection; active state remains textual. GTK's animation
setting disables kinetic scrolling for reduced motion. Logical parent/child
actions are exported for parity and native verification. No Vim mode, column
context actions, drag/drop, new worker, or new persisted path state is added.

Phase 8B adds native horizontally scrolling, virtualized Miller columns. The
active column shares the existing `GtkMultiSelection`, `GioListStore`, browser
worker, and watcher; prior columns retain only already-returned shared entries,
bounded to 16 snapshots and 4,096 entries each. Exact `PathBuf` identity and
logical depth return from recycled-row activation. Active state is named in
text, and one global 180–520 px column width persists through version-3 view
preferences. List/grid, tabs, splits, operations, and file watching remain on
the same application pipeline. Phase 8B adds no per-column context actions,
drag/drop, Preview/Inspector content, or Phase 8C keyboard/trackpad navigation.

Phase 8A adds a GTK-independent `MillerColumnModel` with exact directory and
selected-direct-child paths, stable logical depths, and a fixed 16-column
retention window. It rejects relative, oversized, non-child, stale-depth, and
cross-parent rename input while preserving raw non-UTF-8 identity. Selection,
descent, same-parent rename, deletion truncation, root invalidation, and reset
have explicit transitions. The model owns no directory entries, enumeration,
workers, widgets, GIO, compositor state, or persistence. Visible Miller columns
remain Phase 8B.

Phase 7F makes the inactive pane an exact, live-resolved local-file drop target.
Standard GDK payloads and desktop modifiers reuse the existing copy/move/link
dispatcher, FIFO jobs, no-overwrite conflicts, self-nesting checks, and
non-color-only action/path/commit feedback. Open, Copy, Move, and Create Links in
Other Pane provide explicit keyboard/menu alternatives. The target does not
hover-open, clone the browser pipeline, detach tabs, or implement Miller drag.

Phase 7E adds one native horizontal split over the Phase 7D state. F3 toggles,
F6 switches active side, Ctrl+Alt+Left/Right changes the primary ratio in 5%
steps, and menus expose close, swap, Open Folder in Other Pane, and direct
Copy/Move to Other Pane. Text identifies the active left/right side; the inactive
pane is a bounded, truthfully stale snapshot. Exact opposite destinations reuse
existing no-overwrite FIFO jobs without disturbing the staged transfer buffer.
Only one active list/grid, browser worker, thumbnail/metadata pipeline, and
watcher exists. Split state and ratio restore across clean launches.

Phase 7D adds GTK-independent per-tab split state. Every live and recently
closed tab now owns a primary `BrowserSession`, optional secondary session,
explicit active side, and bounded 20–80% ratio while retaining stable tab
identity. Pane histories, selections, scroll anchors, raw non-UTF-8 paths, and
view policies remain independent. Deterministic close/swap transitions preserve
the surviving content. Workspace version 2 persists complete split state,
rejects hostile side/ratio/duplicate-ID input, and migrates Phase 7C version-1
unsplit files. No GTK split widget, shortcut, second browser worker, or
inter-pane drag is added.

Phase 7C adds a bounded 32-entry recently closed LIFO with fresh-ID reopen,
Close Left/Right/Others, and Ctrl+Shift+T. A versioned workspace envelope
preserves up to 64 ordered live tabs, active ID, complete raw-path session state,
and recently closed state. The application-owned capacity-one worker performs
bounded no-follow startup reads and 0700/0600 synchronized atomic clean-shutdown
writes outside GTK callbacks. Missing, corrupt, hostile, or unsupported state
falls back to one normal tab. Explicit Private/Sensitive integration policy
loads no workspace, removes Floe's owned session file, and suppresses shutdown
recreation; this is not a complete user-facing Private Mode. Optional names,
pins, split view, and crash journaling remain deferred.

Phase 7B adds bounded live tab interaction over the Phase 7A session model. The
compact native strip supports new, close, switch, duplicate, stable-ID pointer
and keyboard reorder, foreground/background folder open, middle-click folder
open, and middle-click tab close. Active transitions capture exact selection,
path/index scroll anchor, and complete view policy, then restore through the
existing single browser pipeline. Tabs share one virtualized model, bounded
workers, watcher, jobs, and Operations Island. They remain memory-only; closed
tab retention, startup persistence, and split view are not implemented.

The post-7A grid-grouping correction makes Type and Extension grouping visible
inside the virtualized grid with the same boundary labels used by list view.
Every grouped tile reserves a stable label row so recycled cells remain aligned;
only the first item in each group exposes the accessible heading text. Extension
grouping also treats all navigable directories as one Folders section, including
dotted directory names. Shared selection, activation, rubber-band selection,
drag/drop, thumbnail requests, and the single virtualized model remain intact.

Phase 7A establishes the GTK-independent tab/session foundation without changing
runtime UI. `floe-core` now canonically owns list/grid, grid-size, density,
sorting/grouping/folder-placement, and list-column view policy. `BrowserSession`
owns a stable nonzero ID plus bounded complete current/back/forward locations:
exact absolute path, exact multi-selection, path/index scroll anchor, and full
folder view state. Whole-location navigation preserves restoration state, clears
forward history after new navigation, and stops parent traversal at root.

The bounded version-1 in-memory codec preserves raw non-UTF-8 Unix path bytes and
rejects invalid IDs, relative/oversized paths, invalid policy fields, duplicate
selection paths, oversized history/selection, malformed/truncated data, and
trailing bytes. The application does not call the codec and Phase 7A performs no
session persistence. No tab widget, action, shortcut, split view, or duplicated
browser worker was added.

Phase 6T completes the daily browser surface. Raw-extension sorting,
folders-first/folders-last placement, and independent None/Type/Extension
grouping remain GTK-independent and deterministic. Compact, Comfortable, and
Spacious density plus nine optional/clamped list columns share the virtualized
list/grid model. MIME, Created, Accessed, and Permissions load only for bound
rows through a capacity-64 worker and 512-entry cache; recycled rows reject
delayed prior bindings and metadata arrival never reorders entries.

Version-2 preferences migrate legacy view/grid/sidebar values and persist the
complete global policy. Opt-in per-folder memory stores at most 256 exact raw
local paths as private hex-encoded records. Status text reports only known
non-recursive bytes. A capacity-32 application worker queries GIO filesystem
size/free/read-only facts for the current local location and mounted local
devices, rejecting stale generation, device ID, and path results.

Phase 6S adds one application-owned non-recursive GIO monitor for the active
successful local listing. One cancellable 140 ms source deduplicates bursts with
explicit caps of 16,384 events, 4,096 paths, and 1,024 rename pairs; overflow
causes one conservative reconciliation. Current generation/directory checks
discard stale callbacks. Each accepted batch submits one superseding existing
directory-worker enumeration. Exact selection and a stable path/index scroll
anchor are reconciled in linear time, including bounded rename chains, and
restored after 256-entry GTK model insertion. Monitor callbacks do no filesystem
enumeration or mutation, logs contain aggregate counts rather than paths, and no
integrity-monitoring claim is made.

Phase 6R adds exact-path internal and external local-file drag-and-drop to the
virtualized list and grid. Folder rows, directory backgrounds, Places,
bookmarks, navigable mounted devices, and Trash resolve authoritative
destinations at interaction time. Standard GDK file-list payloads preserve raw
paths; non-local, empty, invalid, same-destination, and self-nesting drops are
rejected. Copy, move, symbolic-link, and Trash drops reuse FIFO bounded jobs and
fail-if-exists semantics. One cancellable 720 ms hover timer and clamped edge
autoscroll keep GTK responsive. Dashed outlines plus action/destination text and
accessible descriptions avoid color-only feedback. Existing Copy/Cut/Paste and
menu/keyboard actions remain complete non-drag alternatives.

Phase 6Q adds a bounded application-owned creation executor for no-overwrite
folders, empty files, template copies, symbolic links, and hard links. Native
validated-name dialogs submit typed requests; native asynchronous template
selection starts at XDG Templates when available. Multi-selection Duplicate is
FIFO, preserves raw names and symlinks, and advances deterministic `(copy N)`
conflict retries without replacing existing siblings. Hard links accept only one
regular non-symlink file and report cross-filesystem limitations.

File and header menus expose create, duplicate, link, reveal-target, and copy
name/path/relative-path/URI commands. Reveal Link Target reads no-follow GIO
metadata asynchronously, resolves stored relative targets lexically, verifies
accessibility, and navigates without executing content. Text clipboard commands
reject non-UTF-8 identity; local file URI encoding preserves raw path bytes.
GTK callbacks perform no filesystem mutation.

Phase 6P adds bounded operation control. Progress carries explicit unknown,
byte, or item units; Operations Island derives smoothed speed/ETA only from
meaningful determinate byte samples. Stable serial batches expose item counts,
pause-after-current/resume, queued-item cancellation, and terminal summaries.
Conflict handling adds deterministic no-replace Keep Both and batch-scoped Skip
All while Replace/Replace All remain unavailable. A 64-entry memory-only history
supports Clear Completed without discarding failure/conflict/partial/cancelled
evidence. Undo is offered only for completed move/rename records with captured
destination identity; reverse moves revalidate identity and never overwrite the
original path. GTK callbacks only submit application commands.

Phase 6O adds safe transfer semantics. Copy now checks point-in-time
destination user-available space before creation, preserves and synchronizes
regular-file/directory Unix mode plus access/modification timestamps, and
explicitly does not claim ownership/ACL/xattr/security-label/sparse/reflink or
symlink-metadata preservation. `EXDEV` move uses a synchronized hidden sibling
staging path, complete source identity revalidation, atomic no-replace
publication, destination-parent sync, and no-follow source cleanup. A
post-publication cleanup/cancellation failure is a non-retryable partial result;
persistent crash recovery remains deferred. Copy/cut publishes bounded
freedesktop/GNOME/KDE local-file clipboard formats, while paste reads them
asynchronously, rejects malformed/remote/oversized input, preserves exact
decoded paths, and stages only through application state.

Phase 6N adds first-class local Trash browsing for home and mounted-volume
freedesktop roots, standards metadata display, bounded no-overwrite restore,
per-item permanent deletion, and confirmed Empty Trash. Exact paths remain
authoritative; GTK submits commands only. Restore conflicts reuse the existing
explicit decision flow, cleanup partials are non-retryable, and no secure-erase
claim is made.

Phases 0-4 and Phase 5D are complete. One application-owned transfer buffer
supports Ctrl+C copy and Ctrl+X move followed by Ctrl+V paste. F2 and the
file-actions menu open a validated rename dialog. Copy, move, and rename use
bounded workers plus generic non-blocking Operations Island feedback; GTK
callbacks perform no filesystem work. A separate bounded GIO trash executor
provides path-safe, cancellable job infrastructure. The explicitly labelled
“Move to Trash” menu action and Delete shortcut submit through application
state. Phase 6M adds a separate selection-aware `Shift+Delete` and “Delete
Permanently…” path with explicit irreversible confirmation and exact target
context. Its fixed-capacity executor performs full no-follow preflight, refuses
roots and mount boundaries, and reports committed partial failure without retry.
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
on the single directory worker using shared entries; model rebuilds retain
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

Phase 6G replaces weak desktop-theme generic glyphs with a bundled fourteen-icon
full-color SVG family. Exact enumerated kind and executable metadata take
precedence, followed by a case-insensitive extension policy for document, media,
archive, code, PDF, spreadsheet, presentation, and generic families. List icons
use a 28-pixel optical size and grid icons stay within 48-88 pixels independently
of thumbnail edges. Both factories share one policy and fallback; ready
thumbnails still replace icons. File images are decorative GTK Presentation
nodes beside authoritative names/types, selection/focus remains semantic, and
no GTK callback performs icon filesystem I/O.

Phase 6H makes the header path a native pointer/Ctrl+L editable location bar.
It seeds and selects the current display path, validates explicit absolute-path
text inline, and sends directory access to the bounded browser worker. Failed
direct submissions restore the exact previous `NavigationState`; successful
responses close the editor and return focus to the active list/grid view.

Phase 6I resolves normal Open through asynchronous GIO content-type/application
discovery. Registered defaults launch normally; missing defaults return the
already-discovered `OpenWithOptions` and automatically present the existing
chooser. One-time Open remains separate from explicit Set as Default.

Phase 6J replaces the shared single-selection model with `GtkMultiSelection` in
list and grid. Native Ctrl-toggle, Shift-range, grid rubber-band, Ctrl+A,
Ctrl+Shift+A, and focused-view Escape clearing preserve original paths. Sorting
restores every selected item by exact `PathBuf`, including non-UTF-8 names. Open,
Open With, and Rename require exactly one target; Copy, Cut, and Move to Trash
accept the full valid selection. Secondary-click preserves an existing
multi-selection or retargets an unselected entry, while directory-background
secondary-click clears file selection and exposes Paste, Select All, Refresh,
and Edit Location. Application state serializes multi-path copy, move, and Trash
requests over existing bounded executors so large selections are not silently
dropped at queue capacity.

Phase 6K expands the compact, vertically scrollable, user-resizable sidebar into
Places, Bookmarks, and Devices. Places includes Home plus every distinct existing
XDG Desktop, Documents, Downloads, Music, Pictures, Public Share, Templates, and
Videos directory. User bookmarks preserve exact raw Linux paths and load/save
asynchronously through private 0700/0600 atomic persistence outside GTK
callbacks. An application-owned GIO `VolumeMonitor` observes deduplicated drive,
volume, and mount snapshots and refreshes on topology signals. Rows expose
mounted, unmounted, busy, and unavailable states; asynchronous mount, unmount,
and eject actions surface structured failure feedback. Mounted local filesystem
roots navigate normally; remote/network roots remain explicitly unavailable and
deferred.

Phase 6K2 makes that sidebar daily-driver configurable. Compact, Balanced, and
Comfortable density choices apply live and persist. Divider width is clamped to
128-480 pixels, saved after a 320 ms debounce, restored at startup, and can be
reset to the active appearance preset. The Operations Island now separates its
title/cancel, detail, flexible progress, and recovery-action rows inside bounded
340-pixel geometry so Retry and conflict actions remain aligned and reachable.
Device authentication uses a window-parented `GtkMountOperation`; the desktop
owns password prompts and Floe remains credential-opaque.

`Open as Administrator...` is security-designed but intentionally not exposed.
It requires the documented GFile/GVfs `admin://` provider, polkit flow, visible
Administrator state, and all test/rollout gates. Floe must never elevate its
whole GTK process or interpolate paths into shell elevation commands.

Phase 6L discovers policy-accepted freedesktop `.thumbnailer` definitions on
the capacity-64 thumbnail worker. Native rasters stay in-process; supported
video, audio, font, text/code, PDF, office, and archive MIME types may use an
installed provider. Commands are parsed to argv without a shell, receive exact
raw path/URI identity and a private output path, and run in a supervised process
group with timeout, cancellation, bounded no-follow PNG output, stale-source
revalidation, persistent-cache reuse, and cleanup. Failures keep generic icons.
Providers are not sandboxed and retain the user's normal authority; Phase 18L
owns isolation.
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
* Inspector, read-only Properties, Unix permission editing, and checksums are complete; advanced metadata remains planned.
* Command palette is planned.
* A floating operations island is planned.
* Niri IPC integration is planned for a later phase.
* KDE Plasma-specific integration is planned for a later phase.

Currently working:

```text
Phase 10E is complete. The one recommended next branch is
`phase-10f-advanced-metadata`, adding bounded safe EXIF and media/audio metadata
without Phase 18O privacy findings or Phase 11 command-registry work.
```

Verified:

Phase 10E passes focused exact-request, standard-vector, streaming/cancellation,
source-revalidation, and checksum-presentation tests; formatting, workspace
check, strict all-target/all-feature Clippy, full workspace tests, native build,
and diff hygiene. Native Wayland smoke selected isolated regular-file fixtures,
opened Calculate Checksums through exported actions, exposed legacy/no-
authenticity language through AT-SPI, returned the exact SHA-256 for `abc`,
exposed digest-only copy and result state, remained responsive to D-Bus Ping,
quit cleanly, and released the application name.

```text
Phase 10D passes focused typed-request, direct/recursive executor, local-name resolution, application-state lifecycle, and permission-editor validation tests; formatting, workspace check, strict all-target/all-feature Clippy, full workspace tests, native build, and diff hygiene. Native Wayland smoke selected an isolated fixture, opened Properties and Edit Permissions through exported/accessibility actions, exposed the risk controls, reported empty-input validation, applied mode 0600 only to the temporary file, exercised Cancel, answered D-Bus Ping, retained focus state, quit cleanly, and released the application name. Only the documented host RADV/Vulkan warnings appeared.

Phase 10C passes focused Properties model/filesystem/UI tests, formatting,
workspace check, strict all-target/all-feature Clippy, full workspace tests,
native build, diff hygiene, and native Wayland Properties action/dialog health.

Phase 10B passes focused metadata-provider and Inspector-presentation tests,
formatting, workspace check, strict all-target/all-feature Clippy, full workspace
tests, native build, diff hygiene, and native Wayland Inspector lifecycle smoke.

Phase 10A passes formatting, workspace check, strict all-target/all-feature
Clippy, and 375 tests: 284 application and 91 core. Focused tests cover bounded
raw non-UTF-8 aggregation, oversized selection rejection, multi-selection and
stale lifecycle behavior, Ctrl+I/action contract, width clamping, persistence,
and version-3 migration. Native Wayland smoke selected the live directory,
opened and closed Inspector, adjusted width across launches, answered D-Bus
Ping, quit cleanly, and restored 280-pixel persisted width. Only documented host
libadwaita/RADV/Vulkan warnings appeared.

Phase 8C passes formatting, workspace check, strict all-target/all-feature
Clippy, and 340 tests: 249 application and 91 core. Five focused Phase 8C tests
cover LTR/RTL policy, bounded focus selection, horizontal trackpad clamping,
modified-key isolation, reduced motion, and exported logical actions. Native
Wayland smoke activated Miller mode, described both navigation actions,
invoked the safe root-parent path, answered D-Bus Ping, quit cleanly, and
released the application name.

Phase 8B passes formatting, workspace check, strict all-target/all-feature
Clippy, and 335 tests: 244 application and 91 core. Five focused Phase 8B tests
cover bounded snapshots/widths, raw non-UTF-8 identity, active shared-model
pipeline, non-color-only active descriptions, and list/grid/Miller action
parity. Native Wayland smoke activated Miller mode and width adjustment over
D-Bus, remained healthy, quit cleanly, and released the application name.

Phase 7C passes formatting, workspace check, strict all-target/all-feature
Clippy, and 312 tests: 82 core and 230 application. Focused coverage includes
bounded closed-tab transitions, hostile workspace input, private atomic storage,
corruption/symlink fallback, explicit trace suppression, and action contracts.
Two-launch native Niri/Wayland smoke verified 0700/0600 state, multiple restored
tabs, enabled reopen, Close Others, D-Bus health, clean quit, and name release.
An isolated private-policy launch removed and did not recreate the session file.

Phase 7B passes formatting, workspace check, strict all-target/all-feature
Clippy, and 304 tests: 78 core and 226 application. Native Niri/Wayland smoke
exported and activated new/switch/reorder/duplicate/close actions, answered
D-Bus health, quit cleanly, and released the application name. Spectacle
produced no file, so no visual capture is claimed.

The grid-grouping correction passes formatting, workspace check, strict
all-target/all-feature Clippy, and 298 tests: 75 core and 223 application.
Focused `grid_grouping` tests cover visible list/grid boundary parity and dotted
directory grouping. Native Niri/Wayland smoke activated Grid plus Group by
Extension through exported actions, confirmed action state and D-Bus health,
quit cleanly, and released the application name. No visual capture is claimed:
Spectacle produced no file and the screenshot portal timed out.

Phase 7A passes formatting, workspace check, strict all-target/all-feature
Clippy, and 296 tests: 74 core and 222 application. Focused coverage includes
canonical view policy, complete history transitions, explicit history/selection
bounds, non-UTF-8 path round-trip, and malformed codec rejection. No native
Wayland smoke is claimed because Phase 7A adds no runtime application wiring.

Phase 6T passes formatting, workspace check, strict all-target/all-feature
Clippy, and 294 tests: 66 core and 228 application. Focused coverage includes
raw extension/grouping identity, metadata staleness/capacity/cache, column and
density policy, legacy/raw-path preferences, honest byte/capacity wording, and
bounded storage queries. Native Niri/Wayland smoke exported 73 actions,
exercised and restored density/grouping/folder placement/optional columns/name
width across two isolated launches, answered D-Bus Peer.Ping, quit cleanly, and
released the application name twice. Spectacle produced no capture; action
state, persistence, D-Bus health, and lifecycle are the runtime evidence.

`cargo fmt --all -- --check`, `cargo check --workspace`, strict Clippy, and all
277 tests pass: sixty-three core and 214 application tests. Six focused Phase 6S
tests cover monitor replacement/stop, bounded storm coalescing, common and raw
rename event mapping, stale dispatch rejection, 100k-path selection/anchor
reconciliation, and recoverable failure wording. Native Wayland smoke observed
one coalesced two-create batch, exact rename pair, delete reconciliation, 42
exported actions, D-Bus `Peer.Ping`, clean quit, and application-name release.
Six focused Phase 6R
tests cover raw-path/self-nesting policy, local/non-local payload decoding,
FIFO copy/move/link state routing, exact destination planning, bounded edge
motion, and non-color-only feedback. Native Wayland smoke exported 42 window
actions, answered D-Bus `Peer.Ping`, remained healthy, quit cleanly, and released
its application name. Fourteen focused
Phase 6Q tests cover no-overwrite directory/file/template creation, cancellation,
raw and broken symbolic targets, regular-file-only hard links, duplicate naming,
FIFO symlink-preserving batches, stable conflict retry identity, create history,
typed template actions, lexical target resolution, exact text/URI clipboard
policy, and action/menu sensitivity. Native Wayland smoke used isolated HOME/XDG
roots, exported 42 window actions, activated `new-folder`, answered D-Bus
`Peer.Ping`, quit cleanly, and released its application name.

Sixteen focused
Phase 6P tests cover explicit units, frequent-sample telemetry, regression and
indeterminate suppression, raw Keep Both naming, stable batch counts, FIFO
pause/resume, queued cancellation, committed-current cancellation, scoped Skip
All, evidence-preserving history, identity-checked move Undo, non-undoable copy,
and truthful UI summaries. Native Wayland smoke used isolated HOME/XDG roots,
exported 31 window actions, activated `operation-history`, answered D-Bus
`Peer.Ping`, quit cleanly, and released
its application name. Fifteen focused Phase 6O tests cover exact
required/available space errors, file/directory mode
and timestamp preservation, truthful symlink metadata reporting, injected and
real-`EXDEV` staging, no-overwrite cleanup, exact non-UTF-8 paths/symlinks,
changed-source recovery, committed partial failure classification, bounded
GNOME/KDE/URI-list copy/cut encoding and parsing, remote/malformed/duplicate
handling. Native Wayland smoke used isolated HOME/XDG roots, exported 30 window
actions, activated Select All and Copy, observed Paste enabled from Floe's
provider, answered D-Bus `Peer.Ping`, quit cleanly, and released its name.
Automated external clipboard ownership could not satisfy the current
compositor's input-serial rule, so no cross-client native claim is made; focused
provider/parser tests are the interoperability evidence. Fifteen focused
Phase 6N tests cover raw percent-decoded paths, mounted relative metadata,
malformed/orphan handling, symlinked roots, safe restore, no-overwrite conflict,
bounded worker dispatch, exact conflict retry, Trash-mode action policy, and
truthful destructive wording. Isolated native Wayland smokes opened Trash,
exported enabled Empty Trash, restored one temporary GIO-trashed item through
Floe, removed its matching metadata, answered D-Bus health checks, and never
touched the real user Trash. Thirteen focused Phase 6M
tests cover exact non-UTF-8 request identity, unsafe batch rejection, recursive
no-follow deletion, whole-batch preflight, mount refusal, pre/post-commit
cancellation, identity revalidation, exact partial failure, mountinfo decoding,
executor capacity/lifecycle, confirmation shortcuts, truthful feedback, and
partial non-retryability. Eleven focused
Phase 6L tests cover deterministic user/system precedence, malformed definitions,
fixed executable argv and field-code policy, non-UTF-8 identity, MIME denial,
private output, no-follow and size limits, process failure, timeout/cancellation
with process-group termination, cleanup, stale-source rejection, bounded queue
fallback, provider PNG validation, and persistent cache reuse. A two-launch
native Wayland smoke invoked one controlled PDF provider, wrote one persistent
cache entry, then reused it while the provider failed; both launches owned and
released D-Bus cleanly and left no provider temp artifacts. Ten focused Phase
6K2 tests cover stable density names, backward-compatible complete preference
state, clamped/restored/reset width, divider resize policy, mount-authentication
ownership, Operations Island bounds/recovery rows, and action/spacing mappings.
Nineteen focused Phase 6K
tests cover XDG ordering/deduplication; raw non-UTF-8 bookmark validation,
versioned encoding, private atomic persistence and bounded worker behavior;
deduplicated GIO snapshot/action policy; exact local navigation; compact sidebar
behavior; and bookmark/device controller wiring. Eight focused Phase
6J tests cover multi-selection action policy, exact multi-path non-UTF-8
restoration, entry/background context rules, deduplicated transfer staging, and
serial copy/move/Trash batches beyond bounded executor capacity. Two focused Phase
6I tests cover registered-default and no-default routing with exact path
retention. Five focused Phase
6H tests cover empty/relative validation, trimmed absolute input,
file-versus-directory recovery, non-UTF-8 display ownership, and exact
navigation-snapshot rollback. Five focused
Phase 6G tests cover all fourteen semantic icon families, mixed-case extensions,
exact non-UTF-8 identity, directory/file-link/folder-link/executable precedence,
generic fallback, bounded list/grid optical sizes, non-symbolic aliases, and all
embedded SVG resources. Five focused Phase 6F tests cover reviewed mixed-case
format policy, WebP/GIF/BMP/TIFF/ICO
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
Seven focused Phase 6B tests cover direction cycling, directories-first ordering,
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
while another job completes.
A native Niri/Wayland Phase 6K2 action smoke exported 26 window actions,
including `sidebar-density` and `reset-sidebar-width`; Balanced and Comfortable
applied live and Compact restored. An isolated two-launch persistence smoke
held Comfortable width 333 exactly through allocation, then Reset removed the
width while preserving view/grid/density. Clean shutdown kept it absent; a
second launch kept it absent through allocation and shutdown. Both isolated
instances answered D-Bus Peer.Ping, exited 0, and released the application name.
Screenshot tooling was unavailable, so deterministic layout tests and native
action/persistence/health checks are the evidence. Only documented host Adwaita
and RADV/Vulkan warnings appeared across the smokes.
A native Wayland Phase 6J smoke owned the expected
D-Bus name, exported and activated Select All and Clear Selection, remained
healthy, exited 0 through Quit, and released the name. It emitted only the known
host libadwaita and RADV/Vulkan warnings. A native Wayland Phase 6G smoke used isolated
temporary home/cache/config roots with fourteen representative entries. Settled
list and grid captures showed the full-color SVG family, bounded optical sizes,
link/executable/file-family marks, and real WebP thumbnail replacement. Floe
owned its expected D-Bus name, remained healthy, created both normal/large cache
entries, switched views through native actions, exited 0 through Quit, released
the name, and left no fixture/screenshots. Only the known host RADV/Vulkan
suboptimal-swapchain warning appeared. A native Wayland Phase 6F smoke used isolated
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
Normal tab and split state now persists across clean shutdown. It can expose paths,
history, selection, and view state to same-user processes, backups, snapshots,
or storage history. Private/Sensitive environment policy suppresses only Floe's
owned session file and is not a complete user-facing Private Mode. Crash-time
latest-state journaling and optional tab names/pins remain deferred.

Phase 6T supports sorting by Extension and grouping by Type/Extension only.
MIME/Created/Accessed/Permissions sorting, date/size grouping, natural-name
sorting, column reorder/autosize, owner/group/path fields, and recursive folder
sizes remain deferred. Per-folder memory intentionally persists exact local
paths and is not Private Mode. The first live native width-adjustment smoke
emitted transient one-pixel GTK measurement warnings; the restored second launch
did not. Spectacle returned success without producing a capture on this host.

No application correctness failures are known. Phase 6Q template creation
selects one local template file at a time; template discovery, categories, and
management remain deferred. Hard links remain local-filesystem/kernel dependent.
Reveal Link Target reports missing or inaccessible targets but does not yet offer
repair. Copy Name/Path/Relative Path deliberately refuses non-UTF-8 text while
Copy URI remains local-file-only.

Phase 6P operation history is
memory-only and intentionally disappears at exit; it is not persistent recovery.
Pause applies only between serial batch items. Undo is limited to completed
move/rename work and can still fail safely when destination identity changed or
the original path became occupied. Phase 6O's external clipboard
interoperability exposes selected local path URIs to desktop clipboard services
and managers under normal user authority; it is not a privacy feature and
automated cross-client ownership could not be natively exercised without a
real Wayland input serial on the current host.
`BrowserWorker` has one worker thread and generation supersession, but its
request channel is currently unbounded; do not describe its queue as bounded.
Copy preserves mode and file/directory access/modification timestamps but not
ownership, ACLs, extended attributes, security labels, symlink metadata, sparse
extents, or reflink state. Destination-space checks do not reserve capacity.
Cross-filesystem moves are staged and source-safe but do not have persistent
crash journaling; a post-publication interruption can leave a complete
destination and retained or partially cleaned source, reported as Partial when
observed. Native smoke runs may emit host
GtkSettings/libadwaita and Vulkan suboptimal-swapchain warnings; neither
originates from Floe logic.
GIO trash cancellation is cooperative and cannot reverse a move after the
desktop service commits it. Phase 6N Trash browsing and original-location
restore are local-filesystem only; Restore Elsewhere, cleanup preferences, and
browsing inside a trashed directory remain deferred.
Phase 6G icons deliberately use a compact reviewed extension policy rather than
full shared-mime-info resolution; authoritative textual kind currently remains
the coarse Directory/File/Link model. Phase 6F does not thumbnail SVG,
AVIF/HEIF, RAW camera formats, or animation
beyond the first still frame. Cache interoperability remains intentionally
limited to the freedesktop normal/large tiers needed by Floe's current 32-192
pixel requests. Remote and network mount roots are shown as unavailable; browsing
them remains deferred.
Permanent deletion is ordinary filesystem removal, not secure erase; snapshots,
backups, CoW history, storage firmware, and external copies may retain data.
Cancellation cannot reverse an already committed removal, and a later failure
is reported as non-retryable partial completion. Phase 6L system thumbnailers
are supervised but unsandboxed and inherit the
user's normal filesystem, environment, session, and network authority. Coverage
depends on installed freedesktop providers; SVG/image providers and executable
MIME types remain deliberately excluded.
```

Deferred:

```text
Overwrite and apply-to-all conflict policy, Restore Elsewhere and Trash cleanup
preferences, ownership/ACL/xattr/sparse/reflink-complete copies, persistent
operation recovery/history UI, template management, heavyweight/RAW/vector thumbnails,
tab detachment, visible Miller columns, previews, archives, search and duplicate
discovery, richer device
details, Niri IPC, KDE-specific APIs, and network filesystems.
First-class theme/font customization, full file-association/external-tool
management, and privileged GFile browsing remain explicit later milestones.
```

Completed this session:

```text
* Completed Phase 10C read-only Properties on `phase-10c-properties`.
* Added exact single/multi General, Open With, filesystem/mount, and bounded
  recursive folder facts through one stale-safe application worker and native
  accessible dialog, without edits or elevation.
* Completed Phase 10B metadata providers on `phase-10b-metadata-providers`.
* Added bounded exact-path read-only MIME/time/Unix/link/image/folder facts with
  generation supersession, no-follow identity checks, and non-recursive wording.
* Integrated single/multi-selection facts into the accessible Inspector without
  property edits, eager enrichment, recursion, persistent caches, or GTK I/O.
* Completed Phase 10A Inspector foundation on
  `phase-10a-inspector-foundation`.
* Added bounded asynchronous exact-path aggregate facts, stale-generation
  rejection, Ctrl+I, accessible read-only presentation, focus restoration, and
  independently persisted clamped Inspector width without metadata providers or
  property edits.
* Completed Phase 8C Miller keyboard/trackpad navigation on
  `phase-8c-miller-navigation`.
* Added bounded item movement, LTR/RTL logical directory movement, exact focus
  restoration, modified-key fallthrough, horizontal gesture clamping, and
  reduced-motion kinetic suppression without Phase 8D actions.
* Verified 340 tests, strict Clippy, formatting/check/diff hygiene, and native
  Wayland action/health/clean-quit lifecycle. Phase 8D is sole next.
* Completed Phase 8B virtualized Miller columns on `phase-8b-miller-ui`.
* Added one native shared-model active column, bounded retained snapshots,
  exact-path activation, adjustable persistent global width, and accessible
  active-column text without a second enumerator or watcher.
* Verified 335 tests, strict Clippy, formatting/check/diff hygiene, and native
  Wayland view/width/action/health/clean-quit lifecycle. Phase 8C is sole next.
* Completed Phase 8A Miller column model on `phase-8a-miller-model`.
* Added exact direct-child chains, stable logical depths, bounded 16-column
  retention, raw non-UTF-8 identity, structured selection/descent and
  rename/delete/root-invalidation/reset transitions without GTK or filesystem I/O.
* Verified 330 tests, strict Clippy, formatting/check/diff hygiene, and all
  model architecture gates; no native smoke is claimed for GTK-free Phase 8A.
  Phase 8B is the sole recommended next phase.
* Completed Phase 7F split drag on `phase-7f-tab-split-drag`.
* Added a live exact inactive-pane drop target using existing local file-list
  copy/move/link negotiation, no-overwrite FIFO jobs, accessible feedback, and
  complete Open/Copy/Move/Link alternatives without hover-open or a second pipeline.
* Verified 326 tests, strict Clippy, formatting/check/diff hygiene, exported
  action state, split-target construction, D-Bus health, and clean native quit;
  Phase 8A is the sole recommended next phase.
* Completed Phase 7E split interaction on `phase-7e-split-interaction`.
* Added native resizable panes, explicit active-side ownership, bounded inactive
  snapshots, toggle/switch/swap/close, exact opposite-pane open/copy/move, and
  pointer/menu/keyboard parity over one shared browser pipeline.
* Verified 322 tests, strict Clippy, formatting/check/diff hygiene, full native
  action lifecycle, D-Bus health, clean quit, and two-launch split restoration;
  Phase 7F is the sole recommended next phase.
* Completed Phase 7C closed tabs/session restore on
  `phase-7c-tab-session-restore`.
* Added bounded LIFO reopen/close variants, hostile-input workspace codec,
  capacity-one private atomic worker, clean startup restore, and explicit
  Private/Sensitive owned-trace suppression.
* Verified 312 tests, two-launch restored action/D-Bus lifecycle, 0700/0600
  state, and native private-policy deletion; Phase 7D is sole recommended next.
* Completed Phase 7B tab interaction on `phase-7b-tabs-interaction`.
* Added bounded stable-ID tabs, exact complete session capture/restoration,
  accessible native strip, pointer/keyboard reorder, foreground/background and
  middle-click folder/tab interaction over one shared browser pipeline.
* Verified 304 tests plus native tab action, D-Bus health, clean quit, and name
  release; Phase 7C is the sole recommended next phase.
* Corrected grid Group By presentation on `fix-grid-grouping` without replacing
  `GtkGridView` or duplicating the shared model/selection.
* Added stable accessible grid group labels and kept dotted directories in one
  Folders section for Extension grouping.
* Verified 298 tests plus native Grid/Extension-grouping action, D-Bus health,
  clean quit, and name release; Phase 7B remains the sole recommended next phase.
* Completed Phase 7A tab/session foundation on `phase-7a-tabs-foundation`.
* Moved canonical GTK-independent view policy from the app into `floe-core` and
  added stable-ID bounded complete browser-session state plus versioned raw-path
  in-memory codec without runtime UI or persistence.
* Verified 296 tests, formatting, workspace check, strict Clippy, and diff
  hygiene; updated roadmap, matrix, architecture, development, privacy, plan,
  and gates with exactly Phase 7B next.
* Completed Phase 6T browser completeness on `phase-6t-browser-completeness`.
* Added exact extension sorting, independent Type/Extension grouping,
  folder placement, shared density, optional/resizable columns, bounded lazy
  metadata, versioned global/per-folder view preferences, honest known-byte
  status, and bounded current/device GIO storage facts.
* Verified 294 tests and a two-launch native Wayland action/persistence/health
  smoke; updated roadmap, matrix, design, architecture, development, privacy,
  phase plan, and gates with exactly Phase 7A next.

* Completed Phase 6S file watching on `phase-6s-file-watching`.
* Added one active GIO monitor, bounded aggregate-only event coalescing, stale
  generation rejection, and one existing worker enumeration per accepted batch.
* Added linear exact selection/rename/scroll-anchor reconciliation, including a
  focused 100k-path case and manual/job refresh preservation.
* Verified 277 tests and a native Wayland external create/rename/delete cycle.
* Completed Phase 6R drag-and-drop on `phase-6r-drag-drop`.
* Added exact selected-path GDK file-list sources and local-only external drop
  decoding for list/grid, folder/background, Places/bookmarks/devices, and Trash.
* Routed copy/move/link/Trash drops through existing FIFO no-overwrite jobs; added
  self-nesting rejection, hover-open, bounded autoscroll, and accessible feedback.
* Verified 271 tests and native Wayland 42-action/health/clean-quit smoke.
* Completed Phase 6Q create, duplicate, and links on
  `phase-6q-create-duplicate-links`.
* Added exact-path no-overwrite directory, empty-file, template, symbolic-link,
  and hard-link requests plus a fixed-capacity creation executor integrated with
  shared jobs, retries, conflicts, cancellation, history, and refresh.
* Added FIFO multi-selection Duplicate with raw-name/symlink preservation and
  deterministic `(copy N)` conflict retries.
* Added native validated-name and asynchronous template-selection flows, exact
  link action sensitivity, asynchronous non-executing Reveal Link Target, and
  lossless Copy Name/Path/Relative Path/local URI commands.
* Verified 265 tests and a native Wayland 42-action/dialog/health smoke.
* Completed Phase 6P operation control on `phase-6p-operation-control`.
* Added explicit progress units, stable byte telemetry, bounded serial batch
  pause/resume/cancel, item and terminal summaries.
* Added deterministic Keep Both, batch-scoped Skip All, bounded memory-only
  history, evidence-preserving Clear Completed, and identity-checked move/rename Undo.
* Kept Replace/Replace All, persistent recovery history, and irreversible Undo out of scope.
* Completed Phase 6O transfer semantics on `phase-6o-transfer-semantics`.
* Added point-in-time destination-space preflight, synchronized POSIX
  mode/timestamp preservation, and explicit unsupported-metadata reporting.
* Added real `EXDEV` staged move fallback with exact source-tree revalidation,
  no-overwrite atomic publication, destination-parent sync, conservative
  no-follow cleanup, progress, and non-retryable committed partial outcomes.
* Added bounded freedesktop/GNOME/KDE copy/cut clipboard publishing and
  asynchronous local-file URI import while retaining exact internal paths.
* Verified 235 tests, formatting, workspace check, strict Clippy, diff hygiene,
  and isolated native Wayland clipboard-provider/action health smoke.
* Completed Phase 6N Trash lifecycle on `phase-6n-trash-lifecycle`.
* Added first-class home and mounted-volume local Trash browsing with bounded,
  no-follow freedesktop metadata parsing and exact original/backing paths.
* Added fixed-capacity no-overwrite restore jobs, conflict-safe revised-name
  retry, metadata-after-payload cleanup, and explicit cleanup partial failure.
* Added Trash-only Restore/Delete Permanently menus, aggregate confirmed Empty
  Trash, companion metadata deletion through Phase 6M, and no secure-erase claim.
* Verified 220 tests and isolated native Wayland Trash/restore action smokes.
* Previously completed Phase 6M permanent deletion on `phase-6m-permanent-delete`.
* Added exact multi-target request validation, full no-follow postorder
  preflight, root/mount refusal, device/inode/kind revalidation, and truthful
  pre-commit cancellation versus committed partial-failure semantics.
* Added a fixed-capacity application executor, `JobFailureKind::Partial`,
  application tracking/cancellation/retry policy, and Operations Island wording;
  partially completed deletion is never generically retried.
* Added selection-aware “Delete Permanently…” menus, `Shift+Delete`, and a
  safe-focus irreversible dialog with escaped exact target context. GTK submits
  only application commands and never performs deletion work.
* Added the exhaustive code-audited `docs/FEATURE_MATRIX.md` capability ledger.
* Rebuilt `docs/ROADMAP.md` as the bounded dependency-aware sequence through
  Phase 21; subsequent Phases 6M and 6N are now complete.
* Added `docs/PRIVACY_SECURITY.md` with implemented/planned separation, threat
  boundaries, cryptographic rules, cache/sandbox/vault/integrity architecture,
  and prohibited claims.
* Added privacy/security interaction semantics to `DESIGN.md` without
  implementing a security feature or choosing a dependency.
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
* Added a bundled fourteen-icon full-color SVG family with coherent folder/page
  construction and distinct link, document, media, archive, code, PDF,
  spreadsheet, presentation, executable, and generic marks.
* Added GTK-independent icon classification from exact enumerated kind,
  executable metadata, and case-insensitive path extensions without lossy path
  reconstruction or GTK filesystem work.
* Shared one icon/fallback policy between list and grid, added bounded 28/48-88
  pixel optical sizing, preserved thumbnail replacement, and marked decorative
  images with GTK Presentation semantics.
* Added focused Phase 6G coverage for family policy, non-UTF-8 identity,
  directory/link/executable precedence, bounded sizes, and all embedded resource
  aliases, plus native list/grid visual verification.
* Added a pointer-operable header location control retaining Ctrl+L, with
  current-path seeding, selection, Enter submission, and Escape cancellation.
* Added GTK-independent absolute location validation and generation-bound
  rollback after missing, unreadable, or non-directory worker results.
* Preserved exact existing `PathBuf` navigation ownership; lossy displayed text
  becomes authoritative only after explicit user submission.
* Added a distinct default-launch outcome so normal Open automatically reuses
  the application chooser when no GIO default is registered.
* Kept application resolution/launch asynchronous, exact-path based, and kept
  one-time Open separate from explicit association changes.
* Replaced shared `GtkSingleSelection` with `GtkMultiSelection` for list/grid,
  retaining native Ctrl/Shift/rubber-band behavior plus Select All and Clear.
* Preserved all selected original paths across sorting, including colliding
  lossy non-UTF-8 display names; zero/one/many status and action states are explicit.
* Added mature secondary-click semantics and distinct entry/background context
  surfaces with keyboard parity.
* Added application-owned multi-path staging and a serial batch dispatcher for
  copy, move, and Trash so bounded worker capacity cannot silently drop items.
* Resequenced Phase 6K Places/devices, Phase 6L system thumbnailers, and Phase 6M
  confirmed permanent deletion with truthful non-secure-erase wording.
* Added every distinct existing XDG user directory to the compact, scrollable,
  user-resizable Places sidebar.
* Added exact raw-path user bookmarks with bounded asynchronous loading/saving,
  versioned private binary storage, and atomic 0700/0600 persistence.
* Added application-owned GIO drive/volume/mount snapshots refreshed from
  `VolumeMonitor` signals, asynchronous mount/unmount/eject actions, explicit
  busy/failure states, and exact local mounted-root navigation.
* Kept remote/network-root browsing deferred.
* Added persistent Compact/Balanced/Comfortable sidebar density and a clamped,
  debounced, restorable, resettable sidebar width preference.
* Rebuilt the Operations Island into aligned title/cancel, detail, flexible
  progress, and recovery rows with bounded geometry.
* Kept mount authentication window-parented and credential-opaque through native
  `GtkMountOperation` desktop prompts.
* Added `docs/PRIVILEGED_ACCESS.md`: Open as Administrator is designed around
  GFile/GVfs `admin://` and polkit but intentionally remains unexposed until its
  security test and rollout gates pass; whole-process elevation is prohibited.
* Added deterministic freedesktop system-thumbnailer discovery and reviewed MIME
  policy on the existing capacity-64 thumbnail worker.
* Added no-shell raw argv expansion, private temporary output, fixed timeout,
  cancellation with process-group termination, no-follow bounded reads, PNG
  validation, stale-source rejection, cache reuse, and generic-icon fallbacks.
* Kept system providers explicitly unsandboxed; Phase 18L remains responsible
  for restricted execution.
* Verified 192 tests plus a two-launch native Wayland provider/cache smoke.
```

Recommended next task:

```text
Create `phase-18t-integrity-tools` and implement saved SHA-256 fingerprints plus
path-safe portable `SHA256SUMS` generation and verification by reusing the
reviewed Phase 10E streaming engine. Cite `T18-01`–`T18-03`, `T18-11`–`T18-13`,
`T18-15`–`T18-16` and `SEC-18A-08`; preserve exact raw paths, no-follow bounded
work, hash-not-authenticity wording, and the Phase 18 test-plan gates.
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
