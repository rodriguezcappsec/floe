# Gates: Floe Phase 22A — Selection Mode

- [x] S1: Strict invocation/domain policy supports Open File(s), Select Folder,
  Save File, cancellation, exact local URI results, and rejects invalid mixes.
  CHECK: `cargo test -p floe-app phase_22a_contract -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Two contract tests cover every valid/invalid raw invocation,
  normal-start bypass, non-UTF-8/newline identity, and exact local URI encoding.

- [x] S2: Fixed-capacity worker revalidates exact paths/types off GTK, detects
  races/conflicts, never follows an unsafe substitute, and shuts down cleanly.
  CHECK: `cargo test -p floe-app phase_22a_validation -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Three validation tests cover all modes, missing/replaced races,
  normalized paths, save conflicts, the 128-result cap, queue pressure, and Drop.

- [x] S3: Native chooser UI has clear title/accept/status/name/cancel semantics,
  keyboard and accessibility parity, correct single/multiple selection policy.
  CHECK: `cargo test -p floe-app phase_22a_ui -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Mode presentation/action policy tests pass. Selection Mode is
  deny-by-default for unknown GActions and separately rejects bookmark and drop
  mutations; existing preferences are loaded read-only and never migrated.

- [x] S4: Dedicated chooser lifecycle is isolated from ordinary Floe routing,
  emits no result on cancel, and has deterministic result/exit behavior.
  CHECK: `cargo test -p floe-app phase_22a_lifecycle -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Process-scoped application IDs and deterministic completion
  pass. Final isolated Wayland accept returned exactly `file:///tmp` with exit 0;
  cancel returned zero stdout bytes with exit 1. Neither created config/state or
  left its fresh 0700 transient state directory. Normal Floe Ping/Quit passed.

- [x] S5: Full Rust, docs/render/release/diff, E2E, and native Wayland gates pass.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Format/check/strict all-target all-feature Clippy pass. Workspace
  tests pass: 625 app tests with 21 intentional graphical ignores, 21 controller,
  171 core, and six duplicate workflows. Strict docs/render, dependency,
  advisory, environment-matrix, diff, and five E2E contracts pass; semantic
  Dogtail/pyatspi and installed-artifact classes skip truthfully.

- [x] S6: Persistent docs mark verified 22A complete, exactly Phase 22B NEXT,
  and make no portal-service or sandbox-grant claim.
  CHECK: `python3 scripts/check-docs.py --strict && test "$(rg -c '^\| .*\| NEXT \|' docs/ROADMAP.md)" -eq 1`
  EXPECT: `/phase-21c-docs-ok/`
  EVIDENCE: PASS. Strict documentation reports `phase-21c-docs-ok`; roadmap has
  one NEXT row, Phase 22B, and explicitly disclaims portal/sandbox/document grants.
