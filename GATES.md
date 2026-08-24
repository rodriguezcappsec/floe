# Gates: Floe Phase 5A operation resilience foundation

Scope: Generalize backend retry request tracking across copy, move, rename, and trash on branch `phase-5a-operation-resilience`. Retried attempts must preserve logical `OperationId`, receive a fresh `JobId`, and use the original path-safe request. Add bounded terminal operation history. Retry UI, overwrite, pause/resume controls, permanent deletion, and interactive conflict resolution remain out of scope.

- [x] G1: Work is isolated on the requested Phase 5A branch and `main` remains at the verified Phase 4F commit.
  CHECK: git branch --show-current && git rev-parse main
  EXPECT: /phase-5a-operation-resilience[\s\S]*ed17304/
  EVIDENCE: phase-5a-operation-resilience | ed17304807e4d8ef6c5961b19bdd4455cda48e10

- [x] G2: Move/rename and trash executors expose retry submission that delegates identity allocation to `ApplicationJobManager::retry` and reuses bounded queues.
  CHECK: rg -n "submit_.*retry|\.retry\(failed_job_id\)|enqueue" crates/app/src/copy_executor.rs crates/app/src/move_executor.rs crates/app/src/trash_executor.rs
  EXPECT: /submit_trash_retry/
  EVIDENCE: crates/app/src/copy_executor.rs:186:    fn enqueue( | crates/app/src/copy_executor.rs:525:            .submit_retry(failed.job_id(), request(&source, &destination))

- [x] G3: Application state records bounded terminal history with job ID, operation ID, terminal outcome, and original `TrackedOperation` request.
  CHECK: rg -n "MAX_TERMINAL_HISTORY|TerminalOperation|TerminalOutcome|terminal_history|OperationId|TrackedOperation" crates/app/src/state.rs
  EXPECT: /MAX_TERMINAL_HISTORY/
  EVIDENCE: 1045:        let history = state.terminal_history(); | 1046:        assert_eq!(history.len(), MAX_TERMINAL_HISTORY);

- [x] G4: Retrying a failed or cancelled copy/move/rename/trash dispatches the preserved request, tracks the new job, and does not reconstruct paths from display text.
  CHECK: rg -n "retry_operation|submit_retry|submit_move_retry|submit_rename_retry|submit_trash_retry|PathBuf|OsString" crates/app/src/state.rs crates/app/src/*_executor.rs
  EXPECT: /retry_operation/
  EVIDENCE: crates/app/src/state.rs:1058:            state.retry_operation(first_job.expect("first job")), | crates/app/src/state.rs:1062:            state.retry_operation(last_job.expect("last job")),

- [x] G5: Completed jobs and unknown/evicted history entries reject retry clearly; terminal history remains bounded under sustained work.
  CHECK: rg -n "RetryCompleted|RetryNotFound|pop_front|MAX_TERMINAL_HISTORY|completed" crates/app/src/state.rs
  EXPECT: /RetryCompleted/
  EVIDENCE: 1059:            Err(CopyInteractionError::RetryNotFound(_)) | 1063:            Err(CopyInteractionError::RetryCompleted(_))

- [x] G6: Focused tests cover all four operation kinds, original non-UTF-8 paths, preserved operation identity/new job identity, cancelled retry, completed rejection, and history eviction.
  CHECK: cargo test -p floe-app phase_5a -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s | Running unittests src/main.rs (target/debug/deps/floe_app-2fd028b442df4964)

- [x] G7: No retry UI, overwrite path, permanent delete, or direct GTK filesystem work is introduced in this backend foundation.
  CHECK: if rg -n "retry_operation|win.retry|overwrite|permanent|std::fs|gio::File" crates/app/src/browser.rs crates/app/src/ui.rs crates/app/src/operations.rs; then exit 1; else echo "no retry UI or direct filesystem work"; fi
  EXPECT: /no retry UI/
  EVIDENCE: no retry UI or direct filesystem work

- [x] G8: README, architecture, development, roadmap, and project status document Phase 5A scope, bounded history, verification, and next branch.
  CHECK: rg -n "Phase 5A|terminal history|retry|phase-5b-retry-interaction|phase_5a" README.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /phase-5b-retry-interaction/
  EVIDENCE: docs/ARCHITECTURE.md:342:Phase 5A generalizes retry dispatch and bounds application terminal history. | docs/ARCHITECTURE.md:343:There is no retry control in GTK yet; overwrite, interactive conflict c

- [x] G9: Formatting, workspace compilation, strict Clippy, and the complete test suite pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
  EXPECT: /test result: ok/
  EVIDENCE: Running unittests src/lib.rs (target/debug/deps/floe_core-945472730d9a84a6) | Doc-tests floe_core

- [x] G10: A native Wayland smoke launch emits the Floe startup event and remains healthy until the planned timeout.
  CHECK: RUST_LOG=floe_app=info timeout 8s cargo run -p floe-app
  EXPECT: /Floe application started/
  EVIDENCE: WARNING: radv is not a conformant Vulkan implementation, testing use only. | (floe-app:338287): Gdk-WARNING **: 19:47:21.522: vkAcquireNextImageKHR(): A swapchain no longer matches the surface propert
