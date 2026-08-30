# Gates: Floe Phase 6V — Selection and Operation Reveal Polish

Scope: shared selection presentation and successful-result reveal for local
Copy, Move, Rename, Create, Duplicate, and Replace only. No Undo Trash,
multi-window, sidebar/location redesign, preview-provider sandbox, or later
roadmap work.

- [x] V1: Exact reveal intent is bounded, single-use, tied to the authoritative
  directory and browser generation, and rejects stale/mismatched/missing/
  filtered/inactive/partial results without selecting a different item.
  CHECK: cargo test -p floe-app phase_6v_reveal_policy -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Three focused tests pass exact non-UTF-8 identity, stale/directory/generation rejection, batch deduplication/isolation, and the 4,096-result bound.

- [x] V2: Every in-scope successful operation supplies the exact committed
  `PathBuf`; affected-directory refresh and result selection never reconstruct
  identity from labels and preserve deliberate multi-selection behavior.
  CHECK: cargo test -p floe-app phase_6v_operation_results -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: The focused typed-operation regression passes all six in-scope committed destination mappings without display-text reconstruction.

- [x] V3: List, Grid/Search/Trash, and Miller selected items expose redundant
  non-color styling plus native selected/focused accessibility semantics;
  transient result emphasis is bounded, automatically cleared, and safe across
  virtualized widget recycling.
  CHECK: cargo test -p floe-app phase_6v_selection_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: The focused CSS/presentation contract passes border and label-weight selection cues plus bounded view-level result emphasis.

- [x] V4: Focused real-GTK component gates prove exact-result scroll, focus
  preservation, transient-emphasis cleanup, and accessible selection treatment.
  CHECK: cargo test -p floe-app phase_6v_gtk -- --ignored --nocapture
  EXPECT: test result: ok
  EVIDENCE: One focused native GTK widget test passes multi-selection, scroll-without-focus-movement, and transient CSS class cleanup on the active Wayland display.

- [x] V5: Full workspace, documentation, packaging/release, E2E contracts,
  diff hygiene, and isolated native Wayland lifecycle pass or exact external
  dependency skips are recorded.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace
  EXPECT: test result: ok
  EVIDENCE: Format/check/strict Clippy/workspace tests pass: 587 app plus 18 graphical ignores, 21 controller, 169 core, six duplicate workflows. Strict docs/render, migrations/layout, deterministic source, release candidate, frozen release build, diff hygiene, and isolated Wayland Ping/Actions/Quit pass. E2E contracts pass five with two truthful Dogtail/pyatspi skips.

- [x] V6: Phase 6V becomes COMPLETE only after V1–V5 and exactly one later
  roadmap phase becomes NEXT; no later implementation is started.
  CHECK: python3 scripts/check-docs.py --strict
  EXPECT: phase-21c-docs-ok
  EVIDENCE: Strict documentation reports phase-21c-docs-ok across 21 files; Phase 6V is COMPLETE and exactly Phase 6W Undo Trash is NEXT and unimplemented.
