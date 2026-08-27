# Leaf Gates: Phase 18X — Data-Loss Guardrails

- [x] X1: Protected Folder policy stores exact bounded roots privately and
  handles ancestors, descendants, links, mount roots, moves, deletion, and
  non-UTF-8 paths without claiming access control or encryption.
  CHECK: `cargo test --workspace phase_18x_protected -- --nocapture`
  EXPECT: matching, codec, corruption, limits, and exact-path cases pass
  EVIDENCE: `2026-08-27` focused workspace gate passes eight matching tests
  across core, application, and controller harnesses. Versioned raw-path codec,
  private `0700`/`0600` atomic storage, corruption/relative/duplicate/capacity
  rejection, non-UTF-8 round trip, exact component matching, and fixed
  Protect/Unprotect context discovery pass. Separate store filters also prove
  corrupt, unsupported, trailing, insecure-parent, and unsafe-link states stay
  explicitly blocked instead of becoming an empty policy.

- [x] X2: Destructive preflight computes deterministic operation-scale and risk
  decisions off GTK for permanent delete, Trash, move, overwrite, and large
  batches while preserving existing irreversible confirmations.
  CHECK: `cargo test --workspace phase_18x_preflight -- --nocapture`
  EXPECT: thresholds, protected roots, huge operations, mounts, and override pass
  EVIDENCE: `2026-08-27` focused workspace gate passes 18 matching tests across
  application, controller harness, and core. Coverage includes bounded
  no-follow scanning, exact protected destinations, threshold boundaries,
  root/home/injected-mount risks, non-UTF-8 ancestor/descendant scope, safe
  recoverable-operation exclusion, incomplete/unknown fail-confirm behavior,
  cancellation, latest-generation results, and single-use exact-scope permits.

- [x] X3: Every destructive executor entry point requires one fresh exact-scope
  controller authorization, including batch dispatch, undo, retry, and revised
  conflict destinations; policy generation changes revoke outstanding permits.
  CHECK: `cargo test -p floe-app phase_18x_controller -- --nocapture`
  EXPECT: central fail-closed controller, single-use permit, reset, and stale
  generation tests pass
  EVIDENCE: `2026-08-27` all five controller-filtered tests pass. The complete
  legacy app suite also passes using real controller-issued permits. Production
  call-site audit finds direct fallthrough only for non-destructive Copy/Create,
  Keep Existing, and Skip All; Move/Rename/Trash/delete/Restore, serial batches,
  batch rename/undo, Undo, destructive Retry, Keep Both, and Retry With Name all
  pass through the central review and final permit-consumption boundary.

- [x] X4: Native settings, file/folder/background contexts, command palette,
  and accessible preflight UI expose Protect, Unprotect, status, exact scope,
  current state, disabled/busy/error feedback, and truthful limitations without
  filesystem or policy-store I/O in GTK callbacks.
  CHECK: `cargo test --workspace phase_18x -- --nocapture`
  EXPECT: policy worker, persistence success/failure, raw-path action state,
  blocked-store reset, registry/context parity, dialog model, and regressions pass
  EVIDENCE: `2026-08-27` 47 deterministic Phase 18X tests pass across app,
  controller harness, and core; one graphical contract remains intentionally
  ignored by the ordinary suite. Its explicit real-GTK run passed once and
  verified dialog role, accessible label, Close action, blocked wording, and
  limitation text. A later complete ignored-GTK run could not initialize a
  display on this host and is recorded as an environment limitation. The
  asynchronous capacity-one policy path proves busy rejection, no premature
  in-memory install, exact raw-path persistence, failure non-install, and
  explicit acknowledged corrupt-store reset. Full deterministic workspace
  verification passes 466 app tests plus 21 integration tests and 146 core tests
  with four graphical tests intentionally ignored.
