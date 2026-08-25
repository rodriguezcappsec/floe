# Gates: Floe Phase 7A Tab/session model

Status: COMPLETE

- [x] G1: Work is isolated on `phase-7a-tabs-foundation`; no tab widget, tab
  action/shortcut, persistence, split-view, or future interaction code exists.
- [x] G2: Existing GTK-independent view mode/grid/density/column/folder policy
  has one canonical core definition and application behavior remains unchanged.
  CHECK: `cargo test -p floe-core phase_7a_view -- --nocapture`
- [x] G3: A stable-ID browser session owns exact complete current/back/forward
  location state including path, selection, path/index scroll anchor, sort,
  grouping, directory placement, mode, grid size, density, and columns.
  CHECK: `cargo test -p floe-core phase_7a_session -- --nocapture`
- [x] G4: Navigation transitions preserve complete history entries, clear
  forward history after new navigation, stop at root, and enforce explicit
  history/selection bounds without silent identity loss.
  CHECK: focused transition, bound, duplicate-selection, and non-UTF-8 tests.
- [x] G5: Versioned in-memory serialization round-trips raw non-UTF-8 paths and
  all policy fields; malformed, relative, truncated, oversized, invalid-enum,
  and trailing-byte inputs fail structurally without panic or filesystem I/O.
  CHECK: `cargo test -p floe-core phase_7a_codec -- --nocapture`
- [x] G6: `floe-core` remains free of GTK/GIO/compositor dependencies and no new
  dependency, shell, unsafe, filesystem operation, or lossy path reconstruction
  is introduced.
- [x] G7: Formatting, workspace check, strict all-target/all-feature Clippy,
  222 application plus 74 core tests (296 total), and diff hygiene pass.
- [x] G8: Roadmap, matrix, architecture, development, privacy/security, plan,
  gates, and AGENTS status mark verified Phase 7A complete and exactly Phase 7B
  as `NEXT`. No native smoke is claimed because runtime UI is unchanged.

Recommended next phase: `phase-7b-tabs-interaction`. Stop before implementing it
on this branch.
