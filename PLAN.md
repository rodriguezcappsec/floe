# Plan: Permanent Layered Testing Foundation

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
