# Gates: Floe Phase 6O Transfer semantics

Status: COMPLETE

- [x] G1: Work is isolated on `phase-6o-transfer-semantics`; Phase 6P later
  features are not implemented.
  CHECK: `git branch --show-current && git diff --check`
  EXPECT: phase branch, clean diff hygiene, and bounded Phase 6O changes only.
  EVIDENCE: branch command returned `phase-6o-transfer-semantics`; the reviewed
  diff contains only Phase 6O transfer code, tests, and persistent documentation,
  and `git diff --check` exits 0.

- [x] G2: Copy preflight checks destination filesystem available space before
  output creation and reports exact required/available byte counts without
  presenting the check as a reservation guarantee.
  CHECK: `cargo test -p floe-core phase_6o_space -- --nocapture`
  EXPECT: focused space-preflight tests pass.
  EVIDENCE: one focused test passes required/available byte reporting,
  insufficient-space refusal before destination creation, zero-byte handling,
  and injected-query failure behavior.

- [x] G3: Regular files and directories preserve supported POSIX mode plus
  access/modification timestamps after content completion; symlinks are never
  followed and unsupported metadata is not claimed.
  CHECK: `cargo test -p floe-core phase_6o_metadata -- --nocapture`
  EXPECT: focused file/directory/symlink truthful-report tests pass.
  EVIDENCE: two focused tests pass regular-file/directory mode and timestamp
  preservation plus explicit symlink-metadata non-preservation reporting.

- [x] G4: Same-filesystem moves retain atomic no-replace rename; cross-filesystem
  moves use a hidden sibling staging tree, never overwrite, synchronize before
  publication, and preserve exact non-UTF-8 path and symlink identity.
  CHECK: `cargo test -p floe-core phase_6o_cross_filesystem -- --nocapture`
  EXPECT: focused fallback, conflict, symlink, and raw-path tests pass.
  EVIDENCE: three focused tests pass staged fallback with raw non-UTF-8 names and
  symlinks, no-overwrite conflict cleanup, and a real `EXDEV` path when the host
  exposes distinct devices; the atomic rename fast path remains covered by the
  existing move suite.

- [x] G5: Cross-filesystem source identity is revalidated before publication.
  Cancellation or source change before commit removes staging and retains the
  source; failure after commit reports a non-retryable partial outcome.
  CHECK: `cargo test -p floe-core phase_6o_recovery -- --nocapture`
  EXPECT: pre-commit cleanup and post-commit source-retained/partial tests pass.
  EVIDENCE: two focused tests pass changed-source staging cleanup and retained
  source behavior plus explicit post-publication cleanup partial failure.

- [x] G6: The bounded move executor maps fallback completion, progress/failure,
  cancellation, conflict, insufficient space, and committed partial outcomes
  into truthful shared job lifecycle states; GTK performs no filesystem work.
  CHECK: `cargo test -p floe-app phase_6o_executor -- --nocapture`
  EXPECT: all focused executor/state mapping tests pass.
  EVIDENCE: two focused application tests pass real-`EXDEV` progress/completion
  and committed move-cleanup failure mapping to a non-retryable partial outcome.

- [x] G7: Floe publishes bounded interoperable local-file clipboard data for
  copy/cut using URI-list plus GNOME/KDE conventions while retaining exact
  in-process `PathBuf` identity.
  CHECK: `cargo test -p floe-app phase_6o_clipboard_publish -- --nocapture`
  EXPECT: focused copy/cut, multi-path, escaping, non-UTF-8, and limit tests pass.
  EVIDENCE: two focused tests pass exact-path deduplication and relative-path
  rejection plus copy/cut publication and raw non-UTF-8 URI round-tripping.

- [x] G8: External clipboard reads are asynchronous and bounded, prefer explicit
  copy/cut semantics, accept only local file URIs, deduplicate exact paths, and
  stage through `ApplicationState` before paste.
  CHECK: `cargo test -p floe-app phase_6o_clipboard_parse -- --nocapture`
  EXPECT: focused GNOME/KDE/URI-list, malformed, remote, duplicate, and limit
  tests pass.
  EVIDENCE: three focused tests pass GNOME, KDE, and URI-list semantics; preserve
  distinct raw paths with colliding lossy names; and reject remote, malformed,
  oversized, or over-item-limit data.

- [x] G9: Formatting, workspace build, strict Clippy, complete tests, diff
  hygiene, and isolated native Wayland smoke pass.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && git diff --check`
  EXPECT: all commands exit 0; native smoke evidence is recorded below.
  EVIDENCE: the final combined gate exits 0 with 56 core and 179 application
  tests passing, 235 total. An isolated `/tmp` HOME/XDG native session owned the
  D-Bus application name, exported 30 window actions, activated Select All and
  Copy, enabled Paste from Floe's clipboard provider, answered `Peer.Ping`, quit
  cleanly, and released its name. The compositor required a real input serial
  for external-client clipboard ownership, so no native cross-client claim is
  made; deterministic provider/parser tests are the interoperability evidence.
  Only known host accessibility-bus and RADV `VK_SUBOPTIMAL_KHR` warnings appeared.

- [x] G10: `AGENTS.md`, `PLAN.md`, `GATES.md`, `docs/ROADMAP.md`,
  `docs/FEATURE_MATRIX.md`, `docs/ARCHITECTURE.md`, `DESIGN.md`,
  `docs/DEVELOPMENT.md`, and `docs/PRIVACY_SECURITY.md` accurately record
  verified Phase 6O and exactly one later phase marked `NEXT`.
  CHECK: `node <unlazy-skill-dir>/scripts/gate-check.mjs --status GATES.md`
  EXPECT: `ALL MET`
  EVIDENCE: persistent documentation records Phase 6O as complete, explicitly
  bounds preservation/privacy claims, and `rg -n '\| NEXT \|' docs/ROADMAP.md`
  returns exactly one row for `phase-6p-operation-control`.
