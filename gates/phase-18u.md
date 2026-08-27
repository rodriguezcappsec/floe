# Leaf Gates: Phase 18U — Integrity Monitoring

- [x] U1: Explicit local baselines reuse 18T identities/digests report matching, changed, missing, new entries deterministically.
  CHECK: `cargo test --workspace phase_18u_baseline -- --nocapture`
  EXPECT: baseline diff policy tests pass
  EVIDENCE: 2026-08-27 passed: 2 app baseline-engine tests and 1 core raw-path deterministic-diff test.

- [x] U2: Monitoring coalesces bursts, bounds watched roots/events/state, handles watcher overflow/offline gaps/mount loss, supports cancellation/disable.
  CHECK: `cargo test --workspace phase_18u_monitor -- --nocapture`
  EXPECT: storm, stale, cancellation, rescan-required cases pass
  EVIDENCE: 2026-08-27 passed: 3 app cancellation/local-root/worker-capacity tests and 4 core coalescing, stale-reason, watch-policy, pause/disable tests.

- [x] U3: Native UI opt-in, accessible, calm, says monitoring not intrusion detection; no background watch starts implicitly.
  CHECK: `cargo test -p floe-app phase_18u_ui -- --nocapture`
  EXPECT: command/state/result/accessibility contracts pass
  EVIDENCE: 2026-08-27 passed: 2 contracts cover all six explicit baseline/watch command placements and Matching/Changed/Missing/New results with the visible no-attribution, not-intrusion-detection notice. Browser construction leaves monitoring disabled and creates no IntegrityWatchSet.

- [x] U4: Baseline storage private/versioned/bounded respects declared Private/Sensitive policy without watching remote, Trash, links, mounts.
  CHECK: `cargo test --workspace phase_18u -- --nocapture`
  EXPECT: all Phase 18U tests pass
  EVIDENCE: 2026-08-27 passed: 13 app and 5 core tests including private versioned store round-trip/tamper/symlink/permission policy, recursive GIO watch-set bounds, non-UTF-8 paths, and remote/Trash/mount/symlink rejection. Focused strict `cargo clippy -p floe-app --all-targets -- -D warnings` passed.
