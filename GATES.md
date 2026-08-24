# Gates: Floe Phase 6H editable location bar

Scope: Turn the existing hidden Ctrl+L entry into a complete inline, pointer- and keyboard-editable location bar while preserving exact filesystem path ownership.

- [x] G1: Work is isolated on `phase-6h-editable-location-bar` from completed Phase 6G commit `8ce2e73`.
  CHECK: git branch --show-current && git merge-base --is-ancestor 8ce2e73 HEAD && git rev-parse --short 8ce2e73
  EXPECT: /phase-6h-editable-location-bar[\s\S]*8ce2e73/
  EVIDENCE: Branch and ancestor check printed `phase-6h-editable-location-bar` and `8ce2e73`.

- [x] G2: The header location surface is operable by pointer and Ctrl+L, seeds the current displayed path, selects it for immediate replacement, and exposes accessible labels/tooltips.
  CHECK: rg -n 'location_hit_target|connect_clicked|show_location_entry|select_region|Ctrl\+L|Edit location' crates/app/src/ui.rs crates/app/src/browser.rs
  EXPECT: /select_region/
  EVIDENCE: Native button/action wiring, current-path seeding, selection, tooltip, and accessible label are present in `ui.rs` and `browser.rs`.

- [x] G3: Enter submits a non-empty path through application navigation, Escape cancels without navigation, and successful navigation returns focus to the active file view.
  CHECK: rg -n 'submit_location_entry|cancel-location|hide_location_entry|focus_view|navigate_to' crates/app/src/browser.rs
  EXPECT: /submit_location_entry/
  EVIDENCE: Entry activation routes to `submit_location_entry`; cancel restores any snapshot; `hide_location_entry` returns focus to active List/Grid.

- [x] G4: Empty, relative, and invalid/non-directory submissions remain in edit mode with specific inline recovery feedback rather than silently dismissing the editor.
  CHECK: rg -n 'LocationInput|validate_location_input|location_error|set_error|must be an absolute path|not a directory' crates/app/src
  EXPECT: /validate_location_input/
  EVIDENCE: GTK-independent validation and `location_failure_message` feed visible alert-role text plus the entry accessible description.

- [x] G5: Original `PathBuf` navigation state remains authoritative; existing paths are never reconstructed from lossy label text unless the user explicitly submits edited text.
  CHECK: rg -n 'navigation.*current|to_string_lossy|PathBuf::from|location_entry' crates/app/src/browser.rs crates/app/src/location_input.rs
  EXPECT: /navigation/
  EVIDENCE: `resolve_location_input` returns the exact current path when seeded lossy text is unchanged; explicit edited submissions alone create a new `PathBuf`.

- [x] G6: Focused Phase 6H tests cover trimming, empty/relative rejection, absolute paths, directory/file distinction, and non-UTF-8 current-path display ownership.
  CHECK: cargo test -p floe-app phase_6h -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Five focused tests passed, including exact navigation-snapshot rollback.

- [x] G7: README, design, architecture, development, roadmap, and AGENTS status describe Phase 6H and name `phase-6i-open-with-fallback` next.
  CHECK: rg -n 'Phase 6H|phase-6i-open-with-fallback|editable location' README.md DESIGN.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /phase-6i-open-with-fallback/
  EVIDENCE: All six persistent documents contain Phase 6H and next-branch guidance.

- [x] G8: Formatting, workspace compilation, strict Clippy, all tests, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: Formatting, workspace check, strict Clippy, 106 app tests, 33 core tests, doc tests, and diff hygiene passed.

- [x] G9: Native Wayland smoke verifies pointer/Ctrl+L editor exposure and Escape cancellation; focused tests verify Enter submission and invalid-input recovery; the app shuts down cleanly.
  EVIDENCE: Isolated GTK/AT-SPI run exposed `Edit location`, labelled `Folder location`, exact `/tmp/.../home` seed, Ctrl+L/cancel window actions, healthy D-Bus ownership, and clean Quit; only the known RADV warning appeared.

- [x] G10: The phase is committed, pushed, fast-forwarded into `main`, and local/remote phase/main refs all match.
  CHECK: git rev-parse main phase-6h-editable-location-bar origin/main origin/phase-6h-editable-location-bar
  EXPECT: /^([0-9a-f]{40})\n\1\n\1\n\1$/
  EVIDENCE: Initial publication check returned `9511a1ad0a196ac991fdaf460e257d56c703ad4f` for all four refs; equality is rerun after ledger finalization.

- [x] G11: The gate checker reports every Phase 6H acceptance gate met.
  CHECK: node /home/rocappsec/.codex/skills/unlazy/scripts/gate-check.mjs GATES.md
  EXPECT: /ALL MET/
  EVIDENCE: Gate checker reports `ALL MET (11 met)` after publication and is rerun after final ref synchronization.
