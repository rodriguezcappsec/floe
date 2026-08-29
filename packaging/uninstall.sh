#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
manifest="$script_dir/install-manifest.txt"
destdir=${DESTDIR-}
prefix=${PREFIX-/usr/local}

case "$prefix" in
  /*) ;;
  *) echo "PREFIX must be an absolute path" >&2; exit 2 ;;
esac
case "$destdir" in
  ""|/*) ;;
  *) echo "DESTDIR must be empty or an absolute path" >&2; exit 2 ;;
esac

while IFS='|' read -r source destination mode; do
  case "$source" in ""|'#'*) continue ;; esac
  case "$destination" in
    /*|*'..'*) echo "unsafe uninstall destination: $destination" >&2; exit 2 ;;
  esac
  rm -f -- "$destdir$prefix/$destination"
done < "$manifest"

echo "Removed Floe package files below $destdir$prefix; user data was retained"
