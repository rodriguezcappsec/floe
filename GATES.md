# Gates: Floe Phase 6F thumbnail format and orientation polish

Scope: Apply embedded raster-image orientation and expand Floe's reviewed safe static thumbnail formats while preserving bounded worker/cache behavior and stable fallbacks.

- [x] G1: Work is isolated on `phase-6f-thumbnail-format-polish` from the completed Phase 6E commit.
  CHECK: git branch --show-current && git merge-base --is-ancestor 25ccce0 phase-6f-thumbnail-format-polish && git rev-parse --short 25ccce0
  EXPECT: /phase-6f-thumbnail-format-polish[\s\S]*25ccce0/
  EVIDENCE: Branch check printed `phase-6f-thumbnail-format-polish`; `25ccce0` is its verified Phase 6E ancestor.

- [x] G2: A GTK-independent format policy explicitly maps reviewed extensions to decoder formats and rejects SVG/active or unreviewed content.
  CHECK: rg -n 'ThumbnailFormat|WebP|Gif|Bmp|Tiff|Ico|Svg|from_path|image_format' crates/app/src/thumbnail.rs
  EXPECT: /WebP/
  EVIDENCE: `thumbnail.rs` maps PNG/JPEG/WebP/GIF/BMP/TIFF/ICO explicitly and returns `None` for SVG and unreviewed extensions.

- [x] G3: Decoder-provided EXIF/TIFF orientation is applied before tier/request scaling and before persistent-cache storage.
  CHECK: rg -n 'orientation|apply_orientation|decode_source|tier_thumbnail|cache.store' crates/app/src/thumbnail.rs
  EXPECT: /apply_orientation/
  EVIDENCE: `decode_source_image` obtains and applies decoder orientation before `tier_thumbnail` construction and `cache.store`.

- [x] G4: New formats retain no-follow source opening, encoded/decoded limits, exact source revalidation, bounded aspect-preserving scaling, and first-frame-only static thumbnails.
  CHECK: rg -n 'NOFOLLOW|MAX_SOURCE_BYTES|max_image_width|max_image_height|max_alloc|SourceChanged|thumbnail' crates/app/src/thumbnail.rs
  EXPECT: /NOFOLLOW/
  EVIDENCE: Source open uses `NOFOLLOW`; 32-MiB encoded, 128-MiB decoded, 65,535-axis, `SourceChanged`, and aspect-preserving `thumbnail` checks remain on the worker.

- [x] G5: Persistent cache identity and normal/large tier behavior remain format-agnostic and valid oriented pixels are reused across worker restarts.
  CHECK: rg -n 'ThumbnailCacheKey|CacheTier|Floe::MTimeNsec|spawn_with_cache|worker_reuses' crates/app/src/thumbnail_cache.rs crates/app/src/thumbnail.rs
  EXPECT: /ThumbnailCacheKey/
  EVIDENCE: Existing `ThumbnailCacheKey` and `CacheTier` remain unchanged; the Phase 6F restart test reuses an added-format cached result after corrupting source bytes at identical metadata.

- [x] G6: Focused Phase 6F tests cover format allow/reject policy, extension case, orientation transforms/pipeline, each new decoder, malformed inputs, aspect ratio, and cache reuse.
  CHECK: cargo test -p floe-app phase_6f -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Focused command passed 5 Phase 6F tests covering every listed behavior.

- [x] G7: README, design, architecture, development, roadmap, and AGENTS status describe actual Phase 6F behavior and identify `phase-6g-iconography-polish` as the next visual-quality branch.
  CHECK: rg -n 'Phase 6F|phase-6g-iconography-polish|iconography' README.md DESIGN.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /phase-6g-iconography-polish/
  EVIDENCE: All six persistent documents contain Phase 6F behavior and the Phase 6G iconography milestone.

- [x] G8: Formatting, workspace compilation, strict Clippy, all tests, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: Formatting, workspace check, strict Clippy, 96 application plus 33 core tests, and diff hygiene all passed.

- [x] G9: Native Wayland smoke with temporary roots renders an added-format thumbnail, owns the expected D-Bus name, remains healthy, and shuts down cleanly without persistent test artifacts.
  EVIDENCE: Isolated native run owned `io.github.floe.FileManager`, cached a real 96x24 WebP as a 983-byte PNG plus ownership marker, exited 0 via Quit, released D-Bus, and its temporary root was removed.

- [ ] G10: The unlazy gate checker passes all Phase 6F gates after publication.
  CHECK: node <unlazy-skill-dir>/scripts/gate-check.mjs GATES.md
  EXPECT: /ALL MET/
  EVIDENCE: Pending.

- [ ] G11: The Phase 6F commit is pushed, fast-forwarded into `main`, and local/remote phase and main refs are identical.
  CHECK: git rev-parse main phase-6f-thumbnail-format-polish origin/main origin/phase-6f-thumbnail-format-polish
  EXPECT: /^([0-9a-f]{40})\n\1\n\1\n\1$/
  EVIDENCE: Pending publication.
