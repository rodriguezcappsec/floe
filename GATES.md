# Gates: Floe Phase 7D Split state

Status: COMPLETE

- [x] G1: Work is isolated on `phase-7d-split-state`; no GTK split widgets,
  shortcuts, drag-and-drop, second browser worker, or Miller model is added.
  CHECK: git branch --show-current
  EXPECT: phase-7d-split-state
  EVIDENCE: Branch command returned `phase-7d-split-state`; code review found no
  GTK split widget/action, worker, drag, or Miller implementation.

- [x] G2: A GTK-independent split model owns primary/secondary sessions, active
  side, bounded ratio, stable tab identity, and deterministic split/close/swap.
  CHECK: cargo test -p floe-core phase_7d_split_state -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused state test passed ratio bounds, missing-side rejection,
  stable identity, active-side transfer, swap, and close behavior.

- [x] G3: Pane paths, histories, selections, scroll anchors, and view policies
  remain independent and preserve exact raw non-UTF-8 path identity.
  CHECK: cargo test -p floe-core phase_7d_independent_panes -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused independent-pane test passed raw non-UTF-8 history,
  selection, path, and per-pane grid/list view separation.

- [x] G4: Browser tabs, duplicate, close/reopen, and fresh-ID allocation retain
  complete split state while existing unsplit application behavior remains valid.
  CHECK: cargo test -p floe-core phase_7d_split_tabs -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused tab test passed split duplicate, close/reopen, complete state,
  and fresh primary/secondary IDs; all 230 existing app tests passed unchanged.

- [x] G5: Workspace version 2 round-trips split focus/ratio/raw state, migrates
  version 1, and rejects malformed sides, ratios, duplicate IDs, and truncation.
  CHECK: cargo test -p floe-core phase_7d_split_codec -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Two focused codec tests passed version-2 round-trip, version-1
  migration, raw paths, focus/ratio, invalid flags/sides/ratios/IDs/truncation.

- [x] G6: Formatting, workspace check, strict all-target/all-feature Clippy,
  workspace tests, diff hygiene, and existing application behavior pass.
  CHECK: cargo fmt --all -- --check
  EXPECT: /^$/
  EVIDENCE: Formatting, workspace check, strict Clippy, 317 tests (230 app and
  87 core), and diff hygiene passed.

- [x] G7: Persistent docs mark verified Phase 7D complete and exactly Phase 7E
  as `NEXT`, without claiming visible split interaction or inter-pane drag.
  CHECK: rg -n '7D — Split state.*COMPLETE|7E — Split interaction.*NEXT' docs/ROADMAP.md
  EXPECT: 7E — Split interaction
  EVIDENCE: Roadmap, matrix, design, architecture, development, privacy/security,
  plan, gates, and AGENTS mark 7D complete and exactly 7E next.

Recommended next phase: `phase-7e-split-interaction`.
