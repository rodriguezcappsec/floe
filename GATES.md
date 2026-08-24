# Gates: Floe Phase 6N Trash lifecycle

Status: COMPLETE

- [x] G1: Work is isolated on `phase-6n-trash-lifecycle`; Phase 6O and later
  features are not implemented.
  CHECK: `git branch --show-current`
  EXPECT: `phase-6n-trash-lifecycle`
  EVIDENCE: branch command returned `phase-6n-trash-lifecycle`; diff contains
  only Phase 6N code, tests, gates, and persistent documentation.

- [x] G2: Local freedesktop Trash discovery and `.trashinfo` parsing preserve
  exact backing/original path identity, bound metadata reads, and tolerate
  malformed/orphaned entries without hiding deletable payloads.
  CHECK: `cargo test -p floe-core phase_6n_trash_metadata -- --nocapture`
  EXPECT: all focused metadata tests pass using temporary roots.
  EVIDENCE: four metadata tests pass for raw percent-decoded bytes, mounted
  relative paths, malformed/orphan payloads, and symlinked root rejection.

- [x] G3: Restore uses exact no-replace semantics, rejects unsafe or unavailable
  destinations, never overwrites, removes metadata only after a successful move,
  and reports destination conflicts distinctly.
  CHECK: `cargo test -p floe-core phase_6n_restore -- --nocapture`
  EXPECT: all focused restore tests pass.
  EVIDENCE: four restore tests pass for commit/metadata cleanup, conflict
  preservation, missing metadata, and mismatched payload/metadata rejection.

- [x] G4: Trash browse and restore execution are bounded, cancellable application
  work; GTK callbacks perform no filesystem work and shared jobs receive truthful
  terminal outcomes.
  CHECK: `cargo test -p floe-app phase_6n_worker -- --nocapture`
  EXPECT: all focused worker tests pass.
  EVIDENCE: three worker tests pass for off-GTK Trash metadata enumeration,
  completed restore lifecycle, and conflict failure mapping.

- [x] G5: Trash mode has a first-class sidebar route, shows standards deletion
  date/original location where available, preserves exact selected entries, and
  returns to normal browsing without corrupting navigation state.
  CHECK: `cargo test -p floe-app phase_6n_browser -- --nocapture`
  EXPECT: all focused browser/policy tests pass.
  EVIDENCE: focused mode/action policy test passes; implementation retains local
  navigation, uses exact backing paths, displays metadata separately, shows
  hidden Trash payloads, and disables inapplicable mutations/sorts.

- [x] G6: Trash selection actions are limited to Restore and Delete Permanently;
  Empty Trash requires aggregate irreversible confirmation, and all deletion
  submits through the Phase 6M engine with no secure-erase claim.
  CHECK: `cargo test -p floe-app phase_6n_actions -- --nocapture`
  EXPECT: all focused action/wording tests pass.
  EVIDENCE: two action tests pass; native action smoke exported enabled Empty
  Trash and opened its safe-focus Phase 6M confirmation without deleting fixture.

- [x] G7: Individual permanent deletion, confirmed Empty Trash, and restore
  refresh the Trash view; destructive partial outcomes stay explicit and
  non-retryable.
  CHECK: `cargo test -p floe-app phase_6n_state -- --nocapture`
  EXPECT: all focused state/orchestration tests pass.
  EVIDENCE: state test proves restore conflict uses stable logical identity,
  fresh safe sibling attempt, no overwrite, successful cleanup; shared operation
  controller refreshes both backing and destination parents and rejects partial
  generic retry.

- [x] G8: Formatting, workspace build, strict Clippy, complete tests, and diff
  hygiene pass.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && git diff --check`
  EXPECT: every command exits 0.
  EVIDENCE: final combined gate exited 0; 172 application plus 48 core tests
  passed, 220 total; doc tests and `git diff --check` passed.

- [x] G9: Isolated native Wayland smoke proves Trash navigation and confirmation
  surfaces, D-Bus application health, clean shutdown, and no access to real user
  Trash.
  EVIDENCE: temporary HOME/XDG roots and private D-Bus session exported
  `open-trash`, Restore, and enabled Empty Trash. One GIO-trashed fixture restored
  through Floe to its original path and matching `.trashinfo` disappeared.
  Rebuilt binary opened Empty Trash confirmation, answered Peer.Ping, quit through
  application action, preserved the confirmation-only fixture, and exited 0.
  Only isolated-session portal/accessibility and known host rendering warnings
  appeared; no real Trash path was accessed.

- [x] G10: `AGENTS.md`, `PLAN.md`, `GATES.md`, `docs/ROADMAP.md`,
  `docs/FEATURE_MATRIX.md`, `docs/ARCHITECTURE.md`, `DESIGN.md`, and
  `docs/PRIVACY_SECURITY.md` accurately record verified Phase 6N; exactly one
  later phase is marked `NEXT`.
  CHECK: `node /home/rocappsec/.codex/skills/unlazy/scripts/gate-check.mjs --status GATES.md`
  EXPECT: `ALL MET` only after all evidence is recorded.
  EVIDENCE: all listed documents updated; roadmap contains exactly one `NEXT`,
  Phase 6O transfer semantics.
