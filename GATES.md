# Gates: Floe Phase 9B — Image and passive text Preview

- [x] G1: Correct phase branch.
  CHECK: git branch --show-current
  EXPECT: phase-9b-preview-images-text
  EVIDENCE: phase-9b-preview-images-text.
- [x] G2: Raster provider enforces no-follow identity/size/decode limits and returns bounded owned RGBA/first-frame state.
  CHECK: cargo test -p floe-app phase_9b_image -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_9b_image passed 1/1; PNG/raw-name identity, animated GIF first-frame state, allocation limit, no-follow symlink, malformed and changed sources verified.
- [x] G3: Text provider handles bounded UTF-8/UTF-16 and inert Markdown/code/JSON/XML while rejecting binary/active/oversized input.
  CHECK: cargo test -p floe-app phase_9b_text -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_9b_text passed 1/1; UTF-8, both BOM UTF-16 byte orders, inert formats, binary/invalid/oversized text, HTML and SVG verified.
- [x] G4: Final column presents image/text payloads accessibly and stale payloads do not replace current state.
  CHECK: cargo test -p floe-app phase_9b_presentation -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: phase_9b_presentation passed 1/1; terminal payload retention, stale-generation rejection, inert-source accessibility, same-target preservation and retirement verified.
- [x] G5: Native Wayland Preview action/health/lifecycle smoke passes.
  EVIDENCE: Native Wayland app exported view-miller/miller-preview-hook, accepted both actions, answered Peer.Ping, and exited 0 through app quit; known non-fatal VK_SUBOPTIMAL_KHR warning only.
- [x] G6: Formatting, check, strict Clippy, and all workspace tests pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace
  EXPECT: test result: ok
  EVIDENCE: fmt/check/strict all-target all-feature Clippy passed; 359 tests passed (268 app, 91 core), no failures.
- [x] G7: Patch hygiene is clean.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: git diff --check exited 0 with no output.
- [x] G8: Docs mark 9B complete and exactly 9C next with passive/no-active-content boundaries.
  CHECK: rg -n "9B.*COMPLETE|9C.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 9C
  EVIDENCE: Roadmap, matrix, privacy/security, and AGENTS record 9B COMPLETE, sole 9C NEXT, inert content, and no sandbox claim.
