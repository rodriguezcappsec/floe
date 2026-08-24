# Gates: Floe Phase 5E conflict foundation

Scope: Distinguish destination conflicts from generic failures and require an explicit application-layer decision. A conflict can be acknowledged while keeping the existing destination or retried with one validated raw filename under the same logical operation ID. Blind Retry, silent overwrite, apply-to-all, and GTK conflict dialogs remain unavailable.

- [x] G1: Branch is `phase-5e-conflict-foundation` and `main` remains at Phase 5D.
  CHECK: git branch --show-current && git rev-parse main
  EXPECT: /phase-5e-conflict-foundation[\s\S]*8044d67/
  EVIDENCE: branch is `phase-5e-conflict-foundation`; local `main` is Phase 5D commit `8044d67`.

- [x] G2: Terminal conflicts have a distinct outcome and generic Retry rejects them with a decision-required error.
  CHECK: rg -n "TerminalOutcome::Conflict|ConflictDecisionRequired|outcome_is_retryable" crates/app/src
  EXPECT: /ConflictDecisionRequired/
  EVIDENCE: `operations.rs` maps `JobFailureKind::Conflict` to `TerminalOutcome::Conflict`; `retry_operation` returns `ConflictDecisionRequired` and conflicts are not retryable UI outcomes.

- [x] G3: Pending conflict data retains job/operation IDs and original source/destination paths without display-text reconstruction.
  CHECK: rg -n "PendingConflict|operation_id|source|destination" crates/app/src/state.rs
  EXPECT: /PendingConflict/
  EVIDENCE: `PendingConflict` owns `JobId`, `OperationId`, `PathBuf` source, and `PathBuf` destination derived only from preserved requests.

- [x] G4: Explicit decisions are KeepExisting or RetryWithName(OsString); no overwrite decision exists.
  CHECK: rg -n "ConflictDecision|KeepExisting|RetryWithName|Overwrite" crates/app/src/state.rs
  EXPECT: /RetryWithName/
  EVIDENCE: `ConflictDecision` contains exactly `KeepExisting` and `RetryWithName(OsString)`; no overwrite variant or policy path was added.

- [x] G5: RetryWithName validates one raw filename, rebuilds a fail-if-exists request, preserves the logical operation ID, and never supports Trash conflicts.
  CHECK: rg -n "resolve_conflict|validate_rename_name|FailIfExists|ConflictUnsupported" crates/app/src/state.rs
  EXPECT: /resolve_conflict/
  EVIDENCE: `resolve_conflict` uses `validate_rename_name`, rebuilds `FailIfExists` requests, dispatches existing identity-preserving retry APIs, and returns `ConflictUnsupported` for Trash.

- [x] G6: Conflict acknowledgement/resolution is single-use and terminal-history eviction clears resolution bookkeeping safely.
  CHECK: rg -n "resolved_conflicts|insert|remove|forget_terminal" crates/app/src/state.rs
  EXPECT: /resolved_conflicts/
  EVIDENCE: resolved job IDs are rejected as `ConflictAlreadyResolved`; terminal-history eviction removes the same ID from `resolved_conflicts` before forgetting the terminal record.

- [x] G7: Focused Phase 5E tests cover conflict identity, blind-retry rejection, validation, revised retry success, keep-existing, single-use, and no overwrite.
  CHECK: cargo test -p floe-app phase_5e -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: six focused Phase 5E tests pass for terminal mapping, copy/move/rename decisions, non-UTF-8 name preservation, no overwrite, keep-existing, single-use, Trash rejection, and eviction cleanup.

- [x] G8: Documentation/status, formatting, workspace compilation, strict Clippy, all tests, diff hygiene, gate check, and native Wayland smoke pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: README, DESIGN, architecture, development, roadmap, and AGENTS status are updated; fmt, check, strict Clippy, all 81 tests (28 core, 53 app), and diff hygiene pass; native Wayland launch logged startup and stayed healthy until timeout.
