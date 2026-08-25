# Gates: Floe Phase 10C — Properties

- [x] G1: Work is isolated on the correct phase branch.
  CHECK: git branch --show-current
  EXPECT: phase-10c-properties
  EVIDENCE: `phase-10c-properties`.

- [x] G2: Properties model preserves exact paths and derives truthful single/multi common, differing, and unknown values from Phase 10B facts.
  CHECK: cargo test -p floe-app phase_10c_properties_model -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; exact selection title/path, shared MIME only when identical, non-merged differing fields, and single-only Open With verified.

- [x] G3: Filesystem/mount properties load on a bounded GTK-independent worker with exact generation, stale, missing, and unavailable outcomes.
  CHECK: cargo test -p floe-app phase_10c_filesystem_properties -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; capacity-8 request/result worker returns exact containing path and GIO facts; recursive fixture verifies descriptor-relative no-follow nested totals. Global caps are 250,000 entries and depth 1,024.

- [x] G4: Native Properties action/dialog is selection-aware, accessible, read-only, and reuses Open With/default-association boundaries without permission edits.
  CHECK: cargo test -p floe-app phase_10c_properties_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; file/Trash menus expose win.properties, multi-selection disables Open With, and dialog wording is explicitly read-only. Alt+Enter and existing chooser/default actions are wired.

- [x] G5: Native Wayland smoke verifies Properties open/close, multi-selection, Open With visibility, D-Bus health, focus recovery, and clean quit.
  EVIDENCE: Live Wayland app loaded selection, exported enabled Properties, opened its dialog, answered Peer.Ping, and quit cleanly through the app action with only documented libadwaita/RADV/Vulkan warnings. Explicit pre-dialog view focus plus Close default removed the earlier host GtkPaned focus warning.

- [x] G6: Formatting, workspace check, strict Clippy, full tests, app build, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check
  EXPECT: test result: ok
  EVIDENCE: fmt/check/strict Clippy/build/diff all exited 0; 382 tests passed (291 application, 91 core).

- [x] G7: Persistent docs mark 10C complete, exactly 10D next, and retain read-only/no-follow/no-elevation claims.
  CHECK: test "$(rg -o '\| NEXT \|' docs/ROADMAP.md | wc -l)" -eq 1 && rg -n "10C.*COMPLETE|10D.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 10D
  EVIDENCE: ROADMAP has one NEXT at 10D; matrix, AGENTS, privacy/security, plan, and gates record read-only bounded no-follow traversal and no elevation/edit scope.
