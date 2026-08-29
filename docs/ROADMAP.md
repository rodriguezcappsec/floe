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
| 7G — Navigation upgrades | COMPLETE | `phase-7g-navigation-upgrades` | Keyboard-accessible breadcrumbs, asynchronous exact-path completion, bounded recent locations and restored navigation history, command-line file/folder routing. | Verified exact raw breadcrumb and completion identities, bounded workers/history, responsive overflow, Private/Sensitive-compatible session reuse, GApplication one-local-target routing, parent/reveal, deterministic errors, and native GTK accessibility. No remote browsing or application-association work. |
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
| 9F — Preview polish | COMPLETE | `phase-9f-preview-polish` | Space toggle, live navigation, fullscreen, HiDPI and privacy/cache hooks. | Verified file-view-scoped, text-input-safe Space action, exact live generation following/focus restore, 50–400% presentation-only zoom/reset, fullscreen toggle, GTK monitor scaling, explicit cancel-and-purge memory cache, media retirement, accessibility and native lifecycle; sandbox stays 18L. |

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
| 12C — Batch rename | COMPLETE | `phase-12c-batch-rename` | Previewed transforms, regex, sequences, metadata templates and undo. | Verified bounded Unicode transforms, literal/regex/prefix/suffix/number/date/extension/case preview, whole-batch collision validation, explicit non-UTF-8 rejection, two-pass no-replace staging, cycle-safe apply, pre-commit cancellation, rollback/partial failure, shared progress, exact in-session undo, and accessible bounded preview. |
| 12D — Create/templates polish | COMPLETE | `phase-12d-create-templates` | Template discovery/management and immediate rename. | 6Q; no executable surprise; verify safe names, permissions and migration. Verified capacity-256/scan-4096 no-follow regular-file catalog, exact paths, native states/management, no-overwrite creation, post-refresh naming, and execute-bit stripping without source changes. |
| 12E — Links/duplicate polish | COMPLETE | `phase-12e-link-duplicate-polish` | Relative/absolute links, hard-link eligibility and duplicate naming. | 6Q; verify cross-filesystem and broken-link behavior. Verified explicit exact relative/absolute target planning, intentional broken links, same-filesystem regular-file hard-link preflight and identity revalidation, raw-name copy-suffix progression, native accessible choice UI, and existing bounded create/Operations integration. |
| 12F — Action integration | COMPLETE | `phase-12f-productivity-actions` | Context, palette and shortcut integration without a menu wall. | Verified shared list/grid/Miller file and background models, default Archives submenu, seven bounded optional groups, fixed essential actions, version-8 asynchronous persistence, header/palette/shortcut customization discovery, live GAction eligibility, accessible native editor, and Wayland lifecycle. Arbitrary commands, plugins, per-MIME rules, and later privacy actions remain excluded. |

## Phase 13 — Filter and search

Quick Filter and Search Files share one compact surface and query entry:
`Ctrl+F` opens Quick Filter and `Ctrl+Shift+F` opens Search Files directly. The
two modes retain separate bounded engines and truthful scope descriptions.

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 13A — Folder filter | COMPLETE | `phase-13a-folder-filter` | Instant text/glob/regex filtering, count and Escape clear. | Verified 256-scalar queries, Unicode/raw-byte policy, compile-once patterns, capacity-1 generation-superseding worker/latest-result mailbox, stable order, exact visible selection, refresh/sort/watch reapplication, location clearing, accessible native controls, 100,000-entry test and Wayland lifecycle. No recursion, filesystem/content reads, persistence or history. |
| 13B — Filename search | COMPLETE | `phase-13b-filename-search` | Streaming cancellable current-folder/subtree filename search and reveal. | Verified exact raw-path results, case-insensitive Unicode/raw-byte matching, no symlink descent or mount crossing, explicit skipped/depth/cap feedback, 100,000-result/1,000,000-entry bounds, capacity-1 generation-safe worker, 128-result batches, unified native controls, exact-path result actions, Reveal in Folder, and supplied app icon. No contents, remote roots, history, persistence, indexing, advanced predicates, or search ordering. |
| 13C — Advanced filters | COMPLETE | `phase-13c-search-filters` | Type, extension, MIME, size, date, owner, hidden, and case-aware filename predicates shared by Quick Filter and Search Files. | Verified structured bounded predicates, cheap-first evaluation, filename-only GIO MIME guessing, no-follow UID lookup, explicit unknown-metadata exclusion, predicate-only searches, temporary hidden policy, capacity-one generation-safe workers, accessible wrapping controls, strict workspace gates, and native GTK/Wayland lifecycle. Tags, persistence, content reads, remote roots, and result ordering remain excluded. |
| 13D — Content search | COMPLETE | `phase-13d-content-search` | Opt-in bounded text search with glob/regex/case controls. | Verified local-only no-follow reads, exact paths, UTF-8/BOM-declared UTF-16, binary/unsupported/changed/over-limit skip counters, Phase 13C predicates before reads, 64-result batches, capacity-1 generation cancellation, line/snippet/folder results, exact-path actions, strict gates and native Wayland lifecycle. No Trash, remote roots, links, mount crossing, extraction, persistence, indexing, or uploads. |
| 13E — Saved searches | COMPLETE | `phase-13e-saved-searches` | Versioned saved queries and privacy-aware history. | Verified explicit-only version-10 private preference persistence, exact raw roots, complete query/filter round trip, 64-entry validated catalog, independent corruption skipping, 32-entry deduplicated session-only recents with clear/suppress policy, exact-root replay, accessible save/list/delete/clear controls, and deterministic Name/Modified/Size result ordering. No implicit persistent history, indexing, tags, remote/global roots, or Private Mode claim. |
| 13F — Optional indexing | COMPLETE | `phase-13f-search-indexing` | Capability-reviewed index with complete non-indexed fallback. | Verified explicit single-root private filename/metadata-only index, exact raw paths, hidden-tree exclusion, no-follow/device/depth/entry/64-MiB bounds, directory and matching-entry staleness validation, versioned corruption-rejecting codec, `0600` atomic cache, version-12 opt-in, accessible compact controls, and automatic live fallback for missing/stale/corrupt/ineligible/busy indexes. No content, hidden/sensitive override, remote/global root, watcher rebuild, or Phase 18 claim. |
| 13G — Duplicate finder | COMPLETE | `phase-13g-duplicate-finder` | “Check for Duplicates…” over explicit files or roots with size-first candidate grouping, streaming hashes, byte-for-byte confirmation, and review/reveal/Trash actions. | Verified explicit exact roots, 4,096-root/1,000,000-file/100,000-directory/depth-128/256-GiB-file/1-TiB-hash bounds, same-device no-follow traversal, size-first candidates, reviewed Phase 10E SHA-256, byte confirmation, mutation checks, hard-link labels/reclaimable accounting, capacity-one cancellation worker, memory-only results, context/command action, accessible review/reveal/explicit Trash handoff, strict gates, and native Wayland lifecycle. No index dependency, remote/Trash scan, automatic/permanent deletion, duplicate-result persistence, upload, or secure-erasure claim; Phase 13G3 later adds only the validated derived-hash cache described below. |

| 13G2 — Duplicate finder workflows | COMPLETE | `phase-13g2-duplicate-finder-workflows` | Native setup for full recursive folder-tree discovery, exact copies of one selected file in a chosen tree, and existing selected-items scanning. | Verified contextual defaults, exact `PathBuf` scope retention, nested discovery, reference size-bucket restriction, unrelated-group exclusion, no double counting when reference already lies in scope, raw non-UTF-8 reference identity, non-regular reference rejection, unchanged same-device/no-follow/cancellable review-first safety. Exact duplicate means identical bytes; similar-media detection remains separate and planned. |

| 13G3 — Duplicate finder performance | COMPLETE | `phase-13g2-duplicate-finder-workflows` | Faster cold and warm exact-duplicate scans through bounded quick filtering, parallel SHA-256, and a validated private derived-hash cache. | Verified first/last 64-KiB quick samples reject most same-size nonmatches before SHA-256; hashing uses at most four workers and two active reads per device. A versioned 200,000-entry/64-MiB cache stores exact raw path, identity/timestamps, digest, and recency only under private `0700`/`0600` storage; exact fingerprint lookup, watcher/subtree invalidation, scan-time mutation rejection, corrupt/insecure/symlink rejection, and atomic writes are covered. Cache reuse never replaces quick filtering, SHA-256 identity requirements, or final byte-for-byte confirmation. Results remain memory-only; no upload, automatic deletion, perceptual similarity, or Phase 18Y work. |

## Phases 14–17 — Desktop and location integration

### Phase 14 — Generic desktop integration framework

Status: **COMPLETE**
Recommended branch: `phase-14-generic-desktop-integration`

Verified application-layer capability boundary for GIO launch, mounts/volumes,
XDG user folders, portals, notifications, Share availability, GTK/libadwaita
theme signals, Secret Service presence, and reliable session-lock signals. A
capacity-one generation-safe worker performs bounded session-bus probes and
returns path/content-free memory-only snapshots. The native refreshable status
dialog gives available, limited, and unavailable text reasons; missing optional
services preserve existing local browsing. No compositor branch, desktop type
in core, secret read, notification/share transmission, or lock control was
added. Strict Rust, real GTK, and isolated Wayland action/lifecycle gates pass.

Post-Phase-14 appearance correction: Floe vendors a pinned MIT-licensed
Phosphor Core 2.1.1 Regular subset for application-owned interface chrome.
File and folder entries expose persistent live Floe Color, Phosphor Monochrome,
and System Theme styles without changing thumbnail, path, MIME, or filesystem
behavior. Version-12 migration, all 44 symbolic resources, real GTK, and a
two-launch isolated Wayland action/state/lifecycle gate are verified. A
follow-up regression separates plain text from office documents/PDF, adds
distinct family-owned System Theme fallbacks, and clears stale GTK image
storage during style changes. A second regression keeps known file-type icons
on filesystems that synthesize execute bits while retaining executable fallback
for unknown or extensionless executables. These corrections do not advance or
broaden Phase 15.

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 14B — Privileged local browsing | COMPLETE | `phase-14b-privileged-local-browsing` | Open a local folder through a typed GIO/GVfs `admin://` provider with polkit-owned authentication and persistent visible authority state. | Verified experimental read-only opt-in, private validated URI identity, exact non-UTF-8 paths, 128-entry pages/4,096-entry cap, no-follow metadata, generation rejection, 120-second authorization/30-second page cancellation, accessible badge/navigation/return controls, UID-stable native lifecycle. No elevation, password handling, arbitrary admin URI, local-job reuse, preview/terminal/archive/launcher/custom-action/plugin capability, silent fallback, or privileged mutation. |

### Phase 15 — Niri integration

Status: **DEFERRED**
Recommended branch: `phase-15-niri-integration`

Goal: add optional detection/IPC, output/workspace awareness, spatial launch,
and useful Miller enhancements behind Phase 14. Niri must never be required.
Acceptance includes missing/stale socket, protocol failure, graceful fallback,
and native Niri smoke tests.

### Phase 16 — KDE Plasma integration

Status: **DEFERRED**
Recommended branch: `phase-16-plasma-integration`

Goal: add standards-first Plasma capability detection, KDE services only where
useful, and KWallet only after secret-storage requirements are proven. KDE
Frameworks are excluded for cosmetics. Acceptance requires generic fallback,
service-unavailable tests, no KDE types in core, and Plasma Wayland smoke.

### Phase 17 — Remote and external locations

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 17A — Remote model | DEFERRED | `phase-17a-remote-location-model` | URI/location identity, capabilities, timeouts and offline state separate from local paths. | User-deferred on `2026-08-27`; no URI semantics in local core. |
| 17B — GIO remote browsing | DEFERRED | `phase-17b-gio-remote-browsing` | Reviewed GIO SFTP, SMB, WebDAV and NFS browsing/operations. | User-deferred on `2026-08-27`; FTP and silent plaintext staging remain excluded. |
| 17C — MTP/devices | DEFERRED | `phase-17c-mtp-devices` | Android/MTP transfer and disconnect recovery; optional KDE Connect enhancement. | User-deferred on `2026-08-27`; existing local GIO device support remains. |
| 17D — Remote recovery | DEFERRED | `phase-17d-remote-recovery` | Retry/offline transitions, remote thumbnail policy, saved connections and credential abstraction. | User-deferred on `2026-08-27`; depends on deferred remote foundations. |

## Phase 18 — Privacy, security, and data integrity

Phase 18A establishes the architecture baseline only; it implements no runtime
security feature. Every later leaf follows `docs/PRIVACY_SECURITY.md`,
`docs/security/THREAT_MODEL.md`, `docs/security/PHASE_18A_DECISIONS.md`, and
`docs/security/PHASE_18_TEST_PLAN.md`. Every security-critical dependency
requires an implementation-time review. Security terms are not interchangeable.

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 18A — Threat model | COMPLETE | `phase-18a-security-threat-model` | Revalidated assets, adversaries, non-protections, formats, secrets, vaults, caches, sandbox and integrity architecture before code. | Documentation-only baseline verified: threat IDs, 16 decision records, dependency rationale and traceable 18B–18AA test plan; no dependency/runtime implementation. |
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
| 18T — Integrity tools | COMPLETE | `phase-18t-integrity-tools` | Saved SHA-256 fingerprints and portable `SHA256SUMS` generation/verification. | Reviewed SHA-256 reuse, raw-path manifest escaping, bounded cancellation, no-overwrite publication, and changed/missing/new reporting verified; hashes are not authenticity or safety. |
| 18U — Integrity monitoring | COMPLETE | `phase-18u-integrity-monitoring` | Explicit local baselines with bounded coalesced change reporting. | Private strict storage, same-device no-follow watches, overflow/offline rescan state, cancellation, and changed/missing/new reporting verified; this is not intrusion detection. |
| 18V — Verified copy | COMPLETE | `phase-18v-verified-copy` | Optional Copy and Verify with revalidated source/destination SHA-256. | Ordinary Copy remains unchanged; verified, not-created, and copied-but-unverified outcomes, races, corruption, cancellation, and retry boundaries are verified. |
| 18W — Verified USB | COMPLETE | `phase-18w-verified-usb-transfer` | Explicit Copy, Verify, Flush, Eject/Unmount workflow. | Exact removable mount/device revalidation, bounded `syncfs`, partial states, cancellation, and GIO removal verified with mocked/disposable targets; “safe to remove” appears only after successful removal. Real USB lab validation remains skipped without disposable media. |
| 18X — Data-loss guardrails | COMPLETE | `phase-18x-data-loss-guardrails` | Protected Folders and thresholded destructive preflight. | Private fail-closed exact-path policy, bounded preflight, single-use generation-bound permits, all destructive dispatch routes, native Protect/Unprotect/status UI, and corrupt-store reset are verified. Protection prevents mistakes; it is not encryption or access control. |
| 18Y — Operation recovery | COMPLETE | `phase-18y-operation-recovery` | Private bounded raw-path journal, fail-closed restart review, conservative retry/reveal/resolve choices, identity-checked Create Undo through Trash. | Verified journaling before copy/move/create mutation, success cleanup, corrupt/insecure blocked-store reset, stale/uncertain output review, no silent deletion or overwrite. |
| 18Z — Security Center | PLANNED | `phase-18z-security-center` | Calm vault, sensitive, session, integrity and finding status/actions. | 18E/18H/18K/18N/18R/18T; no fear score; verify state and accessibility. |
| 18AA — Security audit | PLANNED | `phase-18aa-security-audit` | Crypto, dependencies, secrets, caches, parsers, vaults, sandbox and recovery audit. | 18B–18Z; no stable Phase 18 claim before pass; close or record every finding. |

Security phases preserve originals on failed encryption or sanitization, never
log or pass secrets in process arguments, never silently downgrade protection,
and add hostile-input and cache/notification/history leak tests.

## Phase 19 — Extensibility and developer features

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 19A — Git awareness | PLANNED | `phase-19a-git-awareness` | Cheap opt-in repository root, branch, status badges and relative-path actions. | 6T/11E; no cost outside repositories; verify large repos and missing Git. |
| 19B — Associations and custom actions | COMPLETE | `phase-19b-associations-custom-actions` | Inspect/set/reset XDG MIME defaults plus context/palette file-type actions with safe argv and eligibility. | Verified bounded GIO mutation worker, explicit result feedback, version-14 private preference records, 32-action/128-selection limits, exact raw argv placeholders, MIME/file/folder/multi-selection eligibility, direct fixed-capacity child launch/reaping, native editor/context/palette flows; no shell, remote, privileged, secret, or plugin authority. |
| 19C — Customization | PLANNED | `phase-19c-customization` | Theme tokens, templates, editor, compare and Share tools. | Appearance/12D/19B; no plugin runtime; verify validation, rollback and upgrades. |

A general plugin runtime is **DEFERRED** until demonstrated demand and a
capability, isolation, permission, and failure-containment design exist.

## Phase 20 — Settings, visual, accessibility, and QoL audit

Status: **COMPLETE**
| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 20A — Settings Center | COMPLETE | `phase-20a-settings-center` | Dedicated searchable native Settings Center organizing appearance, browsing, views/layout, search/preview, operations/safety, applications, shortcuts/menus, and accessibility. | Verified case-insensitive plain-language search, live authoritative preferences, specialized-editor links, `Ctrl+,`, stable GTK accessibility metadata, no duplicate settings store or weakened irreversible confirmations. |
| 20B1 — Sort By completeness | COMPLETE | `phase-20b1-sort-by-menu` | Discoverable native Sort By menu; created/accessed and bounded KDE-compatible rating/tag/comment ordering; direction, folders-first, and hidden-last policy. | Worker-owned deterministic local sorting, exact-path tie-breakers, unknown-last semantics, v15 preference/v2 session migration, honest disabled indexed-media fields, focused/core/native gates. |
| 20B1A — Advanced metadata sort index | COMPLETE | `phase-20b1a-metadata-index` | Replace every disabled Document/Image/Audio/Video/Other sort row with an explicit bounded local-directory metadata scan, progress/cancellation, and private cache controls. | Reuses Phase 10F providers, optional reviewed `ffprobe`, exact raw paths, no-follow identity, 4,096-entry/read/process/cache bounds, watcher invalidation, v16 preference migration; no global/background/remote indexing or metadata editing. |
| 20B2A — Window size persistence | COMPLETE | `phase-20b2a-window-size-persistence` | Restore the last normal Floe width/height across launches. | Version-17 private preference tuple, bounded corruption-safe migration, actual GDK surface notifications, debounce and shutdown capture; no polling, window position, monitor/workspace, maximized/fullscreen, or compositor-specific persistence. |
| 20B2 — Visual, accessibility, and QoL audit | COMPLETE | `phase-20b2-completeness-audit` | Ten bounded daily-driver outcomes: date/size collapsible groups, split persistence, click policy, invert selection, column order/autosize, appearance controls, Escape/focus, non-color accessibility, scaling policy, and detailed feedback/localization boundary. | Version-18 private preference and session-v3 migration; Grid View uses full-width group sections over one authoritative selection; deterministic/core/app/real-GTK/E2E-preflight/native-Wayland gates. Translation catalogs, Orca end-to-end, and physical multi-monitor fractional-scale measurements remain assigned to Phase 21. |

Goal: close the selected high-impact `FEATURE_MATRIX` gaps across appearance,
grouping, layout, selection, focus, non-color accessibility, logical scaling,
feedback, localization boundaries, and recorded daily-driver QoL without
claiming translation or environment testing that was unavailable.

Foundation already complete: the five appearance presets can be selected live
from a persistent radio-style header submenu. Phase 20 retains only the broader
appearance controls and audit work recorded in the feature matrix.

- Excludes: weakening secure defaults or exposing unnecessary crypto knobs.
- Dependencies: all preceding user-facing families.
- Acceptance: measured matrix audit, no accessibility-critical finding,
  migration tests, phase gate, and Niri/Plasma/generic Wayland smoke where
  environments are available.

## Phase 21 — Performance, packaging, and release hardening

| Phase | Status | Recommended branch | Scope | Dependencies; exclusions; acceptance |
| --- | --- | --- | --- | --- |
| 21A — Performance | NEXT | `phase-21a-performance` | Profile 100k folders, thumbnails, search, operations, metadata, and integrity workloads that currently exist. | No speculative rewrite and no benchmark of unimplemented crypto/vault features; record reproducible CPU/memory/latency budgets before optimization. |
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
