# Gates: adversarial Linux integration remediation

Scope: Fix GIO launch capability handling, prefix-correct portal activation,
privileged-operation watchdog UX, and sandbox-consistent thumbnailer discovery.

- [x] L1: Open/Open With retains visible `%f`/`%F` file-only applications and
  launches them through GIO file APIs while URI-only handlers retain URI launch.
  CHECK: `cargo test -p floe-app adversarial_file_only_launcher -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: Passed: 1 launcher regression; `%f` and `%F` fixtures remain visible and use GIO file routing, while `%u` retains URI routing.

- [x] L2: Both supported `/usr` and default `/usr/local` staged installations
  produce a D-Bus service whose `Exec=` resolves to the installed Floe binary.
  CHECK: `sh packaging/tests/test-package-layout.sh`
  EXPECT: `/phase-21b-package-layout-ok/`
  EVIDENCE: Passed with `phase-21b-package-layout-ok`; both staged manifests and exact `/usr/bin/floe` and `/usr/local/bin/floe` activation commands were checked.

- [x] L3: Privileged mutations enter a bounded no-progress state with Continue
  Waiting and Cancel semantics, and the UI cannot remain permanently trapped by
  a missing terminal callback.
  CHECK: `cargo test -p floe-app adversarial_privileged_watchdog -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: Passed: 1 watchdog regression covers 30-second expiry, stale timers, Continue Waiting re-arm, cancellation escape, late progress, and terminal reset.

- [x] L4: Thumbnailer discovery accepts only executables reachable by the
  required production sandbox, or safely exposes an exact reviewed executable;
  user/Nix paths never fail later after being advertised as available.
  CHECK: `cargo test -p floe-app adversarial_thumbnailer_sandbox -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: Passed: 1 discovery regression rejects user and Nix executables while retaining and canonicalizing a provider reachable below the sandbox's read-only `/usr` mount.

- [x] L5: Existing launcher, privileged access, provider sandbox, packaging,
  migration, and portal contracts remain passing without unsandboxed fallback.
  CHECK: `cargo test -p floe-app phase_14c -- --nocapture && cargo test -p floe-app phase_18l -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: Passed: Phase 14C reported 7 passed and 1 documented graphical ignore; Phase 18L reported 3 passed. Formatting, app check, strict all-target Clippy, release metadata, packaging, and scoped diff hygiene also exited 0.
