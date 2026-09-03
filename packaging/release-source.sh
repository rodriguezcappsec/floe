#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)
version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$repo_root/Cargo.toml" | head -n 1)
output=${1-"$repo_root/dist/floe-$version.tar.gz"}
epoch=${SOURCE_DATE_EPOCH-1788037200}

case "$output" in
  /*) ;;
  *) output="$PWD/$output" ;;
esac
mkdir -p -- "$(dirname -- "$output")"
temporary="$output.tmp"
file_list="$temporary.files"
tar_file="$temporary.tar"
rm -f -- "$temporary" "$file_list" "$tar_file"

(
  cd -- "$repo_root"
  git ls-files -z --cached --others --exclude-standard -- \
    ':!packaging/arch/PKGBUILD' \
    ':!packaging/arch/*.tar.gz' ':!packaging/arch/*.pkg.tar.zst' \
    ':!packaging/arch/src' ':!packaging/arch/src/**' \
    ':!packaging/arch/pkg' ':!packaging/arch/pkg/**' \
    ':!AGENTS.md' ':!PLAN.md' ':!GATES.md' ':!gates' ':!gates/**' \
    ':!.agents' ':!.codex' \
    ':!**/__pycache__' ':!**/__pycache__/**' ':!**/*.pyc' |
    LC_ALL=C sort -z > "$file_list"
  tar --sort=name --mtime="@$epoch" --owner=0 --group=0 --numeric-owner \
    --transform="s,^,floe-$version/," --null --files-from="$file_list" \
    -cf "$tar_file"
)
gzip -n -9 < "$tar_file" > "$temporary"
rm -f -- "$file_list" "$tar_file"
mv -- "$temporary" "$output"
sha256sum -- "$output"
