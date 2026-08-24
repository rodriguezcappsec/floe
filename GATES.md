# Gates: Floe Phase 5B retry interaction

Scope: Expose verified retry through the Operations Island on branch `phase-5b-retry-interaction`. Failed/cancelled terminal jobs get an accessible persistent Retry action; completed jobs do not. Retrying submits through `ApplicationState`, preserves browsing responsiveness, and shows the fresh attempt. Overwrite, pause/resume, permanent deletion, and interactive conflict decisions remain out of scope.

- [x] G1: Branch is `phase-5b-retry-interaction` and `main` remains at Phase 5A commit.
  CHECK: git branch --show-current && git rev-parse main
  EXPECT: /phase-5b-retry-interaction[\s\S]*699085b/
  EVIDENCE: `git status --short --branch` reported `phase-5b-retry-interaction`; `git rev-parse main` remained `699085b`.

- [x] G2: Operations Island has a visible-text accessible Retry control wired to failed/cancelled terminal job IDs.
  CHECK: rg -n "operation_retry|Retry|retryable_job|retry_terminal_operation" crates/app/src/ui.rs crates/app/src/operations.rs
  EXPECT: /retry_terminal_operation/
  EVIDENCE: `ui.rs` builds a labelled native Retry button; `operations.rs` stores `retryable_job` and wires it to `retry_terminal_operation`.

- [x] G3: Retry dispatch uses `ApplicationState::retry_operation`, disables duplicate clicks while queued, and lets structured events show the fresh attempt.
  CHECK: rg -n "retry_operation|Retry queued|set_sensitive\(false\)|track_active|show_running" crates/app/src/operations.rs
  EXPECT: /Retry queued/
  EVIDENCE: `retry_terminal_operation` disables Retry before `state.retry_operation`, shows `Retry queued…`, and the existing poll loop renders fresh job events.

- [x] G4: Completed jobs hide Retry; failed/cancelled retry states persist instead of disappearing after the three-second terminal timeout.
  CHECK: rg -n "Completed|Cancelled|Failed|show_retry|clear_retry|schedule_hide|retryable" crates/app/src/operations.rs
  EXPECT: /show_retry/
  EVIDENCE: `outcome_is_retryable` accepts only Cancelled/Failed; retryable terminal paths call `show_retry`, while Completed calls `schedule_hide`.

- [x] G5: Focused tests cover retryability and existing Phase 5A backend retry tests remain green.
  CHECK: cargo test -p floe-app phase_5b -- --nocapture && cargo test -p floe-app phase_5a -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: `phase_5b` passed 2 tests and `phase_5a` passed 4 tests.

- [x] G6: Documentation and project status describe Phase 5B behavior, limits, verification, and `phase-5c-context-menu` next.
  CHECK: rg -n "Phase 5B|Retry|phase-5c-context-menu|phase_5b" README.md DESIGN.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /phase-5c-context-menu/
  EVIDENCE: README, DESIGN, architecture, development, roadmap, and AGENTS status now describe Retry and name `phase-5c-context-menu` next.

- [x] G7: Formatting, workspace compilation, strict Clippy, complete tests, and native Wayland smoke pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
  EXPECT: /test result: ok/
  EVIDENCE: fmt, check, strict Clippy, and all 69 tests passed (28 core, 41 app); native smoke also passed.

- [x] G8: Native launch emits startup and remains healthy until timeout.
  CHECK: RUST_LOG=floe_app=info timeout 8s cargo run -p floe-app
  EXPECT: /Floe application started/
  EVIDENCE: escalated Wayland launch logged `Floe application started` and remained healthy until timeout exit 124; only known host warnings appeared.
