# Administering Floe

Floe is a per-user desktop application with no system daemon. Do not launch it
as root. Installation places application files below a selected prefix; user
state is created only when the signed-in user runs Floe.

## Deployment boundary

- Linux Wayland and generic GTK/GIO/XDG behavior are implemented.
- Niri-specific, Plasma-specific, remote/network, and Android/MTP backends are
  deferred.
- Native Arch and manifest-driven installation are verified. Flatpak is not.

The stable application ID is `io.github.rodriguezcappsec.Floe`; the command is
`floe`. Installation includes desktop, AppStream, and hicolor icon metadata. It
does not set MIME defaults or edit `mimeapps.list`.

GTK 4.14, libadwaita 1.5, GLib/GIO, and hicolor icons are required. `gvfs`
enables additional Trash, mount, and administrator-location behavior. FFmpeg's
`ffprobe` is optional for video metadata. Freedesktop thumbnailers are optional
and run with the user's ordinary authority.

## User-owned data

Package operations never scan or migrate HOME or XDG roots. Floe performs
bounded migration after launch as the signed-in user. Do not centrally delete
recovery, guardrail, or integrity records during upgrade. See
[Migrations](./MIGRATIONS.md).

## Experimental administrator browsing

**Open as Administrator…** is opt-in and read-only. It delegates one local
folder to the desktop GVfs `admin` backend and polkit agent. Floe remains the
normal user's process and never receives the password. Preview, terminals,
archives, external actions, clipboard operations, and every mutation are
disabled. Missing GVfs/polkit support is a normal unavailable capability. Never
wrap Floe in `sudo` or `pkexec`.

The read-only boundary is deliberate: temporary elevated authority must not
leak into ordinary jobs or the rest of the application. See
[Floe Philosophy](./PHILOSOPHY.md) for the user-facing rationale.

## External tools and diagnostics

Custom actions and terminals are ordinary user processes. Direct argument
vectors avoid shell interpolation, but external programs retain their own
authority. Logs and technical details may contain sensitive paths; review and
redact them before sharing. Floe has no telemetry or automatic diagnostic
upload. See [Debugging](./DEBUGGING.md).
