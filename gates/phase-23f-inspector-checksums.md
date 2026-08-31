# Gates: Floe Phase 23F — Inspector and Properties checksums

- [x] H1: Inspector and Properties expose explicit SHA-256 and reuse reviewed
  checksum jobs/results without eager hashing.
  CHECK: `cargo test -p floe-app phase_23f_checksum_surface -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Exactly one regular file is eligible.
- [x] H2: Exact identity, cancellation, stale-result rejection and digest
  lifecycle retain hash-not-authenticity wording.
  CHECK: `cargo test -p floe-app phase_10e -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Existing reviewed checksum executor/UI lifecycle remains green.
- [ ] H3: Native semantic input covers both Inspector and Properties controls.
  EVIDENCE: Controls share existing accessible action/dialog path; native app
  launch remains healthy, but Dogtail/pyatspi are unavailable.
ABANDON: H3 Native semantic activation cannot be automated on this host.
