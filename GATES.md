# Gates: Floe Phase 8A — Miller column model

Scope: GTK-independent exact, bounded Miller navigation chain.

- [x] G1: Work is isolated on `phase-8a-miller-model` and adds no GTK column UI.
  CHECK: git branch --show-current
  EXPECT: phase-8a-miller-model
  EVIDENCE: branch matches; implementation changes are confined to `floe-core` plus phase documentation.

- [x] G2: Exact direct-child selection/descent produces a deterministic parent/selected-child chain.
  CHECK: cargo test -p floe-core phase_8a_chain -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: focused test passed leaf selection, directory descent, existing descent activation, invalid child, and relative-root rejection.

- [x] G3: Retained columns are bounded with stable logical depths and deterministic stale-depth errors.
  CHECK: cargo test -p floe-core phase_8a_bounds -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 21-level descent retained exactly 16 depths 5 through 20 and rejected evicted depth 4 structurally.

- [x] G4: Raw non-UTF-8 identity survives selection, descent, and rename without lossy reconstruction.
  CHECK: cargo test -p floe-core phase_8a_non_utf8 -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: focused raw-byte path test passed selection, child location, and same-parent rename remapping.

- [x] G5: Same-parent rename and delete/root invalidation transitions reconcile retained descendants deterministically.
  CHECK: cargo test -p floe-core phase_8a_reconcile -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: focused test passed prefix rename, cross-parent rejection, child truncation, leaf selection clear, root invalidation, empty-state rejection, and reset.

- [x] G6: The model owns no directory entries, enumeration, GTK, worker, or filesystem I/O.
  EVIDENCE: `miller.rs` imports only std path/collection/raw-byte traits, `thiserror`, and a path-size constant; state fields are paths, depths, and selection only.

- [x] G7: Rust formatting is clean.
  CHECK: cargo fmt --all -- --check
  EXPECT: /^$/
  EVIDENCE: command exited 0 with no output.

- [x] G8: The full workspace type-checks.
  CHECK: cargo check --workspace
  EXPECT: Finished `dev` profile
  EVIDENCE: workspace check completed successfully.

- [x] G9: Strict all-target/all-feature Clippy is warning-free.
  CHECK: cargo clippy --workspace --all-targets --all-features -- -D warnings
  EXPECT: Finished `dev` profile
  EVIDENCE: strict Clippy completed successfully.

- [x] G10: The full workspace test suite passes.
  CHECK: cargo test --workspace
  EXPECT: test result: ok
  EVIDENCE: 330 tests passed: 239 application and 91 core; doc tests also passed.

- [x] G11: Patch whitespace hygiene is clean.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: command exited 0 with no output.

- [x] G12: Persistent docs mark Phase 8A complete and exactly Phase 8B next.
  CHECK: rg -n '8A — Column model.*COMPLETE|8B — Virtualized columns.*NEXT' docs/ROADMAP.md
  EXPECT: 8B — Virtualized columns
  EVIDENCE: roadmap contains both required status rows and no other NEXT phase.

Recommended next phase: `phase-8b-miller-ui`.
