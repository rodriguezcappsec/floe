# Gates: Floe Phase 10E — Checksums

- [x] G1: Correct phase branch.
  CHECK: git branch --show-current
  EXPECT: phase-10e-checksums
  EVIDENCE: `phase-10e-checksums` confirmed before implementation and final verification.

- [x] G2: Typed checksum requests preserve exact paths and strictly validate algorithm-specific expected digests and selection bounds.
  CHECK: cargo test -p floe-core phase_10e_checksum_request -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused core test passed for raw non-UTF-8 identity, strict SHA-256/SHA-512/MD5 lengths, uppercase normalization, duplicate/relative/root/unnormalized rejection, and one-target expected verification.

- [x] G3: The fixed-capacity executor produces standard SHA-256, SHA-512, and legacy MD5 vectors with byte progress and explicit expected match or mismatch.
  CHECK: cargo test -p floe-app phase_10e_checksum_vectors -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused executor test passed standard `abc` vectors for all three algorithms, Match/Mismatch outcomes, worker completion, byte progress, and result retrieval.

- [x] G4: Streaming is cancellable and bounded, uses no-follow opens, and rejects non-regular, replaced, or changed inputs.
  CHECK: cargo test -p floe-app phase_10e_checksum_streaming -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused streaming test passed 1 MiB chunk cancellation, symlink no-follow rejection, and deterministic source replacement/change detection; requests cap at 4,096 targets, queue at 4, and retained results at 16.

- [x] G5: Native selection-aware checksum request and result policy validates input, labels MD5 as legacy, exposes copyable digest text, and avoids authenticity claims.
  CHECK: cargo test -p floe-app phase_10e_checksum_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused UI-policy test passed strict expected input, multi-target expected rejection, legacy MD5 result wording, mismatch presentation, digest-only copy text, and explicit no-authenticity notice.

- [x] G6: Native Wayland smoke verifies action/dialog behavior, exact temporary-fixture SHA-256 output, Operations lifecycle wording, accessibility, D-Bus health/focus, clean quit, and name release.
  EVIDENCE: Isolated `/tmp/floe-phase10e-smoke.rBTTVG` smoke activated selection and checksum actions, exposed native request/results through AT-SPI, returned exact `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad` for `abc`, exposed Calculated/not-compared, copy, legacy/no-authenticity text, completed a sparse-file lifecycle while D-Bus Ping remained healthy, quit cleanly, and released `io.github.floe.FileManager`; only documented RADV/Vulkan warnings appeared.

- [x] G7: Formatting, workspace check, strict Clippy, full tests, build, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check
  EXPECT: test result: ok
  EVIDENCE: Full command passed: 392 tests total (299 application, 93 core), strict Clippy, native build, and diff hygiene all clean.

- [x] G8: Documentation marks 10E complete, sets exactly 10F next, and keeps legacy-MD5 and no-authenticity language explicit.
  CHECK: test "$(rg -o '\\| NEXT \\|' docs/ROADMAP.md | wc -l)" -eq 1 && rg -n "10E.*COMPLETE|10F.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 10F is the sole next phase
  EVIDENCE: Roadmap has one `NEXT` row at 10F; roadmap, matrix, AGENTS, and privacy/security documentation describe 10E truthfully and retain legacy/no-authenticity/no-persistence boundaries.
