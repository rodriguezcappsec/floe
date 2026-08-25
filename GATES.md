# Gates: Floe Phase 8C — Miller keyboard and trackpad navigation

Scope: deliver exact focus-visible Miller keyboard/trackpad navigation without Phase 8D actions.

- [x] G1: Work is isolated on the prescribed phase branch.
  CHECK: git branch --show-current
  EXPECT: phase-8c-miller-navigation
  EVIDENCE: phase-8c-miller-navigation

- [x] G2: Direction policy covers bounded Up/Down, logical Left/Right, RTL, Home/End, and reduced motion.
  CHECK: cargo test -p floe-app phase_8c_policy -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.02s | Running unittests src/main.rs (target/debug/deps/floe_app-c34335fcae0cf6e5)

- [x] G3: Recycled columns retain exact active depth/selection and expose focus state without relying on color.
  CHECK: cargo test -p floe-app phase_8c_focus -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.02s | Running unittests src/main.rs (target/debug/deps/floe_app-c34335fcae0cf6e5)

- [x] G4: Horizontal trackpad/wheel handling is bounded and does not consume ordinary vertical column scrolling.
  CHECK: cargo test -p floe-app phase_8c_trackpad -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.02s | Running unittests src/main.rs (target/debug/deps/floe_app-c34335fcae0cf6e5)

- [x] G5: List/grid/text-entry shortcuts and single browser pipeline remain intact.
  CHECK: cargo test -p floe-app phase_8c_integration -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.02s | Running unittests src/main.rs (target/debug/deps/floe_app-c34335fcae0cf6e5)

- [x] G6: Native Wayland smoke verifies Miller keyboard action state, horizontal navigation health, and clean shutdown.
  EVIDENCE: Isolated native Wayland launch activated Miller, described both logical navigation actions, invoked safe root-parent navigation, answered D-Bus Ping, quit status 0, and released the application name.

- [x] G7: Rust formatting is clean.
  CHECK: cargo fmt --all -- --check
  EXPECT: /^$/
  EVIDENCE: `cargo fmt --all -- --check` exited 0 with no output after final edits.

- [x] G8: The full workspace type-checks.
  CHECK: cargo check --workspace
  EXPECT: Finished `dev` profile
  EVIDENCE: Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.02s

- [x] G9: Strict all-target/all-feature Clippy is warning-free.
  CHECK: cargo clippy --workspace --all-targets --all-features -- -D warnings
  EXPECT: Finished `dev` profile
  EVIDENCE: Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.04s

- [x] G10: All workspace tests pass.
  CHECK: cargo test --workspace
  EXPECT: test result: ok
  EVIDENCE: Running unittests src/lib.rs (target/debug/deps/floe_core-9689122403c8558b) | Doc-tests floe_core

- [x] G11: Patch hygiene is clean.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: `git diff --check` exited 0 with no output after final edits.

- [x] G12: Persistent documentation marks 8C complete, exactly 8D next, and records verified focus/RTL/motion boundaries.
  CHECK: rg -n "8C.*COMPLETE|8D.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 8D
  EVIDENCE: docs/ROADMAP.md:158:| 8C — Keyboard/trackpad | COMPLETE | `phase-8c-miller-navigation` | Left/right, up/down and smooth horizontal trackpad interaction. | Verified bounded Up/Down/Home/End, logical LT
