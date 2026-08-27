# Leaf Gates: Phase 18T — Integrity Tools

- [x] T1: Reuse the reviewed Phase 10E streaming SHA-256 engine for explicit
  saved fingerprint and verification requests; no new hash implementation.
  CHECK: `cargo test --workspace phase_18t_fingerprint -- --nocapture`
  EXPECT: match, changed, missing, stale identity, cancellation, and limits pass
  EVIDENCE: 2026-08-27: `cargo test --workspace phase_18t_fingerprint -- --nocapture` passed 4 app tests; core had 0 matching tests.
- [x] T2: Generate and parse a strict path-safe portable `SHA256SUMS` profile
  without shelling out or reconstructing paths from display text.
  CHECK: `cargo test --workspace phase_18t_manifest -- --nocapture`
  EXPECT: round trip, malformed, duplicate, escape, symlink, non-UTF-8 policy, and bounded cases pass
  EVIDENCE: 2026-08-27: `cargo test --workspace phase_18t_manifest -- --nocapture` passed 7 app tests; core had 0 matching tests.
- [x] T3: Native actions expose save fingerprint, verify fingerprint, generate
  manifest, and verify manifest with cancellable progress and accessible results.
  CHECK: `cargo test -p floe-app phase_18t_ui -- --nocapture`
  EXPECT: controller, command, state, wording, and accessibility contracts pass
  EVIDENCE: 2026-08-27: `cargo test -p floe-app phase_18t_ui -- --nocapture` passed 5 tests for typed requests/private store, escaped labels, current/common roots, results wording/counts, and command accessibility metadata.
- [x] T4: Persistence, if used, is versioned/private/bounded/atomic and declares
  path/hash retention; ordinary Phase 10E checksums remain compatible.
  CHECK: `cargo test --workspace phase_18t -- --nocapture`
  EXPECT: all Phase 18T tests pass
  EVIDENCE: 2026-08-27: `cargo test --workspace phase_18t -- --nocapture` passed 18 app tests; core had 0 matching tests.
