# Gates: Floe Phase 4E trash foundation

Scope: Add an application-layer, standards-based trash job on branch `phase-4e-trash-foundation`. Preserve original Linux paths, use GIO/XDG behavior in production, keep execution off GTK callbacks, provide structured lifecycle/cancellation, and test without touching real user trash. Delete shortcuts, restore UI, permanent deletion, and bulk trash remain out of scope.

- [x] G1: Work is isolated on the requested Phase 4E branch and `main` remains at the verified Phase 4D commit.
  CHECK: git branch --show-current && git rev-parse main
  EXPECT: /phase-4e-trash-foundation[\s\S]*d554549/
  EVIDENCE: phase-4e-trash-foundation | d5545496d47d21522f16017bdb409a0695355164

- [x] G2: Application-layer trash requests retain an original `PathBuf` and reject paths without a final component without converting paths to UTF-8 strings.
  CHECK: rg -n "TrashRequest|PathBuf|source\(|InvalidSource" crates/app/src/trash_executor.rs crates/app/src/state.rs
  EXPECT: /TrashRequest/
  EVIDENCE: crates/app/src/state.rs:667:            _request: &TrashRequest, | crates/app/src/state.rs:692:        assert_eq!(tracked.source(), source);

- [x] G3: Production trash execution uses GIO's standards-based trash API with `gio::Cancellable`, never shell commands or ad-hoc XDG trash layout code.
  CHECK: rg -n "gio::File|\.trash\(|gio::Cancellable|GioTrashBackend" crates/app/src/trash_executor.rs
  EXPECT: /\.trash\(/
  EVIDENCE: 325:    let command = match backend.trash(&task.request, &task.cancellable) { | 428:            cancellable: &gio::Cancellable,

- [x] G4: A fixed-capacity application executor runs trash jobs outside GTK callbacks and integrates queued, started, completed, cancelled, failed, capacity, and shutdown lifecycle behavior.
  CHECK: rg -n "TrashExecutor|sync_channel|JobCommand::Start|JobCommand::Complete|JobCommand::Cancel|JobCommand::Fail|QueueFull|Shutdown" crates/app/src/trash_executor.rs
  EXPECT: /TrashExecutor/
  EVIDENCE: 621:        let executor = TrashExecutor::spawn_blocked(jobs.clone(), 1, backend, gate_receiver) | 644:        let executor = TrashExecutor::spawn_with_backend(jobs.clone(), 1, backend)

- [x] G5: Trash failures map cancellation, missing paths, permission denial, unsupported destinations, and other I/O failures into structured job outcomes without silent loss.
  CHECK: rg -n "NotFound|PermissionDenied|NotSupported|Cancelled|JobFailureKind|trash_failure" crates/app/src/trash_executor.rs
  EXPECT: /PermissionDenied/
  EVIDENCE: 586:            assert_eq!(trash_failure(&error).kind(), expected); | 634:            Some(JobState::Cancelled)

- [x] G6: `ApplicationState` owns and tracks trash submissions by `JobId`, routes generic cancellation, and reports the source parent as affected without adding GTK Delete or permanent-delete actions.
  CHECK: rg -n "TrashExecutor|TrackedOperation::Trash|submit_trash|cancel_operation|affected_directories" crates/app/src/state.rs crates/app/src/browser.rs crates/app/src/ui.rs
  EXPECT: /submit_trash/
  EVIDENCE: crates/app/src/state.rs:694:            tracked.affected_directories(), | crates/app/src/state.rs:697:        assert!(matches!(tracked, TrackedOperation::Trash(_)));

- [x] G7: Focused tests use an injected backend and temporary paths to cover original non-UTF-8 paths, success, cancellation, structured failure mapping, capacity, shutdown, and state tracking without modifying real user trash.
  CHECK: cargo test -p floe-app phase_4e -- --nocapture
  EXPECT: /passed/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.02s | Running unittests src/main.rs (target/debug/deps/floe_app-2fd028b442df4964)

- [x] G8: README, design, architecture, development, roadmap, and project status document Phase 4E scope, safety limits, verification, and next branch.
  CHECK: rg -n "Phase 4E|GIO|trash|phase-4f-trash-interaction|phase_4e" README.md DESIGN.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /phase-4f-trash-interaction/
  EVIDENCE: README.md:45:Phase 4E provides the verified trash job foundation, but intentionally exposes | README.md:48:moves, trash interaction/restore UI, permanent deletion, previews, tabs, split

- [x] G9: Formatting, workspace compilation, strict Clippy, and the complete test suite pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
  EXPECT: /test result: ok/
  EVIDENCE: Running unittests src/lib.rs (target/debug/deps/floe_core-945472730d9a84a6) | Doc-tests floe_core

- [x] G10: A native Wayland smoke launch emits the Floe startup event and remains healthy until the planned timeout.
  CHECK: RUST_LOG=floe_app=info timeout 8s cargo run -p floe-app
  EXPECT: /Floe application started/
  EVIDENCE: (floe-app:320609): Gdk-WARNING **: 19:15:31.760: vkAcquireNextImageKHR(): A swapchain no longer matches the surface properties exactly, but can still be used to present to the surface successfully. (V
