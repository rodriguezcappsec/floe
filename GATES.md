# Gates: Floe Phase 4A safe copy engine

Scope: Implement the first filesystem-mutation slice: path-safe copy requests, explicit conflict and symlink semantics, a bounded application executor, lifecycle integration, cancellation, and temporary-directory verification. Move, rename, trash, delete, and GTK mutation controls remain out of scope.

- [x] G1: `floe-core` exposes path-safe copy request, conflict policy, symlink policy, validation, execution result, and structured copy error types without GTK dependencies.
  CHECK: rg -n "CopyRequest|ConflictPolicy|SymlinkPolicy|CopyOutcome|CopyError" crates/core/src
  EXPECT: /CopyRequest/
  EVIDENCE: crates/core/src/lib.rs:11:    ConflictPolicy, CopyCancellation, CopyError, CopyOutcome, CopyProgress, CopyRequest, | crates/core/src/lib.rs:12:    SymlinkPolicy, execute_copy,

- [x] G2: copy execution preserves bytes and original path identity, supports files/directories, rejects unsafe self/descendant copies, never silently overwrites, and follows the documented symlink policy.
  CHECK: cargo test -p floe-core copy -- --nocapture
  EXPECT: /test result: ok\. 9 passed/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.21s | Running unittests src/lib.rs (target/debug/deps/floe_core-f780aeceafe054cb)

- [x] G3: the application owns a bounded copy executor that maps queued work into existing job started/progress/completed/failed/cancelled lifecycle events without doing filesystem work in GTK callbacks.
  CHECK: rg -n "CopyExecutor|CopyCommand|submit_copy|JobCommand::(Start|SetProgress|Complete|Fail|Cancel)" crates/app/src crates/core/src
  EXPECT: /CopyExecutor/
  EVIDENCE: crates/app/src/job_manager.rs:168:            .transition(queued.job_id(), JobCommand::Cancel) | crates/app/src/job_manager.rs:172:                .transition(queued.job_id(), JobCommand::Start)

- [x] G4: executor tests prove success, failure, cancellation, capacity bounds, retry-safe identity, and shutdown using temporary directories only.
  CHECK: cargo test -p floe-app copy_executor -- --nocapture
  EXPECT: /test result: ok\. 6 passed/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.05s | Running unittests src/main.rs (target/debug/deps/floe_app-cf50067f0f209ac0)

- [x] G5: project documentation and `AGENTS.md` accurately describe implemented Phase 4A semantics, limitations, and the next milestone.
  CHECK: rg -n "Phase 4A|copy executor|ConflictPolicy|SymlinkPolicy|Move|Rename|Trash" README.md DESIGN.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /Phase 4A/
  EVIDENCE: docs/ARCHITECTURE.md:229:bounded copy executor (implemented) | docs/ARCHITECTURE.md:232:Move/rename/trash implementations must not appear in widgets or reuse either

- [x] G6: the complete Rust quality suite passes with warnings denied.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
  EXPECT: /test result: ok\. 20 passed/
  EVIDENCE: Running unittests src/lib.rs (target/debug/deps/floe_core-7b642bdde78542e0) | Doc-tests floe_core

- [x] G7: a native Wayland smoke launch keeps the Floe window healthy after Phase 4A wiring.
  CHECK: RUST_LOG=floe_app=info timeout 8s cargo run -p floe-app
  EXPECT: /Floe application started/
  EVIDENCE: WARNING: radv is not a conformant Vulkan implementation, testing use only. | (floe-app:256014): Gdk-WARNING **: 17:59:27.786: vkAcquireNextImageKHR(): A swapchain no longer matches the surface propert

- [x] G8: the workspace contains exactly 30 registered Rust tests.
  CHECK: cargo test --workspace -- --list 2>/dev/null | rg -c ": test$"
  EXPECT: /^30$/m
  EVIDENCE: 30
