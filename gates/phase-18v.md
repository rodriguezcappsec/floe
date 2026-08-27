# Leaf Gates: Phase 18V — Verified Copy

- [x] V1: Optional Copy and Verify reuses ordinary safe copy publication, then hashes revalidated source and destination through the reviewed Phase 18T/10E SHA-256 engine.
  CHECK: `cargo test --workspace phase_18v_copy -- --nocapture`
  EXPECT: match, changed source, injected mismatch, links, conflicts, retry pass
  EVIDENCE: PASS — 2026-08-27; 4 app executor tests plus 1 core request/retry test passed for successful exact raw-path trees, source change, destination mismatch, preserved symbolic-link target bytes, no-overwrite conflict, bounded worker, and typed retry.

- [x] V2: Cancellation, full disk, read/hash/sync/cleanup failures yield explicit states and never misreport an unverified destination as verified.
  CHECK: `cargo test --workspace phase_18v_failure -- --nocapture`
  EXPECT: failure partial-state matrix passes
  EVIDENCE: PASS — 2026-08-27; 4 app failure tests plus 1 core stage test passed for cancellation, injected ENOSPC, retained cleanup failure, hash/read failure, sync failure, missing source, retained unverified output, and no premature Verified state.

- [x] V3: Native action is distinct from ordinary Copy, exposes progress and hash-not-authenticity wording, and does not change default Copy semantics.
  CHECK: `cargo test -p floe-app phase_18v_ui -- --nocapture`
  EXPECT: command, job, result, wording, accessibility contracts pass
  EVIDENCE: PASS — 2026-08-27; 2 ordinary UI policy/registry tests passed. The opt-in native GTK component gate `cargo test -p floe-app phase_18v_ui_gtk -- --ignored --nocapture` passed on the active display (1 passed), verifying semantic dialog buttons and accessible dialog title.

- [x] V4: All filesystem tests remain below disposable roots and source data survives pre-publication failure.
  CHECK: `cargo test --workspace phase_18v -- --nocapture`
  EXPECT: all Phase 18V tests pass
  EVIDENCE: PASS — 2026-08-27; 10 app and 2 core focused tests passed under tempfile roots. `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `git diff --check` passed; full ordinary suite was 450 app tests passed with 3 graphical tests ignored, plus 146 core tests passed.
