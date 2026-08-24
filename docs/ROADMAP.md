# Floe Roadmap

This is Floe's authoritative implementation sequence. The exhaustive capability
ledger is `docs/FEATURE_MATRIX.md`; security and privacy claims belong in
`docs/PRIVACY_SECURITY.md`; interaction language belongs in `DESIGN.md`.
`PLAN.md` remains the execution tree for one active implementation phase.

Statuses are `COMPLETE`, `PARTIAL`, `NEXT`, `PLANNED`, `DEFERRED`, and
`NOT APPLICABLE`. Code and tests, not documentation alone, determine completion.
Exactly one phase is marked `NEXT`.

## Phase gate

Every implementation phase preserves exact Linux path identity, keeps filesystem
and parser work off GTK, bounds concurrency and retained state, supports
cancellation where meaningful, reports structured errors, and never silently
overwrites data. The core remains GTK- and desktop-independent.

Minimum verification:

```text
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

UI phases also need a native Wayland smoke where practical. Parser and security
phases add hostile-input, failure-path, cache-leak, and claim-accuracy tests.

## Completed history

| Phase | Status | Delivered boundary |
| --- | --- | --- |
| 0 | COMPLETE | Workspace, GTK shell, core/app boundary, appearance foundation. |
| 1 | COMPLETE | Background local browsing, navigation history, virtualized list. |
| 2 | COMPLETE | Exact-path selection and asynchronous default-app opening. |
| 3 | COMPLETE | Operation/job IDs, lifecycle, progress, structured failures. |
| 4A | COMPLETE | Safe no-overwrite recursive copy with cancellation and cleanup. |
| 4B | COMPLETE | Internal copy/paste and Operations Island observation. |
| 4C | COMPLETE | Same-filesystem no-replace move and path-safe rename engine. |
| 4D | COMPLETE | Cut/paste and validated F2 rename interaction. |
| 4E | COMPLETE | Bounded cancellable GIO Trash executor. |
| 4F | COMPLETE | Move to Trash action and Delete shortcut. |
| 5A | COMPLETE | Retry identity and bounded terminal operation history. |
| 5B | COMPLETE | Accessible retry interaction. |
| 5C | COMPLETE | Selection-aware native item context menu. |
| 5D | COMPLETE | Open With chooser and explicit default association change. |
| 5E | COMPLETE | Conflict outcome, Keep Existing, Retry With New Name. |
| 5F | COMPLETE | Non-blocking conflict resolution interaction. |
| 6A | COMPLETE | Virtualized Name, Type, Size, Modified list details. |
| 6B | COMPLETE | Worker-side four-column sorting and selection restoration. |
| 6C | COMPLETE | Capacity-64 raster thumbnail worker and safe fallbacks. |
| 6D | COMPLETE | Shared list/grid model, multi-selection, persistent grid zoom. |
| 6E | COMPLETE | Validated bounded freedesktop thumbnail cache. |
| 6F | COMPLETE | Orientation-aware reviewed raster formats. |
| 6G | COMPLETE | Embedded semantic icon family. |
| 6H | COMPLETE | Editable absolute local location entry and rollback. |
| 6I | COMPLETE | Open routes to chooser when no default exists. |
| 6J | COMPLETE | Multi-selection, batch actions, item/background menus. |
| 6K | COMPLETE | XDG Places, raw-path bookmarks, live GIO devices. |
| 6K2 | COMPLETE | Persistent sidebar density/width and mount-operation polish. |
| 6L | COMPLETE | Supervised freedesktop system-thumbnailer providers with bounded cache integration. |
| 6M | COMPLETE | Confirmed multi-target permanent deletion with mount/symlink safety and truthful partial failure. |

The actual completed phase is **Phase 6M**.

## Phase 6 — Finish browser and filesystem foundations

### Phase 6L — System thumbnailers

Status: **COMPLETE**
Recommended branch: `phase-6l-system-thumbnailers`

Goal: consume reviewed freedesktop thumbnailer providers through the existing
request/result and cache boundary for video, PDF, office documents, fonts,
text/code, audio artwork, and archive previews.

- Scope: provider discovery, exact input identity, timeouts, cancellation,
  bounded output, stale-result rejection, validated cache writes, safe fallback.
- Excludes: Quick Preview, active content, provider installation, and any claim
  that providers are sandboxed. Phase 18L owns isolation.
- Dependencies: Phases 6C–6F.
- Acceptance: malformed, missing, changed, oversized, timed-out, unsupported,
  non-UTF-8, cancellation, and capacity cases retain a usable generic icon.
- Verification: provider-policy and fixture tests, phase gate, native smoke.

### Phase 6M — Permanent deletion

Status: **COMPLETE**
Recommended branch: `phase-6m-permanent-delete`

Goal: add path-safe multi-target permanent-delete jobs and Shift+Delete.

- Scope: exact-target preflight, irreversible confirmation, progress,
  meaningful cancellation boundary, partial failures, conservative retry.
- Excludes: secure erase and silent deletion of uncertain state.
- Dependencies: Phases 4, 5, and 6J.
- Acceptance: truthful “Delete Permanently” wording, symlink/mounted-root tests,
  rich target context, no silent partial success, phase gate, native smoke.
- Delivered: selection-aware `Shift+Delete` and context/menu action, safe-focus
  irreversible confirmation with escaped exact target context, whole-batch
  no-follow preflight, root and mount-boundary refusal, device/inode/kind
  revalidation, fixed-capacity job execution, and explicit non-retryable partial
  failure after the first committed removal.
- Cancellation is successful only before the first removal. Once mutation has
  committed, the worker finishes or reports the exact removed/planned count.

### Remaining Phase 6 leaves

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 6N — Trash lifecycle | NEXT | `phase-6n-trash-lifecycle` | Browse, restore, delete Trash items, Empty Trash, supported cleanup preferences. | 4E/4F/6M; no secure-erase claim; verify freedesktop metadata and restore conflicts. |
| 6O — Transfer semantics | PLANNED | `phase-6o-transfer-semantics` | Cross-filesystem move, metadata-aware copy, space checks, external clipboard. | 4A–5F; no silent metadata loss; verify crash/cancel, symlinks and non-UTF-8 paths. |
| 6P — Operation control | PLANNED | `phase-6p-operation-control` | Queueing, item progress, speed/ETA, truthful pause, richer conflicts, safe undo/history. | 6N/6O; irreversible work is not undoable; verify scoped batch policy and recovery. |
| 6Q — Create/duplicate/links | PLANNED | `phase-6q-create-duplicate-links` | New folder/file, templates, duplicate, links, reveal target, copy path/name/URI. | 6P; no shell or privileged creation; verify collisions, broken links and raw names. |
| 6R — Drag and drop | PLANNED | `phase-6r-drag-drop` | Internal/external drag, sidebar/Trash, modifiers, hover-open, autoscroll, highlighting. | 6O–6Q; no implicit overwrite; verify exact destinations and keyboard alternatives. |
| 6S — File watching | PLANNED | `phase-6s-file-watching` | Coalesced external-change reconciliation, refresh, selection/scroll restoration. | 6P; no integrity-monitoring claim; verify storms, deletion, rename and 100k folders. |
| 6T — Browser completeness | PLANNED | `phase-6t-browser-completeness` | Lazy metadata, sorting/grouping, columns, density, per-folder views, status/device detail. | 6S; no eager metadata storms; verify stable enrichment and preference migration. |

## Phase 7 — Tabs and split view

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 7A — Tab/session model | PLANNED | `phase-7a-tabs-foundation` | Serializable browser sessions: path, history, selection, scroll, sort, view. | 6S/6T; no widgets or duplicated workers; verify exact GTK-independent state. |
| 7B — Tab interaction | PLANNED | `phase-7b-tabs-interaction` | New, close, switch, duplicate, reorder, foreground/background open, middle-click. | 7A; no restore/split; verify pointer, keyboard, focus and native smoke. |
| 7C — Closed tabs/restore | PLANNED | `phase-7c-tab-session-restore` | Reopen closed, close variants, optional names/pins, startup session restore. | 7A/7B; suppress private state; verify atomic versioned persistence and Ctrl+Shift+T. |
| 7D — Split state | PLANNED | `phase-7d-split-state` | Two independent contexts, active side, histories, ratio and view modes. | 7A; widgets are not source of truth; verify serialization and focus identity. |
| 7E — Split interaction | PLANNED | `phase-7e-split-interaction` | Toggle, close, swap, switch side, opposite-pane open and search/filter hooks. | 7D; no drag; active side uses non-color semantics and passes native smoke. |
| 7F — Tab/split drag | PLANNED | `phase-7f-tab-split-drag` | Tab reorder/detach where supported and file operations between contexts. | 6R/7B/7E; no Miller drag; verify destinations, modifiers and keyboard alternatives. |

## Phase 8 — Miller and spatial navigation

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 8A — Column model | PLANNED | `phase-8a-miller-model` | Exact parent/selected-child chain and bounded retained columns. | 7A; no GTK; verify rename/delete/non-UTF-8 state transitions. |
| 8B — Virtualized columns | PLANNED | `phase-8b-miller-ui` | Recyclable floating columns and adjustable widths. | 8A/6T; no detail column; verify bounded memory and no duplicate enumeration. |
| 8C — Keyboard/trackpad | PLANNED | `phase-8c-miller-navigation` | Left/right, up/down and smooth horizontal trackpad interaction. | 8B; Vim stays 11D; verify RTL, focus, reduced motion and native smoke. |
| 8D — Column actions | PLANNED | `phase-8d-miller-actions` | Selection-aware normal context actions in every column. | 8B/6P/6Q; verify exact active-column ownership and parity. |
| 8E — Cross-column drag | PLANNED | `phase-8e-miller-drag-drop` | Drag/drop and hover navigation across columns, sidebar, tabs and panes. | 6R/8D; verify modifiers, autoscroll and no silent overwrite. |
| 8F — Detail hooks | PLANNED | `phase-8f-miller-detail-hooks` | Optional final-column Preview/Inspector contracts. | 8B; Phases 9/10 own content; verify unsupported, focus and lifecycle states. |

Miller mode is generic Wayland functionality and must never require Niri.

## Phase 9 — Quick Preview

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 9A — Provider architecture | PLANNED | `phase-9a-preview-providers` | Typed, cancellable provider lifecycle, limits, cache policy and fallback. | 6L; no active content/sandbox claim; verify hostile, stale and failed providers. |
| 9B — Images/text/code | PLANNED | `phase-9b-preview-images-text` | Images, bounded text, Markdown source, code, JSON and XML read-only preview. | 9A; no active HTML; verify encodings, huge files, malformed input and zoom. |
| 9C — PDF/documents | PLANNED | `phase-9c-preview-documents` | Passive PDF and reviewed office/document rendering. | 9A/provider review; no macros; verify malformed documents, limits and cancellation. |
| 9D — Audio/video | PLANNED | `phase-9d-preview-media` | Playback, seeking, metadata and poster frames. | 9A/6L; no codec installer; verify retired sources and resource release. |
| 9E — Fonts/archives | PLANNED | `phase-9e-preview-fonts-archives` | Read-only font specimen and bounded archive listing. | 9A; no install/extract; verify bombs, traversal and malformed fonts. |
| 9F — Preview polish | PLANNED | `phase-9f-preview-polish` | Space toggle, live navigation, fullscreen, HiDPI and privacy/cache hooks. | 9B–9E/8F; sandbox stays 18L; verify no jumps, accessibility and native smoke. |

## Phase 10 — Inspector, properties, and metadata

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 10A — Inspector foundation | PLANNED | `phase-10a-inspector-foundation` | Toggleable details, aggregate selection and Miller final-column surface. | 8F/9A; no edits; verify async selection, Ctrl+I and width persistence. |
| 10B — Metadata providers | PLANNED | `phase-10b-metadata-providers` | Lazy dates, MIME, links, ownership, dimensions and folder counts/sizes. | 6T/9A; no eager recursion; verify bounds and disappearing files. |
| 10C — Properties | PLANNED | `phase-10c-properties` | General, Open With, filesystem, mount and aggregate properties. | 10A/10B/5D; verify multi-item truth and native dialog smoke. |
| 10D — Permissions | PLANNED | `phase-10d-permissions` | Executable, mode and explicit owner/group editing with recursive preflight. | 10C/6P; no root-process elevation; verify symlinks and partial failure. |
| 10E — Checksums | PLANNED | `phase-10e-checksums` | SHA-256, SHA-512, legacy-labelled MD5, expected-digest verification. | Job boundary; no authenticity claim; verify vectors, huge files and cancellation. |
| 10F — Advanced metadata | PLANNED | `phase-10f-advanced-metadata` | Safe EXIF, media/audio tags, duration and optional columns. | 10B; privacy findings stay 18O; verify malformed metadata and lazy sort stability. |

## Phase 11 — Commands, keyboard, and terminal

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 11A — Command registry | PLANNED | `phase-11a-command-registry` | Human-named commands, eligibility, shortcut and menu metadata. | Reuse app actions; no business-logic duplication; verify registry/action parity. |
| 11B — Command palette | PLANNED | `phase-11b-command-palette` | Ctrl+Shift+P search and bounded recent commands. | 11A/privacy hooks; verify context, focus, screen reader and native smoke. |
| 11C — Keybindings | PLANNED | `phase-11c-keybindings` | Custom bindings, conflicts, reset and discoverability. | 11A; destructive defaults conservative; verify parser/migration/conflicts. |
| 11D — Optional Vim mode | PLANNED | `phase-11d-vim-mode` | Opt-in list/grid/Miller navigation mappings. | 11C/8C; never default; verify input fields and mode/focus transitions. |
| 11E — Terminal integration | PLANNED | `phase-11e-terminal-integration` | Preferred terminal and Open Terminal Here using safe argv. | Generic integration as needed; no shell/password argv; embedded terminal deferred. |

## Phase 12 — Productivity operations

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 12A — Archive engine | PLANNED | `phase-12a-archive-engine` | ZIP, tar, tar.gz, tar.xz and reviewed 7z listing/extract/compress jobs. | 6P; no unsafe argv; verify traversal, bombs, links, conflicts and cleanup. |
| 12B — Archive UX | PLANNED | `phase-12b-archive-ui` | Extract Here/To, compress, progress and safe backend password handoff. | 12A; verify destination preview, conflict, accessibility and native smoke. |
| 12C — Batch rename | PLANNED | `phase-12c-batch-rename` | Previewed transforms, regex, sequences, metadata templates and undo. | 6P; verify whole-batch validation, collisions and non-UTF-8 policy. |
| 12D — Create/templates polish | PLANNED | `phase-12d-create-templates` | Template discovery/management and immediate rename. | 6Q; no executable surprise; verify safe names, permissions and migration. |
| 12E — Links/duplicate polish | PLANNED | `phase-12e-link-duplicate-polish` | Relative/absolute links, hard-link eligibility and duplicate naming. | 6Q; verify cross-filesystem and broken-link behavior. |
| 12F — Action integration | PLANNED | `phase-12f-productivity-actions` | Context, palette and shortcut integration without a menu wall. | 11A/12A–E; verify eligibility and pointer/keyboard parity. |

## Phase 13 — Filter and search

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 13A — Folder filter | PLANNED | `phase-13a-folder-filter` | Instant text/glob/regex filtering, count and Escape clear. | 6T; not recursive search; verify invalid patterns, focus and large folders. |
| 13B — Filename search | PLANNED | `phase-13b-filename-search` | Streaming cancellable folder/subtree/location name search and reveal. | Worker boundary; no content scan; verify symlinks, permission and huge trees. |
| 13C — Advanced filters | PLANNED | `phase-13c-search-filters` | Type, MIME, extension, size, date, owner, hidden and tag-ready filters. | 13B/10B; verify combined predicates and lazy metadata. |
| 13D — Content search | PLANNED | `phase-13d-content-search` | Opt-in bounded text search with glob/regex/case controls. | 13B; no binary/secret upload; verify encodings, permission and cancellation. |
| 13E — Saved searches | PLANNED | `phase-13e-saved-searches` | Versioned saved queries and privacy-aware history. | 13C/13D; verify migration, corruption and private suppression. |
| 13F — Optional indexing | PLANNED | `phase-13f-search-indexing` | Capability-reviewed index with complete non-indexed fallback. | 13E/18J/18K; exclude locked/sensitive content by default; verify stale/fallback. |

## Phases 14–17 — Desktop and location integration

### Phase 14 — Generic desktop integration framework

Status: **PLANNED**
Recommended branch: `phase-14-generic-desktop-integration`

Goal: create an application-layer capability boundary for GIO, XDG, portals,
URI launch, mounts, notifications, Share, themes, credential stores, and reliable
session-lock signals. It depends on stable app commands and location types. It
excludes compositor branches or desktop types in core. Acceptance requires
missing-capability fallback tests and generic Wayland plus first-class desktop
smoke coverage.

### Phase 15 — Niri integration

Status: **PLANNED**
Recommended branch: `phase-15-niri-integration`

Goal: add optional detection/IPC, output/workspace awareness, spatial launch,
and useful Miller enhancements behind Phase 14. Niri must never be required.
Acceptance includes missing/stale socket, protocol failure, graceful fallback,
and native Niri smoke tests.

### Phase 16 — KDE Plasma integration

Status: **PLANNED**
Recommended branch: `phase-16-plasma-integration`

Goal: add standards-first Plasma capability detection, KDE services only where
useful, and KWallet only after secret-storage requirements are proven. KDE
Frameworks are excluded for cosmetics. Acceptance requires generic fallback,
service-unavailable tests, no KDE types in core, and Plasma Wayland smoke.

### Phase 17 — Remote and external locations

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 17A — Remote model | PLANNED | `phase-17a-remote-location-model` | URI/location identity, capabilities, timeouts and offline state separate from local paths. | 14; no URI semantics in local core; verify redaction, reconnect and identity. |
| 17B — GIO remote browsing | PLANNED | `phase-17b-gio-remote-browsing` | Reviewed GIO SFTP, SMB, WebDAV and NFS browsing/operations. | 17A; FTP deferred; no silent plaintext staging; verify timeout/partial failures. |
| 17C — MTP/devices | PLANNED | `phase-17c-mtp-devices` | Android/MTP transfer and disconnect recovery; optional KDE Connect enhancement. | 17A/6K/14; generic path remains; verify disconnect, progress and cancellation. |
| 17D — Remote recovery | PLANNED | `phase-17d-remote-recovery` | Retry/offline transitions, remote thumbnail policy, saved connections and credential abstraction. | 17B/17C/18J; no secret logs/plaintext cache; verify migration and reconnect. |

## Phase 18 — Privacy, security, and data integrity

No Phase 18 feature is implemented by this planning document. Every leaf follows
`docs/PRIVACY_SECURITY.md`, and every security-critical dependency requires an
implementation-time review. Security terms are not interchangeable.

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 18A — Threat model | PLANNED | `phase-18a-security-threat-model` | Revalidate formats, secrets, vaults, caches, sandbox and integrity design before code. | 14; research/docs only; require decision records, dependency rationale and test plan. |
| 18B — Portable encryption engine | PLANNED | `phase-18b-portable-encryption` | Reviewed interoperable format, preferably `age` after review; streaming passphrase jobs. | Stable jobs; no custom crypto/shell/plaintext fallback; verify interop, auth, malformed input and cleanup. |
| 18C — Encryption UI | PLANNED | `phase-18c-encryption-ui` | Encrypt/Decrypt prompts, conflicts, Operations Island and multi-selection. | 18B/Trash; original survives until authenticated completion; verify secret UX and leaks. |
| 18D — Recipient encryption | PLANNED | `phase-18d-recipient-encryption` | Reviewed public identities, fingerprints, multiple recipients and safe identity handling. | 18B; no invented exchange/silent sync; verify interop, duplicates and secret storage. |
| 18E — Privacy session | PLANNED | `phase-18e-privacy-session` | Non-persistent secret wrappers, timeout, Lock and Lock All. | 18A; not a universal password; verify lifetime, no Debug/log/persistence and focus. |
| 18F — Vault crypto design | PLANNED | `phase-18f-vault-crypto` | Review vault format/backend, key hierarchy, password change, recovery and filename privacy. | 18A/18E; no plaintext folder plus dialog/custom crypto; require architecture review. |
| 18G — Vault filesystem | PLANNED | `phase-18g-vault-filesystem` | Selected encrypted storage/virtual filesystem with complete large-file semantics. | 18F; FUSE not assumed, no plaintext temp; verify crash, concurrency, mounts and disconnect. |
| 18H — Vault UI | PLANNED | `phase-18h-vault-ui` | Create/Add/Unlock/Lock/Lock All/change password/remove registration/recovery UX. | 18G/18E; removal is not deletion; verify non-color states, secrets and native smoke. |
| 18I — Vault lifecycle | PLANNED | `phase-18i-vault-autolock` | Timeout, app close, reliable session lock/suspend, open handles and drive removal. | 18G/18H/14; no unsafe forced unmount; verify lifecycle races and truthful delay. |
| 18J — Private cache/history | PLANNED | `phase-18j-private-cache-history` | Vault-safe thumbnails, preview, Recents, search, jobs, notifications and lock invalidation. | 18G/18I; verify cache/config/log/search leak audit and locking. |
| 18K — Sensitive/Private modes | PLANNED | `phase-18k-sensitive-private-mode` | Sensitive Folder trace reduction and non-persistent Private Mode windows. | 18J; neither is encryption; verify history/cache/session suppression and limitation UX. |
| 18L — Sandboxed providers | PLANNED | `phase-18l-sandboxed-previewers` | Target-only read, isolated temp, no unrelated files/network/vault, limits and termination. | 6L/9A/18A; helper execution is not sandboxing; verify policy and setup failure. |
| 18M — Open Safely | PLANNED | `phase-18m-open-safely` | Constrained external launch with reviewed mechanism and persistent indicator. | 18L/14; never silently launch normally; verify policy, fallback and compatibility. |
| 18N — Suspicious files | PLANNED | `phase-18n-suspicious-file-analysis` | Executable/double-extension, MIME mismatch, Unicode hazards and reliable origin evidence. | 10B/18A; no malware verdict; verify false positives and escaped filename display. |
| 18O — Privacy Inspector | PLANNED | `phase-18o-privacy-inspector` | Format-specific GPS, EXIF, author, organization, app and embedded-thumbnail findings. | 10A/10B/10F; no exhaustive-removal claim; verify known corpus and malformed data. |
| 18P — Metadata sanitization | PLANNED | `phase-18p-metadata-sanitization` | Sanitized copies, GPS/selected metadata removal and batch preview. | 18O/6P; preserve source; verify before/after, cancel and atomic finalization. |
| 18Q — Secure Share | PLANNED | `phase-18q-secure-share` | Compose inspect, sanitize, password/recipient encryption and checksum. | 18B/18D/18O/18P; never alter original; verify step failures and output. |
| 18R — Permission auditor | PLANNED | `phase-18r-permission-auditor` | Explain risky modes, ownership, ACL/xattr/capability exposure and conservative fixes. | 10D; no casual expert controls; verify evidence, symlinks and partial edits. |
| 18S — Sensitive scanner | PLANNED | `phase-18s-sensitive-content-scanner` | Explicit local heuristic scan for keys, `.env`, tokens and credential dumps. | 18A/18J; no cloud/secret display/malware claim; verify redaction and false positives. |
| 18T — Integrity tools | PLANNED | `phase-18t-integrity-tools` | Saved SHA fingerprints and portable `SHA256SUMS` generation/verification. | 10E; hash is not authenticity; verify path-safe manifests and changed/missing/new. |
| 18U — Integrity monitoring | PLANNED | `phase-18u-integrity-monitoring` | Opt-in baselines and coalesced change reporting. | 6S/18T; not intrusion detection; verify storms, offline gaps and stale baselines. |
| 18V — Verified copy | PLANNED | `phase-18v-verified-copy` | Optional Copy and Verify with source/destination digest. | 6O/10E; not default; verify changed source, corruption, flush, retry and cancel. |
| 18W — Verified USB | PLANNED | `phase-18w-verified-usb-transfer` | Copy, Verify, Flush and Safe Eject as explicit staged workflow. | 18V/6K/14; no safe-removal claim before eject; verify partial-success states. |
| 18X — Data-loss guardrails | PLANNED | `phase-18x-data-loss-guardrails` | Protected Folders and rich thresholded destructive preflight. | 6M/6P; mistake prevention only; verify mounted roots and huge operations. |
| 18Y — Operation recovery | PLANNED | `phase-18y-operation-recovery` | Privacy-aware journal and conservative interrupted-job/partial-output recovery. | 6P/6O/18J; no secrets/silent deletion; verify crash and corrupt journal. |
| 18Z — Security Center | PLANNED | `phase-18z-security-center` | Calm vault, sensitive, session, integrity and finding status/actions. | 18E/18H/18K/18N/18R/18T; no fear score; verify state and accessibility. |
| 18AA — Security audit | PLANNED | `phase-18aa-security-audit` | Crypto, dependencies, secrets, caches, parsers, vaults, sandbox and recovery audit. | 18B–18Z; no stable Phase 18 claim before pass; close or record every finding. |

Security phases preserve originals on failed encryption or sanitization, never
log or pass secrets in process arguments, never silently downgrade protection,
and add hostile-input and cache/notification/history leak tests.

## Phase 19 — Extensibility and developer features

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 19A — Git awareness | PLANNED | `phase-19a-git-awareness` | Cheap opt-in repository root, branch, status badges and relative-path actions. | 6T/11E; no cost outside repositories; verify large repos and missing Git. |
| 19B — Custom actions | PLANNED | `phase-19b-custom-actions` | Context/palette file-type actions with safe argv and eligibility. | 11A/14/18A; no shell or vault-key access; verify hostile paths and capabilities. |
| 19C — Customization | PLANNED | `phase-19c-customization` | Theme tokens, templates, editor, compare and Share tools. | Appearance/12D/19B; no plugin runtime; verify validation, rollback and upgrades. |

A general plugin runtime is **DEFERRED** until demonstrated demand and a
capability, isolation, permission, and failure-containment design exist.

## Phase 20 — Settings, visual, accessibility, and QoL audit

Status: **PLANNED**
Recommended branch: `phase-20-completeness-audit`

Goal: close every remaining `FEATURE_MATRIX` gap across appearance, fonts,
density, motion, views, previews, operations, shortcuts, applications, privacy,
desktop settings, focus, Orca, high contrast, localization, RTL, HiDPI,
fractional scaling, persistence, errors, and recorded daily-driver QoL.

- Excludes: weakening secure defaults or exposing unnecessary crypto knobs.
- Dependencies: all preceding user-facing families.
- Acceptance: measured matrix audit, no accessibility-critical finding,
  migration tests, phase gate, and Niri/Plasma/generic Wayland smoke where
  environments are available.

## Phase 21 — Performance, packaging, and release hardening

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 21A — Performance | PLANNED | `phase-21a-performance` | Profile 100k folders, thumbnails, search, operations, crypto, vault and integrity workloads. | Feature complete; no speculative rewrite; record CPU/memory/latency budgets. |
| 21B — Packaging/migrations | PLANNED | `phase-21b-packaging` | Release profile, metadata, icons, MIME and selected Flatpak/Arch strategy. | 18AA/20; verify clean install, upgrade, rollback and cache/config/vault migrations. |
| 21C — Release docs | PLANNED | `phase-21c-release-documentation` | User, admin, security, accessibility, recovery and debug-policy documentation. | 21B; verified claims only; run link, terminology and fresh-user walkthrough. |
| 21D — Release candidate | PLANNED | `phase-21d-release-candidate` | Dependency/license/security follow-up, crash recovery and environment matrix. | 21A–21C; no known data-loss/security-critical defect; require reproducible artifacts. |

## Deliberately deferred or not applicable

- Secure erase is **NOT APPLICABLE**: permanent unlinking cannot guarantee
  erasure on SSD, CoW, snapshot, remote, backup, or cached storage layers.
- Antivirus, malware removal, intrusion detection, DRM/file expiration, and
  protection from a compromised kernel or same-user malware are **NOT
  APPLICABLE** without a separately reviewed mechanism.
- FTP, embedded terminal, signed manifests, snapshot-manager integration,
  advanced ACL/xattr editing, tab detachment, and a plugin runtime are
  **DEFERRED** pending demand and architecture/security review.

## Dependency spine

```text
6L–6T browser and operation completeness
  -> 7 reusable tab/split sessions -> 8 Miller navigation
  -> 9 provider-based Preview -> 10 Inspector and metadata
     -> 11 shared commands -> 12 productivity -> 13 search
14 generic desktop boundary -> 15 Niri / 16 Plasma / 17 remote
jobs + providers + metadata + desktop facts -> 18 privacy/security/integrity
18AA + 20 completeness -> 21 release hardening
```

Portable encryption depends on safe jobs and Trash. Recipient encryption depends
on reviewed identity representation. Vault UI and auto-lock depend on proven
storage/key and mount lifecycles. Private Preview and search depend on private
cache/index policy. Secure Share composes inspection, sanitization, encryption,
and checksums. Sandboxed Preview and Open Safely depend on explicit provider and
policy boundaries. Integrity monitoring depends on coalesced file watching.
Verified USB transfer depends on verified copy and safe device actions. Crash
recovery depends on explicit operation semantics and privacy-aware journaling.
