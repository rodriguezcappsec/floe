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

The default appearance is Frosted. Exercise another existing preset with:

```bash
FLOE_APPEARANCE=glass cargo run -p floe-app
```

Accepted values are `native`, `glass`, `frosted`, `minimal`, and `compact`.
Unknown values fall back to Frosted.

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
cargo test -p floe-app move_executor
cargo check -p floe-core
cargo tree -p floe-app --depth 1
```

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
release is pinned with default features disabled and only PNG/JPEG decoders
enabled; it adds no external image decoder system package requirement.

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

## Wayland environments

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
