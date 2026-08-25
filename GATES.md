# Gates: Floe Phase 6P Operation control

Status: COMPLETE

- [x] G1: Work is isolated on `phase-6p-operation-control`; Phase 6Q creation features are not implemented.
  CHECK: `git branch --show-current && git diff --check`
  EXPECT: Phase 6P branch and clean diff hygiene.
  EVIDENCE: Branch is `phase-6p-operation-control`; `git diff --check` exits 0 and the diff contains operation-control work only.

- [x] G2: Progress distinguishes bytes, items, and unknown units; telemetry reports speed/ETA only for meaningful determinate byte samples.
  CHECK: `cargo test -p floe-core phase_6p_progress -- --nocapture && cargo test -p floe-app phase_6p_telemetry -- --nocapture`
  EXPECT: Explicit-unit and deterministic telemetry tests pass.
  EVIDENCE: One focused core progress test and three application telemetry tests pass, including frequent samples, regression, item-unit, zero-rate, and completion suppression.

- [x] G3: Multi-item submissions have stable bounded batch state, FIFO counts, pause-after-current/resume, queued cancellation, and terminal summaries.
  CHECK: `cargo test -p floe-app phase_6p_batch -- --nocapture`
  EXPECT: Focused batch policy tests pass.
  EVIDENCE: Three focused tests pass for bounded snapshots, committed-current cancellation, blocked-conflict/queued cancellation; the state integration test verifies FIFO pause/resume and exact counts.

- [x] G4: Keep Both preserves raw names with bounded deterministic siblings; Skip All is batch-scoped; Replace is unavailable.
  CHECK: `cargo test -p floe-app phase_6p_conflict -- --nocapture`
  EXPECT: Raw non-UTF-8, no-replace, and scoped conflict tests pass.
  EVIDENCE: Two focused tests pass. Keep Both creates `item (copy).txt` without replacing `item.txt`; batch Skip All suppresses only later conflicts in the same batch.

- [x] G5: Bounded memory-only history is visible and Clear Completed preserves actionable evidence.
  CHECK: `cargo test -p floe-app phase_6p_history -- --nocapture`
  EXPECT: History retention policy test passes.
  EVIDENCE: The focused test removes only a completed entry and preserves the unresolved conflict entry; UI labels history as memory-only.

- [x] G6: Completed move/rename Undo captures and revalidates no-follow destination identity and never overwrites the original path.
  CHECK: `cargo test -p floe-core phase_6p_undo -- --nocapture && cargo test -p floe-app phase_6p_undo -- --nocapture`
  EXPECT: Core and application Undo tests pass.
  EVIDENCE: Two core tests cover raw non-UTF-8 identity, occupied original path, and changed destination; one application test restores a completed move and rejects copy Undo.

- [x] G7: Application state integrates batches, conflict scope, history, and Undo while GTK callbacks perform no filesystem work.
  CHECK: `cargo test -p floe-app phase_6p_state -- --nocapture`
  EXPECT: Focused state lifecycle test passes.
  EVIDENCE: The state test passes FIFO pause/resume through application-owned executors; UI callbacks call only `ApplicationState` commands.

- [x] G8: Operations Island exposes accessible pause/resume/history controls, unit-aware progress, terminal summaries, scoped conflicts, and safe Undo; native Wayland remains healthy.
  CHECK: `cargo test -p floe-app phase_6p_ui -- --nocapture`
  EXPECT: Deterministic UI policy test passes and native evidence is recorded.
  EVIDENCE: Two UI policy tests pass exact item/byte/speed/ETA and batch-summary wording plus always-reachable history action. Isolated native launch exported 31 actions, activated `operation-history`, answered `Peer.Ping`, quit cleanly, and released `io.github.floe.FileManager`; only known host accessibility/RADV/Vulkan warnings appeared.

- [x] G9: Formatting, workspace build, strict Clippy, all tests, diff hygiene, and native smoke pass.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && git diff --check`
  EXPECT: Command exits 0.
  EVIDENCE: Command exits 0. All 251 tests pass: 59 core and 192 application tests.

- [x] G10: Persistent documentation records verified Phase 6P and exactly Phase 6Q as `NEXT`.
  CHECK: `node <unlazy-skill-dir>/scripts/gate-check.mjs --status GATES.md`
  EXPECT: `ALL MET`.
  EVIDENCE: `AGENTS.md`, `PLAN.md`, `GATES.md`, `DESIGN.md`, `docs/ROADMAP.md`, `docs/FEATURE_MATRIX.md`, `docs/ARCHITECTURE.md`, `docs/DEVELOPMENT.md`, and `docs/PRIVACY_SECURITY.md` are updated; roadmap marks only Phase 6Q `NEXT`.
