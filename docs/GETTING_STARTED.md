# Getting Started with Floe

This walkthrough uses the installed `floe` command from the verified Arch or
manifest-driven native package. See [Installation](./INSTALLATION.md) first.
Floe currently targets Linux Wayland sessions.

## First launch

```bash
floe
floe /path/to/folder
floe /path/to/file.pdf
```

Floe is single-instance. Each invocation accepts at most one local target;
remote URIs and multiple targets are rejected explicitly.

## Safe walkthrough

Create a disposable folder:

```bash
walkthrough_root=$(mktemp -d)
mkdir -p "$walkthrough_root/Folder A"
printf '%s\n' 'Welcome to Floe' > "$walkthrough_root/readme.txt"
floe "$walkthrough_root"
```

Then:

1. Press `Ctrl+1` and `Ctrl+2` for List and Grid views.
2. Open **Folder A**, then use `Alt+Left`, `Alt+Right`, and `Alt+Up`.
3. Press `Ctrl+L`, inspect the path, and press `Escape`.
4. Press `Ctrl+F`, search for `readme`, then close Search.
5. Select `readme.txt` and press `Space` for Quick Preview.
6. Press `F2` and rename it to `notes.txt`.
7. Exercise Trash/restore only inside an isolated test Trash.
8. Press `Ctrl+,` and inspect Browsing and Accessibility settings.
9. Quit with `Ctrl+Q`, relaunch, and check implemented preferences restore.

The automated release walkthrough uses private temporary HOME, XDG, runtime,
and Trash roots. Never use personal files for a release gate.

## Entry points

- `Ctrl+Shift+P`: Command Palette.
- `Ctrl+?`: Keyboard Shortcuts.
- `Ctrl+,`: Settings.
- `Shift+F10`: context menu.
- `Alt+Enter`: Properties.
- `Ctrl+I`: Inspector.

See the [User Guide](./USER_GUIDE.md) for complete workflows.

## Current limitations

Generic GTK/GIO/XDG Wayland behavior is implemented. Niri-specific,
Plasma-specific, remote/network, and Android/MTP integrations are deferred.
Flatpak is not implemented. Floe is English-only with partial RTL foundations.
Provider helpers are not sandboxed. Encrypted Vault, Sensitive Folder, Private
Mode, Open Safely, Secure Share, and portable encryption are unavailable.
Permanent deletion is not secure erase, Protected Folder is only an
accidental-change guardrail, and recovery is not a backup or rollback guarantee.
