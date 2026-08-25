# Plan: Floe Phase 8B — Virtualized Miller columns

Mode: sequential phase delivery with explicit implementation and verification gates.

## Contract

- Add a native Miller view that renders the Phase 8A exact-path column model as
  horizontally arranged, recyclable floating columns.
- Reuse the existing browser directory-result pipeline. A Miller column may
  retain bounded entry snapshots, but it must not create an unbounded or
  duplicate enumeration pipeline.
- Keep exact `PathBuf` identity authoritative for columns, selections, and
  activation; lossy labels remain display-only.
- Bound visible/retained widget state and column widths. Width adjustment must
  be accessible and must not perform filesystem work in GTK callbacks.
- Preserve list/grid behavior, tabs, split state, operations, thumbnails, and
  file watching when Miller mode is not active.
- Exclude Phase 8C keyboard/trackpad navigation, Phase 8D per-column actions,
  Phase 8E drag/drop, Phase 8F detail hooks, and all Preview content.

## Depth tree

1. Presentation policy and state
   - Define bounded column width/retention policy and deterministic exact-path
     column snapshots outside GTK callbacks.
2. Native virtualized surface
   - Add recyclable list-backed column widgets, active-column semantics, and
     adjustable widths using the existing application/browser pipeline.
3. Application integration
   - Add Miller view switching without duplicating workers, selection identity,
     or directory enumeration ownership.
4. Verification and handoff
   - Add focused bounds/identity/pipeline tests, native Wayland smoke, full
     workspace checks, documentation, and exactly Phase 8C as `NEXT`.

## Status

COMPLETE on `phase-8b-miller-ui`. All gates are met. The sole recommended next
phase is `phase-8c-miller-navigation`.
