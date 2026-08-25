# Gates: Floe Phase 7F — Tab/split drag

Scope: Exact-path file-operation drops between split contexts.

- [x] G1: Work is isolated on `phase-7f-tab-split-drag` with no detachment or Miller drag.
  CHECK: git branch --show-current
  EXPECT: phase-7f-tab-split-drag
  EVIDENCE: branch matches; diff adds only inactive-pane drop and related alternatives/docs.

- [x] G2: Inactive-pane destination resolution is exact, live, split-only, trash-safe, and non-UTF-8 preserving.
  CHECK: cargo test -p floe-app phase_7f_split_drop_destination -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: focused test passed for unsplit, raw secondary, Trash rejection, and switched-side primary destination.

- [x] G3: Inactive pane reuses Phase 6R copy/move/link requests and no-overwrite FIFO job routing.
  CHECK: cargo test -p floe-app phase_7f_split_drop_jobs -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: focused test completed real copy, move, and symbolic-link jobs through `ApplicationState::submit_drop`.

- [x] G4: Drop feedback is non-color-only and every drag operation has a menu/keyboard alternative.
  CHECK: cargo test -p floe-app phase_7f_split_drag_accessibility -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 2 focused tests verify action/path/release wording and distinct Copy/Move/Link actions/accelerators.

- [x] G5: GTK callbacks only resolve state and submit typed requests through the shared dispatcher; no second pipeline or hover-open is added.
  EVIDENCE: one `install_drop_target` uses the existing dispatcher with both hover-open and autoscroll false; resolver reads `BrowserTabs`, and no worker/model/watcher field was added.

- [x] G6: Rust formatting is clean.
  CHECK: cargo fmt --all -- --check
  EXPECT: /^$/
  EVIDENCE: command exited 0 with no output.

- [x] G7: The full workspace type-checks.
  CHECK: cargo check --workspace
  EXPECT: Finished `dev` profile
  EVIDENCE: workspace check completed successfully.

- [x] G8: Strict all-target/all-feature Clippy is warning-free.
  CHECK: cargo clippy --workspace --all-targets --all-features -- -D warnings
  EXPECT: Finished `dev` profile
  EVIDENCE: strict Clippy completed successfully.

- [x] G9: The full workspace test suite passes.
  CHECK: cargo test --workspace
  EXPECT: test result: ok
  EVIDENCE: 326 tests passed: 239 application and 87 core; doc tests also passed.

- [x] G10: Patch whitespace hygiene is clean.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: command exited 0 with no output.

- [x] G11: Native Wayland action/drop-target lifecycle remains healthy.
  EVIDENCE: Isolated native launch exported selection-gated Link to Other Pane, toggled split to construct the inactive target, answered D-Bus Peer.Ping, and quit cleanly. Only documented RADV/Vulkan and transient one-pixel GtkPaned warnings appeared; synthetic pointer drag was covered by focused real-job tests rather than claimed natively.

- [x] G12: Persistent docs mark Phase 7F complete and exactly Phase 8A next.
  CHECK: rg -n '7F — Tab/split drag.*COMPLETE|8A — Column model.*NEXT' docs/ROADMAP.md
  EXPECT: 8A — Column model
  EVIDENCE: roadmap contains both required status rows and no other NEXT phase.

Recommended next phase: `phase-8a-miller-model`.
