# Gates: Floe Phase 6B list sorting

Scope: Add accessible four-column list sorting off the GTK thread while preserving path identity, directory grouping, and virtualization.

- [x] G1: Work is isolated on `phase-6b-list-sorting` and `main` remains at the Phase 6A commit before publication.
  CHECK: git branch --show-current && git rev-parse --short main
  EXPECT: /phase-6b-list-sorting[\s\S]*efe9e66/
  EVIDENCE: phase-6b-list-sorting | efe9e66

- [x] G2: The GTK-independent core policy supports all four columns, both directions, directories first, unknown metadata last, and raw path tie-breaking.
  CHECK: cargo test -p floe-core phase_6b -- --nocapture
  EXPECT: /5 passed; 0 failed/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.01s | Running unittests src/lib.rs (target/debug/deps/floe_core-454dc5653b111925)

- [x] G3: Every visible column heading is a native action-backed control with visible direction, tooltip, accessible label, and active pressed state.
  CHECK: rg -n 'win\.sort-|Sort .*ascending|Sort .*descending|Pressed|active-sort|floe-sort-heading' crates/app/src/ui.rs crates/app/src/browser.rs crates/app/src/appearance.rs
  EXPECT: /Pressed/
  EVIDENCE: crates/app/src/ui.rs:1036:        header.button.add_css_class("active-sort"); | crates/app/src/ui.rs:1038:        header.button.remove_css_class("active-sort");

- [x] G4: Browser actions submit sorting to the bounded worker and do not perform entry comparisons in GTK callbacks.
  CHECK: rg -n 'request_sort|ResponseKind::Sorted|compare_entries|sort_by' crates/app/src/browser.rs crates/app/src/worker.rs crates/core/src/sorting.rs
  EXPECT: /crates\/app\/src\/worker\.rs.*sort_by/
  EVIDENCE: crates/app/src/worker.rs:85: entries.sort_by using DirectorySort::compare_entries; browser submits request_sort and handles ResponseKind::Sorted.

- [x] G5: Reordering restores selection by exact original path, including colliding lossy non-UTF-8 names.
  CHECK: cargo test -p floe-app phase_6b_selection_restoration -- --nocapture
  EXPECT: /1 passed; 0 failed/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.02s | Running unittests src/main.rs (target/debug/deps/floe_app-2fd028b442df4964)

- [x] G6: Sorting retains `GtkListView` virtualization and bounded 256-entry main-loop model insertion.
  CHECK: rg -n 'SignalListItemFactory|BATCH_SIZE: usize = 256|pop_front|ListStore' crates/app/src/ui.rs crates/app/src/browser.rs
  EXPECT: /BATCH_SIZE: usize = 256/
  EVIDENCE: crates/app/src/ui.rs:672:    let store = gio::ListStore::new::<glib::BoxedAnyObject>(); | crates/app/src/ui.rs:680:    let factory = gtk::SignalListItemFactory::new();

- [x] G7: Focused application tests cover worker dispatch, visible direction text, and exact-path selection restoration.
  CHECK: cargo test -p floe-app phase_6b -- --nocapture
  EXPECT: /3 passed; 0 failed/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.02s | Running unittests src/main.rs (target/debug/deps/floe_app-2fd028b442df4964)

- [x] G8: Persistent documentation and project status describe Phase 6B behavior, limits, verification, and the Phase 6C branch.
  CHECK: rg -n 'Phase 6B|phase-6c-thumbnail-foundation' README.md DESIGN.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /phase-6c-thumbnail-foundation/
  EVIDENCE: DESIGN.md:56:Phase 6B turns those headings into native flat buttons with visible arrows, | README.md:86:Phase 6B makes all four headings native keyboard/pointer controls. Activating

- [x] G9: Formatting, workspace compilation, strict Clippy, all tests, diff hygiene, gate audit, and native Wayland smoke verification pass.
  EVIDENCE: fmt/check/strict Clippy/diff hygiene passed; 64 app + 33 core tests passed; native owner :1.616 and healthy S<sl+ process confirmed; only known host warnings appeared.
