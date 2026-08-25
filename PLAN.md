# Plan: Floe Phase 8F — Miller final-column detail hooks

Mode: sequential phase delivery with explicit implementation and verification gates.

## Contract

- Add a GTK-independent application-owned detail-hook lifecycle for Preview and
  Inspector without implementing any Phase 9/10 provider content.
- Carry exact generation, Miller depth, directory, and bounded raw selected
  paths; never reconstruct targets from visible names.
- Represent hidden, empty-selection, ready-for-provider, and unsupported states
  explicitly. Preview requires one supported local file candidate; Inspector
  may hand off a bounded aggregate selection.
- Add optional final-column presentation and focus-visible Preview/Inspector
  controls only in Miller mode. Closing or leaving Miller returns focus safely.
- Reconcile navigation and selection changes without filesystem I/O, provider
  execution, new workers, caches, persistence, or active-content claims.
- Exclude Phase 9 providers/shortcuts/content and Phase 10 inspector metadata.

## Depth tree

1. Exact lifecycle model
   - Define surface, generation, target, eligibility, stale reconciliation, and
     bounded selection policy with non-UTF-8 tests.
2. Final-column presentation
   - Render a truthful optional surface after active columns with non-color-only
     state text and focus behavior; expose accessible controls.
3. Controller integration
   - Bind actions, reconcile current Miller selection/navigation, hide on mode
     exit, and keep list/grid behavior untouched.
4. Verification and handoff
   - Focused lifecycle/presentation/integration tests, native Wayland smoke,
     full checks, docs, and exactly Phase 9A as `NEXT`.

## Status

COMPLETE on `phase-8f-miller-detail-hooks`. All gates are met. The sole
recommended next phase is `phase-9a-preview-providers`.
