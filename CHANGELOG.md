# Changelog

Floe remains pre-release. This file records verified release-facing changes and
does not promise API or format stability beyond documented migration behavior.

## Unreleased

### Added

- Persisted Privacy & Safety controls for local ClamAV per-file and total-request scan limits. Conservative 1 GiB/16 GiB defaults remain, user values are bounded and snapshotted per request, reports show the limits used and link back to Settings, and independent `clamd` limits remain explicit.

- Persistent accessible Background Activity rows for read-only Properties, Privacy
  inspection, local ClamAV, and metadata sanitization. Running and terminal feedback
  survives focus/navigation/selection/tab/pane changes with cancellation, View
  Results/Reveal, and explicit dismissal. Properties is no longer silently superseded
  by selection changes, and Privacy cancellation always returns a terminal result.

- One application-owned multi-window operation/event coordinator and bounded versioned restoration of up to 16 windows, including legacy one-window migration and Private/Sensitive trace suppression.
- Required Bubblewrap isolation for external thumbnail/Preview providers with target-only read, private output/temp, cleared environment, no network/session namespaces, process-group termination, and fail-closed setup.
- **Inspect Privacy & Safety…** evidence for executable/double-extension/MIME/Unicode signals and reviewed JPEG/TIFF/PNG/WebP metadata.
- Optional local `clamd` streaming scans with explicit detection, no-signature, not-scanned, changed, cancelled, limit, unavailable, and error outcomes plus generation-routed multi-window delivery; no cloud, quarantine, or safety claim.
- Preview-confirmed batch JPEG/PNG/WebP sanitized sibling copies with source preservation, private staging cleanup, WebP feature-flag repair, verification, cancellation, and atomic no-overwrite publication.

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

- Flatpak, compositor-specific, remote/network, Android/MTP, cryptography, vault, user-facing Private Mode, Sensitive Folder, Open Safely, and Secure Share remain unavailable.
- Installed external providers require usable Bubblewrap namespaces. When the boundary is prohibited, provider-backed results remain unavailable rather than running unsandboxed.
- ClamAV scanning requires a separately installed and running local `clamd`; Floe is not antivirus protection and no-signature is not proof of safety.
- Complete Orca, translated RTL, physical fractional-scale, and physical media
  remain unclaimed.

## 0.1.0 - development baseline

- Stable application ID `io.github.rodriguezcappsec.Floe` and `floe` command.
- Native package metadata and versioned migration contract.
- Local browsing, List/Grid/Miller views, tabs/split, Preview, operations,
  Trash/recovery, archives, search, duplicate review, and integrity tools.
