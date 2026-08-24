# Gates: Floe Phase 6J multi-selection and context surfaces

Scope: Path-safe multi-file selection, batch-safe actions, and distinct entry/background context menus.

- [x] G1: Phase branch starts from completed Phase 6I.
  CHECK: git branch --show-current && git merge-base --is-ancestor d7c9ccf HEAD
  EXPECT: /phase-6j-multi-selection-context/
  EVIDENCE: Current branch is `phase-6j-multi-selection-context`; `d7c9ccf` is its ancestor.

- [x] G2: List and grid share native GTK multi-selection with select-all and clear-selection keyboard routes.
  CHECK: rg -n 'MultiSelection' crates/app/src/ui.rs crates/app/src/browser.rs && rg -n 'select-all|clear-selection' crates/app/src/browser.rs
  EXPECT: /MultiSelection[\s\S]*select-all[\s\S]*clear-selection/
  EVIDENCE: Both views share `GtkMultiSelection`; Ctrl+A, Ctrl+Shift+A, and focused Escape routes are registered.

- [x] G3: Multiple exact non-UTF-8 paths restore after sorting.
  CHECK: cargo test -p floe-app phase_6j_selection_restoration -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Restoration test passes for two paths whose lossy display names collide.

- [x] G4: Copy, move, and Trash accept full selections through application-owned bounded batching; single-target actions stay single.
  CHECK: cargo test -p floe-app phase_6j_multi_ -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Serial copy/move/Trash tests pass, including a 12-file copy beyond worker queue capacity.

- [x] G5: Entry secondary-click preserves or retargets selection correctly.
  CHECK: cargo test -p floe-app phase_6j_secondary_click -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: `Preserve` and `SelectOnly` policy outcomes pass.

- [x] G6: Background secondary-click uses a directory-only action surface.
  CHECK: cargo test -p floe-app phase_6j_background_context_menu -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Paste, Select All, Refresh, and Edit Location are exact; entry-only actions are absent.

- [x] G7: Zero, one, and many selection states expose accurate status and action policy.
  CHECK: cargo test -p floe-app phase_6j_action_policy -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Action/status policy passes; existing Menu-key and Shift+F10 tests remain green.

- [x] G8: Persistent docs record Phase 6J and sequence Phases 6K-6M with truthful permanent-delete wording.
  CHECK: rg -n 'Phase 6J|Phase 6K|Phase 6L|Phase 6M|Delete Permanently|secure erase' README.md DESIGN.md docs/ROADMAP.md AGENTS.md
  EXPECT: /Delete Permanently/
  EVIDENCE: README, DESIGN, ROADMAP, and AGENTS describe completed 6J and sequenced 6K-6M.

- [x] G9: Formatting, compilation, strict Clippy, all tests, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: All commands passed; 148 tests pass: 115 application and 33 core.

- [x] G10: Native Wayland smoke owns/releases D-Bus and activates the new selection actions.
  EVIDENCE: Floe exported and activated `select-all` and `clear-selection`, remained healthy, exited 0 through Quit, and released its D-Bus name. Only known host warnings appeared.

- [ ] G11: Phase branch and `main` are committed, pushed, merged, and synchronized.
  CHECK: git rev-parse main phase-6j-multi-selection-context origin/main origin/phase-6j-multi-selection-context
  EXPECT: /^([0-9a-f]{40})\n\1\n\1\n\1$/
  EVIDENCE: pending

- [ ] G12: Gate checker reports every Phase 6J gate met.
  CHECK: node /home/rocappsec/.codex/skills/unlazy/scripts/gate-check.mjs GATES.md
  EXPECT: /ALL MET/
  EVIDENCE: pending
