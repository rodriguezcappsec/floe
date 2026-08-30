# Changelog

Floe remains pre-release. This file records verified release-facing changes and
does not promise API or format stability beyond documented migration behavior.

## Unreleased

### Added

- User-facing Floe philosophy and feature-rationale contract explaining what
  important behaviors do, why they exist, their tradeoffs, and what they do not
  claim.
- Getting-started, installation, administration, accessibility, recovery,
  debugging, localization, and security documentation.
- Automated documentation and isolated installed-artifact walkthrough gates.

### Changed

- Fixed fresh-session **Open as Administrator…** so a GVfs `NotMounted` result
  starts desktop authorization and retries the read-only listing once instead
  of being mislabeled as an unavailable administrator service.
- Reconciled release claims with verified performance, native packaging,
  operation recovery, accessibility, and security terminology.
- Documented that logs and technical details may contain sensitive paths.
- Documented Floe as English-only with partial RTL foundations.

### Known limitations

- Flatpak, compositor-specific, remote/network, Android/MTP, cryptography,
  vault, Private Mode, Sensitive Folder, Open Safely, Secure Share, and provider
  sandbox functionality are unavailable.
- Complete Orca, translated RTL, physical fractional-scale, physical media, and
  Phase 21D release-candidate security evidence remain unclaimed.

## 0.1.0 - development baseline

- Stable application ID `io.github.rodriguezcappsec.Floe` and `floe` command.
- Native package metadata and versioned migration contract.
- Local browsing, List/Grid/Miller views, tabs/split, Preview, operations,
  Trash/recovery, archives, search, duplicate review, and integrity tools.
