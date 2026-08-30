# Gates: fresh-session GVfs administrator mounting

Scope: Correct Floe's experimental read-only administrator browser when a
supported `admin://` location has not yet been mounted or authorized.

- [x] A1: A fresh `NotMounted` enumeration result starts exactly one bounded
  GIO mount/authorization request, then retries enumeration only after mount
  success.
  CHECK: cargo test -p floe-app phase_14b_provider -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Focused provider tests pass, including first-`NotMounted` mount,
  second-`NotMounted` fail-closed, `AlreadyMounted`, denial, and timeout cases.

- [x] A2: Denial, cancellation, timeout, missing service/agent, mount failure,
  and stale-generation results remain typed and cannot create a retry loop.
  CHECK: cargo test -p floe-app phase_14b_state -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Phase 14B state tests and full Phase 14B suite pass; generation and
  cancellation guards retain the prior typed terminal outcomes.

- [x] A3: The fix preserves the existing security boundary: Floe remains the
  calling user, never receives credentials, and administrator resources remain
  read-only and unavailable to ordinary jobs, launchers, previews, or tools.
  CHECK: rg -n 'read-only|never.*password|normal user|mount.*authorization' docs/PRIVILEGED_ACCESS.md docs/USER_GUIDE.md
  EXPECT: /read-only/
  EVIDENCE: Source/security contract, User Guide, privileged-access architecture,
  privacy/security ledger, roadmap, and feature matrix agree on one read-only
  request-scoped mount with GVfs/polkit-owned credentials.

- [x] A4: Formatting, workspace checks, strict Clippy, tests, and diff hygiene
  pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: Formatting, workspace check, strict all-target/all-feature Clippy,
  workspace tests, E2E preflight, strict docs/render, deterministic source, and
  diff hygiene pass. The focused real-GTK accessibility test also passes.

- [ ] A5: Native KDE/GVfs evidence proves the initial `NotMounted` condition is
  no longer reported as an unavailable administrator service; any unavoidable
  authentication interaction or host limitation is reported exactly.
  EVIDENCE: KDE session reports GVfs daemon and Plasma polkit agent active and
  reproduces fresh `admin:///boot` as `NotMounted`. A bounded native mount probe
  reached the desktop authorization path, but no password was entered and no
  authenticated listing was claimed.

ABANDON: A5 successful administrator enumeration requires interactive user
authentication; this run did not request or receive the user's password.
