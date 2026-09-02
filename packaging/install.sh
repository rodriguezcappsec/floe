#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)
manifest="$script_dir/install-manifest.txt"
destdir=${DESTDIR-}
prefix=${PREFIX-/usr/local}

case "$prefix" in
    /*) ;;
    *) echo "PREFIX must be an absolute path" >&2; exit 2 ;;
esac
case "$prefix" in
    *'
'*) echo "PREFIX must not contain a newline" >&2; exit 2 ;;
esac
case "$destdir" in
  ""|/*) ;;
  *) echo "DESTDIR must be empty or an absolute path" >&2; exit 2 ;;
esac

while IFS='|' read -r source destination mode; do
  case "$source" in ""|'#'*) continue ;; esac
  case "$destination" in
    /*|*'..'*) echo "unsafe install destination: $destination" >&2; exit 2 ;;
  esac
  install -Dm"$mode" -- "$repo_root/$source" "$destdir$prefix/$destination"
done < "$manifest"

# D-Bus activates the installed binary directly, so a staged /usr/local
# installation must not retain the source package's /usr release prefix.
portal_service="$destdir$prefix/share/dbus-1/services/org.freedesktop.impl.portal.desktop.floe.service"
portal_service_tmp=$(mktemp "${TMPDIR-/tmp}/floe-portal-service.XXXXXX")
trap 'rm -f -- "$portal_service_tmp"' EXIT HUP INT TERM
awk -v executable="$prefix/bin/floe --portal-filechooser-backend" '
    /^Exec=/ { print "Exec=" executable; next }
    { print }
' "$portal_service" > "$portal_service_tmp"
install -m0644 -- "$portal_service_tmp" "$portal_service"
rm -f -- "$portal_service_tmp"
trap - EXIT HUP INT TERM

echo "Installed Floe below $destdir$prefix"
