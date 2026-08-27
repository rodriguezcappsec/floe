# Plan: Floe Phase 13D — Content Search

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
