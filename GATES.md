# Gates: Floe Phase 8E — Miller cross-column drag/drop

Scope: deliver exact cross-column and cross-surface file-list drag/drop without Phase 8F detail hooks.

- [x] G1: Work is isolated on the prescribed phase branch.
  CHECK: git branch --show-current
  EXPECT: phase-8e-miller-drag-drop
  EVIDENCE: phase-8e-miller-drag-drop

- [x] G2: Typed hover targets and exact destinations reject stale, non-local, same-destination, and self-nesting input.
  CHECK: cargo test -p floe-app phase_8e_policy -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_8e_policy passed 1/1 with raw typed targets and non-local/same/self-nesting rejection.

- [x] G3: Active and retained Miller columns drag exact selected raw paths and accept folder/background drops through existing requests.
  CHECK: cargo test -p floe-app phase_8e_miller -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_8e_miller passed 1/1; raw selections and exact depth/path child targets verified, overflow emits no payload.

- [x] G4: Tabs, split panes, sidebar, bookmarks, and mounted devices resolve authoritative destinations with bounded hover ownership.
  CHECK: cargo test -p floe-app phase_8e_surfaces -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_8e_surfaces passed 1/1; live tab/split raw destinations and typed tab/pane ownership verified; UI binds existing sidebar/device paths.

- [x] G5: Vertical and horizontal edge autoscroll are clamped and feedback remains non-color-only.
  CHECK: cargo test -p floe-app phase_8e_autoscroll -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_8e_autoscroll passed 1/1; both axes share bounded deltas and feedback names action/path/release.

- [x] G6: Native Wayland smoke verifies Miller/tab drag action alternatives, application health, and clean shutdown.
  EVIDENCE: Isolated Wayland launch activated Miller, described copy/move/link opposite-pane alternatives and Miller action, answered D-Bus Ping, quit status 0.

- [x] G7: Rust formatting is clean.
  CHECK: cargo fmt --all -- --check
  EXPECT: /^$/
  EVIDENCE: cargo fmt --all -- --check exited 0 with no output.

- [x] G8: The full workspace type-checks.
  CHECK: cargo check --workspace
  EXPECT: Finished `dev` profile
  EVIDENCE: Finished dev profile successfully for the workspace.

- [x] G9: Strict all-target/all-feature Clippy is warning-free.
  CHECK: cargo clippy --workspace --all-targets --all-features -- -D warnings
  EXPECT: Finished `dev` profile
  EVIDENCE: Strict Clippy completed successfully with -D warnings.

- [x] G10: All workspace tests pass.
  CHECK: cargo test --workspace
  EXPECT: test result: ok
  EVIDENCE: 348 tests passed (257 application, 91 core); no failures.

- [x] G11: Patch hygiene is clean.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: git diff --check exited 0 with no output.

- [x] G12: Persistent documentation marks 8E complete, exactly 8F next, and records exact drag/hover ownership boundaries.
  CHECK: rg -n "8E.*COMPLETE|8F.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 8F
  EVIDENCE: Roadmap, matrix, AGENTS, architecture, design, development, and privacy docs record Phase 8E and sole Phase 8F NEXT.
