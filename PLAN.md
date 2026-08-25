# Plan: Floe Phase 10A — Inspector foundation

## Contract

- Add a bounded GTK-independent Inspector worker that aggregates exact selected-entry facts without filesystem reads or GTK dependencies.
- Add explicit ready/loading/inspected/unsupported/failure lifecycle keyed by selection generation; stale results must not replace current state.
- Present single- and multi-selection count/kind/known-byte/common-parent facts in the final Miller detail column as read-only accessible text.
- Bind Ctrl+I to toggle Inspector in Miller view and restore active-column focus on close.
- Give the detail/Inspector column an independently clamped width with accessible narrow/widen controls and asynchronous preference persistence/migration.
- Preserve raw PathBuf identity, GTK responsiveness, Preview behavior, privacy boundaries, and no-edit scope.
- Exclude rich metadata providers (10B), property pages/edits (10C+), checksums, and permissions changes.

## Status

COMPLETE on `phase-10a-inspector-foundation` after focused tests, 375 workspace
tests, strict Clippy, build, patch hygiene, and native Wayland Inspector/action/
width/persistence/clean-quit smoke. Exactly Phase 10B is recommended next.
