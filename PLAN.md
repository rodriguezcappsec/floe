# Plan: Floe Phase 7D Split state

Mode: sequential solo phase, depth 4.

## Contract

- Implement only GTK-independent per-tab split state. Do not add split widgets,
  shortcuts, drag-and-drop, a second filesystem worker, or Miller columns.
- Reuse `BrowserSession` for each pane so paths, histories, selection, scroll,
  sort, grouping, density, columns, and raw non-UTF-8 identity remain exact.
- Preserve a stable tab identity while modelling primary/secondary panes,
  explicit active side, bounded split ratio, close/swap transitions, and
  independent navigation/view state.
- Version the workspace envelope, migrate Phase 7C unsplit data, reject hostile
  split fields and duplicate pane IDs, and keep all existing persistence bounds.

## Depth tree

1. Split domain model
   - Explicit sides, bounded ratio, one/two-pane invariants, focus transitions.
   - Close/swap behavior preserves surviving content and stable tab identity.
2. Tab collection integration
   - Every live/recently-closed tab owns split state without changing unsplit UI.
   - Duplicate/reopen allocate fresh IDs for every pane and retain bounded tabs.
3. Versioned serialization
   - Version-2 split records and safe version-1 migration.
   - Raw paths, active side, ratio, independent histories/views, hostile input.
4. Verification and handoff
   - Focused core tests, full quality gate, persistent docs, exactly Phase 7E next.

## Status

COMPLETE — exactly one recommended next phase: `phase-7e-split-interaction`.
