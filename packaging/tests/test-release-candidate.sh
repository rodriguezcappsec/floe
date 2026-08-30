#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT HUP INT TERM

first="$work/floe-first.tar.gz"
second="$work/floe-second.tar.gz"

SOURCE_DATE_EPOCH=1788037200 "$repo_root/packaging/release-source.sh" "$first"
SOURCE_DATE_EPOCH=1788037200 "$repo_root/packaging/release-source.sh" "$second"
cmp "$first" "$second"

sha256sum "$first" > "$work/SHA256SUMS"
sha256sum -c "$work/SHA256SUMS"

tar -tzf "$first" > "$work/manifest.txt"
test -s "$work/manifest.txt"
test "$(sort "$work/manifest.txt" | uniq -d | wc -l)" -eq 0
rg -q '/Cargo.lock$' "$work/manifest.txt"
rg -q '/packaging/release-policy.json$' "$work/manifest.txt"
rg -q '/scripts/check-release-candidate.py$' "$work/manifest.txt"

echo "phase-21d-release-candidate-ok files=$(wc -l < "$work/manifest.txt") sha256=$(cut -d ' ' -f 1 "$work/SHA256SUMS")"
