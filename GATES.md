# Gates: Floe Phase 4D move and rename interaction

Scope: Add application-owned cut/move and rename interaction on branch `phase-4d-move-rename-interaction`, using the verified Phase 4C backend. Preserve original paths, keep GTK callbacks filesystem-free, and retain atomic fail-if-exists semantics. Overwrite, cross-filesystem fallback, trash, and permanent delete remain out of scope.

- [x] G1: Work is isolated on the requested Phase 4D branch and `main` remains at the verified Phase 4C commit.
  CHECK: git branch --show-current && git rev-parse main
  EXPECT: /phase-4d-move-rename-interaction[\s\S]*a918ad2/
  EVIDENCE: phase-4d-move-rename-interaction | a918ad2281615e7372110ae39e5ebb3bc8a03790

- [x] G2: One application-owned transfer buffer stores copy or move intent with the original `PathBuf`, and paste dispatches through the appropriate bounded executor.
  CHECK: rg -n "TransferBuffer|TransferIntent|stage_copy|stage_move|submit_paste|MoveExecutor|CopyExecutor" crates/app/src/state.rs
  EXPECT: /TransferIntent/
  EVIDENCE: 567:            Some((TransferIntent::Move, source.clone())) | 571:            .submit_paste(&destination_directory)

- [x] G3: Application state tracks copy, move, and rename requests by `JobId`, exposes generic cancellation/event observation, and refresh metadata without reconstructing lossy paths.
  CHECK: rg -n "TrackedOperation|operation_request|finish_operation|cancel_operation|affected_directories|PathBuf|OsString" crates/app/src/state.rs
  EXPECT: /TrackedOperation/
  EVIDENCE: 585:        state.finish_operation(moved.job_id(), true); | 588:        let renamed_name = OsString::from_vec(b"renamed-\xfe".to_vec());

- [x] G4: Ctrl+X stages a move, Ctrl+V dispatches the staged intent, F2 starts rename, and visible menu actions provide pointer alternatives without direct filesystem calls in GTK callbacks.
  CHECK: rg -n "win.cut|win.paste|win.rename|<Control>x|F2|File actions|stage_selected_move|show_rename" crates/app/src
  EXPECT: /win\.rename/
  EVIDENCE: crates/app/src/ui.rs:115:        .tooltip_text("File actions") | crates/app/src/ui.rs:118:    set_accessible_label(&file_actions, "File actions");

- [x] G5: Rename uses a focused native dialog with inline validation, accessible labels, cancel/submit controls, and retains the original source path separately from editable text.
  CHECK: rg -n "Rename item|rename_entry|rename_error|Invalid name|set_accessible|grab_focus|submit_rename" crates/app/src
  EXPECT: /rename_error/
  EVIDENCE: crates/app/src/state.rs:244:        match self.move_executor.submit_rename(request.clone()) { | crates/app/src/state.rs:590:            .submit_rename(moved_path.clone(), renamed_name.clone())

- [x] G6: The Operations Island observes copy/move/rename jobs generically, shows operation-specific text, supports cancellation, refreshes affected directories, and gives recovery-oriented conflict/cross-filesystem feedback.
  CHECK: rg -n "OperationController|TrackedOperation|operation_title|Cross-filesystem|cancel_operation|affected_directories|refresh_if_current" crates/app/src/operations.rs crates/app/src/state.rs crates/app/src/browser.rs
  EXPECT: /Cross-filesystem/
  EVIDENCE: crates/app/src/operations.rs:422:        assert_eq!(operation_title(Some(&moved)), "Moving photos"); | crates/app/src/operations.rs:425:            "Cross-filesystem move is not supported yet"

- [x] G7: Focused application tests cover copy/move transfer replacement, exact move destination, non-UTF-8 path preservation, rename lifecycle, cancellation, conflict feedback, and validation.
  CHECK: cargo test -p floe-app phase_4d -- --nocapture
  EXPECT: /8 passed/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.05s | Running unittests src/main.rs (target/debug/deps/floe_app-2fd028b442df4964)

- [x] G8: README, design, architecture, development, roadmap, and project status document Phase 4D behavior, safety limits, verification, and next branch.
  CHECK: rg -n "Phase 4D|Ctrl\+X|F2|same-filesystem|phase-4e-trash-foundation|phase_4d" README.md DESIGN.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /phase-4e-trash-foundation/
  EVIDENCE: README.md:40:- F2 rename dialog with inline validation and visible file-actions menu | README.md:42:Phase 4D copy/move/paste and rename work within the running Floe application.

- [x] G9: Formatting, workspace compilation, strict Clippy, and the complete test suite pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
  EXPECT: /test result: ok/
  EVIDENCE: Running unittests src/lib.rs (target/debug/deps/floe_core-945472730d9a84a6) | Doc-tests floe_core

- [x] G10: A native Wayland smoke launch emits the Floe startup event and remains healthy until the planned timeout.
  CHECK: RUST_LOG=floe_app=info timeout 8s cargo run -p floe-app
  EXPECT: /Floe application started/
  EVIDENCE: (floe-app:310791): Adwaita-WARNING **: 19:02:21.535: Using GtkSettings:gtk-application-prefer-dark-theme with libadwaita is unsupported. Please use AdwStyleManager:color-scheme instead. | WARNING: rad
