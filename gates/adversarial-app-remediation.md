# Gates: adversarial application lifecycle remediation

Scope: Fix Quit policy, cross-window preferences/bookmarks, and truthful
background-outcome accessibility without filesystem work in GTK callbacks.

- [x] A1: `app.quit` and Ctrl+Q consult the same application-wide active-job
  policy as window close and cannot silently tear down active executors.
  CHECK: `cargo test -p floe-app adversarial_quit_policy -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS 2026-09-01; one matching test passed, zero failed. The action
checks `application_state.has_active_jobs()` before calling GTK quit.

- [x] A2: Disjoint preference edits from multiple windows merge into one
  authoritative application state and survive persistence without stale full
  snapshots reverting sibling changes.
  CHECK: `cargo test -p floe-app adversarial_shared_preferences -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS 2026-09-01; one matching test passed, zero failed. Two stale
window baselines changed disjoint fields including bounded custom actions and
private folder-view overrides, merged both, and reloaded the exact authoritative
snapshot after real worker persistence. The same regression proves the typed
live-presentation delta covers appearance/color/font/motion, click policy,
icons, sidebar, context/custom-action menus, keybindings, Vim, and view policy.

- [x] A3: Every normal/restored window loads and observes one bounded bookmark
  catalog; mutations in either window appear in the other without duplicate
  persistence workers.
  CHECK: `cargo test -p floe-app adversarial_shared_bookmarks -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS 2026-09-01; one matching test passed, zero failed. Cloned window
handles share one catalog and worker, observe one revision, and reload one
persisted catalog.

- [x] A4: Completed, partial, cancelled, failed, running, and stopping
  background rows expose distinct truthful accessible descriptions.
  CHECK: `cargo test -p floe-app adversarial_background_accessibility -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS 2026-09-01; one matching test passed, zero failed. All six states
have distinct nonempty descriptions and the GTK row consumes that description.

- [x] A5: Existing multi-window, preferences, bookmarks, operations feedback,
  and applicable real-GTK component tests pass.
  CHECK: `cargo test -p floe-app phase_23 -- --nocapture && cargo test -p floe-app background_feedback -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS 2026-09-01; Phase 23 passed 21 with one graphical ignore;
background feedback passed five with one graphical ignore. Full app tests passed
667 unit tests with 20 intentional graphical ignores plus 21 integration tests.
The focused GTK feedback test was attempted and skipped because GTK reported
`Failed to initialize GTK` with no usable display.

- [x] A6: Shared state remains bounded/application-owned, Selection Mode
  processes remain isolated, and GTK callbacks submit/present rather than run
  filesystem work.
  EVIDENCE: `run_normal` owns one shared preference model and one bookmark worker;
normal/restored/new windows receive clones while `run_selection` passes `None`.
Sibling windows consume authoritative revisions and apply presentation changes
to their live widgets; persistence retains existing bounded workers. Format,
app check, strict app all-target Clippy, 667 unit tests, and 21 integration tests
passed.
