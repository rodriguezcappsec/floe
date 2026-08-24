# Gates: Floe Phase 6D grid-view foundation

Scope: Add a switchable virtualized grid with adjustable persisted sizing while
reusing Floe's exact-path model, selection, actions, and bounded thumbnail
pipeline.

- [x] G1: Work is isolated on `phase-6d-grid-view-foundation`; `main` remains at Phase 6C before publication.
  CHECK: git branch --show-current && git rev-parse --short main
  EXPECT: /phase-6d-grid-view-foundation[\s\S]*9af5068/
  EVIDENCE: Current branch is `phase-6d-grid-view-foundation`; local `main` is `9af5068` before publication.

- [x] G2: GTK-independent policy defines List/Grid modes, bounded discrete sizes, stable zoom steps, and strict persisted values.
  CHECK: rg -n 'ViewMode|GridSize|GRID_SIZES|zoom_in|zoom_out|from_persisted' crates/app/src/view.rs
  EXPECT: /GRID_SIZES/
  EVIDENCE: `view.rs` defines strict List/Grid values and seven 64-192 pixel steps; focused policy tests pass.

- [x] G3: Native list and grid share one selection/model and preserve activation, exact selection, and context actions.
  CHECK: rg -n 'SingleSelection|GridView|ListView|set_view_mode|connect_activate|context_menu' crates/app/src/ui.rs crates/app/src/browser.rs
  EXPECT: /GridView/
  EVIDENCE: Both factories use one `GtkSingleSelection`; live List/Grid switching and selection-preserving grid rebinding were verified on Wayland.

- [x] G4: View and grid-size controls are pointer/keyboard operable, accessibly named, and visibly focused.
  CHECK: rg -n 'view-list-symbolic|view-grid-symbolic|Grid icon size|set_accessible|view-list|view-grid|zoom-in|zoom-out|focus-visible' crates/app/src/ui.rs crates/app/src/browser.rs crates/app/src/appearance.rs
  EXPECT: /Grid icon size/
  EVIDENCE: Native toggles, zoom buttons, scale, Ctrl shortcuts, accessible labels, and focus-visible CSS are present; controls rendered correctly in native smoke.

- [x] G5: Grid thumbnails remain lazy and bounded, include edge size in cache identity, reject invalid sizes, and decode off GTK.
  CHECK: rg -n 'edge|MAX_THUMBNAIL_EDGE|ThumbnailKey|connect_bind|ThumbnailWorker|WORK_QUEUE_CAPACITY' crates/app/src/thumbnail.rs crates/app/src/ui.rs
  EXPECT: /MAX_THUMBNAIL_EDGE/
  EVIDENCE: Bound-cell factory requests edge-keyed work through the existing capacity-64 worker; invalid-edge identity test passes and live thumbnails completed.

- [x] G6: View preferences load at startup and save through bounded application-layer I/O rather than GTK callbacks.
  CHECK: rg -n 'ViewPreferences|PreferenceWorker|sync_channel|try_send|floe-view-preferences' crates/app/src/preferences.rs crates/app/src/application.rs crates/app/src/browser.rs
  EXPECT: /floe-view-preferences/
  EVIDENCE: Capacity-1 worker loads startup state, atomically writes exact values, retries full submissions, and preserves the latest shutdown value; runtime persisted 112/128/160.

- [x] G7: Focused Phase 6D tests cover policy, requested-size identity, invalid bounds, action names, queue capacity, and persistence.
  CHECK: cargo test -p floe-app phase_6d -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Eight focused Phase 6D tests pass with zero failures.

- [x] G8: Persistent documentation describes Phase 6D behavior, limitations, verification, and next branch `phase-6e-thumbnail-cache-polish`.
  CHECK: rg -n 'Phase 6D|phase-6e-thumbnail-cache-polish' README.md DESIGN.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /phase-6e-thumbnail-cache-polish/
  EVIDENCE: README, DESIGN, architecture, development, roadmap, and AGENTS status all describe Phase 6D and the Phase 6E branch.

- [x] G9: Formatting, workspace compilation, strict Clippy, all tests, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: Format, workspace check, strict Clippy, all 113 tests (80 app and 33 core), and `git diff --check` pass.

- [x] G10: Native Wayland launch owns the expected D-Bus name, exposes grid controls, remains healthy, and is intentionally stopped.
  EVIDENCE: Native app owned `io.github.floe.FileManager`, exported all four view actions, rendered list/grid plus live thumbnails, preserved selection across 160-to-128 rebinding, remained healthy, then released D-Bus on intentional stop; only known host warnings appeared.
