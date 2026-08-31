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

## Experimental administrator access

**Open as Administrator…** is opt-in. It delegates one local folder to the
desktop GVfs `admin` backend and polkit agent. Floe remains the normal user's
process and never receives the password. The separate view offers explicitly
confirmed New Folder, Rename, single-file Copy/Move, Trash, permanent delete,
and Unix-mode operations. Destinations fail if they exist; links are not
followed, Trash never falls back to deletion, and Return is blocked until an
active operation reaches a terminal result.

Recursive administrator copy, ownership, ACL/xattr/capability/immutable edits,
previews, terminals, archives, Open With, clipboard actions, and external tools
remain unavailable. Missing GVfs/polkit or backend write support is a normal
unavailable capability. Never wrap Floe in `sudo` or `pkexec`. The separate
typed boundary keeps elevated authority out ordinary jobs and the rest of the
application. See
[Floe Philosophy](./PHILOSOPHY.md) for the user-facing rationale.

## Optional XDG FileChooser backend

The package installs `floe.portal` and the D-Bus activation service
`org.freedesktop.impl.portal.desktop.floe`, but does not make Floe the default
chooser. To opt in for one user, add this explicit preference:

```ini
# ~/.config/xdg-desktop-portal/portals.conf
[preferred]
org.freedesktop.impl.portal.FileChooser=floe
```

Then end applications using the portal and restart the user
`xdg-desktop-portal` service, or sign out and back in. Desktop-specific portal
configuration may override the generic file. Remove the line to return to the
desktop's previous backend.

The backend is local-file only. It supports Open File, one Select Folder, Save
File, Save Files, multiple-file opening, current folder/name, modal Wayland
parent handles, request Close/cancellation, and exact normalized `file://`
results. It returns portal response 2 for nonempty filters/choices, multiple
folder selection, X11 parent identifiers, malformed options, or capacity
pressure. It does not issue Document Portal grants: `xdg-desktop-portal` remains
responsible for any access it subsequently grants to the sandboxed caller.
Directly running `floe --portal-filechooser-backend` is intended for D-Bus
activation, diagnostics, and tests, not ordinary file management.

## External tools and diagnostics

Custom actions and terminals are ordinary user processes. Direct argument
vectors avoid shell interpolation, but external programs retain their own
authority. Logs and technical details may contain sensitive paths; review and
redact them before sharing. Floe has no telemetry or automatic diagnostic
upload. See [Debugging](./DEBUGGING.md).
