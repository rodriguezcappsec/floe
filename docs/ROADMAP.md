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

The actual completed phase is **Phase 7A**.

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

### Phase 6N — Trash lifecycle

Status: **COMPLETE**

Recommended branch: `phase-6n-trash-lifecycle`

Goal: make standards-correct local Trash a first-class Floe location.

- Home and mounted-volume freedesktop Trash roots are enumerated on the bounded
  browser worker. Bounded no-follow `.trashinfo` parsing retains exact backing
  and original paths; orphan/malformed payloads remain visible and deletable.
- Restore uses a fixed-capacity executor and atomic no-replace move. Conflicts
  reuse Keep Existing / Retry with New Name. Metadata is removed only after the
  payload move; cleanup failure is an explicit non-retryable partial result.
- Individual Delete Permanently and aggregate Empty Trash include companion
  metadata and reuse Phase 6M. Empty Trash always requires aggregate safe-focus
  confirmation; neither action claims secure erase.
- Cleanup preferences and Restore Elsewhere remain deferred until a predictable
  portable desktop policy and destination workflow exist.

### Remaining Phase 6 leaves

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 6N — Trash lifecycle | COMPLETE | `phase-6n-trash-lifecycle` | Local standards Trash browsing, metadata, restore, permanent delete, Empty Trash. | 4E/4F/6M; verified no-overwrite restore and no secure-erase claim. |
| 6O — Transfer semantics | COMPLETE | `phase-6o-transfer-semantics` | Cross-filesystem move, metadata-aware copy, space checks, external clipboard. | Verified staged no-overwrite EXDEV fallback, exact source revalidation, POSIX mode/timestamps, bounded local URI clipboard; no ownership/ACL/xattr/sparse/reflink or crash-journal claim. |
| 6P — Operation control | COMPLETE | `phase-6p-operation-control` | Explicit progress units and measured byte telemetry; stable serial batches with pause-after-current/resume/cancel; Keep Both and batch-scoped Skip All; bounded memory-only history; identity-checked move/rename undo. | Verified without Replace/Replace All, persistent history, or claims that irreversible work is undoable. |
| 6Q — Create/duplicate/links | COMPLETE | `phase-6q-create-duplicate-links` | New folder/file, native template selection, FIFO duplicate, symbolic/hard links, asynchronous reveal target, copy name/path/relative-path/local URI. | Verified bounded create executor, collision-safe `(copy N)` retries, broken/raw symbolic targets, regular-file-only hard links, exact clipboard identity, no shell, no privileged creation, and no overwrite. |
| 6R — Drag and drop | COMPLETE | `phase-6r-drag-drop` | Internal/external local-file drag, list/grid folders, Places/bookmarks/devices/Trash, copy/move/link negotiation, hover-open, edge autoscroll, accessible highlighting. | Verified exact `PathBuf` payloads, no-overwrite FIFO jobs, rejected self-nesting/non-local targets, and keyboard/menu alternatives. |
| 6S — File watching | COMPLETE | `phase-6s-file-watching` | One active GIO monitor, bounded 140 ms burst coalescing, exact rename reconciliation, refresh plus selection/scroll restoration. | Verified stale-generation rejection, event/path/rename caps, create/delete/rename, 100k-path O(n) reconciliation, and no integrity-monitoring claim. |
| 6T — Browser completeness | COMPLETE | `phase-6t-browser-completeness` | Lazy metadata, sorting/grouping, columns, density, per-folder views, status/device detail. | Verified fixed-capacity visible-row enrichment, stable ordering, exact-path preference migration, bounded storage facts, and native restoration smoke. |

## Phase 7 — Tabs and split view

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 7A — Tab/session model | COMPLETE | `phase-7a-tabs-foundation` | Serializable browser sessions: path, history, selection, scroll, sort, view. | Verified exact bounded GTK-independent state and raw non-UTF-8 codec; no widgets, persistence, or duplicated workers. |
| 7B — Tab interaction | COMPLETE | `phase-7b-tabs-interaction` | New, close, switch, duplicate, reorder, foreground/background open, middle-click. | Verified bounded stable-ID tabs, complete exact session restoration, one shared browser pipeline, pointer/keyboard parity, and native Wayland lifecycle; no restore/split. |
| 7C — Closed tabs/restore | COMPLETE | `phase-7c-tab-session-restore` | Reopen closed, close variants, startup session restore; optional names/pins remain deferred. | Verified bounded LIFO/fresh IDs, hostile-input codec, private atomic worker, explicit Private/Sensitive suppression, Ctrl+Shift+T, and two-launch native restore. |
| 7D — Split state | COMPLETE | `phase-7d-split-state` | Two independent contexts, active side, histories, ratio and view modes. | Verified per-tab primary/secondary sessions, stable tab identity, bounded ratio, close/swap transitions, workspace-v2 hostile-input handling and v1 migration; no GTK interaction. |
| 7E — Split interaction | COMPLETE | `phase-7e-split-interaction` | Toggle, close, swap, switch side, resizable ratio, opposite-pane open/copy/move. | Verified one native `GtkPaned`, textual active-side ownership, bounded stale-labelled inactive snapshots, exact destinations, existing no-overwrite FIFO jobs, keyboard/menu parity, two-launch native restore; no inter-pane drag, detached windows, or second browser pipeline. |
| 7F — Tab/split drag | COMPLETE | `phase-7f-tab-split-drag` | File-operation drag/drop between split contexts; existing tab reorder remains supported, detachment stays deferred. | Verified live exact opposite-session destinations, local file-list copy/move/link negotiation, no-overwrite FIFO jobs, accessible action/path/commit feedback, and Open/Copy/Move/Link keyboard/menu alternatives; no hover-open, Miller drag, detachment, or second pipeline. |

## Phase 8 — Miller and spatial navigation

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 8A — Column model | COMPLETE | `phase-8a-miller-model` | Exact parent/selected-child chain and bounded retained columns. | Verified GTK-free direct-child invariants, stable logical depths, 16-column retention, raw non-UTF-8 identity, same-parent rename, delete/truncate/root invalidation, reset, and structured stale-depth errors; no entries, enumeration, workers, or widgets. |
| 8B — Virtualized columns | COMPLETE | `phase-8b-miller-ui` | Recyclable floating columns and adjustable widths. | Verified one shared active browser model, capacity-16 retained columns, 4,096-entry historical snapshots, exact-path activation, clamped persistent 180–520 px widths, accessible active text, and native Wayland actions; no second enumerator/watcher, detail column, column actions, or keyboard/trackpad navigation. |
| 8C — Keyboard/trackpad | COMPLETE | `phase-8c-miller-navigation` | Left/right, up/down and smooth horizontal trackpad interaction. | Verified bounded Up/Down/Home/End, logical LTR/RTL parent/child movement, modified-key fallthrough, dominant-horizontal gesture clamping, focus-visible active list, reduced-motion kinetic suppression, exported logical actions, and native Wayland lifecycle; Vim remains 11D. |
| 8D — Column actions | COMPLETE | `phase-8d-miller-actions` | Selection-aware normal context actions in every column. | Verified exact depth/directory/raw-entry ownership, stale and overflow rejection, retained-column action directories, pointer/keyboard menus, existing no-overwrite command routing, and native Wayland lifecycle; no drag/drop. |
| 8E — Cross-column drag | COMPLETE | `phase-8e-miller-drag-drop` | Drag/drop and hover navigation across columns, sidebar, tabs and panes. | Verified exact raw file-list sources, live tab/split/sidebar/device/Miller destinations, typed cancellable hover ownership, copy/move/link modifier reuse, two-axis clamped autoscroll, no-overwrite jobs, and native lifecycle; no detail providers. |
| 8F — Detail hooks | COMPLETE | `phase-8f-miller-detail-hooks` | Optional final-column Preview/Inspector contracts. | Verified exact bounded generation/depth/directory/path handoff, hidden/empty/ready/unsupported lifecycle, accessible focus controls, truthful no-provider final column, mode-exit cleanup, and native action lifecycle; no Preview or metadata content. |

Miller mode is generic Wayland functionality and must never require Niri.

## Phase 9 — Quick Preview

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 9A — Provider architecture | COMPLETE | `phase-9a-preview-providers` | Typed, cancellable provider lifecycle, limits, cache policy and fallback. | Verified fixed-capacity worker/registry, exact source identity, deterministic order, explicit limits, memory-only cache, generation cancellation/stale rejection, queue/failure/panic containment, honest unsupported fallback, GTK drain, and native lifecycle; no renderer or sandbox claim. |
| 9B — Images/text/code | COMPLETE | `phase-9b-preview-images-text` | Images, bounded text, Markdown source, code, JSON and XML read-only preview. | Verified exact no-follow source identity, bounded owned RGBA first-frame decode, UTF-8/UTF-16 passive selectable source, main-thread GTK presentation, stale payload rejection, active HTML/SVG and binary rejection; no renderer sandbox claim. |
| 9C — PDF/documents | COMPLETE | `phase-9c-preview-documents` | Passive PDF and reviewed office/document rendering. | Verified reviewed freedesktop provider discovery, argv-only supervised execution, exact no-follow source revalidation, bounded PNG-to-RGBA first-page rendition, cancellation/failure/unsupported states, malformed/change/symlink rejection; no macros or sandbox claim. |
| 9D — Audio/video | COMPLETE | `phase-9d-preview-media` | Playback, seeking, metadata and poster frames. | Verified exact no-follow extension/MIME identity, optional bounded supervised poster, main-thread native MediaFile/Video/MediaControls, explicit stream pause/clear on retirement, audio fallback, unsupported/error states; no codec installer or shell path. |
| 9E — Fonts/archives | COMPLETE | `phase-9e-preview-fonts-archives` | Read-only font specimen and bounded archive listing. | Verified reviewed-provider bounded PNG font specimen with no install, exact no-follow identity, built-in capped ZIP central-directory/uncompressed TAR listing, raw member bytes, unsafe-path flags, malformed/truncated/cap rejection, selectable GTK presentation; no extraction or archive commands. |
| 9F — Preview polish | COMPLETE | `phase-9f-preview-polish` | Space toggle, live navigation, fullscreen, HiDPI and privacy/cache hooks. | Verified text-input-safe Space action, exact live generation following/focus restore, 50–400% presentation-only zoom/reset, fullscreen toggle, GTK monitor scaling, explicit cancel-and-purge memory cache, media retirement, accessibility and native lifecycle; sandbox stays 18L. |

## Phase 10 — Inspector, properties, and metadata

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 10A — Inspector foundation | COMPLETE | `phase-10a-inspector-foundation` | Toggleable details, aggregate selection and Miller final-column surface. | Verified bounded GTK-independent aggregation, exact raw multi-selection identity, stale-generation rejection, Ctrl+I, read-only accessible presentation, focus restoration, and independently clamped version-4 width persistence with version-3 migration; no filesystem reads, rich metadata providers, or edits. |
| 10B — Metadata providers | COMPLETE | `phase-10b-metadata-providers` | Lazy dates, MIME, links, ownership, dimensions and folder counts/sizes. | Verified fixed request/result bounds, exact no-follow source identity, generation supersession, MIME/timestamps/Unix identity, raw link targets/status, limited image dimensions, capped non-recursive immediate folder facts, disappearing/change handling, read-only Inspector presentation; no persistent cache or edits. |
| 10C — Properties | COMPLETE | `phase-10c-properties` | General, Open With, filesystem, mount and aggregate properties. | Verified bounded Phase 10B reuse, exact selection generations, native read-only Alt+Enter/header plus list/grid/Miller/Trash context-menu surfaces, truthful multi-selection, GIO containing-filesystem/mount facts, capped descriptor-relative no-follow recursive folder totals, Open With bridge, and native lifecycle; no edits or elevation. |
| 10D — Permissions | COMPLETE | `phase-10d-permissions` | Executable, mode and explicit owner/group editing with recursive preflight. | Verified exact typed targets, numeric/local identity validation, capacity-4 executor, 250,000-entry/depth-1,024 whole-tree preflight, no-follow symlink policy, mount refusal, cancellation and partial-commit reporting, Properties editor acknowledgement, Operations refresh, and native Wayland lifecycle; no root-process elevation, ACL/xattr/capability/immutable editing, or checksum work. |
| 10E — Checksums | COMPLETE | `phase-10e-checksums` | SHA-256, SHA-512, legacy-labelled MD5, expected-digest verification. | Verified exact typed requests, strict digest parsing, capacity-4 streaming worker, 1 MiB chunks, no-follow source revalidation, byte progress, cancellation, explicit match/mismatch, digest-only copy, accessible native dialogs, standard vectors, and Wayland lifecycle; no authenticity claim or persistence. |
| 10F — Advanced metadata | COMPLETE | `phase-10f-advanced-metadata` | Safe EXIF, media/audio tags, duration and optional columns. | Verified exact no-follow identity revalidation, bounded reviewed EXIF/audio fields, strict malformed/limit states, lazy opt-in columns, stable non-sortable enrichment, preference migration, and native Wayland lifecycle; no GPS/privacy finding or persistence. |

## Phase 11 — Commands, keyboard, and terminal

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 11A — Command registry | COMPLETE | `phase-11a-command-registry` | Human-named commands, eligibility, shortcut and menu metadata. | Verified 59 unique bounded human-readable definitions, deterministic categories/search terms/placements/risk, authoritative live GAction eligibility, centralized unchanged accelerators, context-menu parity, explicit internal-action exclusions, runtime parity audit, and native Wayland lifecycle; no palette or callback duplication. |
| 11B — Command palette | COMPLETE | `phase-11b-command-palette` | Ctrl+Shift+P search and bounded recent commands. | Verified 128-character metadata-only ranked search, 64-result cap, live enabled/disabled GAction state, direct existing-action activation, keyboard-first native dialog, explicit unavailable context, accessible labels/descriptions, 16-entry deduplicated memory-only recents, and native Wayland lifecycle; no persistence or shortcut editing. |
| 11C — Keybindings | COMPLETE | `phase-11c-keybindings` | Custom bindings, conflicts, reset and discoverability. | Verified bounded version-5 preference migration, canonical parsing, exact effective-binding conflicts, four-binding cap, individual/all reset, immediate accelerator reinstall, searchable all-command native dialog, accessible state, and native Wayland lifecycle. Confirmation-required and irreversible bindings retain reviewed defaults; no paths or command usage are stored. |
| 11D — Optional Vim mode | COMPLETE | `phase-11d-vim-mode` | Opt-in list/grid/Miller navigation mappings. | Verified off-by-default version-6 migration, persisted stateful toggle, h/j/k/l/g/G/o policy, bounded list/grid selection, existing Miller dispatch, file-view-only capture, visible non-color state, and two-launch native Wayland restoration. Editable widgets and dialogs remain native; no terminal behavior added. |
| 11E — Terminal integration | COMPLETE | `phase-11e-terminal-integration` | Preferred terminal and Open Terminal Here using safe argv. | Verified nine reviewed providers, deterministic explicit-preference fallback, capacity-4 worker and 32-child reaping cap, exact selected/current local-directory policy, direct absolute executable argv with raw working directory, version-7 migration, native chooser/action/accessibility lifecycle, and truthful unavailable state. No shell, password argv/environment injection, embedded terminal, or repository detection added. |

## Phase 12 — Productivity operations

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 12A — Archive engine | COMPLETE | `phase-12a-archive-engine` | ZIP, tar, tar.gz, tar.xz and reviewed 7z listing/extract/compress jobs. | Verified typed raw-path requests, no-follow sources, bounded member/byte/ratio/path plans, traversal/link/conflict rejection, private staging, Linux no-replace publication, cancellation/source revalidation, capacity-4 application executor, capacity-16 memory-only results, and hostile-format round trips. Passwords, overwrite, UI, and external helpers are excluded. |
| 12B — Archive UX | COMPLETE | `phase-12b-archive-ui` | Extract Here/To, compress, progress and safe backend password handoff. | Verified exact selection/destination planning, compact native Archives submenu, accessible Compress dialog, local Extract To chooser, shared Operations progress/cancellation, no-overwrite conflict guidance, truthful password unsupported state with no secret input, raw-path tests, and native Wayland action/lifecycle smoke. |
| 12C — Batch rename | NEXT | `phase-12c-batch-rename` | Previewed transforms, regex, sequences, metadata templates and undo. | 6P; verify whole-batch validation, collisions and non-UTF-8 policy. |
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
| 13G — Duplicate finder | PLANNED | `phase-13g-duplicate-finder` | “Check for Duplicates…” over explicit files or roots with size-first candidate grouping, streaming hashes, byte-for-byte confirmation, and review/reveal/Trash actions. | 10E/job boundary; no index requirement, symlink following, or automatic deletion; distinguish hard-link aliases, revalidate changing files, and verify cancellation, raw paths, permissions, huge/sparse files, mount boundaries, and hash-collision safety. |

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
