# Debugging Floe

Floe writes structured tracing to standard error. It has no telemetry,
automatic crash upload, or automatic diagnostic bundle.

## Logging

The default filter is `floe_app=info,floe_core=info`:

```bash
RUST_LOG=floe_app=debug,floe_core=debug floe
```

Logs, worker errors, and in-app technical details may contain sensitive paths,
filenames, command outcomes, or provider messages. They do not intentionally
log file contents or authentication secrets, but review and redact output before
sharing it. Do not leave debug logging enabled longer than necessary.

## Environment checks

```bash
floe --version
pkg-config --modversion gtk4 libadwaita-1
env | rg '^(WAYLAND_DISPLAY|DISPLAY|XDG_RUNTIME_DIR|XDG_SESSION_TYPE)='
```

Never publish the complete environment. It may contain tokens and session data.

If GTK cannot open a display, verify the process runs in the intended Wayland
session. Missing previews can reflect optional provider absence; helpers are
supervised but not sandboxed. A blocked mutation may require review in
**Operation Recovery…**, not deletion of source or destination files.

Automated reproduction must use temporary HOME, XDG config/cache/data/state,
runtime, and Trash roots. Never test against real HOME, Trash, folders, or
mounts. Reports should contain minimal synthetic steps and the smallest redacted
log excerpt. Security-sensitive reports follow [Security](../SECURITY.md).
