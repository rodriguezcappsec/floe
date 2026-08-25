# Gates: Floe Phase 9D — Audio and video Preview

- [x] G1: Correct phase branch.
  CHECK: git branch --show-current
  EXPECT: phase-9d-preview-media
  EVIDENCE: phase-9d-preview-media.
- [x] G2: Media provider validates exact no-follow audio/video identity and optional bounded poster output.
  CHECK: cargo test -p floe-app phase_9d_media_provider -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; raw non-UTF-8 video, extension/MIME identity, bounded poster, source mutation, and symlink rejection verified.
- [x] G3: Main-thread presentation exposes native controls and retired streams are explicitly paused/released.
  CHECK: cargo test -p floe-app phase_9d_media_presentation -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 lifecycle test passed; presentation labels native controls and retiring state drops payload. MillerView pauses and clears prior MediaFile before every rerender; native open/close smoke passed.
- [x] G4: Unsupported codecs/provider failures remain truthful and do not trigger installation or shell execution.
  CHECK: cargo test -p floe-app phase_9d_media_contract -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; reviewed extension/MIME pairing, audio fallback, and excluded playlist/script/HTML/binary types verified. Playback uses GTK MediaFile only.
- [x] G5: Native Wayland media Preview action/health/lifecycle smoke passes.
  EVIDENCE: Wayland app toggled Miller Preview open/closed, answered Peer.Ping, and exited 0 via app quit; known non-fatal VK_SUBOPTIMAL_KHR warning only.
- [x] G6: Formatting, check, strict Clippy, and all workspace tests pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace
  EXPECT: test result: ok
  EVIDENCE: fmt/check/strict Clippy passed; 365 tests passed (274 app, 91 core), no failures.
- [x] G7: Patch hygiene is clean.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: git diff --check exited 0 with no output.
- [x] G8: Docs mark 9D complete and exactly 9E next with no-codec-install/resource-retirement boundaries.
  CHECK: rg -n "9D.*COMPLETE|9E.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 9E
  EVIDENCE: Roadmap, matrix, privacy/security, and AGENTS record 9D COMPLETE, sole 9E NEXT, no codec install/shell path, and stream retirement.
