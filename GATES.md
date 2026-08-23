# Gates: Floe Phase 4B copy interaction

Scope: Add an application-owned, copy-only clipboard workflow, GTK job observation, and compact non-modal operation feedback on branch `phase-4b-copy-interaction`. Preserve the Phase 4A safety model; overwrite, move, rename, trash, delete, cross-application clipboard formats, and unrelated roadmap features remain out of scope.

- [x] G1: Work is isolated on the requested Phase 4B branch and `main` remains unchanged at the initial commit.
  CHECK: git branch --show-current && git rev-parse main
  EXPECT: /phase-4b-copy-interaction[\s\S]*a720988/
  EVIDENCE: phase-4b-copy-interaction | a720988e4da41f0ec47462d1b98542e2ec4f9365

- [x] G2: Application state owns original-path copy-buffer and paste-submission semantics; destinations are never reconstructed from lossy display text.
  CHECK: rg -n "CopyBuffer|stage_copy|submit_paste|PathBuf|CopyRequest" crates/app/src/state.rs
  EXPECT: /submit_paste/
  EVIDENCE: 336:            .submit_paste_with_cancellation(&destination_directory, cancellation) | 346:            .submit_paste(&completed_directory)

- [x] G3: Ctrl+C and Ctrl+V use controller selection/navigation state and submit through `ApplicationState`; GTK callbacks do not execute filesystem operations directly.
  CHECK: rg -n "win.copy|win.paste|set_accels_for_action|stage_copy|submit_paste" crates/app/src/application.rs crates/app/src/browser.rs
  EXPECT: /win\.copy/
  EVIDENCE: crates/app/src/browser.rs:457:        match self.application_state.submit_paste(&destination) { | crates/app/src/application.rs:32:    application.set_accels_for_action("app.quit", &["<Control>q"]);

- [x] G4: A separate GTK operation observer drains structured job events without blocking, supports cancellation, refreshes a successfully pasted destination, and surfaces recovery-oriented failure feedback.
  CHECK: rg -n "OperationController|drain_job_events|cancel_copy|JobEventKind|refresh_if_current|add_toast" crates/app/src/operations.rs crates/app/src/browser.rs crates/app/src/state.rs
  EXPECT: /OperationController/
  EVIDENCE: crates/app/src/operations.rs:212:        match self.state.cancel_copy(job_id) { | crates/app/src/operations.rs:223:            .add_toast(adw::Toast::builder().title(title).timeout(timeout).build());

- [x] G5: The compact Operations Island is non-modal, uses semantic GTK widgets/icons, exposes an accessible cancel name and tooltip, and keeps visible status text with stable progress layout across appearance presets.
  CHECK: rg -n "operations-island|operation_label|operation_progress|operation_cancel|Cancel copy|ProgressBar|Overlay" crates/app/src/ui.rs crates/app/src/appearance.rs
  EXPECT: /Cancel copy/
  EVIDENCE: crates/app/src/appearance.rs:194:            .operations-island progressbar progress {{ | crates/app/src/appearance.rs:199:            .operations-island button {{

- [x] G6: Focused application tests cover path-safe staging/paste, missing-buffer and inside-source rejection, conflict, cancellation, successful lifecycle, and recovery-oriented operation labels.
  CHECK: cargo test -p floe-app copy_interaction -- --nocapture
  EXPECT: /7 passed/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.02s | Running unittests src/main.rs (target/debug/deps/floe_app-cf50067f0f209ac0)

- [x] G7: User, design, architecture, roadmap, development, and project-status documentation describe the Phase 4B interaction, internal-only clipboard, safety limits, verification, and next branch.
  CHECK: rg -n "Phase 4B|internal-only|internal clipboard|Operations Island|copy_interaction|phase-4c-move-rename-foundation" README.md DESIGN.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /phase-4c-move-rename-foundation/
  EVIDENCE: DESIGN.md:84:Active work appears in a compact, bottom-end Operations Island. It uses visible | DESIGN.md:178:- **Operations Island:** a compact, non-modal observer for queued/running file

- [x] G8: Formatting, workspace compilation, strict Clippy, and the complete test suite pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
  EXPECT: /test result: ok/
  EVIDENCE: Running unittests src/lib.rs (target/debug/deps/floe_core-7b642bdde78542e0) | Doc-tests floe_core

- [x] G9: A native Wayland smoke launch emits the Floe startup event and remains healthy until the planned timeout.
  CHECK: RUST_LOG=floe_app=info timeout 8s cargo run -p floe-app
  EXPECT: /Floe application started/
  EVIDENCE: (floe-app:288055): Adwaita-WARNING **: 18:27:08.455: Using GtkSettings:gtk-application-prefer-dark-theme with libadwaita is unsupported. Please use AdwStyleManager:color-scheme instead. | WARNING: rad
