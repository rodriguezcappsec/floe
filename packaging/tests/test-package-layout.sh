#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
temporary=$(mktemp -d)
trap 'rm -rf -- "$temporary"' EXIT HUP INT TERM
cargo_home=${CARGO_HOME-"$HOME/.cargo"}
rustup_home=${RUSTUP_HOME-"$HOME/.rustup"}

export HOME="$temporary/home"
export XDG_CONFIG_HOME="$temporary/config"
export XDG_CACHE_HOME="$temporary/cache"
export XDG_DATA_HOME="$temporary/data"
export XDG_STATE_HOME="$temporary/state"
export XDG_RUNTIME_DIR="$temporary/runtime"
export CARGO_HOME="$cargo_home"
export RUSTUP_HOME="$rustup_home"
mkdir -m 700 -p "$HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" \
  "$XDG_DATA_HOME" "$XDG_STATE_HOME" "$XDG_RUNTIME_DIR"
printf '%s\n' phase-21b-config-sentinel > "$XDG_CONFIG_HOME/sentinel"

if test ! -x "$repo_root/target/release/floe"; then
  (cd "$repo_root" && cargo build --frozen --release -p floe-app --bin floe)
fi

DESTDIR="$temporary/root" PREFIX=/usr sh "$repo_root/packaging/install.sh"

while IFS='|' read -r source destination mode; do
  case "$source" in ""|'#'*) continue ;; esac
  installed="$temporary/root/usr/$destination"
  test -f "$installed"
  test "$(stat -c '%a' "$installed")" = "${mode#0}"
done < "$repo_root/packaging/install-manifest.txt"

for document in \
  README.md SECURITY.md CHANGELOG.md \
  docs/GETTING_STARTED.md docs/USER_GUIDE.md docs/PHILOSOPHY.md docs/ADMINISTRATION.md \
  docs/ACCESSIBILITY.md docs/RECOVERY.md docs/DEBUGGING.md \
  docs/LOCALIZATION.md docs/INSTALLATION.md docs/MIGRATIONS.md \
  docs/PERFORMANCE.md docs/RELEASE_MATRIX.md docs/PRIVACY_SECURITY.md; do
  test -f "$temporary/root/usr/share/doc/floe/$document"
done

desktop-file-validate \
  "$temporary/root/usr/share/applications/io.github.rodriguezcappsec.Floe.desktop"
appstreamcli validate --no-net \
  "$temporary/root/usr/share/metainfo/io.github.rodriguezcappsec.Floe.metainfo.xml"
grep -Fxq 'Name=org.freedesktop.impl.portal.desktop.floe' \
  "$temporary/root/usr/share/dbus-1/services/org.freedesktop.impl.portal.desktop.floe.service"
grep -Fxq 'Exec=/usr/bin/floe --portal-filechooser-backend' \
  "$temporary/root/usr/share/dbus-1/services/org.freedesktop.impl.portal.desktop.floe.service"
grep -Fxq 'Interfaces=org.freedesktop.impl.portal.FileChooser;' \
  "$temporary/root/usr/share/xdg-desktop-portal/portals/floe.portal"
grep -Fxq 'UseIn=floe' \
  "$temporary/root/usr/share/xdg-desktop-portal/portals/floe.portal"
test "$(rg -c '^MimeType=' "$temporary/root/usr/share/applications/io.github.rodriguezcappsec.Floe.desktop")" -eq 1
rg -q '^MimeType=inode/directory;$' \
  "$temporary/root/usr/share/applications/io.github.rodriguezcappsec.Floe.desktop"
test "$(cat "$XDG_CONFIG_HOME/sentinel")" = phase-21b-config-sentinel
test ! -e "$XDG_CONFIG_HOME/mimeapps.list"

# The service activation command must follow both supported installation
# prefixes; /usr/local is the install script's default.
DESTDIR="$temporary/root-local" "$repo_root/packaging/install.sh"
while IFS='|' read -r source destination mode; do
    case "$source" in ""|'#'*) continue ;; esac
    installed="$temporary/root-local/usr/local/$destination"
    test -f "$installed"
    test "$(stat -c '%a' "$installed")" = "${mode#0}"
done < "$repo_root/packaging/install-manifest.txt"
grep -Fxq 'Exec=/usr/local/bin/floe --portal-filechooser-backend' \
    "$temporary/root-local/usr/local/share/dbus-1/services/org.freedesktop.impl.portal.desktop.floe.service"
DESTDIR="$temporary/root-local" "$repo_root/packaging/uninstall.sh"
while IFS='|' read -r source destination mode; do
    case "$source" in ""|'#'*) continue ;; esac
    test ! -e "$temporary/root-local/usr/local/$destination"
done < "$repo_root/packaging/install-manifest.txt"

DESTDIR="$temporary/root" PREFIX=/usr sh "$repo_root/packaging/uninstall.sh"
while IFS='|' read -r source destination mode; do
  case "$source" in ""|'#'*) continue ;; esac
  test ! -e "$temporary/root/usr/$destination"
done < "$repo_root/packaging/install-manifest.txt"
test "$(cat "$XDG_CONFIG_HOME/sentinel")" = phase-21b-config-sentinel

sh -n "$repo_root/packaging/arch/PKGBUILD"
rg -q "^pkgname=floe$" "$repo_root/packaging/arch/PKGBUILD"
rg -q "cargo build --frozen --release -p floe-app --bin floe" \
  "$repo_root/packaging/arch/PKGBUILD"

echo phase-21b-package-layout-ok
