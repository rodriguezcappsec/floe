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

for root in "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME" "$XDG_STATE_HOME"; do
  printf '%s\n' "phase-21b-sentinel" > "$root/sentinel"
done

(cd "$repo_root" && cargo test -p floe-app phase_21b_migration -- --nocapture)

rg -q 'view-preferences.conf.pre-v18-legacy' "$repo_root/docs/MIGRATIONS.md"
rg -q 'search-index-v1' "$repo_root/docs/MIGRATIONS.md"
rg -q 'duplicate-hashes-v1' "$repo_root/docs/MIGRATIONS.md"
rg -q 'sort-metadata-v1' "$repo_root/docs/MIGRATIONS.md"
rg -q 'no encrypted vault implementation' "$repo_root/docs/MIGRATIONS.md"

for root in "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME" "$XDG_STATE_HOME"; do
  test "$(cat "$root/sentinel")" = "phase-21b-sentinel"
done
test ! -e "$XDG_DATA_HOME/floe/vault"

echo phase-21b-migrations-ok
