# Gates: Phase 6K2 UI integration

Scope: Persist and customize the sidebar, repair Operations Island layout, and keep mount authentication native, window-parented, and credential-opaque.

- [x] G1: Sidebar width restores from persisted preferences or the active appearance default, clamps to stable bounds, and divider changes debounce before the async worker.
  CHECK: cargo test -p floe-app phase_6k2_sidebar_width -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: One focused test passed; runtime uses connect_position_notify, 320ms timeout replacement, 128/480 clamp, and PreferenceWorker submission outside the GTK callback.

- [x] G2: View mode, grid size, sidebar width, and density always submit the newest complete ViewPreferences, while reset stores None and reapplies the active appearance default.
  CHECK: cargo test -p floe-app phase_6k2_preferences -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Five focused preference/controller tests passed; the merge test retains Comfortable and 312px across a view/grid update.

- [x] G3: The header menu exposes native stateful Compact, Balanced, and Comfortable density choices plus Reset Sidebar Width, with Compact as the persisted default and live application.
  CHECK: cargo test -p floe-app phase_6k2_sidebar -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Two focused tests passed, including exact accessible menu labels/action targets and width policy.

- [x] G4: Sidebar spacing uses 2px related rows, 4/8/12px density section gaps, tuned outer margins, and 36/40/44px icon-control targets.
  CHECK: rg -n "sidebar-(compact|balanced|comfortable)|min-(width|height): (36|40|44)px" crates/app/src/appearance.rs
  EXPECT: /sidebar-compact/
  EVIDENCE: Scoped CSS classes provide Compact/Balanced/Comfortable row sizing; pure density tests passed for 2px and 4/8/12px rhythm.

- [x] G5: Operations Island uses bounded 340px geometry, a title/cancel row, flexible full-width progress, and a separate end-aligned retry/conflict action row.
  CHECK: cargo test -p floe-app operation_island_layout -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Two focused tests passed; fixed 220px progress was removed and title/cancel, progress, and recovery rows are distinct.

- [x] G6: Pure metric and structure tests prove every child minimum fits inside the 316px content width and recovery actions remain reachable.
  CHECK: cargo test -p floe-app operation_island_layout -- --nocapture
  EXPECT: /2 passed/
  EVIDENCE: Two tests passed; 340px outer width minus two 12px insets leaves 316px, exceeding 40px cancel and 72px recovery minimums.

- [x] G7: Mount authentication remains a window-parented GtkMountOperation, is credential-opaque, and gives clear nonmodal pre-auth feedback.
  CHECK: cargo test -p floe-app mount_auth -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: One policy test passed; runtime still calls gtk::MountOperation::new(Some(&window)) and stores or logs no password.

- [x] G8: Formatting, workspace compilation, strict Clippy, application tests, and core tests pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
  EXPECT: /test result: ok/
  EVIDENCE: All commands passed; the final integrated suite has 148 application tests and 33 core tests, with zero failures.

- [x] G9: Four expert passes resolved completeness, architecture, edge-case, and polish issues in the assigned files.
  EVIDENCE: Passes fixed lost full-state preferences, obsolete appearance minimums, saturating position clamp, reset shutdown persistence, menu mapping coverage, icon hit targets, 18px overlay outlier, and Operations Island structure evidence; final pass found no remaining assigned-file issue.

- [x] G10: The leaf ledger reports every gate met in read-only status mode.
  CHECK: node /home/rocappsec/.codex/skills/unlazy/scripts/gate-check.mjs --status gates/leaf-6k2-1.2.1.md
  EXPECT: /ALL MET/
  EVIDENCE: Read-only gate status confirmed all 10 gates met after evidence was recorded.
