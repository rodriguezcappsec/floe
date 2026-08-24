# Gates: Sidebar presentation and browser wiring

Scope: Compact resizable Places, Bookmarks, and Devices UI wired to application-owned services.

- [x] L1: Sidebar section and action-policy tests cover compact sizing, add/remove bookmarks, device activation, and device action sensitivity.
  CHECK: cargo test -p floe-app phase_6k_sidebar -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: 4 focused sidebar tests passed; policy covers the 128 px compact floor, load/save gating, explicit removal, local navigation, mount, remote, busy, and unavailable states.

- [x] L2: Browser wiring navigates exact place/bookmark/device paths and routes mutations through service boundaries.
  CHECK: cargo test -p floe-app phase_6k_ -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: 18 Phase 6K tests passed, including raw non-UTF-8 target identity, bookmark-worker persistence, and device policy/action reservation.

- [x] L3: Floe compiles the integrated sidebar subsystem.
  CHECK: cargo check --workspace
  EXPECT: /Finished/
  EVIDENCE: cargo check --workspace finished successfully; strict workspace Clippy and git diff --check also passed.
