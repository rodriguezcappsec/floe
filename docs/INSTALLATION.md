# Installing Floe

Related: [Getting Started](./GETTING_STARTED.md) ·
[Migrations](./MIGRATIONS.md) · [Administration](./ADMINISTRATION.md) ·
[Debugging](./DEBUGGING.md)

Floe currently provides one verified native Linux packaging path: an Arch Linux
package contract plus a manifest-driven source installer. Flatpak is deferred.
Floe needs broad local filesystem access, host GIO/GVfs services, installed
thumbnailers, terminals, and explicitly configured external tools; no truthful
Flatpak permission model has yet been implemented or tested.

The installed command is `floe` and the stable application identity is
`io.github.rodriguezcappsec.Floe`.

## Legal status

The project and application icon currently use `LicenseRef-proprietary` and are
all rights reserved. The repository owner has authorized the packaged project
asset for this build, but redistribution remains subject to owner review. The
vendored Phosphor icon subset has its own MIT notice in
`THIRD_PARTY_LICENSES/Phosphor-Icons.txt`.

## Runtime requirements

Floe requires Linux, GTK 4.14 or newer, libadwaita 1.5 or newer, and GLib/GIO.
On Arch Linux the package dependencies are `gtk4`, `libadwaita`, `glib2`, and
`hicolor-icon-theme`. `gvfs` enables additional GIO Trash, mount, and
administrator-location behavior. `ffmpeg` is optional for `ffprobe`-backed
advanced video metadata. Installed freedesktop thumbnailers remain optional
host providers and run with the signed-in user's ordinary authority.

## Build and stage from source

Use the committed lockfile and release profile:

```bash
cargo build --frozen --release -p floe-app --bin floe
```

Stage the exact package manifest without touching the live system:

```bash
staging_root=$(mktemp -d)
DESTDIR="$staging_root" PREFIX=/usr sh packaging/install.sh
find "$staging_root/usr" -type f -print
DESTDIR="$staging_root" PREFIX=/usr sh packaging/uninstall.sh
```

For a live source installation, use `PREFIX=/usr/local` or `/usr` with the
privilege required by that prefix. The installer rewrites the optional portal
D-Bus service `Exec=` entry to the selected prefix, so activation resolves the
same installed `floe` binary under either layout. Afterward refresh desktop/icon
caches with the distribution's normal package hooks or cache tools. The
installer never sets Floe as the default directory handler and never writes to
`HOME` or user XDG roots.

The installed command is `floe`. It accepts no target or exactly one local
folder/file URI. The desktop entry advertises only `inode/directory` and uses
`floe %u`; regular-file command-line targets remain an explicit reveal workflow,
not a claim that Floe handles every MIME type.

## Arch release source and package

Create the deterministic release archive, place it beside the PKGBUILD, and
ensure its SHA-256 matches `packaging/arch/PKGBUILD`:

```bash
sh packaging/release-source.sh dist/floe-0.1.0.tar.gz
cp dist/floe-0.1.0.tar.gz packaging/arch/
(cd packaging/arch && makepkg --cleanbuild --nodeps)
```

The current host verifies ordinary `makepkg`; Arch `devtools` clean-chroot
commands and `namcap` are unavailable and therefore not claimed. Cargo builds
use `--frozen`, retain panic unwinding for preview-provider failure containment,
and produce the PIE executable `target/release/floe`.

## Uninstall and retained user data

Arch package removal owns only the installed binary, desktop/AppStream files,
hicolor icon, notices, and installed documentation. Source uninstall uses the
same exact manifest:

```bash
PREFIX=/usr/local sh packaging/uninstall.sh
```

Uninstall intentionally retains settings, bookmarks, session state, operation
recovery, integrity records, and caches. Review [Migrations](./MIGRATIONS.md)
before manually removing or rolling back user state.
