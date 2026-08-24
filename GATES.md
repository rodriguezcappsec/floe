# Gates: Floe Phase 6J multi-selection context surfaces

Scope: Path-safe multi-file selection, batch-safe actions, and distinct entry/background context menus.

- [x] G1: The phase branch starts from completed Phase 6I.
  CHECK: git branch --show-current && git merge-base --is-ancestor d7c9ccf HEAD
  EXPECT: /phase-6j-multi-selection-context/
  EVIDENCE: Current branch is `phase-6j-multi-selection-context`; `d7c9ccf` is its ancestor.

- [x] G2: List and grid share native GTK multi-selection with select-all and clear-selection keyboard routes.
  CHECK: rg -n 'MultiSelection' crates/app/src/ui.rs crates/app/src/browser.rs && rg -n 'select-all|clear-selection' crates/app/src/browser.rs
  EXPECT: /MultiSelection[\s\S]*select-all[\s\S]*clear-selection/
  EVIDENCE: Both views share `GtkMultiSelection`; Ctrl+A, Ctrl+Shift+A, and focused Escape routes are registered.

- [x] G3: Sorting restores multiple selected exact paths, including colliding lossy non-UTF-8 names.
  CHECK: cargo test -p floe-app phase_6j_selection_restoration -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: The focused exact-path multi-selection restoration test passes.

- [x] G4: Copy, move, and Trash accept complete selections through application-owned bounded serial batching.
  CHECK: cargo test -p floe-app phase_6j_multi_ -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Focused batch tests pass, including copy beyond the eight-item worker queue capacity.

- [x] G5: Secondary-click preserves an existing selected group or retargets to one unselected entry.
  CHECK: cargo test -p floe-app phase_6j_secondary_click -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: The focused `Preserve` and `SelectOnly` policy test passes.

- [x] G6: Directory-background context exposes only directory-scoped actions.
  CHECK: cargo test -p floe-app phase_6j_background_context_menu -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: The focused menu-policy test verifies Paste, Select All, Refresh, and Edit Location only.

- [x] G7: Action policy restricts Open, Open With, and Rename to one target while allowing multi-target transfer and Trash actions.
  CHECK: cargo test -p floe-app phase_6j_action_policy -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: The focused zero/one/many action-policy test passes.

- [x] G8: Product documentation records Phase 6J and truthfully distinguishes permanent deletion from secure erase.
  CHECK: rg -n 'Phase 6J|Phase 6K|Phase 6L|Phase 6M|Delete Permanently|secure erase' README.md DESIGN.md docs/ROADMAP.md AGENTS.md
  EXPECT: /Delete Permanently/
  EVIDENCE: README, DESIGN, ROADMAP, and AGENTS describe completed 6J, sequenced 6K-6M, and truthful permanent-delete wording.

- [x] G9: Formatting, compilation, strict Clippy, all tests, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: All commands passed; 148 tests passed: 115 application and 33 core.

- [x] G10: Native Wayland smoke owns and releases the D-Bus name and activates the new selection actions.
  EVIDENCE: Floe exported and activated `select-all` and `clear-selection`, remained healthy, exited zero through Quit, and released its D-Bus name; only known host warnings appeared.

- [x] G11: The phase branch and main refs were committed, pushed, merged, and synchronized at the implementation checkpoint.
  CHECK: git rev-parse main phase-6j-multi-selection-context origin/main origin/phase-6j-multi-selection-context
  EXPECT: /^([0-9a-f]{40})\n\1\n\1\n\1$/
  EVIDENCE: Before this ledger-only finalization, all four refs resolved to `a9a09fae61cedafa4d4e0a70c9912dcedef275bf`.

- [x] G12: The gate checker reports every Phase 6J gate met.
  CHECK: node <unlazy-skill-dir>/scripts/gate-check.mjs --status GATES.md
  EXPECT: /ALL MET/
  EVIDENCE: The status-only gate checker reported `ALL MET` after the ledger was repaired.
