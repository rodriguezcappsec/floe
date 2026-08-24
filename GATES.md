# Gates: Floe Phase 6M permanent deletion

Scope: add explicit path-safe multi-target permanent deletion without secure
erase claims or Phase 6N Trash lifecycle work.

- [x] G1: Work is isolated on `phase-6m-permanent-delete`; Phase 6N or later
  behavior is not implemented.
  CHECK: `git branch --show-current`
  EXPECT: `phase-6m-permanent-delete`
  EVIDENCE: `git branch --show-current` returned `phase-6m-permanent-delete`;
  diff scope contains no Trash browser, restore, or Empty Trash implementation.

- [x] G2: Core request preserves raw paths and rejects empty, relative, root,
  unnormalized, duplicate, and nested targets.
  CHECK: `cargo test -p floe-core phase_6m_request -- --nocapture`
  EXPECT: test result: ok
  EVIDENCE: focused core request test passed 1/1.

- [x] G3: Whole-batch preflight does not follow symlinks or cross mounts,
  revalidates identity, emits progress, and reports exact partial failure.
  CHECK: `cargo test -p floe-core phase_6m_delete -- --nocapture`
  EXPECT: test result: ok
  EVIDENCE: focused core delete tests passed 4/4.

- [x] G4: Cancellation succeeds only before the first removal and never claims
  committed partial work was cancelled.
  CHECK: `cargo test -p floe-core phase_6m_cancellation -- --nocapture`
  EXPECT: test result: ok
  EVIDENCE: focused core cancellation tests passed 2/2.

- [x] G5: Fixed-capacity executor maps progress, completion, safe cancellation,
  partial failure, capacity, and bounded shutdown into shared job state.
  CHECK: `cargo test -p floe-app phase_6m_executor -- --nocapture`
  EXPECT: test result: ok
  EVIDENCE: focused executor tests passed 3/3.

- [x] G6: Selection-aware “Delete Permanently…” and `Shift+Delete` always open
  a safe-focus irreversible confirmation with escaped exact target context;
  GTK submits an application command only.
  CHECK: `cargo test -p floe-app phase_6m_confirmation -- --nocapture`
  EXPECT: test result: ok
  EVIDENCE: focused confirmation policy test passed 1/1; native action smoke
  reported the selected `permanent-delete` action enabled and activated.

- [x] G7: Operations feedback distinguishes preparing, deleting, cancelled
  before deletion, completed, and non-retryable partial failure; no secure erase
  or undo claim appears.
  CHECK: `cargo test -p floe-app phase_6m_feedback -- --nocapture`
  EXPECT: test result: ok
  EVIDENCE: focused feedback test passed 1/1; source search found no secure
  erase, secure deletion, or undo wording in application/core source.

- [x] G8: Formatting, compilation, strict Clippy, all tests, and diff hygiene
  pass.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && git diff --check`
  EXPECT: all commands exit 0
  EVIDENCE: final combined gate exited 0; 165 application plus 40 core tests
  passed, 205 total; doc tests passed; `git diff --check` passed.

- [x] G9: Isolated native Wayland smoke proves the action/confirmation surface,
  D-Bus application health, clean shutdown, and no access to real user data.
  EVIDENCE: two temporary-home Wayland launches owned and released the expected
  D-Bus name, exported and enabled `permanent-delete` after Select All, opened
  confirmation, answered Peer.Ping, preserved the only fixture, and exited 0.
  Only host RADV/Vulkan and isolated-session accessibility warnings appeared.

- [x] G10: AGENTS, ROADMAP, FEATURE_MATRIX, ARCHITECTURE, DESIGN,
  PRIVACY_SECURITY, PLAN, and GATES accurately record verified Phase 6M; exactly
  Phase 6N is NEXT.
  CHECK: `node <unlazy-skill-dir>/scripts/gate-check.mjs --status GATES.md`
  EXPECT: ALL MET
  EVIDENCE: all listed documents updated; roadmap contains exactly one phase
  status `NEXT`, Phase 6N Trash lifecycle.
