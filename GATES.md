# Gates: Floe Phase 4F trash interaction

Scope: Expose the verified Phase 4E trash backend through an explicit, single-selection “Move to Trash” action on branch `phase-4f-trash-interaction`. Provide Delete and visible-menu routes, non-blocking lifecycle/recovery feedback, and affected-parent refresh. Preserve original paths and keep all GIO/filesystem work outside GTK callbacks. Permanent deletion, Shift+Delete, bulk trash, restore UI, and undo remain out of scope.

- [x] G1: Work is isolated on the requested Phase 4F branch and `main` remains at the verified Phase 4E commit.
  CHECK: git branch --show-current && git rev-parse main
  EXPECT: /phase-4f-trash-interaction[\s\S]*f43ac45/
  EVIDENCE: phase-4f-trash-interaction | f43ac45b132665d3da55e501529935a256be6019

- [x] G2: A visible file-actions menu item is explicitly labelled “Move to Trash,” selection controls its native enabled state, and Delete provides the keyboard route.
  CHECK: rg -n "Move to Trash|win.trash|Delete|set_selection_actions_enabled|trash_selected" crates/app/src/browser.rs crates/app/src/ui.rs
  EXPECT: /win\.trash/
  EVIDENCE: crates/app/src/browser.rs:431:    fn set_selection_actions_enabled(&self, enabled: bool) { | crates/app/src/browser.rs:521:    fn trash_selected(&self) {

- [x] G3: The GTK callback submits the selected entry's original `PathBuf` through `ApplicationState::submit_trash` and contains no direct GIO, filesystem, or shell mutation.
  CHECK: rg -n "submit_trash|entry\.path\(\)|std::fs|gio::File|std::process" crates/app/src/browser.rs crates/app/src/ui.rs
  EXPECT: /submit_trash/
  EVIDENCE: crates/app/src/browser.rs:529:            .submit_trash(entry.path().to_path_buf()) | crates/app/src/browser.rs:547:        let source = entry.path().to_path_buf();

- [x] G4: Trash interaction remains single-selection, never exposes permanent delete or Shift+Delete, and reports missing selection or submission failure without changing the file.
  CHECK: rg -n "Select an item to move to Trash|Could not move .* to Trash|Shift.*Delete|permanent" crates/app/src/browser.rs crates/app/src/ui.rs
  EXPECT: /Select an item to move to Trash/
  EVIDENCE: crates/app/src/browser.rs:523:            self.show_toast("Select an item to move to Trash", 4); | crates/app/src/browser.rs:536:                &format!("Could not move {display_name} to Trash: {erro

- [x] G5: Operations Island uses explicit trash wording for queued/running/completed/cancelled/unsupported/I/O states and refreshes the affected parent only after completion.
  CHECK: rg -n "Moving to Trash|Moved to Trash|Move to Trash|not silently deleted|affected_directories|TerminalResult::Completed" crates/app/src/operations.rs crates/app/src/state.rs
  EXPECT: /Moved to Trash/
  EVIDENCE: crates/app/src/operations.rs:484:            "Moving to Trash through GIO…" | crates/app/src/operations.rs:486:        assert_eq!(completed_title(Some(&trashed)), "Moved to Trash");

- [x] G6: Focused tests cover trash-specific progress/completion/recovery wording and preserve the Phase 4E backend/path-safety test suite.
  CHECK: cargo test -p floe-app phase_4f -- --nocapture && cargo test -p floe-app phase_4e -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.02s | Running unittests src/main.rs (target/debug/deps/floe_app-2fd028b442df4964)

- [x] G7: README, design, architecture, development, roadmap, and project status document Phase 4F interaction, no-confirmation rationale, safety limits, verification, and next branch.
  CHECK: rg -n "Phase 4F|Move to Trash|Delete|no confirmation|phase-5a-operation-resilience|phase_4f" README.md DESIGN.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /phase-5a-operation-resilience/
  EVIDENCE: README.md:47:Phase 4F exposes the verified trash job as a single-selection, recoverable | README.md:48:desktop Trash action. Permanent delete, Shift+Delete, bulk trash, and built-in

- [x] G8: Formatting, workspace compilation, strict Clippy, and the complete test suite pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
  EXPECT: /test result: ok/
  EVIDENCE: Running unittests src/lib.rs (target/debug/deps/floe_core-945472730d9a84a6) | Doc-tests floe_core

- [x] G9: A native Wayland smoke launch emits the Floe startup event and remains healthy until the planned timeout.
  CHECK: RUST_LOG=floe_app=info timeout 8s cargo run -p floe-app
  EXPECT: /Floe application started/
  EVIDENCE: WARNING: radv is not a conformant Vulkan implementation, testing use only. | (floe-app:331542): Gdk-WARNING **: 19:38:16.882: vkAcquireNextImageKHR(): A swapchain no longer matches the surface propert
