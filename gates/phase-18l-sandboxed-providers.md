# Gates: Floe Phase 18L — Sandboxed providers

Scope: external thumbnail and Preview providers execute only inside an active,
verified, deny-by-default Bubblewrap boundary.

- [x] L1: Policy construction grants target-only read and private output write,
  unshares network/session/IPC namespaces, clears risky environment, preserves
  raw argv identity, and rejects missing/invalid policy setup.
  CHECK: `cargo test -p floe-app phase_18l_policy -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Focused policy test verifies missing launcher fails closed and the production argv contains `--unshare-all`, read-only `/usr`, exact input, private output/tmp, cleared environment, and no direct-provider fallback.

- [x] L2: Provider execution remains bounded/cancellable, terminates process
  groups, validates output/source identity, and never falls back to ordinary user
  authority when Bubblewrap is unavailable or fails.
  CHECK: `cargo test -p floe-app phase_18l_execution -- --nocapture && cargo test -p floe-app phase_6l_execution -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Full/focused execution tests retain process-group timeout, cancellation, output/source validation, and cleanup. The host denies Bubblewrap's required network namespace with `NETLINK_ROUTE ... Operation not permitted`; the live subcase skips truthfully and production remains unavailable rather than unsandboxed.

- [x] L3: Native provider UI truthfully distinguishes sandboxed, unavailable,
  timed-out, and unsupported results without claiming all built-in parsing is a
  process sandbox.
  CHECK: `cargo test -p floe-app phase_18l_presentation -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Focused presentation contract keeps sandbox-unavailable, timed-out, unsupported, and cancelled states distinct and contains no safety claim.
