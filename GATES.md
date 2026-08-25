# Gates: Floe Phase 8D — Miller column actions

Scope: deliver exact-owner standard context actions in every Miller column without Phase 8E drag/drop.

- [x] G1: Work is isolated on the prescribed phase branch.
  CHECK: git branch --show-current
  EXPECT: phase-8d-miller-actions
  EVIDENCE: phase-8d-miller-actions

- [x] G2: Action contexts preserve exact depth and raw selected paths and reject stale ownership.
  CHECK: cargo test -p floe-app phase_8d_context -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_8d_context passed 1/1 with raw non-UTF-8, wrong-parent, and overflow rejection.

- [x] G3: Item/background menus expose pointer and keyboard parity with non-color-only owner feedback.
  CHECK: cargo test -p floe-app phase_8d_menu -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_8d_menu passed 1/1; Shift+F10/Menu and textual active-owner contract verified.

- [x] G4: Existing action eligibility and no-overwrite job routing are reused without GTK filesystem work.
  CHECK: cargo test -p floe-app phase_8d_parity -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_8d_parity passed 1/1; bounded context resolves shared DirectoryEntry identities and controller delegates existing actions.

- [x] G5: Retained-column actions synchronize exact ownership before dispatch and leave list/grid behavior intact.
  CHECK: cargo test -p floe-app phase_8d_integration -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_8d_integration passed 1/1; list, grid, and Miller view commands remain exported.

- [x] G6: Native Wayland smoke verifies Miller context action exports, application health, and clean shutdown.
  EVIDENCE: Isolated native Wayland launch activated Miller, listed file/create actions, described Open/Copy/New Folder sensitivity, answered D-Bus Ping, quit status 0.

- [x] G7: Rust formatting is clean.
  CHECK: cargo fmt --all -- --check
  EXPECT: /^$/
  EVIDENCE: cargo fmt --all -- --check exited 0 with no output after final edits.

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
  EVIDENCE: 344 tests passed (253 application, 91 core); no failures.

- [x] G11: Patch hygiene is clean.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: git diff --check exited 0 with no output.

- [x] G12: Persistent documentation marks 8D complete, exactly 8E next, and records exact action ownership boundaries.
  CHECK: rg -n "8D.*COMPLETE|8E.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 8E
  EVIDENCE: Roadmap, matrix, AGENTS, architecture, design, development, and privacy docs record Phase 8D boundaries and Phase 8E NEXT.
