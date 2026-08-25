# Gates: Floe Phase 7E — Split interaction

Scope: Native two-pane interaction over one shared bounded browser pipeline.

- [x] G1: Work is isolated on the Phase 7E branch; no 7F or Miller scope landed.
  CHECK: git branch --show-current
  EXPECT: phase-7e-split-interaction
  EVIDENCE: `phase-7e-split-interaction`; diff contains no Miller model or inter-pane drop target.

- [x] G2: The two-pane presentation reports bounded snapshot and ratio state truthfully.
  CHECK: cargo test -p floe-app phase_7e_split_presentation -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1 focused presentation test passed.

- [x] G3: Toggle/switch/close/swap action policy is deterministic.
  CHECK: cargo test -p floe-app phase_7e_split_actions -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1 focused action-policy test passed.

- [x] G4: Opposite-pane open/copy/move resolves exact authoritative destinations and uses existing no-overwrite jobs.
  CHECK: cargo test -p floe-app phase_7e_opposite_pane -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 2 focused tests passed, including real direct copy and move completion without changing staged transfer state.

- [x] G5: Split controls have explicit keyboard alternatives and stable action names.
  CHECK: cargo test -p floe-app phase_7e_split_accessibility -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: F3, F6, Ctrl+Alt+Left/Right, open/copy/move action contracts passed.

- [x] G6: GTK callbacks submit state or job commands; only the active pane owns the browser/watcher pipeline.
  EVIDENCE: `BrowserController` retains one model/worker set; inactive snapshots are capped at 512 names and activating a side calls the existing generation-safe restore path.

- [x] G7: Rust formatting is clean.
  CHECK: cargo fmt --all -- --check
  EXPECT: /^$/
  EVIDENCE: command exited 0 with no output.

- [x] G8: The full workspace type-checks.
  CHECK: cargo check --workspace
  EXPECT: Finished `dev` profile
  EVIDENCE: workspace check completed successfully.

- [x] G9: Strict all-target/all-feature Clippy is warning-free.
  CHECK: cargo clippy --workspace --all-targets --all-features -- -D warnings
  EXPECT: Finished `dev` profile
  EVIDENCE: strict Clippy completed successfully after removing one useless conversion.

- [x] G10: The full workspace test suite passes.
  CHECK: cargo test --workspace
  EXPECT: test result: ok
  EVIDENCE: 322 tests passed: 235 application and 87 core; doc tests also passed.

- [x] G11: Patch whitespace hygiene is clean.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: command exited 0 with no output.

- [x] G12: Native Wayland split lifecycle and persistence are healthy.
  EVIDENCE: Isolated two-launch smoke activated toggle, narrow, widen, switch, swap, close; verified close-disabled and restored-enabled action states, D-Bus Peer.Ping, clean quits, and split restore. Only documented host RADV/Vulkan and transient one-pixel GtkPaned warnings appeared.

- [x] G13: Persistent roadmap marks exactly Phase 7E complete and Phase 7F next.
  CHECK: rg -n '7E — Split interaction.*COMPLETE|7F — Tab/split drag.*NEXT' docs/ROADMAP.md
  EXPECT: 7F — Tab/split drag
  EVIDENCE: roadmap contains both required status rows and no other NEXT phase.

Recommended next phase: `phase-7f-tab-split-drag`.
