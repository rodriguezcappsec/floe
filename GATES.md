# Gates: Floe Phase 4C move and rename foundation

Scope: Add GTK-independent, path-safe move and rename request models plus a bounded application executor on branch `phase-4c-move-rename-foundation`. Preserve original Linux paths and atomic fail-if-exists semantics. GTK actions, overwrite, trash, permanent delete, and cross-filesystem copy-delete fallback remain out of scope.

- [x] G1: Work is isolated on the requested Phase 4C branch and the Phase 4B commit remains unchanged.
  CHECK: git branch --show-current && git rev-parse phase-4b-copy-interaction
  EXPECT: /phase-4c-move-rename-foundation[\s\S]*99382d3/
  EVIDENCE: phase-4c-move-rename-foundation | 99382d34f454c5c256bb924ddbaca3a4f1fc78be

- [x] G2: Core exposes separate exact-path move and same-parent rename requests without representing paths as strings.
  CHECK: rg -n "MoveRequest|RenameRequest|PathBuf|OsString|new_name" crates/core/src
  EXPECT: /RenameRequest/
  EVIDENCE: crates/core/src/navigation.rs:104:        assert_eq!(state.current(), PathBuf::from("/one")); | crates/core/src/navigation.rs:106:        assert_eq!(state.current(), PathBuf::from("/"));

- [x] G3: Linux execution uses atomic no-replace rename semantics, preserves symlinks as links, rejects missing sources and invalid rename names, and reports cross-filesystem moves explicitly.
  CHECK: rg -n "NOREPLACE|CrossFilesystem|symlink_metadata|InvalidName|SourceMissing" crates/core/src
  EXPECT: /NOREPLACE/
  EVIDENCE: crates/core/src/copy.rs:591:fn symlink_metadata(path: &Path, action: &'static str) -> Result<fs::Metadata, CopyError> { | crates/core/src/copy.rs:592:    fs::symlink_metadata(path).map_err(|source| Co

- [x] G4: Cancellation is observed before the irreversible rename boundary and lifecycle failures map into structured job events.
  CHECK: rg -n "is_cancelled|Cancelled|JobFailureKind|execute_move|execute_rename" crates/core/src crates/app/src
  EXPECT: /execute_move/
  EVIDENCE: crates/core/src/directory.rs:29:            return Err(DirectoryError::Cancelled); | crates/core/src/directory.rs:159:        assert!(matches!(result, Err(DirectoryError::Cancelled)));

- [x] G5: A fixed-capacity application executor runs move/rename jobs off GTK and supports cancellation, lifecycle completion/failure, and clean shutdown without adding GTK mutation callbacks.
  CHECK: rg -n "MoveExecutor|submit_move|submit_rename|sync_channel|cancel|shutdown" crates/app/src
  EXPECT: /MoveExecutor/
  EVIDENCE: crates/app/src/copy_executor.rs:553:            .expect("queued copy should be cancellable"); | crates/app/src/copy_executor.rs:563:                .expect("cancelled copy should remain registered")

- [x] G6: Temporary-directory core tests cover file, directory, symlink, conflict, invalid rename, missing source, cancellation, and non-UTF-8 paths while leaving conflicting targets unchanged.
  CHECK: cargo test -p floe-core move_operation -- --nocapture
  EXPECT: /8 passed/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.18s | Running unittests src/lib.rs (target/debug/deps/floe_core-454dc5653b111925)

- [x] G7: Application tests cover move and rename lifecycle completion, cancellation, failure mapping, queue capacity, and shutdown.
  CHECK: cargo test -p floe-app move_executor -- --nocapture
  EXPECT: /6 passed/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.02s | Running unittests src/main.rs (target/debug/deps/floe_app-2fd028b442df4964)

- [x] G8: Architecture, development, roadmap, README, and project status document Phase 4C scope, atomic same-filesystem limit, verification, and next branch.
  CHECK: rg -n "Phase 4C|same-filesystem|cross-filesystem|phase-4d-move-rename-interaction|move_executor" README.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /phase-4d-move-rename-interaction/
  EVIDENCE: README.md:42:clipboard formats, overwrite, cross-filesystem copy-delete moves, trash, | docs/DEVELOPMENT.md:91:cargo test -p floe-app move_executor

- [x] G9: Formatting, workspace compilation, strict Clippy, and the complete test suite pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
  EXPECT: /test result: ok/
  EVIDENCE: Running unittests src/lib.rs (target/debug/deps/floe_core-945472730d9a84a6) | Doc-tests floe_core

- [x] G10: A native Wayland smoke launch emits the Floe startup event and remains healthy until the planned timeout.
  CHECK: RUST_LOG=floe_app=info timeout 8s cargo run -p floe-app
  EXPECT: /Floe application started/
  EVIDENCE: (floe-app:296616): Adwaita-WARNING **: 18:38:44.598: Using GtkSettings:gtk-application-prefer-dark-theme with libadwaita is unsupported. Please use AdwStyleManager:color-scheme instead. | WARNING: rad
