# Gates: Floe Phase 6S File watching

Status: COMPLETE

- [x] G1: Work is isolated on `phase-6s-file-watching`; Phase 6T browser completeness is not implemented.
  CHECK: `git branch --show-current && git diff --check`
  EXPECT: Phase 6S branch and clean bounded diff.
  EVIDENCE: Branch is `phase-6s-file-watching`; diff hygiene passes and no Phase 6T metadata, grouping, column, or per-folder preference work exists.

- [x] G2: Exactly one application-owned GIO monitor follows the active successful local directory and is cancelled on navigation, replacement, Trash mode, and shutdown.
  CHECK: `cargo test -p floe-app phase_6s_monitor -- --nocapture`
  EXPECT: Focused lifecycle, exact path, generation, replacement, and stop tests pass.
  EVIDENCE: Focused monitor test passes exact directory replacement, generation change, and stop; browser starts only after successful local listing and stops before every navigation/reload, Trash, failure, and shutdown.

- [x] G3: Duplicate and storm events are coalesced by one cancellable timer with explicit event/path/rename caps and conservative overflow behavior.
  CHECK: `cargo test -p floe-app phase_6s_coalescer -- --nocapture`
  EXPECT: Focused debounce, deduplication, caps, overflow, and reset tests pass.
  EVIDENCE: Focused coalescer test deduplicates 100 same-path events, caps 16,384 events, 4,096 paths, and 1,024 renames, records overflow, emits once, and resets.

- [x] G4: Create, delete, attribute, move-in/out, and exact rename pairs map to one typed watcher batch without lossy path reconstruction.
  CHECK: `cargo test -p floe-app phase_6s_events -- --nocapture`
  EXPECT: Focused event mapping and raw non-UTF-8 rename tests pass.
  EVIDENCE: Focused event test passes create/delete/attribute/move-in/move-out mapping and preserves raw non-UTF-8 source/destination rename identities through GIO paths.

- [x] G5: Each current coalesced batch submits at most one superseding `BrowserWorker` enumeration; GTK performs no directory enumeration, metadata scan, mutation, or per-event model rebuild.
  CHECK: `cargo test -p floe-app phase_6s_dispatch -- --nocapture`
  EXPECT: Focused current/stale batch dispatch policy tests pass.
  EVIDENCE: Focused dispatch test rejects old generation/directory batches; handler logs aggregate batch counts then calls `load_current_inner` exactly once, which submits one existing superseding worker request.

- [x] G6: Exact selection and stable scroll-anchor identity survive creates and renames, deleted identities disappear, and reconciliation is linear for 100k paths.
  CHECK: `cargo test -p floe-app phase_6s_reconcile -- --nocapture`
  EXPECT: Focused create/delete/rename/raw-path/100k selection and anchor tests pass.
  EVIDENCE: Focused reconciliation test passes surviving/deleted selection, direct and chained rename translation, stable/fallback anchor, raw identity, scroll ratio, and 100,000 current paths.

- [x] G7: Deleted/inaccessible directories and watcher failures show recoverable feedback, stale batches cannot replace newer navigation, and no integrity-monitoring claim is made.
  CHECK: `cargo test -p floe-app phase_6s_failure -- --nocapture`
  EXPECT: Focused failure wording and stale-generation tests pass.
  EVIDENCE: Focused failure test rejects invalid watch locations and requires “use Refresh” recovery wording without integrity language; generation/path policy prevents stale application.

- [x] G8: Formatting, workspace build, strict Clippy, tests, and diff hygiene pass.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && git diff --check`
  EXPECT: All commands exit zero with measured test totals.
  EVIDENCE: Command exits zero; all 277 tests pass: 63 core and 214 application, with zero failures.

- [x] G9: Native Wayland smoke observes a coalesced external create/rename/delete cycle, application ownership, healthy responsiveness, and clean quit/name release.
  CHECK: native Wayland external-change smoke procedure recorded in `GATES.md`
  EXPECT: One reconciliation per event burst and clean application lifecycle.
  EVIDENCE: Isolated HOME/XDG native run coalesced two creates as events=4/paths=2, mapped rename as events=1/paths=2/renames=1, reconciled delete once, exported 42 actions, answered `Peer.Ping`, quit cleanly, released its name; temporary root was removed.

- [x] G10: No new dependency, shell, privilege, path logging, recursive watch, or GTK-thread filesystem work is introduced.
  CHECK: `git diff -- Cargo.toml crates/app/Cargo.toml && rg -n "std::fs|Command::new|pkexec|sudo|recursive" crates/app/src/file_watcher.rs crates/app/src/browser.rs`
  EXPECT: Existing GIO dependency only and no forbidden Phase 6S path.
  EVIDENCE: Cargo manifests are unchanged; source audit finds no shell, elevation, filesystem mutation/enumeration, recursive watch, or path-valued tracing in the watcher/GTK integration.

- [x] G11: Persistent documentation records verified Phase 6S and exactly Phase 6T as `NEXT`.
  CHECK: `node /home/rocappsec/.codex/skills/unlazy/scripts/gate-check.mjs --status GATES.md`
  EXPECT: `ALL MET`.
  EVIDENCE: `AGENTS.md`, `PLAN.md`, `GATES.md`, `DESIGN.md`, `docs/ROADMAP.md`, `docs/FEATURE_MATRIX.md`, `docs/ARCHITECTURE.md`, `docs/DEVELOPMENT.md`, and `docs/PRIVACY_SECURITY.md` record verified Phase 6S and only Phase 6T next.
