# Gates: Floe Phase 6U — Replace Conflict Safety

Scope: explicit local Copy/Move/Rename Replace and stable-batch Replace All
only; no later multi-window, Trash Undo, sidebar, provider-sandbox, or Phase 6V
selection/reveal work.

- [x] R1: Exact no-follow identities, private bounded backup, commit revalidation, no silent overwrite, rollback/partial evidence.
  CHECK: cargo test -p floe-core phase_6u_engine -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Six engine tests pass copy/move retained versions, atomic Undo exchange, changed-destination rollback, pre-commit cancellation, backup collision, and changed-backup cleanup refusal. Owner-only root validation passes under the recovery filter.

- [x] R2: Replace All remains compatible-conflict stable-batch scoped with fresh identities, cancellation boundaries, Protected Folder pause, and no cross-batch leakage.
  CHECK: cargo test -p floe-app phase_6u_batch -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Two tempfile workflows prove first confirmed Replace All, automatic second-conflict fresh capture, completed two-item batch, unrelated later batch pausing for its own decision, and cancellation preventing the third item after an in-flight replacement. Protected intersections deliberately remain blocked for fresh review.

- [x] R3: Private manifest/backups support durable Undo/Redo, restart review, bounded expiry, and cleanup only with owned identity proof.
  CHECK: cargo test -p floe-app phase_6u_recovery -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Four recovery tests pass two-version executor commit, restart round trip, identity-owned expiry cleanup, and changed-backup review retention. Full workspace tests cover Undo/Redo state-machine identity exchange.

- [x] R4: Accessible conflict comparison and explicit second-confirmed Replace/Replace All preserve existing decisions.
  CHECK: cargo test -p floe-app phase_6u_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Headless wording contract passes. Focused real-GTK ignored gate passes distinct visible destructive buttons, semantic Button roles, and preserved Keep Existing, Keep Both, and Retry controls; host emitted only the already-known libadwaita GtkSettings warning.

- [x] R5: Full Rust, GTK, E2E, docs, package, release, and native Wayland gates pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace
  EXPECT: test result: ok
  EVIDENCE: Format/check/strict Clippy pass. Workspace tests pass 582 app plus 17 intentional graphical ignores, 21 controller, 169 core, and six duplicate workflows. Strict docs/render, migrations/layout/source/release-candidate, release build, and diff hygiene pass. E2E contracts pass five with two truthful skips because Python dogtail/pyatspi and the staged installed artifact are unavailable. Focused real-GTK passes; isolated release Wayland Ping/Actions/Quit exits 0 with only documented RADV warning.

- [x] R6: Docs/status truthfully mark Phase 6U COMPLETE and exactly Phase 6V NEXT.
  CHECK: python3 scripts/check-docs.py --strict
  EXPECT: phase-21c-docs-ok
  EVIDENCE: Strict documentation check passes 21 files; ROADMAP has exactly one NEXT row, Phase 6V. README, User Guide, Feature Matrix, privacy/security, PLAN, GATES, and AGENTS describe replacement limits and retained-content privacy. Phase 6V was not implemented.
