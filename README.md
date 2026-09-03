<div align="center">

<img src="./flow_logo_transparent.png" alt="Floe — blue folder crossed by a flowing river" width="620">

<h3>A spatial file manager for Wayland</h3>

<p>Native Linux file management with flexible views, deep keyboard workflows,<br>bounded background jobs, and exact path handling in Rust.</p>

</div>

# Floe

Floe is a GTK4/libadwaita file manager for Linux Wayland desktops. It combines
virtualized List and Grid views, split panes, spatial Miller columns, background
file operations, Quick Preview, search, archives, integrity tools, and
configurable keyboard workflows. Exact Linux paths remain authoritative even
when a filename cannot be displayed as valid UTF-8.

> **Alpha software:** Floe is under active development. Back up important data,
> review destructive operations carefully, and read the current limitations
> before relying on it as a daily file manager.

> **License:** Floe is source-available but proprietary software. The source is
> public for transparency, auditing, learning, issue investigation, and
> contribution through the official repository. Public access does not grant
> general permission to redistribute, repackage, sell, or publish forks or
> derivative versions. See [LICENSE](./LICENSE).

Floe follows a documented product philosophy: user ownership, exact path
identity, narrow authority, local-first processing, responsive bounded work,
standards-based integration, and honest security language. Read
[Why Floe works this way](./docs/PHILOSOPHY.md).

The implemented desktop path uses generic GTK/GIO/XDG facilities. Niri and KDE
Plasma are first-class product targets, but compositor-specific integrations are
deferred and are never required by the filesystem core.

## Highlights

- Virtualized List and Grid views plus bounded spatial Miller columns.
- Tabs, split panes, breadcrumbs, session restoration, and multiple windows.
- Copy, move, rename, duplicate, links, templates, drag and drop, Trash,
  restore, permanent deletion, conflicts, cancellation, and private 30-day
  Undo/Redo for supported local operations.
- Successful local operations reveal and briefly emphasize their exact output
  when its destination is already visible.
- Quick Preview, Inspector, Properties, permissions, metadata, checksums,
  archives, unified search, an optional local filename index, and exact duplicate
  review.
- Integrity fingerprints and manifests, verified copy/transfer, Protected Folder
  guardrails, and conservative interrupted-operation recovery review.
- Five appearance presets, icon choices, scalable text, density controls,
  reduced motion, customizable shortcuts, optional Vim navigation, and native
  contextual help with accessibility descriptions.
- Explainable suspicious-file, metadata, and Unix permission inspection;
  optional local `clamd` scanning with configurable bounds; source-preserving
  JPEG/PNG/WebP sanitized copies; and Bubblewrap-isolated external thumbnail and
  preview providers.

Long-running safety and inspection work remains visible in Background Activity,
with cancellation, results, reveal, and dismiss controls as appropriate.

## Install and run

Floe currently has one verified packaging strategy: an Arch package contract and
a manifest-driven native installer. Flatpak is not implemented. See
[Installation](./docs/INSTALLATION.md) for staging, packaging, optional runtime
dependencies, and uninstall behavior.

After installation:

```bash
floe
floe /path/to/folder
floe /path/to/file.pdf
```

The stable application ID is `io.github.rodriguezcappsec.Floe`. Each normal
invocation accepts at most one local target; remote URIs are rejected.

To build and run from a checkout on Arch Linux or CachyOS:

```bash
sudo pacman -S --needed rust gtk4 libadwaita pkgconf
cargo run -p floe-app
```

Rust 1.85+, GTK 4.14+, and libadwaita 1.5+ are required. Other distributions use
different development-package names; see
[Developing Floe](./docs/DEVELOPMENT.md).

## Local file-selector mode

Floe can act as a native local file selector for applications and scripts:

```bash
floe --choose-open [--multiple] [--initial-directory /path]
floe --choose-folder [--initial-directory /path]
floe --choose-save [--initial-directory /path] [--suggested-name report.txt]
```

Accepted paths are printed as one exact percent-encoded local `file://` URI per
line. Cancel emits no path and returns nonzero. Each selector is an independent
process, so simultaneous callers do not collide.

Floe also ships an optional XDG FileChooser backend. Installation does not select
it automatically; administrators or users must explicitly choose `floe` in
`portals.conf`. The current backend supports local Open File, single Select
Folder, Save File, and Save Files requests, including multiple file opening,
modal Wayland parent handles, current folders/names, cancellation, and exact URI
results. Unsupported nonempty filters/choices, multiple-folder requests, and X11
parents fail explicitly. Read [Administration](./docs/ADMINISTRATION.md) before
enabling it.

## Keyboard entry points

| Shortcut | Action |
| --- | --- |
| `Ctrl+Shift+P` | Command Palette |
| `Ctrl+,` | Settings |
| `Ctrl+?` | Keyboard Shortcuts |
| `Ctrl+L` | Edit location |
| `Ctrl+F` | Unified Search |
| `Ctrl+N` | New window |
| `Ctrl+T` / `Ctrl+W` | New / close tab |
| `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | Copy / cut / paste |
| `F2` | Rename |
| `Delete` / `Shift+Delete` | Trash / permanent-delete confirmation |
| `Space` | Quick Preview |
| `Ctrl+I` / `Alt+Enter` | Inspector / Properties |

## Important limitations

- Niri-specific, Plasma-specific, remote/network, and Android/MTP integrations
  are deferred. Generic local Wayland behavior remains implemented.
- Flatpak is not implemented.
- Experimental administrator access is opt-in. Its separate view supports a
  bounded subset of explicitly confirmed operations; preview, external tools,
  archives, ownership, ACL/xattr, and recursive administrator copy remain
  unavailable. See [Privileged Access](./docs/PRIVILEGED_ACCESS.md).
- External thumbnail and Quick Preview providers require Bubblewrap and fail
  unavailable if their isolation boundary cannot start. Ordinary **Open** and
  **Open With** are normal desktop application launches and are not sandboxed by
  Floe.
- Encrypted Vault, Sensitive Folder, user-facing Private Mode, Open Safely,
  Secure Share, portable encryption, quarantine, and automatic malware deletion
  are unavailable.
- Protected Folder is an accidental-change guardrail, not encryption or access
  control. Permanent deletion is not secure erase.
- Hashes do not prove authenticity, authorship, malware safety, or trust.
- Recovery is conservative restart review, not a transaction, universal rollback
  guarantee, or backup.
- Floe is English-only with partial RTL foundations. Complete Orca, translated
  RTL, and physical multi-monitor/fractional-scale verification are unclaimed.
- Logs and technical errors may contain sensitive paths. Review and redact them
  before sharing.

Floe is not an antivirus product, sandbox for ordinary applications, backup
system, or general security boundary. Read [SECURITY.md](./SECURITY.md) and the
[Privacy and Security architecture](./docs/PRIVACY_SECURITY.md) for exact claims.

## Contributing

Outside contributions are welcome through pull requests in the official Floe
repository. Start with [CONTRIBUTING.md](./CONTRIBUTING.md), and read the
[Contributor License Agreement](./CLA.md) before submitting work. Contributors
retain copyright in work they personally author and grant the Floe project
copyright holder the rights needed to incorporate accepted contributions into
proprietary and commercial Floe versions.

Report non-sensitive bugs with the GitHub issue form. Report vulnerabilities
through GitHub Private Vulnerability Reporting when enabled; never disclose
credentials, personal files, sensitive paths, or active exploit details in a
public issue. See [SECURITY.md](./SECURITY.md).

## Documentation

- [Getting Started](./docs/GETTING_STARTED.md)
- [User Guide](./docs/USER_GUIDE.md)
- [Floe Philosophy](./docs/PHILOSOPHY.md)
- [Installation](./docs/INSTALLATION.md) and [Migrations](./docs/MIGRATIONS.md)
- [Administration](./docs/ADMINISTRATION.md)
- [Accessibility](./docs/ACCESSIBILITY.md)
- [Recovery](./docs/RECOVERY.md)
- [Debugging](./docs/DEBUGGING.md)
- [Localization and RTL](./docs/LOCALIZATION.md)
- [Security Policy](./SECURITY.md) and
  [Privacy/Security Architecture](./docs/PRIVACY_SECURITY.md)
- [Performance](./docs/PERFORMANCE.md)
- [Release environment matrix](./docs/RELEASE_MATRIX.md)
- [Feature Matrix](./docs/FEATURE_MATRIX.md) and [Roadmap](./docs/ROADMAP.md)
- [Public release checklist](./docs/PUBLIC_RELEASE_CHECKLIST.md)
- [Changelog](./CHANGELOG.md)

## Architecture and project status

```text
GTK4 / libadwaita UI
          |
Application commands and state
          |
Bounded workers and job managers
          |
GTK-independent filesystem core
```

Filesystem work does not belong in GTK callbacks. Read
[Architecture](./docs/ARCHITECTURE.md),
[Developing Floe](./docs/DEVELOPMENT.md), and [AGENTS.md](./AGENTS.md) before
changing the project.

Floe remains alpha software. Phase 23H multi-window runtime/session hardening,
Phase 18R Permission Audit, Phase 20C contextual help, and bounded local
privacy/safety tools are implemented. Phase 19A **Git awareness** remains the
sole recommended next feature. The [Roadmap](./docs/ROADMAP.md) is the sequencing
authority; code and tests determine completion.
