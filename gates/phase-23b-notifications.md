# Gates: Floe Phase 23B — Completion notifications

- [x] N1: Deterministic policy notifies only long typed completed jobs, suppresses
  focused windows and paths, deduplicates IDs, retains in-app terminal evidence.
  CHECK: `cargo test -p floe-app phase_23b_notification_policy -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Two-second boundary, focus policy, path-free text and stable ID pass.
- [x] N2: GIO dispatch is preference-controlled, bounded and nonfatal.
  CHECK: `cargo test -p floe-app phase_23b_notification_dispatch -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Dispatch contract and preference migration/persistence pass.
- [x] N3: User, architecture and privacy documentation state focus suppression,
  generic desktop retention and path-free wording.
  EVIDENCE: docs/USER_GUIDE.md, docs/PRIVACY_SECURITY.md and
  docs/ARCHITECTURE.md contain the explicit contract.
