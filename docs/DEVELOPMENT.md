# Developing Floe

## Platform assumptions

Floe currently targets Linux on Wayland. Niri and KDE Plasma are first-class
targets, but the implemented application uses generic GTK/GIO/GLib mechanisms
and has no compositor-specific backend. Other standards-compliant Wayland
sessions should receive the same current behavior.

An X11 fallback is not a project target and is not currently tested.

## Toolchain and system libraries

The workspace uses Rust edition 2024 and declares Rust 1.85 as its minimum.
The current verified development host uses Rust/Cargo 1.98.0.

Cargo enables GTK 4.14 and libadwaita 1.5 API features. The verified host has:

```text
GTK 4.22.4
libadwaita 1.9.3
```

On Arch Linux or CachyOS, the required packages correspond to:

```bash
sudo pacman -S --needed rust gtk4 libadwaita pkgconf
```

`pkgconf` provides `pkg-config`, which the Rust `-sys` crates use to locate the
system libraries. These package names were verified on the current Arch-based
host; other distributions use different names, commonly `-dev`/`-devel` forms.

Check the installed libraries with:

```bash
pkg-config --modversion gtk4 libadwaita-1
```

## Build and run

From the repository root:

```bash
cargo build --workspace
cargo run -p floe-app
```

The default appearance is Frosted. The header menu's **Appearance** submenu
switches Native, Glass, Frosted, Minimal, and Compact live and persists the
selected stable preset ID through the bounded preference worker.

Override the stored preset for one process launch with:

```bash
FLOE_APPEARANCE=glass cargo run -p floe-app
```

Accepted values are `native`, `glass`, `frosted`, `minimal`, and `compact`.
Unknown values fall back to the stored selection, or Frosted for migrated and
new preferences. The environment override does not itself rewrite the stored
selection.

The preset is read when the primary Floe process starts. Floe is a
single-instance application, so close an existing Floe window before launching
with a different `FLOE_APPEARANCE` value; otherwise the new command activates
the already-running process with its existing appearance.

The appearance-chooser native Wayland smoke verified live Frosted-to-Glass
switching, Glass restoration on a second isolated launch, and a Frosted
environment override that left the stored Glass preference unchanged. Each
instance answered D-Bus `Peer.Ping` and exited cleanly through Quit.

The adjacent **File & Folder Icons** submenu switches **Floe Color**,
**Phosphor Monochrome**, and **System Theme** live. The stable choice persists
in `view-preferences.conf`; legacy preferences migrate to Floe Color. Phosphor
SVGs are bundled resources, so this setting performs no network or filesystem
icon discovery. The source pin and MIT attribution are in
`THIRD_PARTY_LICENSES/Phosphor-Icons.txt`. Plain text, office documents, and PDF
are distinct families. System Theme uses the host MIME icon when available,
then a family-specific bundled Floe fallback.
Known extensions also retain their semantic icon when a mount reports synthetic
execute bits. Run `cargo test -p floe-app post_phase_14_synthetic_execute --
--nocapture` for the exFAT-style `0755` regression; unknown AppImage and
extensionless executables must still use executable artwork.

## Logging

The default tracing filter is `floe_app=info,floe_core=info`. Override it with
`RUST_LOG`:

```bash
RUST_LOG=floe_app=debug,floe_core=debug cargo run -p floe-app
```

Avoid adding file contents or verbose user paths to normal-level logs.

## Quality commands

Run the full project gate before handing work off:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Useful focused commands include:

```bash
cargo test -p floe-core
cargo test -p floe-app
cargo test -p floe-core jobs
cargo test -p floe-core copy
cargo test -p floe-core move_operation
cargo test -p floe-app job_manager
cargo test -p floe-app copy_executor
cargo test -p floe-app phase_4d
cargo test -p floe-app phase_4e
cargo test -p floe-app phase_4f
cargo test -p floe-app phase_5a
cargo test -p floe-app phase_5b
cargo test -p floe-app phase_5c
cargo test -p floe-app phase_5d
cargo test -p floe-app phase_5e
cargo test -p floe-app phase_5f
cargo test -p floe-app phase_6a
cargo test -p floe-core phase_6b
cargo test -p floe-app phase_6b
cargo test -p floe-app phase_6c
cargo test -p floe-app phase_6d
cargo test -p floe-app phase_6l_ -- --nocapture
cargo test -p floe-app move_executor
cargo check -p floe-core
cargo tree -p floe-app --depth 1
```

## Layered testing strategy

The baseline on `main` before this testing-foundation pass was 469 passing Rust
tests: 365 in `floe-app` and 104 in `floe-core`. The existing suite remains the
foundation; the layers below add boundaries and commands rather than moving
hundreds of tests.

### Unit and temporary-filesystem tests

Run deterministic crate tests with:

```bash
cargo test -p floe-core
cargo test -p floe-app
```

Unit tests stay beside their modules. Real filesystem integration tests use
`tempfile` roots only. A test must never fall back to the login user's HOME,
Trash, Downloads, Documents, configuration, cache, data, mounts, or other user
files. Trash tests construct/inject `Trash/files` and `Trash/info` below the
temporary XDG data root. Non-UTF-8, symlink, hard-link, permission, conflict,
cancellation, and mutation-race fixtures remain inside the disposable root.

### Property-based tests

`floe-core` uses `proptest 1.11` as a development-only dependency. It is justified
for large input spaces and invariants, not test-count inflation and not E2E.
Run the current properties with:

```bash
cargo test -p floe-core property_ -- --nocapture
```

The initial properties cover arbitrary valid Linux filename bytes without lossy
identity reconstruction, deterministic sorting with multiset preservation,
non-UTF-8 folder-filter behavior, and navigation Back/Forward round trips. On a
failure, retain the seed/regression case printed by proptest so it reproduces.

### GTK component and accessibility gate

GTK component tests construct real Floe widgets. They are ignored by ordinary
headless `cargo test` and must run in a disposable graphical session:

```bash
cargo test -p floe-app phase_testing_gtk -- --ignored --nocapture
```

The initial contract covers header navigation controls, location and folder
filter roles/help/action wiring, and Operations Island progress/recovery
controls. Extend this layer for real widget state, dialog focus/default actions,
tab/sidebar/header presentation, keyboard actions, and accessible
roles/names/descriptions. Do not use hidden test widgets, layout coordinates, or
CSS selectors as identity.

### Native Dogtail/AT-SPI E2E

The `e2e/` suite is an opt-in native test of the built GTK executable. Required
system capabilities are:

- Python 3 with `dogtail` and `pyatspi` imports;
- `at-spi2-core` and a functioning accessibility bus;
- a Wayland graphical test session and D-Bus session bus;
- a built `target/debug/floe-app` (or `FLOE_E2E_BINARY` pointing to one).

Package names vary by distribution; verify the Python imports rather than
assuming the Rust workspace installs these system components:

```bash
python3 -c 'import dogtail, pyatspi'
cargo build -p floe-app
dbus-run-session -- python3 -m unittest discover -s e2e -p 'test_*.py' -v
```

Prefer an isolated nested Mutter/GNOME Wayland session for the full semantic
suite. Do not run it against a normal desktop containing important open Floe
windows. The harness creates a private temporary HOME, XDG config/cache/data/
state/runtime, and freedesktop Trash per process. It refuses paths outside that
root. The relative Wayland socket is referenced from the temporary
runtime while all Floe-owned data remains isolated. `GSETTINGS_BACKEND=memory`
and `FLOE_SESSION_POLICY=private` prevent normal preference/session writes.

The suite registers eight workflows:

- E2E-01 launch/responsiveness/clean quit;
- E2E-02 navigation Back/Forward/Parent;
- E2E-03 create and rename;
- E2E-04 copy and move;
- E2E-05 the currently implemented current-folder search/filter;
- E2E-06 isolated Trash and restore;
- E2E-07 multi-selection batch copy;
- E2E-08 Ctrl+L, Ctrl+F, Ctrl+T, Ctrl+W, Alt+Left, Alt+Right, Alt+Up, F2.

Waits are tied to accessible, process, location, or filesystem conditions. The
harness does not use coordinate clicks or fixed sleeps. When Dogtail, AT-SPI,
the binary, or a suitable session is missing, discovery reports a skip; that is
preflight evidence, not a passed native E2E run.

### Compositor-specific smoke

Keep graphical gates independent:

- compositor-independent Dogtail/AT-SPI E2E runs in isolated Mutter/GNOME;
- Niri smoke retains current D-Bus action, liveness, clean-quit, and supported
  native interaction checks;
- Plasma Wayland smoke validates the same generic GTK/GIO surface and later any
  explicit Plasma integration.

Do not make all GUI coverage depend on input injection under Niri. Wayland
compositors may intentionally restrict synthetic pointer/keyboard input.

### Regression, security, and CI policy

Each reproduced bug fix gets a lowest-practical-layer regression test. Future
security/privacy work also tests wrong passwords, corrupt/truncated/tampered
data, authentication failure, interruption, cleanup, atomicity, permissions,
symlink/path traversal, secret-free logs, and source preservation as applicable.

No GitHub Actions workflow exists in the current repository, so this pass does
not invent CI infrastructure. When ordinary CI is introduced, run formatting,
check, strict Clippy, deterministic Rust tests, and property tests there. Keep
GTK component, native E2E, Niri, and Plasma jobs opt-in/separate with explicit
graphical environments. Never add Playwright, Selenium, Tauri/browser DOM
testing, or label `proptest` as E2E.

Use `cargo fmt --all` to apply formatting. Core tests use temporary directories
and must never target real user data. Phase 6A presentation tests are
locale-independent: they verify stable column semantics, kind text, size
boundaries, and successful timestamp conversion without asserting a
host-specific rendered date string.

Phase 6B tests exercise both sort directions, directories-first ordering,
unknown optional metadata, raw non-UTF-8 name identity, background worker
sorting, visible direction text, and exact-path selection restoration when two
lossy display names collide.

Phase 6C tests use temporary PNG/JPEG fixtures and cover format eligibility,
exact non-UTF-8 path identity, metadata-sensitive invalidation, no-follow
symlink replacement rejection, encoded-source limits, bounded decoding/scaling,
stale navigation generations, non-blocking queue capacity, pending deduplication,
and the 256-entry presentation-cache bound. The MSRV-compatible `image 0.25.9`
release remains pinned with default features disabled.

Phase 6D tests cover strict view preference parsing, bounded discrete grid zoom,
stable action names, requested thumbnail-edge cache identity, invalid edge
rejection, fixed-capacity nonblocking preference submission, and atomic
persistence. Runtime view preferences live below GLib's user configuration
directory at `floe/view-preferences.conf`; tests use temporary directories and
never write the real user preference file.

Phase 6E tests use temporary cache roots and cover canonical non-UTF-8 file
URIs and MD5 identity, normal/large tier mapping, required PNG metadata,
source-size/time/URI invalidation, corrupt/oversized/symlink cache rejection,
0700 directories, 0600 atomic writes, Floe-only age/count/byte cleanup,
nonfatal cache-root failure, and reuse across thumbnail-worker restarts. The
implementation directly uses the already-locked `png 0.18.1` API for text
chunks and GLib's standard URI/checksum facilities; no external thumbnailer or
new system image package is required. Runtime cache state follows
`$XDG_CACHE_HOME`, falling back to `$HOME/.cache`.

Phase 6F enables only the reviewed `image` features `bmp`, `gif`, `ico`,
`jpeg`, `png`, `tiff`, and `webp`. GIF, WebP, and TIFF add pure-Rust Cargo
dependencies but no external decoder system package. Focused tests create
temporary fixtures for every added decoder, mixed-case extension policy,
malformed WebP fallback, aspect-preserving scaling, real JPEG EXIF orientation
before cache storage, and added-format reuse across worker restarts. SVG,
AVIF/HEIF, animation playback, and unreviewed formats remain disabled.

Phase 6G uses `glib-build-tools 0.21` to compile
`resources/floe.gresource.xml`; this is the matching gtk-rs helper and invokes
the GLib resource compiler already supplied by the GTK/GLib development
environment. No runtime icon files are read from the checkout. Focused tests
cover all semantic families, extension case, exact non-UTF-8 paths, directory,
file-link, folder-link and executable precedence, bounded list/grid optical
sizes, and all fifteen registered full-color SVG resource aliases.

Phase 6H adds no dependency. Focused tests run with `cargo test -p floe-app phase_6h -- --nocapture` and cover location syntax, absolute-path trimming, file-versus-directory recovery wording, non-UTF-8 display ownership, and exact navigation-snapshot rollback. Native checking should exercise both the clickable header path and Ctrl+L, Enter success/failure, Escape cancellation, and restored file-view focus.

Phase 6I adds no dependency. Run `cargo test -p floe-app phase_6i -- --nocapture` for deterministic default/no-default routing coverage. Native smoke should confirm the Open and Open With window actions remain registered and Floe owns/releases `io.github.floe.FileManager`; installed host application associations are intentionally not changed during tests.

Phase 6J adds no dependency. Run
`cargo test -p floe-app phase_6j -- --nocapture` for multi-selection policy,
exact-path restoration, context-surface rules, and bounded copy/move/Trash batch
coverage. Native smoke should confirm `select-all` and `clear-selection` are
exported and activatable while Floe owns its D-Bus name.

Phase 6K adds no new dependency. Its 19 focused tests run with
`cargo test -p floe-app phase_6k -- --nocapture` and cover XDG ordering/deduplication,
raw non-UTF-8 bookmark format and persistence, GIO snapshot/action policy, exact
local-root navigation, compact sidebar behavior, and bookmark/device controller
coverage. Runtime bookmarks live below GLib's user configuration root at
`floe/bookmarks.bin`; the application worker owns asynchronous load/save,
same-directory atomic replacement, 0o700 parent and 0o600 file permissions.

Device discovery uses the session's GIO `VolumeMonitor`. Topology signals refresh
immutable drive/volume/mount snapshots. Mount, unmount, and eject operations are
asynchronous, expose Busy while in flight, accept desktop authentication through
`GtkMountOperation`, and report failure without removing the device row. Floe
navigates only mounted local filesystem roots; remote/network roots remain
explicitly unavailable and are deferred to Phase 17.

The Phase 6K native smoke built `target/debug/floe-app`, owned
`io.github.floe.FileManager`, exported 24 window actions, activated `refresh`,
and answered `Peer.Ping` afterward. Its `quit` action returned successfully, the
process exited 0, and the D-Bus name was released. Only the documented host
libadwaita and RADV warnings appeared.

Phase 6K2 adds no dependency. Run
`cargo test -p floe-app phase_6k2 -- --nocapture` for persisted density/width,
complete preference-state, 128-480 clamping, 320 ms debounce/reset policy,
window-parented credential-opaque mount authentication, menu mapping, and
Operations Island geometry. Runtime view preferences remain at
`floe/view-preferences.conf`; legacy files without sidebar keys still load with
Compact density and the active appearance's default width.

The Phase 6K2 full gate passes formatting, workspace check, strict Clippy, and
181 tests: 148 application plus 33 core. Ten application tests are focused on
Phase 6K2.

Encrypted or password-protected devices use a `GtkMountOperation` parented to
the Floe window. The desktop owns any password/passphrase prompt. Do not add
password fields, log credentials, or replace this flow with shell elevation.
**Open as Administrator...** is intentionally not a live action: its GFile/GVfs
`admin://` and polkit architecture must pass the test and rollout gates in
`docs/PRIVILEGED_ACCESS.md`. Never run the whole Floe application as root.

The Phase 6K2 Niri/Wayland action smoke exported 26 window actions, including
`sidebar-density` and `reset-sidebar-width`. Balanced and Comfortable applied
live and Compact restored. An isolated two-launch persistence smoke started
with Comfortable density and width 333; allocation left width exactly 333.
Reset removed `sidebar-width` while preserving view, grid size, and density, and
clean Quit kept it absent. A second launch with no width kept it absent through
allocation and shutdown. Both isolated instances answered
`org.freedesktop.DBus.Peer.Ping`, exited 0, and released
`io.github.floe.FileManager`; only the known RADV/Vulkan
`VK_SUBOPTIMAL_KHR` host warning appeared. The earlier action smoke also emitted
the documented Adwaita dark-setting warning.

Both Spectacle and ImageMagick screenshot attempts were unavailable in this
session, so the layout audit uses deterministic geometry tests plus native
action, persistence, and health verification rather than claiming screenshot
evidence.

Phase 6L enables rustix process-group support but adds no new crate. Run
`cargo test -p floe-app phase_6l_ -- --nocapture` for provider precedence,
definition/argv policy, non-UTF-8 identity, timeout/cancellation and process-tree
termination, output limits, stale sources, queue fallback, and persistent-cache
reuse. Runtime providers are discovered from freedesktop user/system data
directories. Installed `.thumbnailer` helpers are supervised but not sandboxed;
they retain normal user authority until Phase 18L. A missing, excluded, failed,
or malformed provider must leave the generic icon usable. After Phase 6L,
Phase 6N adds no dependency. Focused checks are:

```bash
cargo test -p floe-core phase_6n -- --nocapture
cargo test -p floe-app phase_6n -- --nocapture
```

Trash tests must use temporary `Trash/files` and `Trash/info` roots and must
never enumerate, restore, empty, or permanently delete the real user Trash.
Native smoke should override `HOME`, `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, and
`XDG_CACHE_HOME`, use a private D-Bus session, activate `open-trash`, and verify
restore/confirmation actions against only that fixture.

Phase 6O adds no dependency. Focused checks are:

```bash
cargo test -p floe-core phase_6o -- --nocapture
cargo test -p floe-app phase_6o -- --nocapture
```

The core suite includes an actual `EXDEV` test when `/tmp` and the checkout are
different devices, plus deterministic injected fallback/recovery tests. It
creates only `tempfile` roots and removes them automatically. Clipboard tests
round-trip raw non-UTF-8 filename URIs, GNOME/KDE copy/cut semantics, remote and
malformed rejection, duplicate handling, and 4 MiB/4096-item bounds. Native
Wayland smoke uses isolated HOME/XDG roots, activates Select All and Copy through
the exported window actions, confirms Paste becomes enabled from Floe's provider,
answers `Peer.Ping`, quits cleanly, and releases the application name. Automated
cross-client clipboard claiming is compositor/input-serial constrained on the
current Niri host, so deterministic MIME parsing/provider tests are the
interoperability evidence; no native external-owner claim is made.

Phase 6P adds no dependency. Focused checks are:

```bash
cargo test -p floe-core phase_6p -- --nocapture
cargo test -p floe-app phase_6p -- --nocapture
```

The tests cover explicit progress units, deterministic telemetry, bounded stable
batches, FIFO pause/resume/cancel, raw-name Keep Both, batch-scoped Skip All,
memory-only history retention, and exact-identity move/rename Undo. Native smoke
uses isolated HOME/XDG roots, verifies the application owns its D-Bus name,
exports its window actions, answers `Peer.Ping`, quits cleanly, and releases the
name. It must not touch real user files merely to exercise Operations Island UI.

After verified Phase 6P, continue only on `phase-6q-create-duplicate-links`.

Phase 6Q adds no dependency. Focused checks are:

```bash
cargo test -p floe-core phase_6q_create -- --nocapture
cargo test -p floe-core phase_6q_links -- --nocapture
cargo test -p floe-app phase_6q_duplicate -- --nocapture
cargo test -p floe-app phase_6q_state -- --nocapture
cargo test -p floe-app phase_6q_templates -- --nocapture
cargo test -p floe-app phase_6q_reveal -- --nocapture
cargo test -p floe-app phase_6q_clipboard -- --nocapture
cargo test -p floe-app phase_6q_ui -- --nocapture
```

Tests use only `tempfile` roots and cover no-overwrite folder/file/template
creation, broken and raw symbolic-link targets, regular-file-only hard links,
FIFO duplicate batches, deterministic `(copy N)` conflict retries, stable job
identity, asynchronous reveal policy, and lossless text/URI clipboard behavior.
Native Wayland smoke used isolated HOME/XDG roots, exported 42 window actions,
activated `new-folder` to open the validated-name dialog without creating a
file, answered `Peer.Ping`, quit cleanly, and released
`io.github.floe.FileManager`.

Phase 6R adds no dependency. Focused checks are:

```bash
cargo test -p floe-app phase_6r_drag_policy -- --nocapture
cargo test -p floe-app phase_6r_payload -- --nocapture
cargo test -p floe-app phase_6r_state -- --nocapture
cargo test -p floe-app phase_6r_destination -- --nocapture
cargo test -p floe-app phase_6r_motion -- --nocapture
cargo test -p floe-app phase_6r_accessibility -- --nocapture
```

Native Wayland smoke verifies the D-Bus owner, 42 exported window actions,
healthy list/grid/sidebar/Trash targets, `Peer.Ping`, clean quit, and application
name release.

Phase 6S also adds no dependency. Focused checks are:

```bash
cargo test -p floe-app phase_6s_monitor -- --nocapture
cargo test -p floe-app phase_6s_coalescer -- --nocapture
cargo test -p floe-app phase_6s_events -- --nocapture
cargo test -p floe-app phase_6s_dispatch -- --nocapture
cargo test -p floe-app phase_6s_reconcile -- --nocapture
cargo test -p floe-app phase_6s_failure -- --nocapture
```

The native Wayland smoke uses isolated HOME/XDG roots and performs an external
two-file create burst, rename, and delete. Logs confirm one coalesced reload per
burst without recording paths; D-Bus health, 42 actions, clean quit, and name
release remain intact. After verified Phase 6S, continue only on
`phase-6t-browser-completeness`.

Phase 6T adds no dependency. Focused checks are:

```bash
cargo test -p floe-core phase_6t_sort_group -- --nocapture
cargo test -p floe-app phase_6t_metadata -- --nocapture
cargo test -p floe-app phase_6t_columns -- --nocapture
cargo test -p floe-app phase_6t_density -- --nocapture
cargo test -p floe-app phase_6t_preferences -- --nocapture
cargo test -p floe-app phase_6t_status -- --nocapture
```

The full gate passes 294 tests: 228 application and 66 core. Native Niri/Wayland
smoke used isolated HOME/XDG roots, exported 73 window actions, activated and
restored Compact density, Extension grouping, folders-last, MIME/Permissions
columns, and a widened Name column across two launches. Both instances answered
`Peer.Ping`, exited through the application Quit action, and released
`io.github.floe.FileManager`. Spectacle did not produce an image on this host;
action state, persistence, D-Bus health, and clean process lifecycle are the
runtime evidence. The first live resizing pass emitted transient one-pixel GTK
measurement warnings plus the documented RADV `VK_SUBOPTIMAL_KHR` warning; the
restored second launch emitted no warning. That checkpoint handed off to
`phase-7a-tabs-foundation`.

Phase 7A adds no dependency and no runtime UI path. Focused checks are:

```bash
cargo test -p floe-core phase_7a_view -- --nocapture
cargo test -p floe-core phase_7a_session -- --nocapture
cargo test -p floe-core phase_7a_codec -- --nocapture
```

The full gate passes 296 tests: 222 application and 74 core. Formatting,
workspace check, strict all-target/all-feature Clippy, and diff hygiene pass.
The application continues to compile against the moved canonical core view
types. No native Wayland smoke is claimed for Phase 7A because it adds no tab
widget, action, shortcut, persistence, application wiring, or other runtime UI
behavior. After verified Phase 7A, continue only on
`phase-7b-tabs-interaction`.

The post-7A grid-grouping correction adds no dependency. Focused
`grid_grouping` core/application tests verify dotted directories remain one
Folders section and list/grid use identical visible boundary labels. The full
gate passes 298 tests: 223 application and 75 core. Native Niri/Wayland smoke
activated Grid plus Group by Extension through exported actions, confirmed the
state and D-Bus health, quit cleanly, and released the application name.
Spectacle produced no file and the screenshot portal timed out, so no visual
capture is claimed.

Phase 7B adds no dependency. Focused checks are:

```bash
cargo test -p floe-core phase_7b_tabs -- --nocapture
cargo test -p floe-app phase_7b_ -- --nocapture
```

The full gate passes 304 tests: 226 application and 78 core, plus formatting,
workspace check, strict all-target/all-feature Clippy, and diff hygiene. A native
Niri/Wayland smoke exported and activated new/switch/reorder/duplicate/close tab
actions, answered `org.freedesktop.DBus.Peer.Ping`, quit cleanly, and released
`io.github.floe.FileManager`. Only the documented RADV/Vulkan swapchain warning
appeared. Spectacle again produced no file, so no visual capture is claimed.

Phase 7C adds no dependency. Focused checks are:

```bash
cargo test -p floe-core phase_7c_ -- --nocapture
cargo test -p floe-app phase_7c_ -- --nocapture
```

The full gate passes 312 tests: 230 application and 82 core, plus formatting,
workspace check, strict all-target/all-feature Clippy, and diff hygiene. A
two-launch isolated Niri/Wayland smoke saved two live tabs and closed-tab state
to a 0700/0600 file, restored multiple live tabs, observed enabled Reopen Closed
Tab, reopened and ran Close Other Tabs, remained D-Bus healthy, quit cleanly,
and released the application name. A third isolated launch with
`FLOE_SESSION_POLICY=private` removed the Floe-owned session file and did not
recreate it. Only documented RADV/Vulkan warnings appeared.

Normal clean-shutdown session state is stored at
`$XDG_CONFIG_HOME/floe/browser-session-v1.bin`. `FLOE_SESSION_POLICY=private`
or `sensitive` is an explicit integration/testing policy that suppresses this
owned trace; it is not a complete user-facing Private Mode and makes no claim
about other applications, the same-user processes, backups, or storage history.

Phase 7D adds no dependency. Its focused GTK-independent checks are:

```bash
cargo test -p floe-core phase_7d_ -- --nocapture
```

The version-2 workspace codec retains the existing file location and migrates
version-1 unsplit records. It adds bounded primary/secondary pane sessions,
active side, and ratio; Phase 7D intentionally exposes no GTK split control.

Phase 7E also adds no dependency. Its focused application checks are:

```bash
cargo test -p floe-app phase_7e -- --nocapture
```

Native verification should exercise the window-scoped toggle, side switch,
swap, close, narrow/widen, and opposite-pane action states over D-Bus, then
cleanly relaunch the same isolated configuration to confirm split restoration.
The inactive pane is a bounded snapshot; only the active pane owns the live
browser and watcher pipeline.

Phase 7F adds no dependency. Its focused application checks are:

```bash
cargo test -p floe-app phase_7f -- --nocapture
```

Native smoke verifies the exported Link to Other Pane action, split target
construction, D-Bus health, and clean shutdown. Automated focused tests perform
real copy/move/link jobs because synthetic Wayland drag serials are not a
reliable native-smoke input mechanism.

Phase 8A adds no dependency and no runtime GTK wiring. Its focused checks are:

```bash
cargo test -p floe-core phase_8a -- --nocapture
```

No native Wayland smoke is appropriate for the GTK-independent Phase 8A model.
Phase 8B adds no dependency. Its focused checks are:

```bash
cargo test --workspace phase_8b -- --nocapture
```

Native Wayland verification activates `win.view-miller`, adjusts width through
`win.widen-miller-columns`, confirms the application remains responsive over
D-Bus, quits through the application action, and checks the version-3 private
preference file records the clamped global width. The active column must keep
using the one existing browser result model and worker.

Phase 8C focused verification is:

```bash
cargo test -p floe-app phase_8c -- --nocapture
```

The native Wayland smoke activates Miller mode, describes and invokes the
logical `miller-parent`/`miller-child` actions, checks D-Bus health, quits, and
confirms name release. Policy tests cover LTR/RTL mapping, bounded item movement,
modified-key fallthrough, dominant-horizontal clamping, and reduced motion.

Phase 8D focused verification is:

```bash
cargo test -p floe-app phase_8d -- --nocapture
```

The native Wayland smoke activates Miller mode, confirms standard file and
creation actions remain exported with truthful sensitivity, pings the window
action object, and exits cleanly. Focused tests cover raw non-UTF-8 identity,
stale/overflow/wrong-parent rejection, pointer/keyboard menu policy, and
list/grid/Miller action parity.

Phase 8E focused verification is:

```bash
cargo test -p floe-app phase_8e -- --nocapture
```

The native Wayland smoke activates Miller mode, describes the explicit
copy/move/link-to-other-pane alternatives, pings the window action object, and
quits cleanly. Synthetic drag serials are not used for native automation;
focused tests cover exact raw sources, live destinations, typed stale hover
rejection, modifier-preserving requests, and two-axis clamping.

Phase 8F focused verification is:

```bash
cargo test -p floe-app phase_8f -- --nocapture
```

The native Wayland smoke activates Miller mode, verifies both detail-hook
actions are enabled, opens and closes the empty Preview surface, pings the
window action object, and quits cleanly. Tests cover raw target identity,
generation stability, stale/unsupported states, truthful provider boundaries,
accessible presentation text, and list/grid/Miller integration.

Phase 9A focused verification is:

```bash
cargo test -p floe-app phase_9a -- --nocapture
```

The native Wayland smoke activates Miller, opens/closes the Preview hook,
verifies its action remains enabled, pings the window action object, and quits
cleanly. Fake-provider tests cover raw identity, unsupported fallback,
memory-only reuse, queue pressure, stale requests, cancellation, provider
failure/panic containment, current-result lifecycle, and bounded GTK draining.

## Wayland environments

Phase 14 focused verification is:

```bash
cargo test -p floe-app phase_14_integration_model -- --nocapture
cargo test -p floe-app phase_14_integration_worker -- --nocapture
cargo test -p floe-app phase_14_generic_fallback -- --nocapture
cargo test -p floe-app phase_14_integration_ui -- --nocapture
```

The generic integration boundary is application-only and adds no dependency.
Runtime probing uses one capacity-one worker and GIO session-bus calls with a
1.5-second per-call timeout. It records only aggregate booleans in a memory-only
snapshot; do not add paths, notification contents, credentials, or compositor
types. Missing optional services must retain existing GIO launcher, device,
XDG-location, appearance, and local browsing behavior. Real GTK verification
uses the ignored `phase_14_integration_ui_real_gtk` component test under a live
display.

Phase 13E passes four focused core saved-query/history/order tests, three app
persistence/UI/order contract tests, the complete workspace suite, strict
Clippy, and the opt-in real GTK component gate. An isolated Plasma Wayland
launch exported `save-search`, `delete-saved-search`, and
`clear-recent-searches`; Clear Recent activated, D-Bus Peer.Ping remained
responsive, and Quit exited 0. The standalone AT-SPI bus remained unavailable,
so semantic automation beyond the real-widget component contract is not
claimed.

Phase 13G focused verification is:

```bash
cargo test -p floe-core phase_13g -- --nocapture
cargo test -p floe-app phase_13g -- --nocapture
```

The final suite passes 387 application tests (386 passed plus one intentional
graphical ignore) and 132 core tests with strict all-target/all-feature Clippy.
The real GTK component gate passes. An isolated Plasma Wayland launch exported
`check-duplicates` and `cancel-duplicate-scan`, answered
`org.freedesktop.DBus.Peer.Ping`, and exited status 0 through Quit. Standalone
AT-SPI remained unavailable, so the native claim is action/lifecycle plus the
real-widget component contract, not semantic AT-SPI E2E.

Phase 13F focused verification is:

```bash
cargo test -p floe-core phase_13f -- --nocapture
cargo test -p floe-app phase_13f -- --nocapture
```

The final strict workspace gate passes 384 application tests (383 passed plus
one intentional graphical ignore), 129 core tests, strict all-target/all-feature
Clippy, native build, and diff hygiene. The opt-in real GTK component gate
passes. An isolated Plasma Wayland launch exported `build-search-index` and
`clear-search-index`, built a mode-`0600` private cache, cleared it, answered
`org.freedesktop.DBus.Peer.Ping`, and exited status 0 through Quit. The host
accessibility bus was unavailable in the standalone launch, so Floe does not
claim semantic AT-SPI E2E beyond the real-widget component contract.

Phase 13D rebuilt Floe with isolated HOME/XDG config, data, cache, and state
roots in an active Plasma Wayland session. The live window exported the shared
search actions, accepted open/clear search lifecycle activation, answered D-Bus
`Peer.Ping` before and after, and exited status 0 through the application Quit
action. The opt-in real GTK component/accessibility gate passed. Focused core
and worker tests cover UTF-8/UTF-16, Text/Glob/Regex/case, advanced predicates,
binary/unsupported/over-limit skips, symlink exclusion, cancellation, bounded
batches, and generation supersession. The host accessibility bus was not
available to the standalone smoke, so semantic AT-SPI automation is not
claimed; the real-widget component gate supplies the accessibility contract.

Phase 13C rebuilt Floe and launched it with isolated HOME/XDG config, data,
cache, and state roots in the active Plasma Wayland session. The live window
exported and accepted `folder-filter`, `filename-search`,
`start-filename-search`, `stop-filename-search`, and `close-search-surface`;
`org.freedesktop.DBus.Peer.Ping` remained responsive and the exported
application `quit` action exited with status 0. The opt-in real GTK component
gate constructed the native search surface and verified advanced toggle,
dropdown, entry, Match Case, Apply, and Clear Filters roles/labels. Focused
tests cover combined predicates, predicate-only hidden search, invalid MIME and
ranges, unknown-metadata exclusion, lazy owner/MIME resolution, raw non-UTF-8
case behavior, and generation-safe capacity-one workers. Dogtail/AT-SPI native
E2E remains unavailable because the host does not provide the Python `dogtail`
module.

Phase 13B rebuilt Floe and launched it with isolated HOME/XDG config, data,
cache, and state roots in the active Wayland session. The live window exported
enabled `folder-filter`, `filename-search`, `start-filename-search`,
`stop-filename-search`, and `close-search-surface` actions; `reveal-in-folder`
was correctly disabled without a selected result. Search, Start, Stop, and Close
activated successfully. View and zoom actions changed from enabled to disabled
while the dedicated result surface was active and returned to enabled on Close.
`org.freedesktop.DBus.Peer.Ping` responded, and the application exited cleanly
through its exported Quit action. The host emitted only its known RADV and
unavailable accessibility-bus warnings. Focused tests
cover both scopes, raw non-UTF-8 names, symlink non-descent, mount-boundary
policy, cancellation, explicit truncation, bounded streaming/supersession,
unified UI feedback/actions, exact Reveal mapping, and the supplied icon
resource. Dogtail/AT-SPI native E2E is not claimed because those dependencies
remain unavailable on this host.

Phase 13A built the local application and launched it with isolated HOME/XDG config/data/cache roots in the active Wayland session. The rendered window exposed the visible Text/Glob/Regex filter row; native D-Bus described enabled `folder-filter` and `clear-folder-filter` actions, activating each succeeded, `org.freedesktop.DBus.Peer.Ping` responded, and application Quit exited cleanly. The host emitted only its documented RADV warning. Focused tests additionally cover invalid patterns, raw non-UTF-8 names, exact visible-selection restoration, latest-generation supersession, bounded queue pressure, and 100,000 loaded entries.

Phase 12E rebuilt the native app and repeated the active Wayland D-Bus lifecycle smoke after adding the accessible relative/absolute symbolic-link choice. Filesystem planning/execution remains in the typed core and bounded create executor.

Phase 12D built the native app, launched it in the active Wayland session, answered `org.freedesktop.DBus.Peer.Ping`, and exited through the exported Quit action. Focused tests cover the chooser policy; template filesystem work remains on the bounded worker/create executor rather than GTK callbacks.

Phase 12B native Wayland smoke built the local app, launched with isolated XDG
config/cache/data roots, verified the live window exported `extract-here`,
`extract-to`, and `compress`, confirmed Extract Here was disabled with no archive
selection, answered `org.freedesktop.DBus.Peer.Ping`, and exited 0 through the
application Quit action. The host emitted its documented RADV warning and an
existing GtkPaned focus warning during shutdown; neither affected lifecycle or
action state.

Phase 12C repeated the isolated native lifecycle and verified `batch-rename`
and `undo-batch-rename` were both exported and disabled without eligible
selection/completed history, answered D-Bus Ping, and exited 0 through Quit.

Run Floe inside an active graphical session with `WAYLAND_DISPLAY` and
`XDG_RUNTIME_DIR` set. The current native smoke tests run under Niri.

### Niri

No Niri IPC integration exists. Floe starts as a normal Wayland application
with app ID `io.github.floe.FileManager`. A missing `NIRI_SOCKET` is irrelevant
to current operation and must remain non-fatal when integration is added.

### KDE Plasma

No Plasma-specific API or KDE Framework dependency exists. Floe should launch
normally in a Plasma Wayland session using GTK/GIO behavior. Prefer XDG, GIO,
GLib, and portals before proposing a KDE-specific dependency.

## Phase 18T–18X integrity and data-loss safety verification

The implemented integrity actions are Save/Verify SHA-256 Fingerprint,
Generate/Verify `SHA256SUMS`, explicit baseline create/check/monitor controls,
Copy and Verify, Copy/Verify/Flush/Eject, and Protect/Unprotect/Protected
Folders. Hashing, scans, copy verification, mount flushing, and policy writes
run through bounded application workers; GTK callbacks do not perform the
filesystem work.

Focused deterministic verification is available with:

```bash
cargo test --workspace phase_18 -- --nocapture
node /home/rocappsec/.codex/skills/unlazy/scripts/gate-check.mjs --status \
  gates/phase-18t.md gates/phase-18u.md gates/phase-18v.md \
  gates/phase-18w.md gates/phase-18x.md
```

GTK contracts must run as separate filtered processes because GTK can only be
initialized from one test thread per process:

```bash
cargo test -p floe-app phase_testing_gtk_header_filter_and_operations_accessibility_contract -- --ignored --nocapture
cargo test -p floe-app phase_testing_gtk_phase_18x_guardrail_dialog_accessibility_contract -- --ignored --nocapture
```

Automated removable-transfer tests use mock/disposable targets only. Do not run
the workflow against user media for a gate. A real-device claim requires an
explicit disposable lab device. The current host lacks Dogtail/pyatspi, so the
native semantic E2E suite reports that dependency skip while its harness
contracts pass.

## Troubleshooting

### GTK or libadwaita not found

Errors mentioning `gtk4.pc`, `libadwaita-1.pc`, `pkg-config`, or a failed
`-sys` crate build usually mean the development library or `pkgconf` is absent
or outside `PKG_CONFIG_PATH`. Verify with the `pkg-config` command above before
changing Rust dependencies.

### Display cannot be opened

If GTK reports that no display is available, confirm the command is running
inside the desktop session and inspect:

```bash
env | rg '^(WAYLAND_DISPLAY|DISPLAY|XDG_RUNTIME_DIR|XDG_SESSION_TYPE)='
```

### libadwaita dark-theme warning

The current host may emit a warning that
`GtkSettings:gtk-application-prefer-dark-theme` is unsupported by libadwaita.
That setting comes from the host environment; Floe uses libadwaita semantic
theme colors and does not set it.

### Vulkan `VK_SUBOPTIMAL_KHR` warning

The current RADV/Wayland host may report a suboptimal swapchain after surface
changes. It has not prevented the smoke-tested window from rendering. Treat a
crash or persistent rendering defect separately from this warning.

### Dependency download failures

Cargo requires registry access the first time dependencies are resolved. Once
`Cargo.lock` and crate sources are present, normal local checks should not need
to modify system packages.
