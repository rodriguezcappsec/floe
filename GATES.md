# Gates: Floe Phase 7C Closed tabs and session restore

Status: COMPLETE

- [x] G1: Work is isolated on `phase-7c-tab-session-restore`; no split view,
  detached tabs/windows, optional names/pins, or later phase code exists.
  CHECK: git branch --show-current
  EXPECT: phase-7c-tab-session-restore
  EVIDENCE: Branch command returned `phase-7c-tab-session-restore`.

- [x] G2: Recently closed state is bounded LIFO; reopen, close-left/right/others,
  active ownership, fresh IDs, and last-tab invariants are deterministic.
  CHECK: cargo test -p floe-core phase_7c_closed_tabs -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Two focused closed-tab tests passed with a 32-entry cap and fresh IDs.

- [x] G3: A versioned bounded workspace codec preserves raw non-UTF-8 paths and
  rejects malformed, oversized, empty, duplicate-ID, relative, unsupported, and
  trailing input without panic or I/O.
  CHECK: cargo test -p floe-core phase_7c_workspace_codec -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Two codec tests passed raw round-trip and hostile envelope cases;
  nested session validation retains Phase 7A relative/path/history limits.

- [x] G4: Session storage uses no-follow bounded reads, 0700/0600 ownership,
  same-directory atomic replacement, capacity-one shutdown, shutdown flush,
  corruption fallback, and no GTK-thread filesystem work.
  CHECK: cargo test -p floe-app phase_7c_session_store -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Store tests passed atomic round-trip, modes, corruption and symlink
  fallback; code review confirmed the named worker owns startup read/codec and
  suppression cleanup; native first launch wrote one 824-byte private session file.

- [x] G5: Explicit Private/Sensitive policy suppresses persistence and removes
  Floe's session file; no cryptographic or same-user-process privacy claim is made.
  CHECK: cargo test -p floe-app phase_7c_session_privacy -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Unit tests covered both policies; native private-policy launch removed
  the owned file and clean shutdown did not recreate it.

- [x] G6: Ctrl+Shift+T and tab context close variants operate accessibly; startup
  restore preserves active tab/order/state while invalid data falls back safely.
  CHECK: cargo test -p floe-app phase_7c_tab_actions -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Action contract passed. Second native launch restored multiple tabs,
  exposed enabled reopen, reopened, ran Close Others, and remained healthy.

- [x] G7: Formatting, workspace check, strict all-target/all-feature Clippy,
  workspace tests, diff hygiene, and two-launch native Wayland restore/action/
  D-Bus lifecycle smoke pass.
  CHECK: cargo fmt --all -- --check
  EXPECT: /^$/
  EVIDENCE: Formatting, check, strict Clippy, 312 tests, and diff hygiene passed
  again after the final worker-ownership correction. Three isolated launches
  quit cleanly; only documented RADV warnings appeared.

- [x] G8: Persistent docs mark verified Phase 7C complete and exactly Phase 7D
  as `NEXT`, with truthful privacy and persistence limits.
  CHECK: rg -n '7C — Closed tabs/restore.*COMPLETE|7D — Split state.*NEXT' docs/ROADMAP.md
  EXPECT: 7D — Split state
  EVIDENCE: Roadmap, matrix, design, architecture, development, privacy/security,
  plan, gates, and AGENTS mark 7C complete and exactly 7D next.

Recommended next phase: `phase-7d-split-state`.
