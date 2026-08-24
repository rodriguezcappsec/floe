# Gates: Floe Phase 6L system thumbnailers

Scope: Discover and supervise freedesktop system thumbnailers through the existing bounded thumbnail worker while preserving safe fallbacks.

- [x] G1: Work is isolated on the recommended Phase 6L branch and does not
  implement Phase 6M or later roadmap features.
  CHECK: git branch --show-current
  EXPECT: /^phase-6l-system-thumbnailers$/
  EVIDENCE: Direct `git branch --show-current` returned `phase-6l-system-thumbnailers`; source and roadmap audit found no Phase 6M implementation. The gate runner could not spawn `/bin/sh` for this Git check under the managed sandbox, so the direct command is recorded.

- [x] G2: Freedesktop thumbnailer discovery is standards-based, deterministic,
  MIME-aware, testable without GTK, and rejects malformed/unsafe definitions.
  CHECK: cargo test -p floe-app phase_6l_provider -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Three focused provider tests passed for deterministic precedence, unsafe/unreviewed definition rejection, and raw-byte reviewed field-code expansion.

- [x] G3: Provider execution uses argv without a shell, exact URI/path identity,
  private temporary output, a fixed timeout, cancellation, child termination,
  cleanup, bounded output, and no active-content execution by Floe.
  CHECK: cargo test -p floe-app phase_6l_execution -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Four focused execution tests passed for private success/cleanup, exit/timeout/cancellation, process-group termination, and missing/symlink/oversized output.

- [x] G4: Existing raster and persistent-cache behavior remains intact, while
  supported non-raster MIME requests reach system providers and all failure,
  stale, unsupported, or full-queue cases retain stable generic fallbacks.
  CHECK: cargo test -p floe-app phase_6l_integration -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Four worker integration tests passed for non-UTF-8 PDF generation/cache reuse, unsupported/malformed fallback, source changes/stale cancellation, and full-queue behavior.

- [x] G5: Tests cover non-UTF-8 inputs, hostile/malformed definitions and output,
  MIME precedence, timeout, cancellation, source changes, output limits, process
  failure, and temporary-artifact cleanup.
  CHECK: cargo test -p floe-app phase_6l_ -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: All eleven `phase_6l_` tests passed with zero failures.

- [x] G6: Phase 6L does not add a crypto/sandbox dependency or claim providers
  are sandboxed; documentation clearly preserves Phase 18L as isolation work.
  CHECK: git diff -- Cargo.toml crates/core/Cargo.toml crates/app/Cargo.toml docs/PRIVACY_SECURITY.md docs/ROADMAP.md docs/FEATURE_MATRIX.md
  EXPECT: /sandbox/
  EVIDENCE: Cargo changes only enable rustix process-group APIs; no crypto or sandbox crate was added. Privacy/security and roadmap docs explicitly state helpers run with normal user authority and Phase 18L owns isolation.

- [x] G7: Formatting, compilation, strict Clippy, workspace tests, and diff
  hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: Formatting, workspace check, strict all-target/all-feature Clippy, 159 application tests, 33 core tests, doc tests, and diff hygiene passed with zero failures.

- [x] G8: Native Wayland smoke proves a controlled system provider produces and
  reuses a thumbnail, application ownership/health remain stable, and clean
  shutdown releases the D-Bus name without leaving temporary artifacts.
  EVIDENCE: Two isolated native Wayland launches owned the D-Bus name. The first ran one controlled PDF provider and wrote one cache PNG; the second reused it while that provider exited 9, leaving PROVIDER_RUNS=1 and CACHE_FILES=1. Both exited 0 through app.quit, released ownership, and left PROVIDER_TEMP_CLEAN_AFTER_EXIT=1. Only the known host Vulkan suboptimal-swapchain warning appeared.

- [x] G9: AGENTS, ROADMAP, FEATURE_MATRIX, ARCHITECTURE, PRIVACY_SECURITY, PLAN,
  and GATES accurately record verified Phase 6L behavior; exactly Phase 6M is
  next.
  CHECK: rg -n 'Phase 6L|Phase 6M|system thumbnail|sandbox' AGENTS.md docs/ROADMAP.md docs/FEATURE_MATRIX.md docs/ARCHITECTURE.md docs/PRIVACY_SECURITY.md PLAN.md GATES.md
  EXPECT: /Phase 6M/
  EVIDENCE: All named documents record Phase 6L complete and supervised-unsandboxed behavior, and ROADMAP contains exactly one NEXT status at Phase 6M.

- [x] G10: Status-only gate checker reports every Phase 6L gate met.
  CHECK: node <unlazy-skill-dir>/scripts/gate-check.mjs --status GATES.md
  EXPECT: /ALL MET/
  EVIDENCE: Final direct status-only gate check reports all ten Phase 6L gates met after code, native QA, documentation, and adversarial review.
