# Gates: Floe Phase 6C thumbnail foundation

Scope: Add bounded, lazy PNG/JPEG list thumbnails with exact-path cache identity, off-main-thread decoding, stable generic fallbacks, and no grid view.

- [x] G1: Work is isolated on `phase-6c-thumbnail-foundation` and `main` remains at the Phase 6B commit before publication.
  CHECK: git branch --show-current && git rev-parse --short main
  EXPECT: /phase-6c-thumbnail-foundation[\s\S]*dc6749a/
  EVIDENCE: phase-6c-thumbnail-foundation | dc6749a

- [x] G2: Thumbnail requests preserve exact paths, whitelist PNG/JPEG, reject symlinks and oversized sources, and use no-follow file opening.
  CHECK: rg -n 'ThumbnailKey|Png|Jpeg|RegularFile|MAX_SOURCE_BYTES|NOFOLLOW|PathBuf' crates/app/src/thumbnail.rs
  EXPECT: /NOFOLLOW/
  EVIDENCE: 479:        let key = ThumbnailKey::from_entry(entry_for(listing.entries(), &path)) | 503:        let key = ThumbnailKey::from_entry(entry_for(listing.entries(), &path))

- [x] G3: A fixed-capacity single worker decodes and scales images off GTK, skips stale generations, and shuts down cleanly.
  CHECK: rg -n 'sync_channel|WORK_QUEUE_CAPACITY|floe-thumbnail-worker|generation|shutdown|join' crates/app/src/thumbnail.rs
  EXPECT: /floe-thumbnail-worker/
  EVIDENCE: 510:            .try_request(generation, key.clone()) | 513:            worker.try_request(generation, key),

- [x] G4: Only virtualized bound rows request thumbnails; unbound, pending, failed, unsupported, and disabled cases retain stable generic icons.
  CHECK: rg -n 'connect_bind|connect_unbind|request_thumbnail|set_icon_name|ThumbnailPresentation|disable' crates/app/src/ui.rs crates/app/src/browser.rs
  EXPECT: /request_thumbnail/
  EVIDENCE: crates/app/src/ui.rs:976:    factory.connect_unbind(move |_, object| { | crates/app/src/ui.rs:1303:        let presentation = ThumbnailPresentation::new();

- [x] G5: GTK receives already-decoded pixel buffers and creates `MemoryTexture` objects only on the main thread.
  CHECK: rg -n 'ImageReader|into_rgba8|into_raw|ThumbnailPixels|MemoryTexture|MemoryFormat|from_owned' crates/app/src/thumbnail.rs crates/app/src/ui.rs
  EXPECT: /MemoryTexture/
  EVIDENCE: crates/app/src/thumbnail.rs:304:    Ok(ThumbnailPixels { | crates/app/src/thumbnail.rs:309:        pixels: thumbnail.into_raw(),

- [x] G6: The in-memory presentation cache is bounded, deduplicates pending keys, and keys exact paths plus size and modification metadata.
  CHECK: rg -n 'CACHE_CAPACITY|HashMap|HashSet|VecDeque|size|modified|completed|pending' crates/app/src/ui.rs crates/app/src/thumbnail.rs
  EXPECT: /CACHE_CAPACITY/
  EVIDENCE: crates/app/src/ui.rs:1360:        let rendered = format_modified(UNIX_EPOCH).expect("Unix epoch should be representable"); | crates/app/src/ui.rs:1366:        let rendered = format_modified(pre_epoch)

- [x] G7: Focused Phase 6C tests cover format eligibility, non-UTF-8 identity, metadata invalidation, no-follow replacement safety, source limits, decoding, stale generations, and queue behavior.
  CHECK: cargo test -p floe-app phase_6c -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.02s | Running unittests src/main.rs (target/debug/deps/floe_app-d2026dabb3767b08)

- [x] G8: Persistent documentation and `AGENTS.md` describe Phase 6C behavior, limitations, verification, and the next branch.
  CHECK: rg -n 'Phase 6C|phase-6d' README.md DESIGN.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /phase-6d/
  EVIDENCE: DESIGN.md:69:Phase 6C adds a 32-pixel thumbnail slot without changing row identity or | DESIGN.md:265:- **Grid view:** visual browsing that reuses the Phase 6C bounded, path-safe

- [x] G9: Formatting, workspace compilation, strict Clippy, all tests, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: With image 0.25.9 resolved, formatting, workspace check, strict Clippy, 72 application tests, 33 core tests, doc tests, and diff hygiene all passed.

- [x] G10: Native Wayland launch owns the expected D-Bus name and remains healthy until intentionally stopped.
  EVIDENCE: PID 43477 owned io.github.floe.FileManager, remained S<sl after 7 seconds, and released the name after intentional Ctrl+C; only documented host libadwaita, RADV, and Vulkan suboptimal-swapchain warnings appeared.

- [x] G11: The pure-Rust decoder remains compatible with Floe's declared Rust 1.85 minimum and enables only PNG/JPEG formats.
  CHECK: rg -n 'image = \{ version = "=0\.25\.9".*default-features = false.*"jpeg", "png"' Cargo.toml && cargo tree -p floe-app -e features | rg 'image v0\.25\.9'
  EXPECT: /image v0\.25\.9/
  EVIDENCE: │   └── image v0.25.9 | │   └── image v0.25.9 (*)
