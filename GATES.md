# Gates: Floe Phase 10F — Advanced Metadata

- [x] G1: Correct phase branch; reviewed parser dependencies remain compatible with Rust 1.85.
  CHECK: git branch --show-current && cargo tree -p floe-app --depth 1
  EXPECT: `phase-10f-advanced-metadata`; `kamadak-exif 0.6.1` and `lofty 0.22.4` are direct dependencies.
  EVIDENCE: Branch confirmed; dependency tree reports exact reviewed pinned versions and dependency MSRVs were reviewed as Rust 1.60 and 1.85 respectively.

- [x] G2: Advanced metadata requests/results preserve exact paths and explicit bounded, unsupported, malformed, and changed states.
  CHECK: cargo test -p floe-app phase_10f_advanced_metadata_contract -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Passed; exact PathBuf keys, 16 MiB reads, 1,024-character strings, ten EXIF fields, and explicit state contract are covered.

- [x] G3: EXIF parsing is bounded, no-follow, source-revalidated, passive, and exposes only reviewed presentation fields.
  CHECK: cargo test -p floe-app phase_10f_exif_metadata -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Passed TIFF fixture, symlink refusal, identity validation, reviewed-field policy, and sparse safety-limit coverage.

- [x] G4: Audio/media metadata parsing handles duration and reviewed tags while bounding oversized input and rejecting malformed, symlink, and changed inputs safely.
  CHECK: cargo test -p floe-app phase_10f_media_metadata -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Passed tagged WAV duration/artist/album/track, malformed MP3, stale size, symlink, and sparse oversized input coverage.

- [x] G5: Optional Dimensions, Duration, Artist, Album, and Track columns are lazy, virtualized, bounded, persisted, migration-safe, and stable during enrichment.
  CHECK: cargo test -p floe-app phase_10f_advanced_columns -- --nocapture && cargo test -p floe-core phase_10f_advanced_columns -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Passed bound-row presentation, truthful states, textual persistence, legacy nine-column migration, default widths, and non-sortable stability coverage.

- [x] G6: Inspector and Properties presentation is accessible and truthful for present, limited, and malformed metadata without privacy, safety, or authenticity verdicts.
  CHECK: cargo test -p floe-app phase_10f_advanced_metadata_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Six Phase 10F app tests pass, including Inspector EXIF text, Properties EXIF/media rows, limited/malformed labels, and non-verdict assertions.

- [x] G7: Native Wayland smoke verifies optional columns, Inspector metadata, asynchronous responsiveness, accessibility, D-Bus health/focus, clean quit, and name release.
  EVIDENCE: Niri/Wayland launch loaded enabled Dimensions and Duration column actions from isolated version-4 preferences; D-Bus Ping returned, AT-SPI exposed accessible ID `io.github.floe.FileManager` and window name `rocappsec — Floe`, standard Quit exited cleanly, and the application bus name was released.

- [x] G8: Formatting, workspace check, strict Clippy, full tests, native build, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check
  EXPECT: all commands exit 0
  EVIDENCE: Passed; 399 tests total (305 app, 94 core), zero failures, native app build succeeded, and diff hygiene is clean.

- [x] G9: Documentation marks 10F complete, sets exactly 11A next, and retains lazy, no-persistence, and no-privacy-finding boundaries.
  CHECK: test "$(rg -o '\| NEXT \|' docs/ROADMAP.md | wc -l)" -eq 1 && rg -n "10F.*COMPLETE|11A.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 11A is the sole next phase.
  EVIDENCE: Roadmap has exactly one NEXT row at 11A; AGENTS, matrix, roadmap, privacy/security, plan, and gates describe the verified 10F boundary.
