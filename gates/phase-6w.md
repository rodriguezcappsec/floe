# Gates: Floe Phase 6W — Undo Trash

- [x] W1: Complete exact local Trash receipts only; unsupported or ambiguous
  successful Trash operations remain explicitly non-undoable.
  CHECK: `cargo test -p floe-app phase_6w_trash_receipt -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Core and executor receipt tests accept one exact new raw-path payload/metadata pair and leave missing, ambiguous, changed, or incomplete successful Trash work non-undoable.

- [x] W2: Bounded private raw-path history survives restart and fails closed for
  hostile, incomplete, expired, interrupted, or changed Trash recipes.
  CHECK: `cargo test -p floe-app phase_6w_history -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Version-3 history round-trips non-UTF-8 Trash recipes, Applied/Undone identities, restart states, malformed pairs, and Redo receipt replacement.

- [x] W3: Undo restores no-overwrite after exact identity checks and Redo creates
  a fresh complete Trash receipt with truthful cancellation/partial/conflict state.
  CHECK: `cargo test -p floe-app phase_6w_undo_redo -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Isolated executor test verifies restore, destination conflict, exact no-follow revalidation, and fresh receipt after Redo with truthful conflict/partial mapping.

- [x] W4: History, recovery, reveal/refresh, and accessible action policy cover
  the supported boundary without broader Trash claims.
  CHECK: `cargo test -p floe-app phase_6w_ui -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Operation History labels supported Trash records and exposes only state-valid Undo/Redo; completion carries exact restored path into existing refresh/reveal policy.

- [x] W5: Full Rust, docs/render/release/diff, E2E, and applicable native gates pass.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Format, workspace check, strict all-target/all-feature Clippy, workspace tests, strict docs/render, release dependencies/advisories/matrix, eight E2E contracts, and diff hygiene pass. Current target/debug/floe native Wayland Ping/Quit exits 0; host AT-SPI bus remains unavailable and is not claimed.

- [x] W6: Docs mark verified 6W complete, exactly one later NEXT phase, and no
  chooser work in this branch.
  CHECK: `python3 scripts/check-docs.py --strict && test "$(rg -c '^\| .*\| NEXT \|' docs/ROADMAP.md)" -eq 1`
  EXPECT: `/phase-21c-docs-ok/`
  EVIDENCE: PASS. Persistent docs record exact-receipt boundary; exactly Phase 22A Selection Mode is NEXT and chooser code absent from this branch.
