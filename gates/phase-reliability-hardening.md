# Gates: Floe Reliability Hardening

Scope: repair confirmed error, durability, and worker-shutdown defects before
Phase 6W; no Undo Trash or FileChooser implementation.

- [x] R1: Replace and nested Copy/Move errors produce truthful Cancelled,
  Partial, Conflict, PermissionDenied, Unsupported, or Io job outcomes.
  CHECK: `cargo test -p floe-app reliability_replace_failure -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Focused classification regression passes permission, ordinary
  I/O, conflict, unsupported, partial, direct cancellation, nested Copy
  cancellation, and nested Move cancellation cases.

- [x] R2: Every tracked partial operation uses operation-specific or neutral
  terminal wording and never calls non-delete work permanent deletion.
  CHECK: `cargo test -p floe-app reliability_partial_title -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Focused title coverage passes for Copy, Move, Rename, Trash,
  Restore, Create, Link, Duplicate, Replace, Archive, and permanent deletion.

- [x] R3: Undo-history capacity failure persists every cleanup-failed
  `NeedsReview` transition and restart restores the same review state.
  CHECK: `cargo test -p floe-app reliability_undo_capacity -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. A full 256-record store with an identity-changed replacement
  backup rejects new history, retains the occupant, and reopens with all 256
  records review-required.

- [x] R4: Advanced-metadata responses remain capacity-bounded, coalesce
  progress, preserve terminal outcomes under pressure, and worker Drop cannot
  block on result delivery.
  CHECK: `cargo test -p floe-app reliability_metadata_queue -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Repeated progress coalesces, the queue stays at 32 events, a
  terminal result survives pressure, 1,057 files finish without a reader, and
  Drop joins within the bounded deadline. The regression passed eight repeats.

- [x] R5: Format, workspace check, strict all-target/all-feature Clippy,
  workspace tests, strict docs/render/diff, and applicable native lifecycle
  gates pass or record exact external limitations.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Rust gates pass: 593 app tests plus 18 intentional graphical
  ignores, 21 controller tests, 169 core tests, and six duplicate workflows.
  Strict docs/render, release dependency/advisory/environment matrix, diff
  hygiene, and E2E contracts pass. Semantic native E2E records two exact
  external-dependency skips; no GTK widget/action boundary changed here.

- [x] R6: Persistent status records only verified reliability work and leaves
  exactly Phase 6W as the sole roadmap `NEXT` phase.
  CHECK: `python3 scripts/check-docs.py --strict && test "$(rg -c '^\| .*\| NEXT \|' docs/ROADMAP.md)" -eq 1`
  EXPECT: `/phase-21c-docs-ok/`
  EVIDENCE: PASS. AGENTS, architecture, roadmap, matrix, privacy/security, plan,
  and gates record only this checkpoint. Strict docs pass and Phase 6W is the
  one NEXT row.
