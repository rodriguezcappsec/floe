# Gates: Floe Phase 9C — PDF and document Preview

- [x] G1: Correct phase branch.
  CHECK: git branch --show-current
  EXPECT: phase-9c-preview-documents
  EVIDENCE: phase-9c-preview-documents.
- [x] G2: Document provider eligibility and exact no-follow identity are bounded and deterministic.
  CHECK: cargo test -p floe-app phase_9c_document_contract -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; PDF/common document allowlist, macro/HTML/SVG/archive rejection, and honest no-provider fallback verified.
- [x] G3: Reviewed helper execution returns only bounded PNG, cancels/times out, and rejects malformed/changed/symlink sources.
  CHECK: cargo test -p floe-app phase_9c_document_provider -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; supervised Phase 6L boundary, raw non-UTF-8 source, PNG/RGBA limits, malformed output, mutation, and no-follow symlink rejection verified; Phase 6L timeout/cancel tests remain green.
- [x] G4: Final column presents a labelled passive first-page/document rendition and rejects stale payloads.
  CHECK: cargo test -p floe-app phase_9c_document_presentation -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; first-page/MIME label, no-execution accessibility, and stale-generation rejection verified.
- [x] G5: Native Wayland document Preview action/health/lifecycle smoke passes.
  EVIDENCE: Wayland app accepted view-miller and miller-preview-hook, answered Peer.Ping, and exited 0 via app quit.
- [x] G6: Formatting, check, strict Clippy, and all workspace tests pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace
  EXPECT: test result: ok
  EVIDENCE: fmt/check/strict Clippy passed; 362 tests passed (271 app, 91 core), no failures.
- [x] G7: Patch hygiene is clean.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: git diff --check exited 0 with no output.
- [x] G8: Docs mark 9C complete and exactly 9D next with no-macro/no-sandbox boundaries.
  CHECK: rg -n "9C.*COMPLETE|9D.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 9D
  EVIDENCE: Roadmap, matrix, privacy/security, and AGENTS record 9C COMPLETE, sole 9D NEXT, macro rejection, and no sandbox claim.
