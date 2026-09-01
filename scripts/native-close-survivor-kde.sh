#!/usr/bin/env bash
set -euo pipefail

application_name="io.github.rodriguezcappsec.Floe"
application_path="/io/github/rodriguezcappsec/Floe"
kwin_script_name="floe_phase23_close"
repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
kwin_script="$repository_root/scripts/native-close-survivor-kde.js"
floe_binary="$repository_root/target/debug/floe"
floe_test_root="$(mktemp -d /tmp/floe-native-close-session.XXXXXX)"
floe_pid=""

cleanup() {
    qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.unloadScript \
        "$kwin_script_name" >/dev/null 2>&1 || true
    if [[ -n "$floe_pid" ]] && kill -0 "$floe_pid" >/dev/null 2>&1; then
        kill -TERM "$floe_pid" >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

for dependency in gdbus qdbus6 rg; do
    command -v "$dependency" >/dev/null || {
        printf 'missing native smoke dependency: %s\n' "$dependency" >&2
        exit 2
    }
done
[[ -x "$floe_binary" ]] || {
    printf 'build Floe first: cargo build -p floe-app --bin floe\n' >&2
    exit 2
}

if gdbus call --session --dest "$application_name" --object-path \
    "$application_path" --method org.freedesktop.DBus.Peer.Ping >/dev/null 2>&1; then
    printf 'refusing to run while another Floe process owns the session-bus name\n' >&2
    exit 2
fi

mkdir -p \
    "$floe_test_root/home" \
    "$floe_test_root/config" \
    "$floe_test_root/cache" \
    "$floe_test_root/data" \
    "$floe_test_root/state"

env \
    HOME="$floe_test_root/home" \
    XDG_CONFIG_HOME="$floe_test_root/config" \
    XDG_CACHE_HOME="$floe_test_root/cache" \
    XDG_DATA_HOME="$floe_test_root/data" \
    XDG_STATE_HOME="$floe_test_root/state" \
    "$floe_binary" >"$floe_test_root/floe.log" 2>&1 &
floe_pid=$!

application_ready=false
for _attempt in {1..120}; do
    if gdbus call --session --dest "$application_name" --object-path \
        "$application_path" --method org.gtk.Actions.List >/dev/null 2>&1; then
        application_ready=true
        break
    fi
    sleep 0.05
done
[[ "$application_ready" == true ]] || {
    printf 'Floe did not export its application actions\n' >&2
    exit 1
}

gdbus call --session --dest "$application_name" --object-path \
    "$application_path" --method org.gtk.Actions.Activate \
    new-window '[]' '{}' >/dev/null

window_alive() {
    local window_id="$1"
    gdbus call --session --dest "$application_name" --object-path \
        "$application_path/window/$window_id" \
        --method org.freedesktop.DBus.Peer.Ping >/dev/null 2>&1
}

second_ready=false
for _attempt in {1..120}; do
    if window_alive 1 && window_alive 2; then
        second_ready=true
        break
    fi
    sleep 0.05
done
[[ "$second_ready" == true ]] || {
    printf 'Floe did not expose two live native windows\n' >&2
    exit 1
}

qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.unloadScript \
    "$kwin_script_name" >/dev/null 2>&1 || true
qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.loadScript \
    "$kwin_script" "$kwin_script_name" >/dev/null
qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.start

# GTK may retain an exported action-group object briefly after its native window
# is gone, so object-path disappearance is not a reliable close signal. Exercise
# the main loop repeatedly across the teardown interval instead, then require a
# fresh third native window to be constructed after the close.
for _attempt in {1..40}; do
    gdbus call --session --dest "$application_name" --object-path \
        "$application_path" --method org.freedesktop.DBus.Peer.Ping >/dev/null
    sleep 0.05
done
gdbus call --session --dest "$application_name" --object-path \
    "$application_path/window/1" --method org.gtk.Actions.Activate \
    refresh '[]' '{}' >/dev/null
gdbus call --session --dest "$application_name" --object-path \
    "$application_path" --method org.gtk.Actions.Activate \
    new-window '[]' '{}' >/dev/null

third_ready=false
for _attempt in {1..120}; do
    if window_alive 3; then
        third_ready=true
        break
    fi
    sleep 0.05
done
[[ "$third_ready" == true ]] || {
    printf 'Floe stopped constructing windows after one native window closed\n' >&2
    exit 1
}

gdbus call --session --dest "$application_name" --object-path \
    "$application_path" --method org.gtk.Actions.Activate quit '[]' '{}' \
    >/dev/null 2>&1 || true

exited=false
for _attempt in {1..120}; do
    if ! kill -0 "$floe_pid" >/dev/null 2>&1; then
        exited=true
        break
    fi
    sleep 0.05
done
[[ "$exited" == true ]] || {
    printf 'Floe did not quit after the close-survivor test\n' >&2
    exit 1
}
wait "$floe_pid"
floe_pid=""

# Restart with the same private roots. Two live workspaces remained at clean
# shutdown (the survivor and the third window), so both must be restored.
env \
    HOME="$floe_test_root/home" \
    XDG_CONFIG_HOME="$floe_test_root/config" \
    XDG_CACHE_HOME="$floe_test_root/cache" \
    XDG_DATA_HOME="$floe_test_root/data" \
    XDG_STATE_HOME="$floe_test_root/state" \
    "$floe_binary" >>"$floe_test_root/floe.log" 2>&1 &
floe_pid=$!

restored_windows=false
for _attempt in {1..120}; do
    if window_alive 1 && window_alive 2; then
        restored_windows=true
        break
    fi
    sleep 0.05
done
[[ "$restored_windows" == true ]] || {
    printf 'Floe did not restore both surviving native windows\n' >&2
    exit 1
}

gdbus call --session --dest "$application_name" --object-path \
    "$application_path" --method org.gtk.Actions.Activate quit '[]' '{}' \
    >/dev/null 2>&1 || true
restored_exited=false
for _attempt in {1..120}; do
    if ! kill -0 "$floe_pid" >/dev/null 2>&1; then
        restored_exited=true
        break
    fi
    sleep 0.05
done
[[ "$restored_exited" == true ]] || {
    printf 'Restored Floe process did not quit cleanly\n' >&2
    exit 1
}
wait "$floe_pid"
floe_pid=""

if rg -n 'Finalizing GtkEntry|children left' "$floe_test_root/floe.log"; then
    printf 'GTK reported an owned transient during window finalization\n' >&2
    exit 1
fi

printf 'PASS close-survivor-responsive=true third-window=true restored-windows=true log=%s\n' \
    "$floe_test_root/floe.log"
