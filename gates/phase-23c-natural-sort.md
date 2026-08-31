# Gates: Floe Phase 23C — Natural filename sorting

- [x] S1: GTK-independent ordering handles unbounded digit runs, leading zeros,
  case folding and raw-byte ties deterministically.
  CHECK: `cargo test -p floe-core phase_23c_natural_sort -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS.
- [x] S2: Natural Name is visible and persistent with ordinary direction,
  directory, hidden, grouping and per-folder policies; sorting stays off GTK.
  CHECK: `cargo test -p floe-app phase_23c_natural_sort -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Preference round-trip and legacy fallback pass.
- [x] S3: Arbitrary raw filename property tests preserve multiset and total,
  deterministic ordering.
  CHECK: `cargo test -p floe-core natural_sort -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Dedicated 128-case proptest plus examples pass.
