# Plan: Floe Phase 8A — Miller column model

Mode: sequential solo phase, depth 4.

## Contract

- Add a GTK-independent Miller chain model to `floe-core`; it owns only exact
  directory/selected-child navigation identity, never directory entries,
  enumeration, widgets, workers, or compositor state.
- Preserve raw `PathBuf` identity and enforce direct parent/child relationships.
- Retain a bounded window of columns with stable logical depths so deep
  navigation cannot grow state without limit.
- Define deterministic selection, directory descent, rename, deletion, root
  invalidation, and stale-depth behavior. Filesystem events are inputs supplied by
  the application; the model performs no filesystem I/O.
- Do not implement GTK columns, keyboard/trackpad UI, column actions, drag/drop,
  preview, Inspector, Niri, or Plasma integration.

## Depth tree

1. Invariants and public types
   - Exact absolute paths, stable logical depth, direct-child validation,
     bounded capacity, structured errors and transitions.
2. Navigation state
   - Select leaf, descend directory, clear/truncate, active logical depth,
     left-window eviction without losing retained chain identity.
3. External reconciliation
   - Same-parent rename remaps exact raw prefixes through retained descendants.
   - Delete clears/truncates deterministically and reports root invalidation.
4. Verification and handoff
   - Focused hostile/non-UTF-8/boundary tests, full gates, persistent docs,
     exactly Phase 8B next; no native smoke for a GTK-free model.

## Status

COMPLETE — all Phase 8A gates verified. Exactly one recommended next phase:
`phase-8b-miller-ui`.
