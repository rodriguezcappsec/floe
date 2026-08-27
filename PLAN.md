# Plan: Floe Phase 13C — Advanced Filters

## Contract

- Add one optional advanced-filter section inside the existing unified search
  surface; it serves both Quick Filter and Search Files without creating a third
  search pipeline.
- Support combined filename, entry-type, MIME, extension, size, modified-date,
  owner, and hidden predicates. Filename matching supports Text, Glob, and Regex
  with an explicit match-case control.
- Define predicates in `floe-core`, including bounded validation, cheap-first
  evaluation, explicit missing/unknown metadata semantics, and a tag-ready
  extension point that does not implement tags.
- Preserve exact `PathBuf`/`OsStr` identity. Display text is never converted back
  into a path.
- Keep GTK responsive: both existing capacity-one application workers evaluate
  predicates; MIME and Unix owner metadata are resolved lazily on those workers
  only when active predicates require them. MIME guessing must not read contents.
- Quick Filter may include hidden entries independently from the global Show Hidden
  setting while active; closing/clearing restores the global hidden policy. Search
  Files uses Current Setting/Include Hidden/Hidden Only.
- Allow an empty filename query only when an advanced predicate is active. Keep
  cancellation, generation supersession, streaming, traversal bounds, exact
  Reveal, and memory-only privacy behavior from Phase 13A/13B.
- Add compact accessible native controls with plain-language descriptions, Apply,
  and Clear Filters. Exclude tags, content search, saved searches/history,
  indexing, remote roots, persistence, and search-specific sorting/grouping.

## Execution tree

1. Core query/predicate model and adversarial tests.
2. Lazy application metadata resolver and bounded worker integration.
3. Unified native controls, browser state, validation, and stale-result handling.
4. Full quality gates, native Wayland smoke, and persistent documentation.

## Status

COMPLETE on `phase-13c-search-filters`. Core predicates, both bounded workers,
wrapping accessible controls, strict workspace gates, and native Wayland action,
health, and clean-Quit lifecycle are verified. Phase 13D is the sole `NEXT` phase.

---

# Archived plan: Floe Post-13B — Unified Search Surface

## Contract

- Replace the two competing header search buttons and separate visible rows with one
  discoverable `Search` action and one shared search surface.
- `Ctrl+F` opens the shared surface in `Quick Filter` mode. `Ctrl+Shift+F` remains a
  power-user direct shortcut into `Search Files` mode, but uses that same surface.
- Expose two plainly named modes: `Quick Filter` narrows entries already loaded in the
  active folder; `Search Files` runs the existing bounded filename-search worker.
- Preserve the existing Text/Glob/Regex chooser and feedback in Quick Filter. Preserve
  This Folder/Include Subfolders, Search/Stop, streaming results, Reveal in Folder, and
  cancellation in Search Files.
- Give the mode selector and both modes visible, accessible, plain-language help. Do not
  collapse distinct execution semantics or imply that Quick Filter traverses disk.
- Switching modes must cancel/clear incompatible active state predictably, keep keyboard
  focus in the shared query entry, and never allow stale filter/search results to replace
  the active mode.
- Reuse the Phase 13A/13B workers, exact `PathBuf` selection/results, bounds, privacy
  behavior, and action ownership. Do not add advanced filters, content search, saved
  searches, indexing, persistence, remote roots, or filesystem work to GTK callbacks.

## Status

COMPLETE on `phase-13b-filename-search`. One native two-row surface, shared query/Close,
mode-specific controls, direct shortcuts, cancellation, deferred focus restore, focused
tests, full strict workspace gates, and native Wayland visual/D-Bus lifecycle are
verified. This remains a bounded interaction correction to the implemented Phase 13A/13B
features; Phase 13C is still the sole next roadmap phase.

---

# Archived plan: Floe Phase 13B — Filename Search

## Contract

- Add explicit case-insensitive filename search rooted at the active local
  folder, with fixed `This Folder` and `Include Subfolders` scopes. Search is
  distinct from Phase 13A filtering and never reads file contents.
- Preserve exact `PathBuf`/`OsStr` identity and reuse the reviewed Phase 13A
  Unicode text/raw-byte fallback semantics. Cap queries at 256 Unicode scalar
  values.
- Run traversal on one bounded application worker. Stream at most 128 results
  per response; retain bounded response/request state; support generation-based
  cancellation and stale-result rejection. Cap one search at 100,000 results,
  1,000,000 examined entries, 100,000 directories, and depth 128 with explicit
  incomplete status instead of silently claiming completeness.
- Use no-follow metadata for traversal, never descend symbolic links, and do
  not cross filesystem boundaries. Continue past inaccessible child entries or
  directories while reporting skipped counts; root failures remain explicit.
- Add a native search bar discoverable as `Search Filenames…` with
  `Ctrl+Shift+F`, visible scope, Search, Stop, and Close controls, streamed count
  feedback, and keyboard/accessibility labels. Keep `Ctrl+F` as the existing
  current-folder filter.
- Present results in a dedicated list backed by the shared exact-entry
  selection model. Show filename and containing folder, reuse normal open and
  context actions, and expose selection-aware `Reveal in Folder` that navigates
  to the exact parent and selects the exact result.
- Keep queries, roots, results, skipped-path counts, and usage memory-only.
  Clear/cancel on location changes and exit. Do not persist/log names or add
  content search, advanced filters, saved searches, history, indexing, remote
  locations, arbitrary roots, or duplicate finding.
- Use the user-provided `icon_floe.png` as Floe's embedded application icon,
  registered under `io.github.floe.FileManager` and selected as the GTK window
  icon without changing file/folder iconography.

## Status

COMPLETE on `phase-13b-filename-search`. Core traversal, streaming application
worker, native search/results/reveal UI, supplied app icon, strict workspace
gates, and isolated Wayland D-Bus lifecycle are verified. Phase 13C advanced
filters is the sole recommended next phase; Phase 13C and later remain
unimplemented.

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
Phase 13B filename search was excluded from Phase 13A and was completed in the
following bounded phase.

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
