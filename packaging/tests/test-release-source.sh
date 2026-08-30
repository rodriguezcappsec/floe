#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
temporary=$(mktemp -d)
trap 'rm -rf -- "$temporary"' EXIT HUP INT TERM

version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$repo_root/Cargo.toml" | head -n 1)
first="$temporary/floe-$version-first.tar.gz"
second="$temporary/floe-$version-second.tar.gz"

sh "$repo_root/packaging/release-source.sh" "$first" >/dev/null
sh "$repo_root/packaging/release-source.sh" "$second" >/dev/null
cmp "$first" "$second"

archive_files="$temporary/archive-files.txt"
tar -tzf "$first" > "$archive_files"
for required in \
  README.md SECURITY.md CHANGELOG.md \
  docs/GETTING_STARTED.md docs/USER_GUIDE.md docs/PHILOSOPHY.md docs/ADMINISTRATION.md \
  docs/ACCESSIBILITY.md docs/RECOVERY.md docs/DEBUGGING.md \
  docs/LOCALIZATION.md docs/INSTALLATION.md docs/MIGRATIONS.md \
  docs/PERFORMANCE.md docs/PRIVACY_SECURITY.md \
  scripts/check-docs.py scripts/test_check_docs.py scripts/render-docs.sh \
  e2e/test_release_walkthrough.py; do
  grep -Fxq "floe-$version/$required" "$archive_files"
done

checksum=$(sha256sum "$first" | cut -d ' ' -f 1)
grep -Fq "sha256sums=('$checksum')" "$repo_root/packaging/arch/PKGBUILD"

echo "phase-21c-release-source-ok sha256=$checksum"
