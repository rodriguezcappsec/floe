<div align="center">

<img src="./flow_logo_transparent.png" alt="Floe — blue folder crossed by a flowing river" width="620">

<h3>A spatial file manager for Wayland</h3>

<p>Fast native Linux file management, flexible views, deep keyboard workflows,<br>and a safety-first Rust core.</p>

</div>

# Floe

Floe is a GTK4/libadwaita file manager for Linux Wayland desktops. It combines
List, Grid, split-pane, and spatial Miller views with background file jobs,
Quick Preview, search, archives, integrity tools, and configurable keyboard
workflows. Exact Linux paths remain authoritative even when a filename cannot
be displayed as valid UTF-8.

Floe's choices follow a documented product philosophy: user ownership, exact
path identity, narrow authority, local-first processing, responsive bounded
work, standards-based integration, and honest security language. Read
[Why Floe works this way](./docs/PHILOSOPHY.md).

The implemented desktop path is generic GTK/GIO/XDG. Niri and KDE Plasma are
first-class product targets, but compositor-specific integrations are deferred
and never required by the filesystem core.

## Install and start

Floe currently has one verified native packaging strategy: the Arch package
contract and manifest-driven native installer. Flatpak is not implemented.

See [Installation](./docs/INSTALLATION.md), then launch:

```bash
floe
floe /path/to/folder
floe /path/to/file.pdf
```

The stable application ID is `io.github.rodriguezcappsec.Floe`. Each invocation
accepts at most one local target; remote URIs are rejected.

To build from a checkout:

```bash
sudo pacman -S --needed rust gtk4 libadwaita pkgconf
cargo run -p floe-app
```

Rust 1.85+, GTK 4.14+, and libadwaita 1.5+ are required. Other distributions
use different development-package names.

## Current capabilities

- Virtualized List and Grid plus bounded spatial Miller columns.
- Tabs, session restore, split panes, breadcrumbs, and local CLI routing.
- Copy, move, rename, duplicate, links, templates, drag and drop, Trash,
  restore, permanent deletion, identity-checked Replace/Replace All, conflicts,
  cancellation, and private 30-day Undo/Redo for reversible local work,
  including exact-receipt Floe-owned local Trash actions.
- Exact completed-operation reveal: successful copy, move, rename, create,
  duplicate, and replace results are selected, scrolled into view, and briefly
  emphasized when their destination folder is already visible.
- Quick Preview, Inspector, Properties, permissions, metadata, checksums,
  archives, search, an optional local index, and exact duplicate review.
- Integrity fingerprints and manifests, baselines, verified copy/transfer,
  Protected Folder guardrails, and conservative interrupted-operation plus
  Undo/Redo recovery review.
- Searchable Settings, five appearance presets, text scale, reduced motion,
  customizable shortcuts, optional Vim navigation, and direct-argv actions.

## Keyboard entry points

| Shortcut | Action |
| --- | --- |
| `Ctrl+Shift+P` | Command Palette |
| `Ctrl+,` | Settings |
| `Ctrl+?` | Keyboard Shortcuts |
| `Ctrl+L` | Edit location |
| `Ctrl+F` | Unified Search |
| `Ctrl+T` / `Ctrl+W` | New / close tab |
| `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | Copy / cut / paste |
| `F2` | Rename |
| `Delete` / `Shift+Delete` | Trash / permanent-delete confirmation |
| `Space` | Quick Preview |
| `Ctrl+I` / `Alt+Enter` | Inspector / Properties |

## Release documentation

- [Getting Started](./docs/GETTING_STARTED.md)
- [User Guide](./docs/USER_GUIDE.md)
- [Floe Philosophy](./docs/PHILOSOPHY.md)
- [Installation](./docs/INSTALLATION.md) and [Migrations](./docs/MIGRATIONS.md)
- [Administration](./docs/ADMINISTRATION.md)
- [Accessibility](./docs/ACCESSIBILITY.md)
- [Recovery](./docs/RECOVERY.md)
- [Debugging](./docs/DEBUGGING.md)
- [Localization and RTL](./docs/LOCALIZATION.md)
- [Security Policy](./SECURITY.md) and [Privacy/Security Architecture](./docs/PRIVACY_SECURITY.md)
- [Performance](./docs/PERFORMANCE.md)
- [Release environment matrix](./docs/RELEASE_MATRIX.md)
- [Feature Matrix](./docs/FEATURE_MATRIX.md) and [Roadmap](./docs/ROADMAP.md)
- [Changelog](./CHANGELOG.md)

## Important limitations

- Niri-specific, Plasma-specific, remote/network, and Android/MTP integrations
  are deferred. Generic local Wayland behavior remains implemented.
- Flatpak is not implemented.
- Experimental administrator access is opt-in. Its separate view supports
  explicitly confirmed, no-overwrite New Folder, Rename, file Copy/Move, Trash,
  empty-item permanent deletion, and Unix mode changes; previews, external
  tools, archives, ownership, ACL/xattr, and recursive administrator copy remain
  unavailable.
- Provider helpers are supervised but not sandboxed and retain normal user
  authority.
- Encrypted Vault, Sensitive Folder, Private Mode, Open Safely, Secure Share,
  portable encryption, and provider sandboxing are unavailable.
- Protected Folder is an accidental-change guardrail, not encryption or access
  control. Permanent deletion is not secure erase.
- Hashes do not prove authenticity, authorship, malware safety, or trust.
- Recovery is conservative restart review, not a transaction, rollback, or
  backup guarantee.
- Floe is English-only with partial RTL foundations. Complete Orca, translated
  RTL, and physical multi-monitor fractional-scale verification are unclaimed.
- Logs and technical details may contain sensitive paths; review and redact
  them before sharing.

## Architecture and development

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
[Architecture](./docs/ARCHITECTURE.md), [Developing Floe](./docs/DEVELOPMENT.md),
and [AGENTS.md](./AGENTS.md) before changing the project.

## Project status

Phase 21C release documentation is complete after verified Phase 21A
performance and Phase 21B packaging/migrations. Phase 21D release-candidate
hardening is the only next phase. [Roadmap](./docs/ROADMAP.md) is the phase-
sequencing authority; code and tests determine completion.
