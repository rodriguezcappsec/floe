# Plan: Floe Phase 13A — Current-Folder Filter

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
