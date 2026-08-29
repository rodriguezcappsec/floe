# Gates: Floe Phase 20B2A — Window Size Persistence

- [x] W1: Version-17 preferences migrate and validate one atomic window size.
  CHECK: cargo test -p floe-app phase_20b2a_window_size_preferences -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: One focused migration/corruption/clamping/round-trip test passes.
- [x] W2: Startup restores saved size and retains the old default when absent.
  CHECK: cargo test -p floe-app phase_20b2a_window_size_policy -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: One focused 1060x720 fallback and 1600x960 restore test passes.
- [x] W3: Tracking records only changed normal geometry.
  CHECK: cargo test -p floe-app phase_20b2a_window_size_tracking -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: One focused changed/unchanged/maximized/fullscreen policy test passes.
- [x] W4: Real GTK and isolated native lifecycle prove the user-visible path.
  EVIDENCE: Focused real-GTK 1460x880 restoration passes; isolated Wayland
  launch/Ping/Quit exits 0 and writes version 17 plus a bounded size tuple.
- [x] W5: Full deterministic quality and documentation gates pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check
  EXPECT: test result: ok
  EVIDENCE: Exit 0; 535 app, 21 app integration, 158 core, and six duplicate
  workflow tests pass; ten graphical tests are intentionally ignored.
