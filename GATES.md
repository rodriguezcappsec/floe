# Gates: Floe Phase 9A — Preview provider architecture

Scope: deliver bounded cancellable Preview provider lifecycle without format renderers.

- [x] G1: Work is isolated on the prescribed phase branch.
  CHECK: git branch --show-current
  EXPECT: phase-9a-preview-providers
  EVIDENCE: phase-9a-preview-providers

- [x] G2: Typed registry ordering, exact raw targets, limits, cache policy, and unsupported fallback are deterministic.
  CHECK: cargo test -p floe-app phase_9a_contract -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_9a_contract passed 1/1 with raw non-UTF-8 identity, explicit limits/default memory cache, nonzero generation, empty-registry fallback.

- [x] G3: Fixed-capacity worker handles cancellation, stale generations, queue pressure, provider failure, and clean shutdown.
  CHECK: cargo test -p floe-app phase_9a_worker -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_9a_worker passed 1/1; memory reuse, full queue, stale submit, cooperative cancel, provider failure, and worker drops verified.

- [x] G4: Phase 8F detail hook shows truthful loading/unsupported/failed lifecycle and drops stale responses.
  CHECK: cargo test -p floe-app phase_9a_lifecycle -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_9a_lifecycle passed 1/1; loading accepts only matching request generation and unsupported fallback is explicit.

- [x] G5: GTK drain integration stays bounded and existing thumbnails/Miller/list/grid behavior remain intact.
  CHECK: cargo test -p floe-app phase_9a_integration -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_9a_integration passed 1/1; queue is 16, drain cap is 8, existing views and detail actions remain.

- [x] G6: Native Wayland smoke verifies Preview loading/fallback action lifecycle, health, and clean shutdown.
  EVIDENCE: Isolated Wayland launch activated Miller, described enabled Preview hook, opened/closed Preview lifecycle, answered D-Bus Ping, quit status 0.

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
  EVIDENCE: 356 tests passed (265 application, 91 core); no failures.

- [x] G11: Patch hygiene is clean.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: git diff --check exited 0 with no output.

- [x] G12: Persistent documentation marks 9A complete, exactly 9B next, and records no-renderer/no-sandbox boundaries.
  CHECK: rg -n "9A.*COMPLETE|9B.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 9B
  EVIDENCE: Roadmap, matrix, AGENTS, architecture, design, development, and privacy docs record 9A complete and sole 9B NEXT.
