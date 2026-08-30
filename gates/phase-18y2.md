# Gates: Floe Phase 18Y2 — Complete Undo and Recovery

Scope: durable operation-specific Undo/Redo and interruption review for only
those local and administrator mutations whose exact inverse can be proven.

- [x] U1: Versioned private bounded history store coexists with the Phase 18Y recovery journal.
  CHECK: cargo test -p floe-app phase_18y2_store -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Four focused store tests pass: private atomic mode/ownership/symlink/corruption rejection, exact non-UTF-8 path round-trip, bounded retention/expiry, and persisted action states.
- [x] U2: Local copy/move/rename/create durable Undo/Redo and hostile races.
  CHECK: cargo test -p floe-app phase_18y2_local -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Five focused local tests pass, including journal-before-mutation recipes, exact identity validation, no-overwrite Undo/Redo, recoverable Trash, non-empty-directory refusal, and duplicate-submission isolation.
- [x] U3: Provable administrator inverse support and explicit exclusions.
  CHECK: cargo test -p floe-app phase_18y2_administrator -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: One focused administrator-policy test passes; current GVfs administrator mutations remain explicitly outside durable Undo until exact inverse and fresh-authorization evidence can be proven.
- [x] U4: Accessible persistent history and conservative recovery UI.
  CHECK: cargo test -p floe-app phase_18y2_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: UI model test and focused real-GTK semantic Undo/Redo control test pass; persistent Applied/Undone and interrupted/uncertain recovery states are distinct.
- [x] U5: Full Rust, GTK, E2E, docs, package, release, native Wayland gates.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace
  EXPECT: test result: ok
  EVIDENCE: Format/check/strict Clippy pass; workspace tests pass 575 app plus 16 intentional GTK ignores, 21 controller, 162 core, and 6 duplicate; release build, docs/render, migrations/layout/source/candidate, diff hygiene, focused real-GTK, and isolated release-binary Wayland Ping/Quit pass. E2E contracts pass 5 with 2 truthful dependency/environment skips (Dogtail/pyatspi and unstaged installed-artifact binary).
- [x] U6: Docs/status are truthful and exactly one later phase is NEXT.
  CHECK: python3 scripts/check-docs.py --strict
  EXPECT: phase-21c-docs-ok
  EVIDENCE: Strict documentation check reports phase-21c-docs-ok across 21 files; Phase 18Y2 is COMPLETE and Phase 6U is the sole NEXT row, with the other four requested features not started.
