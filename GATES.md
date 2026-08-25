# Gates: Floe Phase 11B — Command Palette

- [x] G1: Correct phase branch and Phase 11B-only scope.
  CHECK: git branch --show-current
  EXPECT: phase-11b-command-palette
  EVIDENCE: Branch confirmed; no shortcut customization, Vim mode, terminal integration, or filesystem changes were added.

- [x] G2: Search is deterministic, metadata-only, query/result bounded, category/term aware, and gives useful exact/prefix ranking.
  CHECK: cargo test -p floe-app phase_11b_palette_search -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Passed exact Open With ranking, checksum search term, Split View category, 128-character query bound, 64-result cap, and deterministic stable score order.

- [x] G3: Recent commands are de-duplicated, capped at 16, memory-only, and never record disabled/missing activation attempts.
  CHECK: cargo test -p floe-app phase_11b_palette_recent -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Passed 16-entry eviction, move-to-front de-duplication, unknown rejection, and process-owned structure with no persistence path.

- [x] G4: Palette activation delegates only to live GActions, reflects enabled/disabled state, and closes/records only after valid activation.
  CHECK: cargo test -p floe-app phase_11b_palette_activation -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Passed disabled no-op/no-recent behavior, enabled live SimpleAction activation, one signal delivery, and successful recent recording.

- [x] G5: Native UI exposes Ctrl+Shift+P, search/status/results, keyboard navigation, focus, disabled explanations, and accessible labels/descriptions.
  CHECK: cargo test -p floe-app phase_11b_palette_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Passed registry shortcut, non-self-search policy, and explicit query/result/recent bounds; native dialog code labels search, results, status, rows, and confirmation risk.

- [x] G6: Native Wayland smoke verifies open/search/activate/disabled behavior, focus/accessibility, D-Bus health, clean quit, and name release.
  EVIDENCE: Niri/Wayland exported enabled `command-palette`, D-Bus activation opened it while Ping remained healthy, AT-SPI exposed `Command Palette` with the memory-only-recents description, startup registry parity remained clean, Quit exited, and the bus name was released.

- [x] G7: Formatting, workspace check, strict Clippy, full tests, native build, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check
  EXPECT: all commands exit 0
  EVIDENCE: Passed; 407 tests total (313 app, 94 core), zero failures, native app build succeeded, and diff hygiene is clean.

- [x] G8: Documentation marks 11B complete, sets exactly 11C next, and keeps recents memory-only.
  CHECK: test "$(rg -o '\| NEXT \|' docs/ROADMAP.md | wc -l)" -eq 1 && rg -n "11B.*COMPLETE|11C.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 11C is the sole next phase.
  EVIDENCE: Roadmap has exactly one NEXT row at 11C; AGENTS, matrix, privacy/security, plan, and gates document bounded metadata-only search and memory-only recents.
