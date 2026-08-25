# Gates: Floe Phase 6Q Create, duplicate, links

Status: COMPLETE

- [x] G1: Work is isolated on `phase-6q-create-duplicate-links`; Phase 6R drag-and-drop is not implemented.
  CHECK: `git branch --show-current && git diff --check`
  EXPECT: Phase 6Q branch and clean bounded diff.
  EVIDENCE: Branch is `phase-6q-create-duplicate-links`; diff hygiene passes and no drag-and-drop code exists.

- [x] G2: Core creation requests preserve exact paths for directories, empty files, and templates without shell, privilege, or overwrite.
  CHECK: `cargo test -p floe-core phase_6q_create -- --nocapture`
  EXPECT: Validation, collision, template, cancellation, and raw non-UTF-8 tests pass.
  EVIDENCE: Two focused tests pass create-new directory/file/template behavior, collision refusal, and pre-commit cancellation.

- [x] G3: Symbolic links preserve raw targets and may remain broken; hard links accept only regular non-symlink files and report unsupported failures without overwrite.
  CHECK: `cargo test -p floe-core phase_6q_links -- --nocapture`
  EXPECT: Symbolic, broken, hard-link, collision, symlink-source, and raw-name tests pass.
  EVIDENCE: Two focused tests pass raw broken symbolic targets, hard-link inode identity, symlink-source rejection, and no-overwrite collisions.

- [x] G4: Duplicate uses deterministic bounded sibling names, preserves symlinks, batches FIFO, and never replaces an existing sibling.
  CHECK: `cargo test -p floe-app phase_6q_duplicate -- --nocapture`
  EXPECT: Naming, multi-selection, collision, and symlink tests pass.
  EVIDENCE: Two focused tests pass bounded raw `(copy N)` naming and FIFO exact-source duplication with raw-name and symlink preservation.

- [x] G5: Fixed-capacity executor and `ApplicationState` integrate create/template/link lifecycle, retry identity, conflict revision, cancellation, history, and refresh without GTK filesystem work.
  CHECK: `cargo test -p floe-app phase_6q_state -- --nocapture`
  EXPECT: Focused executor/state lifecycle tests pass.
  EVIDENCE: Three focused state/executor tests pass completion, conflict, cancellation, stable retry identity, no-overwrite `(copy 2)`, history, and affected-directory tracking.

- [x] G6: New Folder, New Empty File, and New From Template are visible keyboard/pointer actions using validated names and native asynchronous template selection.
  CHECK: `cargo test -p floe-app phase_6q_templates -- --nocapture`
  EXPECT: Action, dialog, template, and typed-request routing policy tests pass.
  EVIDENCE: Focused typed-request test passes; native smoke exported all create actions and activated `new-folder` to open the validated-name dialog without mutation.

- [x] G7: Reveal Link Target resolves raw relative/absolute targets asynchronously and handles broken targets without executing content or blocking GTK.
  CHECK: `cargo test -p floe-app phase_6q_reveal -- --nocapture`
  EXPECT: Exact target resolution, lexical traversal normalization, and raw-target tests pass.
  EVIDENCE: Focused relative, absolute, traversal, and raw non-UTF-8 target policy test passes; implementation uses no-follow asynchronous GIO metadata and an accessibility check.

- [x] G8: Copy Name/Path/Relative Path/URI actions retain exact identity, reject lossy text, and expose standard clipboard text without filesystem inspection.
  CHECK: `cargo test -p floe-app phase_6q_clipboard -- --nocapture`
  EXPECT: UTF-8, non-UTF-8 rejection, relative-base, URI encoding, and multi-selection tests pass.
  EVIDENCE: Focused test passes newline-separated text, explicit current-folder base, outside-base rejection, non-UTF-8 text refusal, and exact percent-encoded local URI.

- [x] G9: Accessible context/header actions have truthful sensitivity; native Wayland action/dialog smoke stays healthy.
  CHECK: `cargo test -p floe-app phase_6q_ui -- --nocapture`
  EXPECT: UI policy tests pass and native evidence is recorded.
  EVIDENCE: Focused menu mapping test passes. Isolated native Wayland smoke exported 42 window actions, opened New Folder, answered `Peer.Ping`, quit cleanly, and released `io.github.floe.FileManager`.

- [x] G10: Formatting, workspace build, strict Clippy, all tests, diff hygiene, and native smoke pass.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && git diff --check`
  EXPECT: Command exits 0.
  EVIDENCE: Command exits 0; all 265 tests pass with 63 core and 202 application tests, zero failures.

- [x] G11: Persistent documentation records verified Phase 6Q and exactly Phase 6R as `NEXT`.
  CHECK: `node <unlazy-skill-dir>/scripts/gate-check.mjs --status GATES.md`
  EXPECT: `ALL MET`.
  EVIDENCE: `AGENTS.md`, `PLAN.md`, `GATES.md`, `DESIGN.md`, `docs/ROADMAP.md`, `docs/FEATURE_MATRIX.md`, `docs/ARCHITECTURE.md`, `docs/DEVELOPMENT.md`, and `docs/PRIVACY_SECURITY.md` record verified Phase 6Q and Phase 6R only as next.
