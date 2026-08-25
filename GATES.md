# Gates: Floe Phase 9E — Font specimens and archive listings

- [x] G1: Correct phase branch.
  CHECK: git branch --show-current
  EXPECT: phase-9e-preview-fonts-archives
  EVIDENCE: phase-9e-preview-fonts-archives.
- [x] G2: Font provider returns bounded passive PNG/RGBA and never installs or executes font content in-process.
  CHECK: cargo test -p floe-app phase_9e_font_provider -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; reviewed provider, raw name, bounded specimen, no-follow symlink rejection, and excluded installer verified.
- [x] G3: ZIP/TAR parsers enforce identity, entry/name/byte caps and preserve raw hostile member names without extraction.
  CHECK: cargo test -p floe-app phase_9e_archive_provider -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; raw ZIP names, unsafe-path flag, TAR checksum/type/size, entry truncation, malformed input, compressed-format rejection, and zero extraction verified.
- [x] G4: Main-thread presentation is passive/selectable and stale payloads retire safely.
  CHECK: cargo test -p floe-app phase_9e_presentation -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; read-only archive label, no-execution accessibility, and stale depth retirement verified.
- [x] G5: Native Wayland font/archive Preview action/health/lifecycle smoke passes.
  EVIDENCE: Wayland app accepted Miller/Preview actions, answered Peer.Ping, and exited 0 via app quit; known non-fatal VK_SUBOPTIMAL_KHR warning only.
- [x] G6: Formatting, check, strict Clippy, and all workspace tests pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace
  EXPECT: test result: ok
  EVIDENCE: fmt/check/strict Clippy passed; 368 tests passed (277 app, 91 core), no failures.
- [x] G7: Patch hygiene is clean.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: git diff --check exited 0 with no output.
- [x] G8: Docs mark 9E complete and exactly 9F next with no-install/no-extract boundaries.
  CHECK: rg -n "9E.*COMPLETE|9F.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 9F
  EVIDENCE: Roadmap, matrix, privacy/security, and AGENTS record 9E COMPLETE, sole 9F NEXT, no install, and no extraction/command execution.
