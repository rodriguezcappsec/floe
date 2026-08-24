# Gates: Floe Phase 6A list-view polish

Scope: Improve the existing virtualized list with a compact, desktop-native metadata hierarchy using metadata already loaded by directory enumeration. Preserve keyboard interaction, large-directory batching, and original path ownership. Thumbnails and a separate grid remain deferred.

- [x] G1: Work is isolated on `phase-6a-list-view-polish`, with `main` still at Phase 5F.
  CHECK: git branch --show-current && git rev-parse main
  EXPECT: /phase-6a-list-view-polish[\s\S]*1290d5d/
  EVIDENCE: phase-6a-list-view-polish | 1290d5ddfd280dafe455bbab6c13192aa447caf7

- [x] G2: The list exposes clear Name, Type, Size, and Modified hierarchy and formats available size/modified metadata compactly.
  CHECK: rg -n "Name|Type|Size|Modified|format_size|format_modified" crates/app/src/ui.rs
  EXPECT: /format_modified/
  EVIDENCE: 1045:        let rendered = format_modified(pre_epoch).expect("pre-epoch times should be representable"); | 1075:            ["Keep Existing", "Retry with New Name"]

- [x] G3: Metadata presentation remains inside `GtkSignalListItemFactory` bind-time virtualization, while bounded model insertion remains unchanged.
  CHECK: rg -n "SignalListItemFactory|connect_bind|BATCH_SIZE: usize = 256|pop_front" crates/app/src/ui.rs crates/app/src/browser.rs
  EXPECT: /BATCH_SIZE: usize = 256/
  EVIDENCE: crates/app/src/ui.rs:656:    let factory = gtk::SignalListItemFactory::new(); | crates/app/src/ui.rs:742:    factory.connect_bind(|_, object| {

- [x] G4: Row/header alignment and density use centralized appearance CSS, with visible hover, selection, and keyboard focus states in all presets.
  CHECK: rg -n "floe-list-header|floe-entry-type|floe-entry-size|floe-entry-modified|focus-visible|row_padding" crates/app/src/appearance.rs crates/app/src/ui.rs
  EXPECT: /floe-entry-modified/
  EVIDENCE: crates/app/src/appearance.rs:194:            .floe-entry-size, .floe-entry-modified {{ | crates/app/src/appearance.rs:234:            row_padding = self.row_padding,

- [x] G5: Presentation code consumes `DirectoryEntry` metadata without reconstructing filesystem paths from display strings or adding thumbnail/grid work.
  CHECK: rg -n "DirectoryEntry|display_name_lossy|entry\.path\(|thumbnail|GridView" crates/app/src/ui.rs crates/app/src/browser.rs
  EXPECT: /entry\.path\(\)/
  EVIDENCE: crates/app/src/ui.rs:767:        let entry = object.borrow::<DirectoryEntry>(); | crates/app/src/ui.rs:768:        let display_name = entry.display_name_lossy();

- [x] G6: Focused Phase 6A tests cover column semantics, kind labels, size boundaries, and safe modified-time formatting.
  CHECK: cargo test -p floe-app phase_6a -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.02s | Running unittests src/main.rs (target/debug/deps/floe_app-2fd028b442df4964)

- [x] G7: Persistent documentation and `AGENTS.md` status describe the implemented Phase 6A slice, its limits, verification, and next branch.
  CHECK: rg -n "Phase 6A|list-view polish|phase-6b" README.md DESIGN.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /phase-6b/
  EVIDENCE: docs/ARCHITECTURE.md:161:rows. Phase 6A keeps presentation inside that bind boundary and exposes aligned | docs/ARCHITECTURE.md:289:semantic colors. Phase 6A adds shared list-heading, secondary metada

- [x] G8: Formatting, workspace compilation, strict Clippy, all tests, diff hygiene, gate check, and native Wayland smoke pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: fmt, workspace check, strict Clippy, all 89 tests (28 core, 61 app), and diff hygiene pass. Native Wayland build registered the expected D-Bus owner and stayed healthy until stopped; only the known host Vulkan warning appeared. Gate checker reports 8/8 met.
