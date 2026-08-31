# Changelog

Floe remains pre-release. This file records verified release-facing changes and
does not promise API or format stability beyond documented migration behavior.

## Unreleased

### Added

- Optional FileChooser portal filters, current-filter selection, and bounded
  boolean/combo choices with responsive visual filtering.
- Multi-window browsing through `Ctrl+N`, repeated activation, and **Open Folder
  in New Window**, with safe newest-live-window action routing.
- Focus-aware completion notifications for operations lasting at least two
  seconds, including a persistent opt-out.
- Natural filename sorting, bookmark rename/reorder controls, a remembered
  collapsible sidebar, explicit SHA-256 actions in Properties/Inspector, and
  Owner, Group, Path, and Link Target detail columns.

- Compact adaptive tabs with calmer active treatment, device name plus concise
  free-space rows that do not wrap, and a neutral split-pane boundary replacing
  the blue content frame.
- Safe, separately typed administrator New Folder, Rename, single-file
  Copy/Move, Trash, permanent delete, and Unix-mode operations through
  GIO/GVfs with explicit confirmation and without elevating Floe itself.
- User-facing Floe philosophy and feature-rationale contract explaining what
  important behaviors do, why they exist, their tradeoffs, and what they do not
  claim.
- Getting-started, installation, administration, accessibility, recovery,
  debugging, localization, and security documentation.
- Automated documentation and isolated installed-artifact walkthrough gates.

### Changed

- Fixed the location-completion `GtkPopover` remaining parented to a finalizing
  `GtkEntry`. All manually parented browser popovers now detach before an
  allowed window close; a native KDE Wayland close/survivor regression covers it.

- Fixed closing one of several windows freezing the survivor when a metadata,
  preview, thumbnail, search, or mount worker was blocked. Read-only teardown is
  now cooperative and nonblocking; active file jobs prevent window close until
  completion or cancellation.
- Fixed failed new-window construction redirecting its requested folder into an
  existing window, Properties checksums retargeting after selection changed,
  cross-window completion-notification ID collisions, and XDG portal filters
  being enforced as restrictions instead of advisory selection aids.
- Fixed fresh-session **Open as Administrator…** so a GVfs `NotMounted` result
  starts desktop authorization and retries the read-only listing once instead
  of being mislabeled as an unavailable administrator service.
- Reconciled release claims with verified performance, native packaging,
  operation recovery, accessibility, and security terminology.
- Documented that logs and technical details may contain sensitive paths.
- Documented Floe as English-only with partial RTL foundations.
- Resolved three medium `tar` dependency advisories by updating to 0.4.46;
  added deterministic dependency/license/advisory and reproducible-candidate
  gates plus the release environment matrix.

### Known limitations

- Multi-window browsing does not yet share one application-owned operation
  coordinator or restore a bounded set of windows; Phase 23H owns that hardening.

- Flatpak, compositor-specific, remote/network, Android/MTP, cryptography,
  vault, Private Mode, Sensitive Folder, Open Safely, Secure Share, and provider
  sandbox functionality are unavailable.
- Complete Orca, translated RTL, physical fractional-scale, and physical media
  remain unclaimed.

## 0.1.0 - development baseline

- Stable application ID `io.github.rodriguezcappsec.Floe` and `floe` command.
- Native package metadata and versioned migration contract.
- Local browsing, List/Grid/Miller views, tabs/split, Preview, operations,
  Trash/recovery, archives, search, duplicate review, and integrity tools.
