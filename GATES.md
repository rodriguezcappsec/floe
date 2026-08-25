# Gates: Floe Phase 8F — Miller final-column detail hooks

Scope: deliver exact Preview/Inspector handoff and presentation contracts without Phase 9/10 content.

- [x] G1: Work is isolated on the prescribed phase branch.
  CHECK: git branch --show-current
  EXPECT: phase-8f-miller-detail-hooks
  EVIDENCE: phase-8f-miller-detail-hooks

- [x] G2: Lifecycle preserves exact raw identity, bounded generations, stale reconciliation, and explicit hidden/empty/ready/unsupported states.
  CHECK: cargo test -p floe-app phase_8f_lifecycle -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_8f_lifecycle passed 1/1 with raw non-UTF-8 target, stable generation, stale unsupported, and hidden cleanup.

- [x] G3: Preview and Inspector eligibility are truthful and do not claim provider content, sandboxing, or metadata work.
  CHECK: cargo test -p floe-app phase_8f_contract -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_8f_contract passed 1/1; Preview/Inspector messages explicitly defer content to Phases 9/10.

- [x] G4: Final-column presentation has accessible text/focus controls and closes safely on mode changes.
  CHECK: cargo test -p floe-app phase_8f_presentation -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_8f_presentation passed 1/1; title, unavailable state, and accessible description are textual; controller hides on mode exit.

- [x] G5: List/grid, existing Miller navigation/actions/drag, and single browser pipeline remain intact.
  CHECK: cargo test -p floe-app phase_8f_integration -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_8f_integration passed 1/1; all view commands remain and only two hook actions were added.

- [x] G6: Native Wayland smoke verifies detail-hook actions, focus/lifecycle health, and clean shutdown.
  EVIDENCE: Isolated Wayland launch activated Miller, described enabled Preview/Inspector hooks, opened/closed empty Preview, answered D-Bus Ping, quit status 0.

- [x] G7: Rust formatting is clean.
  CHECK: cargo fmt --all -- --check
  EXPECT: /^$/
  EVIDENCE: cargo fmt --all -- --check exited 0 with no output.

- [x] G8: The full workspace type-checks.
  CHECK: cargo check --workspace
  EXPECT: Finished `dev` profile
  EVIDENCE: Finished dev profile successfully for the workspace.

- [x] G9: Strict all-target/all-feature Clippy is warning-free.
  CHECK: cargo clippy --workspace --all-targets --all-features -- -D warnings
  EXPECT: Finished `dev` profile
  EVIDENCE: Strict Clippy completed successfully with -D warnings.

- [x] G10: All workspace tests pass.
  CHECK: cargo test --workspace
  EXPECT: test result: ok
  EVIDENCE: 352 tests passed (261 application, 91 core); no failures.

- [x] G11: Patch hygiene is clean.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: git diff --check exited 0 with no output.

- [x] G12: Persistent documentation marks 8F complete, exactly 9A next, and records the hook/provider boundary.
  CHECK: rg -n "8F.*COMPLETE|9A.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 9A
  EVIDENCE: Roadmap, matrix, AGENTS, architecture, design, development, and privacy docs record 8F complete and sole 9A NEXT.
