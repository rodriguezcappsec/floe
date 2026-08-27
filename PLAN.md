# Plan: Floe Phases 18T–18X — Integrity and Data-Loss Safety

## Contract

Implement exactly these five dependency-ordered roadmap leaves:

1. **18T Integrity Tools:** saved SHA-256 fingerprints plus path-safe portable
   `SHA256SUMS` generation and verification, reusing Phase 10E streaming hashes.
2. **18U Integrity Monitoring:** explicit local baselines and bounded/coalesced
   changed, missing, and new-file reporting. This is not intrusion detection.
3. **18V Verified Copy:** an optional Copy and Verify job that compares
   revalidated source and destination bytes through SHA-256 after publication.
4. **18W Verified USB Transfer:** explicit Copy → Verify → Flush → Eject workflow
   with truthful partial states and no safe-removal claim before successful eject.
5. **18X Data-Loss Guardrails:** persisted exact-path Protected Folders and
   thresholded destructive-operation preflight. Protection prevents mistakes;
   it is not encryption or access control.

Preserve exact `PathBuf`/`OsString` identity, no-follow/no-shell/no-silent-
overwrite policy, bounded workers and retained state, GTK responsiveness,
existing operation semantics, and Phase 18A security terminology. Add no Niri,
Plasma-specific, remote, MTP, encryption, vault, sandbox, recovery-journal, or
Security Center work. Do not claim authenticity, malware detection, secure
erase, universal monitoring, or guaranteed safe removal.

## Depth tree and dependency order

```text
18T–18X root integration
├── 18T checksum/fingerprint/manifest engine + native UI
├── 18U baseline/monitor engine + native UI         (depends on 18T)
├── 18V verified-copy operation + progress/UI       (depends on 18T)
├── 18W verified-removable workflow + device UI     (depends on 18V)
└── 18X protected-path/preflight policy + settings/UI
```

Leaves run in order because later phases reuse earlier types and shared app
controllers. Each leaf owns `gates/phase-18*.md`; root integration evidence is
recorded at the top of `GATES.md`. Before each leaf, inspect its existing core,
worker, command, preference, GTK, and test seams and update the documented file
ownership if reality differs. Do not overwrite unrelated dirty icon work.

## Permanent testing layers

- Lowest-layer Rust unit/property tests for manifests, raw names, baselines,
  monitoring diffs/coalescing, verification states, workflow transitions,
  protected-root matching, and threshold decisions.
- Filesystem integration tests only below `tempfile` roots for links, races,
  changed/missing/new files, corrupt manifests, copy corruption, full/partial
  workflow outcomes, and protected targets. Never use real HOME, Trash, mounts,
  or removable devices.
- GTK component/accessibility tests for commands, dialogs, state, progress,
  confirmation, and non-color wording; ignored by ordinary headless tests.
- Isolated native Wayland smoke for user-visible actions and clean Quit where
  the environment permits. Real removable-device gates require explicit
  disposable media and must otherwise be reported as skipped, never simulated
  against user storage.
- Every phase runs focused tests while developing. Root completion requires
  formatting, workspace check, strict Clippy, complete tests, native build,
  diff hygiene, gate checker, status documents, and exactly Phase 18Y as NEXT.

## Status log

- `2026-08-27`: Root plan and leaf ledgers created; implementation inspection
  begins with Phase 18T.
- `2026-08-27`: Parent reverified Phase 18T at 4/4 gates with all 18 focused
  tests passing.
- `2026-08-27`: Parent reverified Phase 18U at 4/4 gates with 13 app and 5 core
  focused tests passing; monitoring remains explicit, bounded, local, and does
  not claim intrusion detection.
- `2026-08-27`: Parent reverified Phase 18V at 4/4 gates with 10 app and 2 core
  focused tests passing; copied-but-unverified output remains explicit and
  ordinary Copy semantics are unchanged.
- `2026-08-27`: Parent reverified Phase 18W at 4/4 gates with all 11 aggregate
  focused tests passing; real removable-media validation is explicitly skipped
  because no disposable lab device was provided.
- `2026-08-27`: Parent reverified Phase 18X at 4/4 gates with 20 app, 21
  integration, and 7 core focused tests passing. The complete Phase 18 family
  filter passes 71 app, 21 integration, and 14 core deterministic tests.
- `2026-08-27`: Root integration formatting, workspace check, strict Clippy,
  466 app plus 21 integration plus 146 core tests, native build, separate GTK
  contracts, E2E harness contracts, leaf gate checker, and diff hygiene pass.
  Native semantic E2E is skipped because Dogtail/pyatspi are unavailable; real
  USB is skipped because no disposable lab device was provided. An isolated
  D-Bus lifecycle retry was blocked by portal services closing the disposable
  bus, so no additional native-lifecycle claim is made.

## Status

COMPLETE. Phases 18T–18X satisfy their 20 leaf gates and root integration
evidence. Exactly Phase 18Y — Operation Recovery is recommended next and is not
implemented by this plan.

---

# Plan: Floe Phase 18A — Security Threat Model

## Contract

- Revalidate the implemented Phase 14 baseline and every planned Phase 18
  security/privacy/integrity family against explicit assets, adversaries,
  non-protections, authority boundaries, data classes, and failure behavior.
- Record decisions as **Accepted for later implementation**, **Candidate**,
  **Deferred**, or **Rejected**. Phase 18A selects no Rust dependency, crypto
  library, vault backend, credential backend, or sandbox mechanism.
- Provide dependency rationale and a traceable implementation-time test plan for
  portable encryption, secrets, vaults, caches/history, provider isolation,
  suspicious/privacy inspection, integrity, verified transfer, guardrails,
  recovery, privileged access, and the final security audit.
- Mark Niri, Plasma-specific integration, remote browsing/recovery, and MTP as
  deferred while retaining generic Wayland/Plasma compatibility gates.
- Add no runtime feature, GTK surface, filesystem operation, secret handling,
  cryptographic claim, dependency, or Phase 18B+ implementation.
- Preserve exact-path, no-shell, no-overwrite, no-follow, bounded-worker, and
  truthful-claim rules already implemented.

## Applicable verification layers

- Documentation structure and cross-reference checks for authoritative sources,
  decision IDs, threat IDs, phase/test mappings, and exactly one roadmap NEXT.
- Dependency manifest/lockfile audit proving Phase 18A adds no runtime crate.
- Existing formatting, workspace check, strict Clippy, and complete tests to
  prove a documentation-only phase did not disturb the verified baseline.
- Native GTK/Wayland smoke is not applicable because Phase 18A changes no
  runtime behavior or UI; later user-visible security phases retain those gates.

## Status

COMPLETE. The threat model, 16 stable decisions, and 26-leaf Phase 18 test
traceability plan are cross-linked from the authoritative project documents.
All Phase 18A gates and repository regressions pass; Phase 18T is the sole next
phase and has not started.

---

# Regression plan: post-Phase-14 synthetic execute-bit icon handling

## Contract

- Make recognized extension families authoritative for presentation even when
  exFAT, FAT, NTFS, network, or other mounts synthesize execute permission bits.
- Preserve executable presentation for extensionless or unknown executable
  files, including existing AppImage behavior.
- Keep the change in app-layer icon policy; perform no MIME/content/filesystem
  probe and do not alter operation permissions or execution authorization.
- Add a lowest-layer regression using executable regular-file fixtures.
- Preserve Phase 15 as the sole roadmap NEXT phase.

## Status

COMPLETE. Known file types now outrank synthetic execute bits for icon
presentation, while unknown and extensionless executables retain executable
artwork. Focused/full Rust, strict Clippy, real GTK, native build, diff hygiene,
and documentation gates pass. Exactly Phase 15 remains the sole roadmap NEXT.

---

# Regression plan: post-Phase-14 file-type icon distinction

## Contract

- Reproduce why distinct PDF and plain-text families render identically or
  disappear in one or more of Floe Color, Phosphor Monochrome, and System Theme.
- Keep classification and rendering policy in `floe-app`; preserve exact paths,
  existing thumbnail precedence, responsive GTK behavior, and live switching.
- Make every current semantic family resolve to a usable icon, with a distinct
  PDF/text outcome even when a host icon theme omits or aliases MIME artwork.
- Reset stale `gtk::Image` state when changing representations and rebind every
  already-loaded list/grid/search/Miller presentation.
- Add lowest-layer regression tests plus a real GTK contract and native smoke.
- Do not start Phase 15 or broaden the supported taxonomy speculatively.

## Applicable testing layers

- Rust policy/resources: extension families, distinct style mappings, registered
  app-owned resources, bounded System Theme fallback chains.
- GTK component: resolved paintables, stale-state reset, immediate live rebind.
- Native Wayland: visible PDF/TXT fixture, three-style switching, persistence,
  responsiveness, and clean quit.

## Status

COMPLETE. Plain text is separate from office documents and PDF; all fifteen
families have distinct app-owned fallbacks, stale GTK image storage is cleared,
focused/full Rust tests, real GTK resolved-paintable checks, E2E contracts, and
native Wayland switching/liveness/clean-Quit smoke pass. Exactly Phase 15
remains the sole roadmap NEXT phase.

---

# Plan: Floe post-Phase-14 — Phosphor Icon System

## Contract

- Vendor a reviewed, pinned subset of Phosphor Core 2.1.1 Regular SVGs with its
  MIT license. Icons remain local resources; Floe performs no runtime download.
- Use Phosphor symbolic icons for Floe-owned navigation/action/sidebar chrome
  while preserving stable accessible labels, actions, focus, and keyboard paths.
- Add one persistent live entry-icon style with exactly three choices: Floe
  Color, Phosphor Monochrome, and System Theme. Existing thumbnails continue to
  replace generic icons and exact file paths remain authoritative.
- Keep icon policy in `floe-app`; add no filesystem I/O, worker, core dependency,
  compositor branch, MIME probe, or future Phase 15 integration.
- Rebind the already-loaded virtualized model when style changes. Do not perform
  synchronous enumeration or icon filesystem work on GTK callbacks.
- Preserve Phase 15 as the sole roadmap `NEXT` phase.

## Applicable testing layers

- Rust policy/preferences: stable style IDs/order/labels, invalid and legacy
  fallback, version-12 round trip, semantic icon-family mapping.
- Resource tests: every vendored Phosphor alias resolves, is symbolic SVG using
  `currentColor`, and the original Floe Color resources remain available.
- GTK component/native: menu action state, immediate live rebind, accessible
  toolbar controls, list/grid generic fallback, thumbnail replacement, clean
  Wayland launch/action/Quit lifecycle.

## Status

COMPLETE. Stable style policy, version-12 migration, 42 pinned symbolic
resources, Phosphor interface chrome, live virtualized rebinding, shared toast
escaping, strict workspace tests, real GTK, and two-launch isolated Plasma
Wayland action/state/persistence/lifecycle behavior are verified in `GATES.md`.
Phase 15 remains the sole recommended next phase and has not started.

---

# Plan: Floe Phase 14 — Generic Desktop Integration Framework

## Contract

- Create one application-layer `DesktopIntegration` capability boundary that
  inventories generic Linux desktop services without leaking GIO/GTK, desktop,
  compositor, or environment-specific types into `floe-core`.
- Represent standards-backed capabilities for local/URI launch, GIO mounts and
  volumes, XDG user directories, portals, notifications, Share, theme signals,
  credential service, and reliable session-lock signals. Capabilities must
  distinguish available, unavailable, and degraded/unknown with a human reason.
- Detect asynchronously on one bounded application worker. Missing session bus,
  portal, Secret Service, notification, theme, or lock service is normal and
  must preserve browsing, local paths, GIO launch, and existing device behavior.
  No GTK callback may perform blocking capability probing.
- Route existing generic launcher/device/location/theme facts through the
  boundary where coherent, expose a native **Desktop Integration** status dialog
  and human command, and keep all fallbacks truthful under generic Wayland,
  Plasma, Niri, and no-specialized-backend environments.
- Use standard GLib/GIO/XDG/session-bus signals only. Add no Niri IPC, KDE
  Framework, Plasma type, compositor conditional branches, credential reads,
  notification contents, Share transmission, session-lock control, or remote
  filesystem phase work.
- Keep capability snapshots memory-only and path/content-free. Do not log bus
  names, account identifiers, filenames, secrets, notification bodies, or user
  paths merely to report capabilities.

## Applicable testing layers

- Pure application policy tests: stable capability IDs/order/status/fallback,
  no-specialized-backend behavior, missing/malformed service probes.
- Worker tests: bounded request/result channel, generation supersession, clean
  shutdown, memory-only snapshots, no GTK-thread blocking.
- Integration/UI: native accessible status dialog and command, existing
  launcher/device/location/theme regressions, isolated generic Wayland plus
  first-class Plasma smoke, D-Bus health and clean Quit.

## Status

COMPLETE. Acceptance evidence is recorded in `GATES.md`. Phase 15 is the sole
recommended next phase and has not started.

---

# Archived plan: Floe Phase 13G — Duplicate Finder

## Contract

- Add a GTK-independent duplicate scan over explicitly selected absolute local
  files and/or directory roots. Preserve exact `PathBuf`/`OsStr`, cap roots,
  traversed entries/directories/depth/results/bytes, stay on each root device,
  and never follow symbolic links.
- Discover candidates cheaply by exact regular-file size, then stream the
  reviewed Phase 10E SHA-256 implementation only for same-size candidate sets,
  and finally compare candidate bytes before calling independent files equal.
  A digest match alone is never proof.
- Open inputs no-follow, support cooperative cancellation, and revalidate
  device/inode/size/mtime/ctime before and after hashing and byte comparison.
  Report inaccessible, changed, over-limit, mount, link, and sparse/huge-file
  exclusions truthfully.
- Distinguish hard-link aliases from independent copies by device/inode. Never
  count aliases as reclaimable bytes, follow symbolic links, cross mount roots,
  upload hashes/content, persist results/history, require Phase 13F's index, or
  delete anything automatically.
- Add selection-aware **Check for Duplicates…** to useful list/grid/Miller
  context surfaces and command discovery. Run one bounded application worker;
  present cancellable progress and accessible native review groups with exact
  reveal plus explicit ordinary Trash handoff. No filesystem work in GTK
  callbacks.
- Exclude automated cleanup, permanent deletion, fuzzy/similar matching,
  content-defined chunks, background scans/watchers, remote/Trash roots,
  Phase 14 desktop integration, and unrelated refactors.

## Applicable testing layers

- Core hostile fixtures: size-first pruning, reviewed SHA-256, byte-confirmed
  collision safety, hard links, raw names, links, mutation, permissions,
  cancellation, huge/sparse limits, mount/depth/result/root bounds.
- Application worker: capacity-one generation lifecycle, progress/result
  bounds, cancellation, exact memory-only outcomes, clean shutdown.
- GTK/controller: context/menu/action parity, explicit-root planning, accessible
  review/reveal/Trash policy, native Wayland action/liveness/Quit smoke.

## Status

COMPLETE. Size/hash/byte-confirmed core discovery, reviewed SHA-256 application
worker, cancellation, hard-link accounting, context/command integration, native
review/reveal/Trash handoff, strict workspace suite, real GTK gate, and isolated
Wayland action/lifecycle smoke are verified in `GATES.md`. Exactly Phase 14 is
recommended next.

---

# Archived plan: Floe Phase 13F — Optional Search Indexing

## Contract

- Add a GTK-independent, versioned filename/metadata index for one explicitly
  chosen local root. Preserve exact `PathBuf`/`OsStr` bytes, directory
  fingerprints, no-follow file identity, and all metadata required by Phase
  13C predicates. The index must never contain file contents or snippets.
- Index building is explicit and optional, runs on one bounded application
  worker, stays on the root filesystem, never descends symbolic links, and
  excludes hidden entries/directories conservatively because Phase 18
  Sensitive Folder, Private Mode, and vault classification do not yet exist.
- Persist the optional index only in a private (`0600`) bounded cache file via
  atomic sibling replacement. Reject relative/Trash/remote/symbolic roots,
  malformed records, excessive paths/records/bytes, and unknown versions.
- Accelerate eligible subtree **Search Files** requests only. Before returning
  indexed results, validate the root and every recorded directory fingerprint;
  validate matching entries no-follow before presentation. A missing, corrupt,
  stale, policy-ineligible, or busy index must automatically run the existing
  bounded non-indexed search with truthful status feedback.
- Expose accessible native Enable, Build/Rebuild, and Clear controls with a
  clear capability description: filenames and metadata only, hidden content
  excluded, no content-search acceleration. Persist only the reviewed enabled
  boolean in preferences; explicit saved searches remain independent.
- Exclude content indexing, background filesystem watching/rebuilds, global or
  remote indexes, hidden/sensitive overrides, tags, Phase 13G duplicates,
  Phase 14 desktop integration, Phase 18 privacy claims, and unrelated refactors.

## Applicable testing layers

- Core fixtures: bounded traversal, raw names, symlink/mount/hidden policy,
  codec corruption/version/capacity, directory and entry stale detection,
  predicates, ordering, cancellation, and live-search fallback signal.
- Application worker/persistence: bounded request/event channels, private
  atomic cache, load/build/query/clear lifecycle, generation supersession,
  corrupt/missing/stale fallback, clean shutdown.
- GTK/controller: accessible capability controls, enable/build/clear actions,
  indexed/live status, saved-search compatibility, native Wayland D-Bus health
  and clean Quit.

## Status

COMPLETE. Core index policy/codec/query, bounded application persistence worker,
compact native controls, automatic live fallback, strict workspace suite, real
GTK component gate, and isolated Wayland cache/action/lifecycle smoke are all
verified in `GATES.md`. Exactly Phase 13G is recommended next.

---

# Archived plan: Floe Phase 13E — Saved Searches

## Contract

- Add a GTK-independent validated saved-query model for Search Files and Search
  Contents. Preserve exact absolute `PathBuf` roots, explicit folder/subtree
  scope, Text/Glob/Regex, Match Case, hidden policy, and every Phase 13C
  predicate. Reject relative roots, empty names, invalid queries, malformed
  filters, duplicates, and over-capacity data.
- Persist only searches the user explicitly names and saves. Use a versioned,
  private (`0600`), bounded file written by one capacity-one application worker
  with atomic sibling replacement. Preserve non-UTF-8 root bytes on Unix; never
  reconstruct a path from display text. Corrupt/unknown records are skipped
  independently without discarding valid records.
- Keep recent executed searches bounded, deduplicated, and memory-only for the
  current process. Provide visible Clear Recent control. Do not persist implicit
  history, snippets, results, counters, selection, or usage.
- Add accessible native Save Search and Saved Searches controls to the unified
  search surface. The manager must distinguish saved and recent entries, run a
  selected query against its exact saved root, and delete saved entries only
  after explicit activation. Missing/unavailable roots fail through ordinary
  search feedback; they are not silently rewritten to the current folder.
- Add deterministic result ordering by Name, Modified, or Size without changing
  exact-path identity or performing filesystem work on GTK callbacks. Content
  matches for one file retain stable line order.
- Exclude Phase 13F indexing, Phase 13G duplicate finding, Phase 14 desktop
  integration, tags, global/remote roots, Private Mode/Sensitive Folder claims,
  persistent implicit history, result export, and unrelated refactors.

## Applicable testing layers

- Core model/catalog tests: validation, capacity, deduplication, exact raw roots,
  all filter fields, session history suppression/clear, deterministic ordering.
- Application persistence tests: versioned round trip, migration, corruption,
  `0600` mode, atomic replace, capacity-one latest-save behavior and shutdown.
- UI/controller tests: accessible labels, saved/recent manager policy, exact-root
  replay, delete/clear actions, filename/content mode restoration.
- Strict workspace gates and isolated native Wayland D-Bus/lifecycle smoke.

## Status

COMPLETE. Every Phase 13E gate has measured evidence in `GATES.md`. Exactly
Phase 13F optional indexing is recommended next; later phases remain excluded.

---

# Archived plan: Floe Phase 13D — Content Search

## Contract

- Add an explicit **Search Contents** mode to the existing unified search
  surface. It searches only local regular files rooted at the active folder,
  with This Folder and Include Subfolders scopes; it never searches Trash,
  remote locations, symbolic-link targets, or another filesystem device.
- Add a GTK-independent bounded content-search engine with exact `PathBuf`
  identity, Text/Glob/Regex query modes, explicit Match Case, cancellation,
  stable line-number/snippet results, and explicit result/file/byte/depth caps.
- Read only candidate regular files that pass the existing Phase 13C advanced
  predicates. Use no-follow opens and revalidation. Reject binary/NUL-bearing,
  over-limit, unsupported-encoding, inaccessible, or changed inputs with
  truthful aggregate counters instead of lossy fallback or silent broadening.
- Support bounded UTF-8 plus BOM-declared UTF-16LE/UTF-16BE text. Do not guess
  legacy encodings, execute helpers, inspect archives, upload content, create an
  index, persist queries/results/snippets, or log file contents.
- Run traversal and content reads on one capacity-one application worker with
  bounded response batches and generation cancellation. GTK callbacks only
  submit typed requests and render events.
- Reuse ordinary exact-path Open/Open With/Properties/Reveal actions while the
  result surface visibly presents filename, containing folder, line number, and
  a whitespace-normalized bounded snippet. Closing, navigating, or switching
  modes cancels incompatible work and clears content-derived state.
- Exclude Phase 13E saved searches/history, Phase 13F indexing, Phase 13G
  duplicate discovery, Phase 14 desktop integration, remote roots, regex
  replacement, archive/PDF/document extraction, and search-result sorting.

## Applicable testing layers

- Core fixtures: UTF-8/UTF-16, case/Text/Glob/Regex, raw names, binary and
  malformed inputs, symlink/mount policy, byte/result/depth caps, cancellation,
  changed-file revalidation, and snippets.
- Application worker: capacity-one request/response bounds, generation
  supersession, streaming batches, clean shutdown, no GTK-thread reads.
- UI/accessibility: visible plain-language mode/help, result columns/labels,
  error/stopped/limit feedback, exact Reveal mapping, real GTK component gate.
- Strict workspace gates plus isolated native Wayland D-Bus action/liveness/Quit
  smoke before documentation can mark the phase complete.

## Status

COMPLETE. All Phase 13D gates have measured evidence in `GATES.md`. Exactly
Phase 13E saved searches is recommended next; later phases remain excluded.

---

# Archived plan: Floe Phase 13C — Advanced Filters

## Contract

- Add one optional advanced-filter section inside the existing unified search
  surface. It must serve Quick Filter and Search Files without creating a third
  search pipeline.
- Support combined filename, entry-type, extension, MIME, size, modified-date,
  Unix owner, and hidden predicates. Filename matching retains Text, Glob, and
  Regex and gains one explicit Match Case control.
- Own the predicate model and bounded validation in `floe-core`. Evaluate cheap
  facts first, define missing/unknown metadata semantics explicitly, and leave a
  tag-ready boundary without implementing tags.
- Preserve exact `PathBuf`/`OsStr` identity. Display text must never become a
  filesystem path.
- Keep both existing capacity-one application workers responsive and
  generation-safe. Resolve MIME and owner metadata lazily on workers only when
  active predicates require them; MIME guessing must not read file contents.
- Let Quick Filter temporarily include or isolate hidden loaded entries without
  mutating the global Show Hidden preference. Search Files supports Current
  Setting, Include Hidden, and Hidden Only under its existing traversal bounds.
- Allow an empty filename query only when at least one advanced predicate is
  active. Preserve cancellation, stale-result rejection, exact Reveal,
  memory-only privacy, and all Phase 13A/13B limits.
- Expose compact wrapping native controls with visible labels, plain-language
  descriptions, Apply, and Clear Filters. Exclude tags, content search, saved
  searches/history, indexing, remote roots, persistence, and search-specific
  sorting/grouping.

## Applicable testing layers

- Deterministic core predicate tests for combinations, case, raw names,
  validation, missing metadata, and hidden policy.
- Application worker tests for lazy MIME/owner resolution, fixed capacity,
  generation supersession, and both filter/search engines.
- GTK/accessibility contracts and isolated native Wayland action/liveness/Quit
  smoke for the shared wrapping controls and mode transitions.

## Status

COMPLETE. Structured core predicates, both bounded application workers, the
shared accessible native controls, strict workspace gates, real GTK component
coverage, and isolated Plasma Wayland D-Bus/lifecycle smoke are verified in
`GATES.md`. Exactly Phase 13D content search is next; later phases remain
excluded.

---

# Archived plan: Floe Phase 13B — Filename Search

## Contract

- Add explicit case-insensitive filename-only search rooted at the active local
  folder with fixed `This Folder` and `Include Subfolders` scopes. It must never
  read file contents or search remote/non-local roots.
- Preserve exact `PathBuf`/`OsStr` identity and reuse Phase 13A's reviewed
  Unicode text plus raw-byte non-UTF-8 matching semantics. Cap queries at 256
  Unicode scalar values.
- Run traversal on one capacity-one application worker. Stream batches of at
  most 128 exact results through bounded response state; support generation
  cancellation, explicit Stop, and stale-event rejection.
- Cap one search at 100,000 matches, 1,000,000 examined entries, 100,000
  directories, and depth 128. Report incomplete/skipped states truthfully.
- Traverse with no-follow metadata, never descend symbolic links, and never
  cross the root filesystem device. Continue past inaccessible child entries or
  directories while counting skips; root failures remain explicit.
- Consolidate Quick Filter and Search Files into one native search surface:
  `Ctrl+F` opens Quick Filter, `Ctrl+Shift+F` opens Search Files, and a visible
  mode selector explains their different scope. Preserve typing focus and Space.
- Present streamed results in a dedicated exact-entry multi-selection list with
  filename and containing folder. Reuse ordinary exact-path file actions and add
  selection-aware `Reveal in Folder` that navigates to the exact parent and
  restores exact result selection.
- Clear/cancel on location exit. Keep query, root, results, counters, and usage
  memory-only; do not persist or log them.
- Embed the user-provided `icon_floe.png` under the stable application ID and
  use it as Floe's application/window icon without changing entry iconography.
- Exclude Phase 13C advanced predicates, MIME/owner/date/size filters, Phase 13D
  content search, arbitrary roots, remote search, history, saved searches,
  indexing, tags, duplicate finding, and unrelated refactors.

## Applicable testing layers

- Deterministic core unit and `tempfile` filesystem integration tests for scope,
  raw paths, links, mount boundaries, caps, skips, and cancellation.
- Application worker tests for bounded streaming, cancellation, stale generations,
  and backpressure.
- GTK/action/accessibility contract tests plus an isolated native Wayland
  action/liveness/clean-quit smoke. Full Dogtail E2E remains conditional on its
  external dependencies.
- No new property test is planned: fixed filesystem trees and injected limits
  express these traversal invariants more clearly than generated cases.

## Status

COMPLETE. Core traversal, the bounded streaming worker, unified native search
surface, exact result actions, supplied icon, strict workspace gates, and an
isolated native Wayland action/liveness/clean-Quit smoke are verified in
`GATES.md`. Exactly Phase 13C is next; later phases remain excluded.

---

# Archived plan: Permanent Layered Testing Foundation

## Contract

- Preserve the existing 469-test Rust suite, `tempfile` filesystem isolation,
  and native Wayland smoke practice without reorganizing unrelated tests.
- Define permanent, distinct unit, filesystem-integration, property, GTK
  component/accessibility, native E2E, Niri smoke, and Plasma smoke layers.
- Add `proptest` only to `floe-core` dev dependencies and use it selectively
  for exact Linux filename identity, deterministic sorting, navigation-history,
  and folder-filter invariants that benefit from generated input.
- Add graphical GTK component/accessibility contract tests as an explicit
  opt-in gate. They must exercise real Floe widget/action metadata and must not
  make headless `cargo test --workspace` depend on a display server.
- Add a native Dogtail/AT-SPI E2E layer under `e2e/` with dependency and
  environment preflight, semantic accessible-node interaction, deterministic
  condition waits, and eight named workflow scenarios.
- Every E2E launch must create private temporary HOME/XDG roots and an isolated
  freedesktop Trash. Never touch the developer's real user directories, Trash,
  mounts, preferences, cache, or data.
- Keep compositor-independent E2E separate from Niri and Plasma smoke policy.
  Do not claim graphical execution when Dogtail, AT-SPI, a suitable session, or
  compositor support is unavailable.
- Permanently document regression, future security/privacy, CI, test-layer
  selection, commands, dependencies, limitations, and evidence requirements in
  `AGENTS.md` and `docs/DEVELOPMENT.md`.
- Do not add Playwright, Selenium, Tauri/browser testing, hidden test-only UI,
  timing-only sleeps, or unrelated application features/refactors.

## Audit baseline

- Clean `main` baseline: `ecf2fc2` on isolated branch `testing-foundation`.
- `cargo test --workspace`: 365 `floe-app` + 104 `floe-core` = 469 passing.
- Existing source contains 233 `tempfile` references and extensive exact-path,
  non-UTF-8, bounded-worker, operation, persistence, and lifecycle coverage.
- No `.github` CI workflow, property-test dependency, dedicated GTK test
  target, or `e2e/` directory exists.
- Dogtail and `pyatspi` are unavailable on this host; no isolated Mutter,
  Weston, or Cage runner is installed. Native E2E execution cannot be claimed.

## Status

COMPLETE. All seven active gates contain measured evidence; the repository-wide
gate checker reports all 57 current and historical gates met. This
testing-foundation pass does not advance Phase 13, and Phase 13B remains the
sole recommended next roadmap phase.

---

# Previous completed plan: Floe Phase 13A — Current-Folder Filter

## Contract

- Add an explicit current-folder filter over entries already returned by the
  existing directory worker. It must not enumerate a subtree, read file
  contents, or create a second browsing pipeline.
- Support three fixed modes: case-insensitive text substring, filename glob,
  and regular expression. Cap queries at 256 Unicode scalar values and compile
  glob/regex patterns once per request rather than in GTK row bindings.
- Match valid UTF-8 names with Unicode semantics. For non-UTF-8 Linux names,
  retain exact raw bytes: text performs ASCII-insensitive byte matching, while
  glob and regex use a documented raw-byte fallback. Display text never becomes
  path identity.
- Run filtering on one bounded, generation-superseding application worker so
  large loaded directories and rapid query changes do not block GTK or retain
  unbounded request/result state.
- Expose a discoverable header action and `Ctrl+F`, an accessible mode chooser,
  visible match count, inline non-color-only invalid-pattern feedback, and
  Escape/close clearing. Keep native focus behavior for the editable field.
- Preserve the existing directory order and exact selection only for entries
  that remain visible. Reapply the active filter after refresh, sorting, hidden
  file changes, and watcher reconciliation; clear it when the active location
  changes.
- Keep filter query, mode, results, and usage memory-only. Do not persist or log
  names/queries, add history, indexing, content search, metadata filters, or
  saved searches.

## Status

COMPLETE. Core and application focused tests, strict workspace gates, and native
Wayland action/render/lifecycle smoke verified the bounded current-folder filter.
Phase 13B filename search remains excluded and is the sole recommended next phase.

---

# Previous completed plan: Floe Phase 12F — Productivity Action Integration

## Contract

- Expose the existing selection-aware Extract Here, Extract To, and Compress
  actions in a compact Archives submenu in every normal file context surface.
- Keep a small reviewed essential action set fixed while allowing users to show
  or hide bounded optional productivity groups in file and background context
  menus through an accessible native dialog.
- Persist only stable reviewed group identifiers through the existing bounded
  preference worker; migrate older preference files with safe defaults.
- Rebuild list, grid, Miller, and background menu models from the same policy so
  pointer and keyboard invocation have parity and live GAction eligibility stays
  authoritative.
- Register customization as a human-readable command reachable from the header,
  command palette, and Keyboard Shortcuts dialog.
- Preserve exact paths and existing archive/job architecture; GTK builds menus
  and submits existing actions but performs no filesystem work.
- Exclude arbitrary external commands, plugin actions, shell command templates,
  per-MIME rules, privacy/safe-open actions, and later roadmap phases.

## Status

COMPLETE on `phase-12f-productivity-actions`; verified evidence is recorded in
`GATES.md`. Phase 13A folder filter is the sole recommended next phase.
