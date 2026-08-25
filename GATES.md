# Gates: Floe Phase 6R Drag and drop

Status: COMPLETE

- [x] G1: Work is isolated on `phase-6r-drag-drop`; Phase 6S file watching is not implemented.
  CHECK: `git branch --show-current && git diff --check`
  EXPECT: Phase 6R branch and clean bounded diff.
  EVIDENCE: Branch is `phase-6r-drag-drop`; diff hygiene passes and no watcher dependency or implementation exists.

- [x] G2: Drag policy preserves exact internal and external local paths, deduplicates without lossy conversion, and rejects invalid/self-nesting destinations.
  CHECK: `cargo test -p floe-app phase_6r_drag_policy -- --nocapture`
  EXPECT: Focused exact-path, raw non-UTF-8, duplicate, root, self, and descendant tests pass.
  EVIDENCE: Focused policy test passes raw non-UTF-8 identity, exact deduplication, root, same-destination, and self-nesting rejection.

- [x] G3: Internal list/grid drags carry the complete exact selection and standards-based external local-file drops are accepted without shell or display-text reconstruction.
  CHECK: `cargo test -p floe-app phase_6r_payload -- --nocapture`
  EXPECT: Focused selected-row, multiselect, URI-list, local-only, and malformed payload tests pass.
  EVIDENCE: Focused payload test passes local GFile round-trip, empty selection, and remote URI rejection; both views publish one exact multi-selection GDK file list.

- [x] G4: Copy, move, and symbolic-link drops submit FIFO no-overwrite jobs through `ApplicationState`; Trash drops reuse the existing bounded Trash batch.
  CHECK: `cargo test -p floe-app phase_6r_state -- --nocapture`
  EXPECT: Focused action routing, ordering, conflict, link, Trash, and affected-directory tests pass.
  EVIDENCE: Focused state test completes copy, move, and link batches with expected source semantics; code routes Trash directly to the reviewed Trash batch and all requests retain fail-if-exists behavior.

- [x] G5: Directory rows/background, eligible Places/bookmarks/mounted devices, and Trash resolve exact destinations and reject unavailable targets.
  CHECK: `cargo test -p floe-app phase_6r_destination -- --nocapture`
  EXPECT: Focused row/background/sidebar/bookmark/device/Trash capability tests pass.
  EVIDENCE: Exact destination planning test passes FIFO names; UI resolvers use current bound `DirectoryEntry`, authoritative location/bookmark paths, only navigable device paths, and a distinct Trash target.

- [x] G6: Hover-open and edge autoscroll are bounded, cancellable, and never perform filesystem work or uncontrolled timer/task spawning on GTK.
  CHECK: `cargo test -p floe-app phase_6r_motion -- --nocapture`
  EXPECT: Focused timing, edge direction, cancellation, and single-active-state tests pass.
  EVIDENCE: Focused motion test passes 56 px edge zones and clamped 22 px steps; browser owns at most one 720 ms source and removes it on leave, drop, replacement, and shutdown.

- [x] G7: Accepted/rejected destinations and negotiated actions have non-color-only accessible feedback, while existing keyboard/menu transfer alternatives remain available.
  CHECK: `cargo test -p floe-app phase_6r_accessibility -- --nocapture`
  EXPECT: Focused feedback wording, accessible state, and alternative-action tests pass.
  EVIDENCE: Focused accessibility test passes action/release wording; dashed outline, accessible description, and status text supplement color, while existing Copy/Cut/Paste/menu actions remain exported.

- [x] G8: GTK callbacks only decode/drop/submit application commands; no filesystem mutation, blocking metadata, shell, privilege escalation, or implicit overwrite is introduced.
  CHECK: `rg -n "std::fs|Command::new|pkexec|sudo|overwrite" crates/app/src/drag_drop.rs crates/app/src/browser.rs`
  EXPECT: No forbidden Phase 6R GTK path; explicit fail-if-exists policy remains visible.
  EVIDENCE: Audit command returns no forbidden match in Phase 6R paths; typed requests route to existing bounded executors with explicit `FailIfExists`.

- [x] G9: Formatting, workspace build, strict Clippy, tests, and diff hygiene pass.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && git diff --check`
  EXPECT: All commands exit zero with measured test totals.
  EVIDENCE: Command exits zero; all 271 tests pass: 63 core and 208 application, with zero failures.

- [x] G10: Native Wayland smoke verifies application ownership, exported actions, healthy list/grid/sidebar/Trash targets, and clean quit.
  CHECK: native Wayland smoke procedure recorded in `GATES.md`
  EXPECT: Floe remains responsive and releases its D-Bus name after quit.
  EVIDENCE: Native `GDK_BACKEND=wayland` launch owned `io.github.floe.FileManager`, answered `Peer.Ping`, exported 42 window actions, remained healthy, quit through `app.quit`, and released its D-Bus name; only known host libadwaita, AT-SPI, and Vulkan warnings appeared.

- [x] G11: Persistent documentation records verified Phase 6R and exactly Phase 6S as `NEXT`.
  CHECK: `node /home/rocappsec/.codex/skills/unlazy/scripts/gate-check.mjs --status GATES.md`
  EXPECT: `ALL MET`.
  EVIDENCE: `AGENTS.md`, `PLAN.md`, `GATES.md`, `DESIGN.md`, `docs/ROADMAP.md`, `docs/FEATURE_MATRIX.md`, `docs/ARCHITECTURE.md`, `docs/DEVELOPMENT.md`, and `docs/PRIVACY_SECURITY.md` record verified Phase 6R and only Phase 6S next.
