# Gates: Floe Phase 19B — Associations and Custom Actions

Scope: local XDG MIME defaults and explicitly configured unprivileged external
tools only; no shell, plugins, remote targets, secrets, or administrator access.

- [x] A1: Association discovery preserves exact local target/type identity,
  reports default/recommended/all applications, and bounded worker requests set
  or reset the XDG MIME default with explicit result/error state.
  CHECK: `cargo test -p floe-app phase_19b_association -- --nocapture`
  EVIDENCE: Two focused association tests pass bounded explicit set/reset requests
  and missing-application failure without mutation. The worker resolves desktop
  IDs and applies GIO changes off GTK; Open With reports queued and final result
  states rather than claiming early success.
- [x] A2: Custom-action definitions and private persistence are bounded,
  versioned, atomic, permission-checked, reject malformed/unsupported/oversized
  input, and never contain implicit shell or environment expansion.
  CHECK: `cargo test -p floe-app phase_19b_custom_action_store -- --nocapture`
  EVIDENCE: Two focused store tests pass version-13 round-trip and hostile record
  rejection. Definitions reuse the private atomic preference worker and enforce
  32 actions, 32 argv items, 16 MIME patterns, 1,024-character fields, valid IDs,
  whole placeholders, and duplicate-ID replacement.
- [x] A3: Eligibility and placeholder expansion preserve raw `OsString` path
  identity, apply reviewed single/multi/folder/MIME rules, build argv directly,
  and spawn with fixed capacity without `sh -c`, `bash`, `eval`, or interpolation.
  CHECK: `cargo test -p floe-app phase_19b_custom_action_launch -- --nocapture`
  EVIDENCE: Three focused launch tests pass MIME/target/multiple eligibility,
  non-UTF-8 `OsString` argv identity, literal shell metacharacters, direct spawn,
  bounded queue/children, and start/finish lifecycle events. No shell is invoked.
- [x] A4: Applications & Tools UI supports add/edit/remove/reorder/testable action
  definitions, association set/reset, selection-aware context menu and command
  palette visibility, accessible copy, explicit changes, and truthful failures.
  CHECK: `cargo test -p floe-app phase_19b_custom_action_ui -- --nocapture`
  EVIDENCE: Plain-label UI test and ignored real-GTK accessibility test pass.
  Settings editor, context-menu eligibility, Command Palette chooser, Open With
  set/reset feedback, stable roles/labels, and the Adwaita ampersand-markup
  regression are covered. Native D-Bus lists `custom-actions`,
  `custom-action-chooser`, and typed `run-custom-action`; opening the editor,
  `Peer.Ping`, Quit, exit 0, and a clean second launch after the markup fix pass.
- [x] A5: Formatting, workspace check, strict Clippy, workspace tests, native
  build, real-GTK/E2E/Wayland gates, diff hygiene, README/User Guide/security and
  project ledgers, exactly one next phase pass.
  EVIDENCE: `cargo fmt --all -- --check`, workspace check, strict all-target/all-
  feature Clippy, workspace tests, native build, and `git diff --check` pass. The
  application binary has 520 tests: 512 passed and eight intentional graphical
  ignores; app integration, duplicate workflow, and 150 core tests pass. E2E
  harness reports three passed and one native suite skipped because Python
  Dogtail/pyatspi are unavailable. README, User Guide, Roadmap, Feature Matrix,
  Privacy/Security, plan/status ledgers are updated and exactly Phase 14B is NEXT.

---

# Gates: Floe Phase 7G — Navigation Upgrades

Scope: exact local navigation improvements only; no remote, association,
external-action, or privileged-location work.

- [x] N1: Breadcrumb segments preserve exact raw component paths, expose root
  and ancestors with keyboard/action semantics, and collapse responsively
  without using display text as identity.
  CHECK: `cargo test --workspace phase_7g_breadcrumb -- --nocapture`
EVIDENCE: Three `floe-core` Phase 7G tests pass, including raw non-UTF-8
identity and root/ancestor ordering; the GTK surface uses exact action indices,
a bounded horizontal scroller, and a non-actionable truthful Trash label.
- [x] N2: Location completion enumerates only the submitted parent on a bounded
  superseding worker, preserves non-UTF-8 identity, and handles missing,
  inaccessible, huge, and changed directories without blocking GTK.
  CHECK: `cargo test -p floe-app phase_7g_completion -- --nocapture`
EVIDENCE: Three completion tests pass for directory-only matching, raw
non-UTF-8 identity, 4,096-entry/64-result limits, errors, and supersession.
- [x] N3: Recent locations are bounded/deduplicated over authoritative
  navigation history; workspace restore retains back/forward state and existing
  Private/Sensitive policies suppress owned persistence.
  CHECK: `cargo test --workspace phase_7g_recent -- --nocapture`
EVIDENCE: Core and application recent-history tests pass; the list reuses the
restorable current/back/forward session and adds no persistence store.
- [x] N4: GApplication command-line local folder and file targets route to exact
  navigation or parent plus reveal; malformed, non-local, missing, multi-target,
  and non-UTF-8 identities have deterministic truthful behavior.
  CHECK: `cargo test -p floe-app phase_7g_cli -- --nocapture`
EVIDENCE: Six CLI/application tests pass for one local target, folder, exact
file parent/reveal, raw names, missing/unsupported/non-local/multiple targets,
and worker supersession. Isolated native Wayland/D-Bus folder then file launch,
Peer.Ping, second-instance routing, Quit, and clean exit pass.
- [x] N5: Native controls expose meaningful roles, labels, descriptions,
  focus/order, keyboard alternatives, and clean real-GTK component behavior.
  CHECK: `cargo test -p floe-app phase_testing_gtk_phase_7g -- --ignored --nocapture`
EVIDENCE: Real-display `phase_testing_gtk_phase_7g` passes breadcrumb group,
recent button, location textbox, suggestion list, action, label, and tooltip
contracts. The host still emits its documented external libadwaita dark-setting
warning; Floe does not set that unsupported GTK property.
- [x] N6: Formatting, workspace check, strict Clippy, workspace tests, native
  build, applicable E2E/Wayland gates, diff hygiene, README/user/project docs,
  and exactly one next phase pass.
EVIDENCE: `cargo fmt --all -- --check`, workspace check, strict all-target/all-
feature Clippy, 510 app tests (503 passed, seven intentional graphical ignores),
21 app integration tests, 150 core tests, six duplicate workflow tests, native
build, and `git diff --check` pass. E2E harness has three contract tests pass;
native Dogtail workflows skip because Python Dogtail and pyatspi/AT-SPI are
unavailable. README, User Guide, privacy/status ledgers, and exactly one Phase
19B `NEXT` are updated.

---

# Archived gates: Floe Phase 20A — Settings Center

Scope: one organized native settings surface over existing preference and
action boundaries. No later roadmap feature is in scope.

- [x] S1: `win.settings` opens one searchable native Settings Center whose
  categories cover Appearance, Browsing, Views & Layout, Search & Preview,
  Operations & Safety, Applications, Shortcuts & Menus, and Accessibility.
  CHECK: `cargo test -p floe-app settings_center_model -- --nocapture`
  EXPECT: `test result: ok`
EVIDENCE: Four deterministic Phase 20A model/search/action/preference tests pass; the model contains exactly eight non-empty sections and 20 unique settings.
- [x] S2: Safe controls apply live through existing BrowserController methods,
  update authoritative `ViewPreferences`, and persist through the bounded
  writer without a second settings store.
  CHECK: `cargo test -p floe-app settings_center_preferences -- --nocapture`
  EXPECT: `test result: ok`
EVIDENCE: Appearance, icons, default view, per-folder memory, Vim mode, grid size, file/sidebar density, and filename index route to existing stateful actions or controller methods and `queue_preferences`; the 20-test preference migration/persistence slice passes.
- [x] S3: Existing Keyboard Shortcuts, Context Menu, and Terminal editors remain
  the only detailed editors and are reachable from Settings; irreversible
  safety confirmations remain unchanged.
  CHECK: `cargo test -p floe-app settings_center_actions -- --nocapture`
  EXPECT: `test result: ok`
EVIDENCE: Focused action-link test verifies every Settings action is registered, including keyboard/context/terminal/recovery/protection/preview surfaces; no destructive confirmation preference was added.
- [x] S4: Search is case-insensitive across labels, descriptions, and category
  language; empty and no-result states remain clear and keyboard usable.
  CHECK: `cargo test -p floe-app settings_center_search -- --nocapture`
  EXPECT: `test result: ok`
EVIDENCE: Multi-term search regression covers case-insensitive Frosted, right-click, reduced-animation, empty-query, and no-result behavior.
- [x] S5: Interactive controls expose meaningful labels, descriptions, states,
  and keyboard focus; the ignored real-GTK component contract passes with a
  display or its unavailable environment is reported exactly.
  CHECK: `cargo test -p floe-app settings_center_gtk -- --ignored --nocapture`
  EXPECT: `test result: ok`, or exact graphical-environment skip reason
EVIDENCE: Focused real-GTK Settings Center accessibility contract passes on the native display after correcting escaped libadwaita group-title markup; dialog, SearchBox, ComboBox, Switch, Button, and Status roles are verified.
- [x] S6: Formatting, workspace check, strict all-target/all-feature Clippy,
  workspace tests, native build, E2E preflight/applicable native workflows,
  diff hygiene, README/user/project docs, and exactly one roadmap `NEXT` pass.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check`
  EXPECT: all available deterministic gates pass
EVIDENCE: Formatting, workspace check, strict Clippy, 500 app tests (493 passed, seven intentional graphical ignores), 21 app integration, 147 core, six duplicate workflow tests, native build, E2E harness contract, diff hygiene, README/user guide/status ledgers, and exactly one Phase 7G NEXT pass. Native Dogtail workflows skip because Python Dogtail and pyatspi/AT-SPI are unavailable.

---

# Archived gates: Daily-Driver Priority Program

Scope: recovery, Settings, navigation, application tools, and administrator
locations are complete only when their real native workflows and failure
boundaries are verified.

- [x] D1: Interrupted copy/move/create outputs are journaled privately and
  boundedly, detected after restart, and offered explicit recovery choices;
  corrupt/stale journals never authorize deletion or overwrite.
  CHECK: cargo test -p floe-app phase_18y -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Thirteen named phase_18y regressions pass: pre-mutation fail-closed copy, copy/move/create success cleanup, raw non-UTF-8 private atomic journal, corrupt/symlink/insecure rejection, explicit blocked-store reset, conservative retry state, uncertain destination retention, Recovery Center reveal/retry/record-only resolution.
- [x] D2: Safe Undo and conflict decisions cover only operation-specific,
  freshly revalidated identities; Trash/create/replace/apply-to-all behavior
  never silently destroys uncertain data.
  CHECK: cargo test -p floe-app phase_18y -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Create Undo captures no-follow identity and uses ordinary GIO Trash only after executor-side identity and empty-directory checks. Replacements and later directory contents fail without backend mutation; move/rename Undo stays green; Replace/Replace All and Undo Trash remain unavailable.
- [ ] D3: A native accessible Settings Center organizes appearance, browsing,
  preview, operations, applications, shortcuts/context menus, and accessibility;
  supported controls apply live and persist through bounded migrations.
  CHECK: cargo test -p floe-app settings_center -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: pending
- [ ] D4: Breadcrumbs, path completion, recent locations, history restoration,
  and CLI file/folder routing preserve exact raw paths, remain bounded, and do
  not leak sensitive histories.
  CHECK: cargo test --workspace navigation_upgrade -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: pending
- [ ] D5: MIME association management can inspect/set/clear reviewed defaults;
  external actions use validated executable plus argv templates, capability and
  file-type eligibility, never a shell or lossy path reconstruction.
  CHECK: cargo test -p floe-app phase_19b -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: pending
- [ ] D6: Open as Administrator is explicit and application-layer only, uses
  reviewed GIO/polkit authority, never elevates the Floe process or captures a
  password, and fails with actionable honest feedback when unavailable.
  CHECK: cargo test -p floe-app open_as_administrator -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: pending
- [ ] D7: Every new interactive control has meaningful accessible metadata,
  shortcut/command discovery where applicable, and native GTK component gates.
  EVIDENCE: pending
- [ ] D8: Formatting, workspace check, strict all-target/all-feature Clippy,
  workspace tests, native build, E2E harness/applicable workflows, Wayland
  smoke, diff hygiene, documentation, and exactly one roadmap `NEXT` pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check
  EXPECT: test result: ok
  EVIDENCE: pending

---

# Archived gates: Header Options Information Architecture Follow-up

- [x] O1: The root menu exposes at most six task/utility sections and no long
  flat list of file operations.
  EVIDENCE: Deterministic model test verifies five named task submenus plus one
  utility section; File Operations contains five focused submenus.
- [x] O2: Every pre-existing header action remains present exactly once and
  continues to use its existing `win.*` GAction and shortcut boundary.
  EVIDENCE: Sorted pre/post `win.*` action-model diff is empty.
- [x] O3: Category order and submenu depth are deterministic; nested disclosure
  stays bounded and uses plain-language labels.
  EVIDENCE: `header_options_are_task_grouped_and_preserve_nested_actions`
  verifies order, uniqueness, root size, linked action retention, and depth.
- [x] O4: The options button has a stable accessible name and description, and
  a focused real-GTK component contract passes where a display is available.
  EVIDENCE: Main menu label, description, and tooltip are explicit; serial real
  GTK header/filter/Operations accessibility contract passes. The broad GTK
  selector was not used as evidence because concurrent tests contended for the
  one default GTK main context.
- [x] O5: Formatting, workspace check, strict all-target/all-feature Clippy,
  workspace tests, native build, diff hygiene, docs, and sole Phase 18Y `NEXT`
  all pass.
  EVIDENCE: `cargo fmt --all -- --check`, workspace check, strict Clippy,
  workspace tests (481 app tests: 476 passed, five intentional GTK ignores;
  21 app integration, 147 core, six duplicate workflow), native build, E2E
  harness contract (native workflow skipped: Dogtail/pyatspi unavailable),
  `git diff --check`, docs audit, and exactly one Phase 18Y `NEXT` pass.

---

# Archived gates: Floe Phase 13G3 — Duplicate Finder Performance

Scope: Make initial and repeated recursive exact-duplicate scans materially
faster without weakening Phase 13G/13G2 path, mutation, privacy, cancellation,
or byte-confirmation guarantees.

- [x] G3-1: Size grouping adds a bounded first/last-chunk quick signature that
  rejects same-size nonmatches before full SHA-256, while matching files still
  receive reviewed SHA-256 and final byte confirmation.
  CHECK: `cargo test -p floe-core phase_13g3_quick_signature -- --nocapture`
  EXPECT: `test result: ok`
  EVIDENCE: Focused core workflow regression passes: three same-size files
  receive bounded quick samples, the nonmatch is rejected before SHA-256, the
  two matches are hashed and remain one byte-confirmed group.
- [x] G3-2: Candidate hashing uses a bounded configurable worker pool, remains
  deterministic and generation-cancellable, and never spawns per-file threads
  or performs unbounded I/O.
  CHECK: `cargo test -p floe-core phase_13g3_parallel_hash -- --nocapture`
  EXPECT: `test result: ok`
  EVIDENCE: Two focused core regressions pass. One filesystem is capped at two
  concurrent hashes even when configured for eight; the synthetic imbalanced
  two-device schedule proves no device exceeds two active reads while the
  global worker pool remains bounded.
- [x] G3-3: A private bounded persistent SHA-256 cache preserves raw paths,
  reuses only exact current fingerprints, rejects corrupt/insecure/symlinked
  storage, writes atomically with private permissions, and never stores content.
  CHECK: `cargo test -p floe-app phase_13g3_hash_cache -- --nocapture`
  EXPECT: `test result: ok`
  EVIDENCE: Three focused cache regressions pass: private atomic raw non-UTF-8
  round trip, corrupt/insecure/symlink rejection, and pre/post-hash replacement
  rejection. Review verifies 200,000-entry/64-MiB bounds and batch LRU eviction.
- [x] G3-4: Incremental invalidation removes changed/deleted identities from
  reusable cache state through existing file-watcher reconciliation and scan-time
  revalidation; unchanged files demonstrably avoid repeat hashing.
  CHECK: `cargo test -p floe-app phase_13g3_incremental -- --nocapture`
  EXPECT: `test result: ok`
  EVIDENCE: Two focused incremental regressions pass. Restarted cold/warm scans
  report 2/0 then 0/2 computed/reused hashes; one-path invalidation reports 1/1,
  and an overflowed watcher batch clears reusable state and reports 2/0.
- [x] G3-5: Progress distinguishes discovery, quick filtering, hashing, and byte
  confirmation, exposes cache reuse, remains accessible, and uses no fake
  percentage or blocking GTK filesystem work.
  CHECK: `cargo test -p floe-app phase_13g3_progress -- --nocapture`
  EXPECT: `test result: ok`
  EVIDENCE: Focused accessible-status regression passes for all four named
  phases, separate calculated and validated-cache counters, and no percentage.
  Progress is throttled off GTK through the bounded worker event channel.
- [x] G3-6: Focused tests, formatting, workspace check, strict all-target/all-
  feature Clippy, complete workspace tests, native build, real GTK component
  gate, diff hygiene, documentation, and exactly one roadmap `NEXT` all pass.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check`
  EXPECT: `test result: ok`
  EVIDENCE: Formatting, workspace check, strict all-target/all-feature Clippy,
  480 app tests (475 passed, five intentional GTK ignores), 21 app integration
  tests, 147 core unit tests, six duplicate workflow integration tests, native
  build, focused real GTK setup contract, diff hygiene, documentation audit,
  and exactly one roadmap NEXT (Phase 18Y) pass.

---

# Archived gates: Floe Phase 13G2 — Duplicate Finder Workflows

- [x] G2-1: Recursive chosen-folder mode finds every exact duplicate group in
  nested subfolders without changing Phase 13G safety policy.
  EVIDENCE: `duplicate_finder_workflows` nested folder-tree regression passes.
- [x] G2-2: Selected-reference mode returns only groups containing that exact
  file, preserves raw path identity, avoids double counting, and rejects a
  non-regular reference.
  EVIDENCE: Four core integration regressions pass, including unrelated groups,
  raw non-UTF-8 identity, in-scope reference deduplication, and invalid reference.
- [x] G2-3: Native setup exposes folder-tree, copies-of-file, and selected-items
  modes with truthful recursive/exact-byte wording and contextual defaults.
  EVIDENCE: Two deterministic setup-policy tests and the real GTK modal,
  transient-parent component contract pass.
- [x] G2-4: Focused core/app tests, formatting, workspace check, strict
  all-target/all-feature Clippy, workspace tests, native build, GTK component
  gate, and diff hygiene pass.
  EVIDENCE: Formatting, workspace check, strict Clippy with warnings denied,
  474 app tests (469 passed, five intentional GTK ignores), 21 Phase 18X
  integration tests, 146 core tests, four new duplicate workflow integration
  tests, native build, focused real GTK gate, and `git diff --check` pass.
- [x] G2-5: `AGENTS.md`, roadmap, feature matrix, plan, gates, and user guide
  describe the verified result while exactly Phase 18Y remains `NEXT`.
  EVIDENCE: All named documents updated; roadmap audit reports exactly one
  `NEXT`, Phase 18Y.

---

# Archived gates: Floe Phases 18T–18X — Integrity and Data-Loss Safety

Scope: Complete exactly Phase 18T, 18U, 18V, 18W, and 18X with native,
responsive, standards-first behavior. Phase 18Y and all other roadmap work stay
unimplemented.

- [x] TX1: Every leaf ledger is complete with concrete evidence and its parent
  re-runs focused checks.
  CHECK: `node <unlazy-skill-dir>/scripts/gate-check.mjs --status gates/phase-18t.md gates/phase-18u.md gates/phase-18v.md gates/phase-18w.md gates/phase-18x.md`
  EXPECT: every phase leaf reports all gates met
  EVIDENCE: Parent rerun reports all five leaf ledgers ALL MET: 20/20 gates.
- [x] TX2: Core/application boundaries preserve exact paths, no-follow and
  no-shell behavior, bounded asynchronous work, cancellation, generation
  safety, and no silent overwrite across all five phases.
  CHECK: `cargo test --workspace phase_18 -- --nocapture`
  EXPECT: all focused Phase 18T–18X deterministic tests pass
  EVIDENCE: Phase-family filter passes 71 app tests, 21 integration tests, and
  14 core tests; two unrelated graphical contracts remain intentionally
  ignored by the ordinary deterministic filter.
- [x] TX3: Integrity wording and behavior never claim authenticity, malware
  safety, intrusion detection, secure erase, or safe eject before completion.
  CHECK: `rg -n 'hash is not authenticity|not authenticity|not intrusion detection|not encryption|safe to remove' crates docs/FEATURE_MATRIX.md docs/PRIVACY_SECURITY.md`
  EXPECT: truthful limitations are represented in policy and user surfaces
  EVIDENCE: Core policy, dialogs, command metadata, matrix, and security
  architecture explicitly distinguish integrity from authenticity/safety,
  monitoring from intrusion detection, Protected Folders from encryption or
  access control, and safe removal from pre-eject partial states.
- [x] TX4: Existing ordinary checksum, copy, destructive, device, file-view,
  job, command, preference, and accessibility behavior remains compatible.
  CHECK: `cargo test --workspace`
  EXPECT: complete deterministic workspace suite passes
  EVIDENCE: Complete deterministic workspace passes: 466 app tests, 21 Phase
  18X integration tests, and 146 core tests; four graphical tests are ignored
  by the ordinary suite as designed.
- [x] TX5: GTK remains responsive and user-visible actions expose accessible
  progress, result, conflict, failure, cancellation, and partial states.
  CHECK: `cargo test -p floe-app phase_testing_gtk_header_filter_and_operations_accessibility_contract -- --ignored --nocapture && cargo test -p floe-app phase_testing_gtk_phase_18x_guardrail_dialog_accessibility_contract -- --ignored --nocapture`
  EXPECT: each real GTK component contract passes in its own process
  EVIDENCE: Both contracts pass 1/1 on the active display. Running the broad
  filter in one Rust test process is invalid because GTK initialization is
  thread-affine; separate processes are the documented permanent gate.
- [x] TX6: Security isolation policy is respected: tests use disposable roots,
  no real user data/device, no implicit network, no secret/content telemetry,
  and no future Phase 18 feature is smuggled into scope.
  CHECK: `git diff --check`
  EXPECT: diff is hygienic and review confirms bounded scope
  EVIDENCE: Diff hygiene passes. Filesystem tests use `tempfile` or injected
  stores/devices; no real HOME, Trash, mount, or USB media was used. No new
  dependency, network transmission, secret handling, or Phase 18Y work exists.
- [x] TX7: Formatting, workspace check, strict all-target Clippy, native build,
  and complete tests pass after integration.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo build -p floe-app`
  EXPECT: every deterministic build-quality gate passes
  EVIDENCE: Root rerun passes formatting, workspace check, strict all-target
  Clippy with warnings denied, the complete deterministic workspace suite, and
  native `floe-app` build.
- [x] TX8: Applicable isolated native Wayland workflows pass; environment-only
  skips and real-removable-device limitations are recorded exactly.
  EVIDENCE: Real GTK contracts pass separately on the active Wayland display;
  E2E harness contracts pass 3/3 and semantic workflows skip exactly because
  Dogtail/pyatspi are unavailable. Real USB validation is skipped because no
  disposable lab media was provided. A root isolated-D-Bus lifecycle retry was
  blocked by portal services closing the disposable bus, so no additional
  lifecycle claim is made and the user's existing Floe instance was untouched.
- [x] TX9: AGENTS, ROADMAP, FEATURE_MATRIX, PRIVACY_SECURITY, PLAN, GATES,
  architecture/development/user docs agree with verified implementation.
  CHECK: `rg -n '18(T|U|V|W|X).*COMPLETE|18Y.*NEXT' docs/ROADMAP.md`
  EXPECT: 18T–18X complete and exactly 18Y next only after verification
  EVIDENCE: Persistent project, roadmap, matrix, security, architecture,
  development, user, plan, and gate documents record 18T–18X COMPLETE, their
  limitations, and exactly Phase 18Y NEXT.
- [x] TX10: Root gate checker passes and no dependency is added without a
  written Phase 18A admission review.
  CHECK: `node <unlazy-skill-dir>/scripts/gate-check.mjs GATES.md`
  EXPECT: every project gate met with evidence
  EVIDENCE: No Cargo manifest or lockfile change was introduced for 18T–18X;
  root gate checker passes after all evidence is recorded.

---

# Gates: Floe Phase 18A — Security Threat Model

Scope: Research, decision records, dependency rationale, and a traceable
security test plan only. No Phase 18B+ runtime implementation or dependency.

- [x] A1: Authoritative threat model identifies protected assets, adversaries,
  trusted components, trust/authority boundaries, abuse cases, explicit
  non-protections, data classes, and prohibited claims against current code.
  CHECK: `rg -n '^## (Protected assets|Adversaries|Trusted components and assumptions|Trust and authority boundaries|Abuse cases|Explicit non-protections|Security data classification|Prohibited claims)' docs/security/THREAT_MODEL.md`
  EXPECT: all required sections are present
  EVIDENCE: All eight required headings are present. The record defines 18
  numbered abuse cases, C0-C5 data classes, an authority matrix, explicit
  non-protections, invariants, prohibited claims, and change control.
- [x] A2: Decision record covers formats, secrets, credential storage, vaults,
  caches/history, sandboxing, integrity, privileged access, and dependency
  policy with explicit accepted/candidate/deferred/rejected status and rationale.
  CHECK: `rg -n '^### SEC-18A-[0-9]{2}' docs/security/PHASE_18A_DECISIONS.md`
  EXPECT: stable decision IDs cover every required architecture family
  EVIDENCE: Exactly 16 stable records cover no-runtime scope, portable
  encryption, secret lifetime, desktop-neutral credentials, vault selection,
  sandboxing, trace invalidation, SHA-256 integrity, privileged access,
  dependencies, publication, terminology, recovery, findings, local-first
  networking, and sequencing. Candidate names remain unselected dependencies.
- [x] A3: Test plan maps every Phase 18B–18AA leaf to threats, applicable test
  layers, required success/failure cases, isolation, and evidence needed before
  any IMPLEMENTED claim.
  CHECK: `rg -n '^\| 18(B|C|D|E|F|G|H|I|J|K|L|M|N|O|P|Q|R|S|T|U|V|W|X|Y|Z|AA) ' docs/security/PHASE_18_TEST_PLAN.md`
  EXPECT: one traceability row for every later Phase 18 leaf
  EVIDENCE: Exactly 26 rows cover every leaf from 18B through 18AA with threat
  and decision links, required layers, success/failure cases, isolation, and
  completion evidence. Global hostile-corpus, failure-taxonomy, dependency, and
  evidence-template requirements are present.
- [x] A4: Roadmap/matrix/status defer Niri, Plasma-specific integration, remote,
  and MTP; mark only verified 18A complete and exactly one high-impact next leaf.
  CHECK: `rg -n 'Status: \*\*NEXT\*\*|\| .*\| NEXT \|' docs/ROADMAP.md`
  EXPECT: exactly Phase 18T is NEXT
  EVIDENCE: Roadmap has exactly one NEXT row, Phase 18T. Phases 15 and 16 plus
  17A-17D are DEFERRED; Phase 18A is COMPLETE. Feature matrix, AGENTS, README,
  privacy/security architecture, and the three security records agree.
- [x] A5: Phase adds no dependency or runtime behavior; security terms and
  persistent documents cross-link the authoritative artifacts consistently.
  CHECK: `git diff -- Cargo.toml crates/app/Cargo.toml crates/core/Cargo.toml Cargo.lock`
  EXPECT: no dependency diff
  EVIDENCE: Manifest/lockfile diff is empty. Phase 18A adds only documentation
  and status/sequence records; existing source/resource diffs belong to the
  already verified post-Phase-14 icon work. Later capabilities remain PLANNED,
  and protected terms retain the authoritative meanings.
- [x] A6: Documentation hygiene, formatting, workspace check, strict Clippy,
  complete tests, native build, diff hygiene, and gate checker pass.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
  EXPECT: all deterministic gates pass; native smoke explicitly N/A
  EVIDENCE: Structure/count checks, dependency diff, and `git diff --check`
  pass. Formatting, workspace check, strict all-target Clippy, complete workspace
  tests, and native `floe-app` build pass; 397 app and 132 core tests are listed,
  with the two existing graphical GTK tests intentionally ignored by the
  ordinary suite. Native GTK/E2E/Wayland smoke is N/A because Phase 18A changes
  no runtime or UI behavior. The gate checker passes all 107 recorded gates when
  run outside the command sandbox required by its `/bin/sh` subprocess.

---

# Gates: Floe post-Phase-14 — Synthetic Execute-Bit Icon Regression

Scope: Correct icon classification on exFAT and similar filesystems that report
ordinary files as executable. No MIME probing, filesystem I/O, or Phase 15 work.

- [x] X1: Recognized PDF, text, document, media, archive, and code extensions
  retain semantic icons when their metadata includes execute bits.
  CHECK: `cargo test -p floe-app post_phase_14_synthetic_execute -- --nocapture`
  EXPECT: focused executable-precedence regression passes
  EVIDENCE: Focused regression passes ten representative known families as
  executable `0755` regular files; each retains its semantic icon instead of
  collapsing to Executable.
- [x] X2: Unknown or extensionless executable files retain the executable icon;
  directories, links, non-UTF-8 paths, and ordinary unknown files are unchanged.
  CHECK: `cargo test -p floe-app phase_6g -- --nocapture`
  EXPECT: complete entry-icon policy suite passes
  EVIDENCE: Complete nine-test Phase 6G suite passes. Unknown executable
  `tool.AppImage` and extensionless executable retain Executable; directory,
  folder-link, file-link, non-UTF-8, ordinary extension, and resources pass.
- [x] X3: Formatting, check, strict Clippy, workspace tests, real GTK component,
  native build, diff hygiene, and documentation gates pass.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
  EXPECT: all applicable gates pass
  EVIDENCE: Formatting, workspace check, strict all-target Clippy, native build,
  397 app tests (395 passed plus two intentional graphical ignores), 132 core
  tests, real GTK component gate, diff hygiene, and persistent docs pass. The
  GTK run emitted only the pre-existing libadwaita host warning.

---

# Gates: Floe post-Phase-14 — File-Type Icon Distinction Regression

Scope: Repair missing or visually collapsed file-type icons in the existing
three-style entry-icon system. No Phase 15 work or unrelated icon redesign.

- [x] R1: PDF and plain-text fixtures classify into distinct semantic families,
  and every reviewed extension family retains a deterministic mapping.
  CHECK: `cargo test -p floe-app post_phase_14_file_type_icon -- --nocapture`
  EXPECT: focused icon taxonomy tests pass
  EVIDENCE: Focused regression test passes `.pdf` as PDF, `.txt`/`.md` as
  Plain Text, `.docx` as Document, case-insensitive reviewed extension mapping,
  and distinct stable names across Floe Color, Phosphor, and System fallbacks.
- [x] R2: Floe Color and Phosphor map every entry family to a registered,
  nonblank app-owned resource; PDF and text resource bytes and resolved
  paintables are distinct.
  CHECK: `cargo test -p floe-app post_phase_14_file_type_icon -- --nocapture`
  EXPECT: focused resource and mapping tests pass
  EVIDENCE: Focused style/resource tests pass all fifteen Floe Color aliases and
  all 44 pinned Phosphor symbolic resources with `currentColor`. Real GTK
  resolves PDF and plain text to different resource files in both styles.
- [x] R3: System Theme lookup has an explicit usable fallback for every family,
  preserves native theme icons when available, and cannot collapse PDF and text
  merely because the host theme lacks or aliases a MIME icon.
  CHECK: `cargo test -p floe-app post_phase_14_file_type_icon -- --nocapture`
  EXPECT: focused fallback-policy tests pass
  EVIDENCE: Every native MIME chain ends in its matching distinct Floe resource;
  PDF, plain-text, and document chains share no app fallback. The real active
  GTK theme resolves PDF and TXT to different files.
- [x] R4: Live style switching clears stale image state and rebinds already
  loaded list, grid, search, and Miller presentations without filesystem work.
  CHECK: `cargo test -p floe-app phase_testing_gtk -- --ignored --nocapture`
  EXPECT: real GTK icon-style component contract passes
  EVIDENCE: Real GTK gate passes app-owned-to-GIcon-to-app-owned storage changes,
  distinct resolved paintables, stale GIcon removal, CSS/action/accessibility
  contracts. Native Wayland action cycle Floe Color -> Phosphor -> System ->
  Floe Color stayed responsive and Quit exited 0.
- [x] R5: Formatting, workspace check, strict Clippy, full tests, native build,
  E2E contract discovery, diff hygiene, and applicable native Wayland smoke pass.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
  EXPECT: all deterministic gates pass; graphical limitations reported exactly
  EVIDENCE: Formatting, workspace check, strict all-target Clippy, native build,
  and workspace tests pass: 396 app tests (394 passed, two intentional GTK
  ignores) plus 132 core tests. Real GTK gate passes. E2E harness contracts pass;
  semantic native E2E skips exactly because Dogtail and pyatspi are unavailable.
  Native Wayland style/liveness/Quit smoke passes with only documented RADV host
  warning. Diff hygiene passes.
- [x] R6: Persistent documentation records the verified regression without
  changing Phase 15 as the sole roadmap NEXT phase.
  CHECK: `node <unlazy-skill-dir>/scripts/gate-check.mjs GATES.md`
  EXPECT: every regression gate checked with concrete evidence
  EVIDENCE: AGENTS, DESIGN, README, ROADMAP, FEATURE_MATRIX, ARCHITECTURE,
  DEVELOPMENT, PLAN, resource attribution, and this ledger record the verified
  fifteen-family/44-Phosphor-resource boundary. Exactly Phase 15 remains NEXT.

---

# Gates: Floe post-Phase-14 — Phosphor Icon System

Scope: app-layer icon resources, three persistent entry-icon styles, and live
virtualized rebinding only. Phase 15 remains untouched.

- [x] P1: Pinned Phosphor Core 2.1.1 Regular subset and exact MIT attribution are
  bundled as GTK symbolic resources; no runtime download or new dependency.
  EVIDENCE: GResource compiles and the focused resource test enumerates exactly
  42 `floe-phosphor-*-symbolic.svg` children, verifies `currentColor`, and retains
  all fourteen original Floe Color resources. Source commit and MIT text are
  recorded under `THIRD_PARTY_LICENSES/Phosphor-Icons.txt`.
- [x] P2: Floe Color, Phosphor Monochrome, and System Theme have stable IDs,
  labels, deterministic semantic mappings, version-12 persistence, and safe
  legacy/invalid fallback.
  CHECK: `cargo test -p floe-app post_phase_14 -- --nocapture`
  EVIDENCE: Three focused tests pass stable style/resource mapping, legacy and
  invalid fallback, all-style round trip, and toast-markup escaping.
- [x] P3: Header/menu/sidebar Floe-owned chrome uses Phosphor resources without
  changing accessible labels, action names, keyboard paths, or non-color cues.
  EVIDENCE: The real GTK component gate passes existing header accessibility and
  action contracts plus Phosphor back-button resource and live CSS-class change.
- [x] P4: Switching style applies immediately from already-loaded entries across
  list/grid/Miller-visible state; thumbnails still replace generic icons and GTK
  performs no filesystem operation.
  EVIDENCE: `icon-style` changes the shared style cell and rebuilds only the
  already-loaded virtualized stores; the existing thumbnail presentation remains
  authoritative. Native D-Bus state changed Floe Color to Phosphor to System and
  back while `Peer.Ping` remained responsive.
- [x] P5: Focused tests, formatting, check, strict Clippy, workspace tests, native
  build, diff hygiene, real GTK component gate, and applicable Wayland lifecycle
  smoke pass with exact evidence recorded.
  EVIDENCE: `cargo fmt --all -- --check`, workspace check, strict all-target
  Clippy, 395 app tests (393 passed plus two intentional graphical ignores), 132
  core tests, native build, diff hygiene, and one real GTK component test pass.
  Native E2E harness contracts pass; semantic workflows are skipped explicitly
  because Python dogtail and pyatspi are unavailable.
- [x] P6: Persistent docs accurately describe the verified feature and keep
  exactly Phase 15 as `NEXT`.
  EVIDENCE: Two-launch isolated Plasma Wayland smoke restored persisted
  `phosphor`, switched live through `system` and back, answered D-Bus Ping, wrote
  private version-12 state, and Quit exited 0 without the prior toast markup
  warning. Only the documented host RADV warning appeared.

---

# Gates: Floe Phase 14 — Generic Desktop Integration Framework

Scope: App-layer standards capability boundary, asynchronous detection,
truthful missing-service fallback, status UI, no compositor/core leakage.

- [x] I1: Stable generic capability model covers URI launch, mounts, XDG,
  portals, notifications, Share, theme, credentials, and session lock without
  compositor or core types.
  CHECK: cargo test -p floe-app phase_14_integration_model -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused model test passes nine stable ordered IDs, complete nonempty text reasons, and conservative Limited session-lock state without any `floe-core` type.

- [x] I2: Detection worker is bounded, generation-safe, path/content-free,
  cleanly shuts down, and reports missing/degraded services as fallback state.
  CHECK: cargo test -p floe-app phase_14_integration_worker -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused worker test passes capacity-one nonblocking submission, generation-tagged response, aggregate-only snapshot, and joined shutdown; GIO calls use 1.5-second timeout.

- [x] I3: Existing generic launcher/device/location/theme behavior routes or
  coexists through the boundary and remains usable with every optional service
  unavailable.
  CHECK: cargo test -p floe-app phase_14_generic_fallback -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Missing-service test keeps compiled GIO launch, volume monitor, XDG folders, and GTK/libadwaita theme facts Available while portal-dependent facts remain explicit Unavailable or Limited.

- [x] I4: Native Desktop Integration command/status dialog is accessible,
  deterministic, non-color-only, refreshable, and makes no unsupported claim.
  CHECK: cargo test -p floe-app phase_14_integration_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Deterministic UI-policy test and opt-in real GTK widget gate pass nine text-labelled rows, nonempty reasons, aggregate summary, Refresh Status accessible description, present/close lifecycle.

- [x] I5: Formatting, workspace check, strict Clippy, complete tests, native
  build, diff hygiene, and real GTK component gate pass.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: Final formatting check, workspace check, strict all-target/all-feature Clippy with warnings denied, 392 app tests (390 passed plus two intentional graphical ignores), 132 core tests, native app build, diff hygiene, and real GTK Phase 14 gate pass.

- [x] I6: Isolated generic/Plasma Wayland D-Bus lifecycle and persistent docs
  mark only Phase 14 complete with exactly Phase 15 NEXT.
  CHECK: rg -n 'Status: \*\*NEXT\*\*|\| .*NEXT' docs/ROADMAP.md
  EXPECT: Phase 15
  EVIDENCE: Isolated Plasma Wayland launch exported enabled desktop-integration-status, activated its real dialog, answered Peer.Ping afterward, and Quit exited cleanly. Standalone AT-SPI bus remained unavailable, so no semantic E2E claim is made. Persistent docs mark Phase 14 complete and exactly Phase 15 NEXT.

---

# Archived gates: Floe Phase 13G — Duplicate Finder

Scope: Explicit bounded size/hash/byte-confirmed duplicate review with hard-link
distinction, cancellation, reveal/Trash handoff, and no automatic deletion.

- [x] G1: Core duplicate scan is exact-path, bounded, cancellable, no-follow,
  same-device, size-first, reviewed-hash, byte-confirmed, and change-safe.
  CHECK: cargo test -p floe-core phase_13g -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Three focused core tests pass explicit/raw roots, same-device no-follow traversal, size-first pruning, injected reviewed 32-byte hash boundary, byte comparison, cancellation, root/file/directory/depth/group/result/logical-byte limits, inaccessible/over-limit/change handling, and digest-collision rejection.

- [x] G2: Hard-link aliases are distinct from independent copies and never
  inflate reclaimable-byte estimates; digest matches alone are not proof.
  CHECK: cargo test -p floe-core phase_13g_duplicate_identity -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused identity test passes one inode/two paths plus one independent copy under forced digest collision: alias is labelled, three paths remain, independent count is two, reclaimable estimate counts one copy only, byte-different collision is excluded.

- [x] G3: Application worker bounds requests/events/results, streams progress,
  cancels by generation, retains outcomes memory-only, and shuts down cleanly.
  CHECK: cargo test -p floe-app phase_13g_duplicate_worker -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Two focused application tests pass capacity-one requests/capacity-32 events, existing reviewed Phase 10E GLib SHA-256 execution, progress/final memory-only outcome, generation cancellation, bounded backpressure, and clean Drop join.

- [x] G4: Selection-aware context/command/native review UI exposes truthful
  cancellation, hard-link labels, reveal, and explicit ordinary Trash handoff.
  CHECK: cargo test -p floe-app phase_13g_duplicate_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused UI contract passes review-first/no-auto-delete/hard-link/Trash wording. Full context and registry suites pass default file-tools submenu plus human command discovery. Controller compiles authoritative selection/running action state, non-modal progress Cancel, exact Reveal, default-off review checkboxes, and ordinary recoverable Trash batch handoff.

- [x] G5: Formatting, workspace check, strict Clippy, complete tests, native
  build, diff hygiene, and real GTK component gate pass.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: Final formatting check, workspace check, strict all-target/all-feature Clippy warnings denied, 387 app tests (386 passed plus one intentional graphical ignore), 132 core tests, native app build, and diff hygiene pass. Real GTK component gate passes.

- [x] G6: Native Wayland action/liveness/Quit smoke and persistent documents
  mark only verified 13G complete with exactly Phase 14 NEXT.
  CHECK: rg -n 'Status: \*\*NEXT\*\*|\| .*NEXT' docs/ROADMAP.md
  EXPECT: Phase 14
  EVIDENCE: Isolated Plasma Wayland launch exports check-duplicates and cancel-duplicate-scan, answers Peer.Ping, and Quit exits 0. Standalone AT-SPI bus unavailable is recorded without semantic E2E claim. AGENTS, DESIGN, ROADMAP, FEATURE_MATRIX, PRIVACY_SECURITY, DEVELOPMENT, PLAN, and GATES mark only verified Phase 13G complete and exactly Phase 14 NEXT.

---

# Archived gates: Floe Phase 13F — Optional Search Indexing

Scope: Explicit private filename/metadata index with conservative eligibility,
stale detection, and mandatory live-search fallback; no content index.

- [x] F1: Core index model/build/codec is bounded, exact-path, no-follow, and
  conservative.
  CHECK: cargo test -p floe-core phase_13f -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Three focused core tests pass hidden-subtree exclusion, exact raw non-UTF-8 names, symlink non-descent, entry/encoded limits, versioned round trip, malformed/hidden/duplicate-policy rejection, and cancellation-safe bounded build.

- [x] F2: Indexed filename queries preserve Phase 13C semantics and detect
  stale roots/directories/entries before presentation.
  CHECK: cargo test -p floe-core phase_13f_index_query -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused indexed-query test passes exact result identity, stale directory rejection after mutation, hidden-policy ineligibility, no-follow matching-entry validation, Phase 13C advanced predicate/MIME boundary, and deterministic result cap.

- [x] F3: Application index worker has bounded build/query/clear persistence,
  private atomic files, generation safety, and explicit fallback outcomes.
  CHECK: cargo test -p floe-app phase_13f_search_index -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Two focused worker tests pass capacity-one commands/capacity-32 events, mode-0600 synchronized atomic cache, query batches, missing/stale fallback with unchanged request, stale-cache discard, clear lifecycle, generation cancellation, and clean Drop join.

- [x] F4: Native controls and controller expose truthful optional capability,
  accessibility, and complete non-indexed fallback without content indexing.
  CHECK: cargo test -p floe-app phase_13f_search_index_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused UI/preference tests pass filenames-metadata-only/hidden/content/fallback wording, version-11 off-by-default opt-in round trip, compact Index menu, worker-only Build/Clear actions, and content-mode hiding. Controller compiles indexed result/fallback generation lifecycle; live Wayland action smoke verifies exported controls.

- [x] F5: Full formatting, check, strict Clippy, workspace tests, build, and
  diff hygiene pass.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: Final cargo fmt check, workspace check, strict all-target/all-feature Clippy with warnings denied, 384 app tests (383 passed plus one intentional graphical ignore), 129 core tests, native app build, and git diff hygiene all pass.

- [x] F6: Native Wayland lifecycle verifies index actions, application health,
  and clean Quit; persistent documents mark only verified Phase 13F complete
  and exactly Phase 13G NEXT.
  CHECK: rg -n '\| .*NEXT' docs/ROADMAP.md
  EXPECT: 13G — Duplicate finder | NEXT
  EVIDENCE: Real GTK component gate passes. Isolated Plasma Wayland launch exports Build/Clear Index, creates 600 private cache, clears it, answers Peer.Ping, and Quit exits 0. Standalone AT-SPI bus unavailable is recorded without semantic E2E claim. AGENTS, DESIGN, ROADMAP, FEATURE_MATRIX, PRIVACY_SECURITY, DEVELOPMENT, PLAN, and GATES mark only verified 13F complete and exactly 13G NEXT.

---

# Archived gates: Floe Phase 13E — Saved Searches

Scope: Explicit versioned saved queries, session-only clearable recents, exact
roots, deterministic result ordering, and no indexing.

- [x] E1: Saved-query and recent-history models are bounded, validated, exact,
    and deterministic.
  CHECK: cargo test -p floe-core phase_13e -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Four focused core tests pass names/IDs/catalog capacity, validation, exact raw roots and every filter field, 32-entry deduplicated/suppressible/clearable recents, complete versioned record round trip, and three deterministic result orders.

- [x] E2: Explicit saved-query persistence is private, versioned, atomic,
    corruption-tolerant, capacity-one, and cleanly shut down.
  CHECK: cargo test -p floe-app phase_13e_saved_search -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused preference test passes version-10 round trip, independent corrupt-record skipping, exact non-UTF-8 roots, existing capacity-one worker path, atomic sibling replacement, 0600 mode, load, and shutdown-backed full suite.

- [x] E3: Native UI can save, list, replay, delete, and clear recent searches
    with accessible non-color-only controls and exact-root behavior.
  CHECK: cargo test -p floe-app phase_13e_saved_search_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Two focused UI/persistence tests pass explicit Save/Delete/Clear labels, session-only disclosure, accessible dropdown/order labels, and exact query persistence. Controller compiles and native actions export for named save, exact-root replay, delete, and clear.

- [x] E4: Filename and content results use deterministic Name/Modified/Size
    ordering without changing exact path identity or content-line stability.
  CHECK: cargo test -p floe-app phase_13e_search_result_order -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Core comparator test and app label contract pass Name ascending, Modified newest, Size largest, deterministic raw-path tie-breaks, and content line-number tie-breaks without filesystem reads.

- [x] E5: Strict workspace gates and isolated native Wayland lifecycle pass.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: Formatting, workspace check, strict all-target/all-feature Clippy, 380 app tests (379 passed plus one intentional graphical ignore), 126 core tests, native build, and diff hygiene pass. Real GTK component gate passes; isolated Plasma Wayland exports new actions, activates Clear Recent, answers Peer.Ping, and Quit exits 0.

- [x] E6: Persistent documents mark only verified Phase 13E complete and set
    exactly Phase 13F NEXT without claiming indexing or duplicate finding.
  CHECK: rg -n '\| .*NEXT' docs/ROADMAP.md
  EXPECT: 13F — Optional indexing | NEXT
  EVIDENCE: AGENTS, DESIGN, ROADMAP, FEATURE_MATRIX, PRIVACY_SECURITY, DEVELOPMENT, PLAN, and this ledger record the verified boundary; exactly Phase 13F is NEXT while indexing and duplicates remain unimplemented.

---

# Archived gates: Floe Phase 13D — Content Search

Scope: Explicit bounded local text-content search with worker-owned reads,
exact paths, truthful limits, and no persistence/indexing.

- [x] D1: Core search is bounded, cancellable, encoding-aware, and path-safe.
  CHECK: cargo test -p floe-core phase_13d -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Five focused core tests pass UTF-8/UTF-16, Text/Glob/Regex/case, advanced predicates before reads, binary/unsupported/over-limit/link skips, cancellation, caps, snippets, and request validation.

- [x] D2: Application worker streams only current generation through bounded
    capacity and shuts down cleanly.
  CHECK: cargo test -p floe-app phase_13d_content_search_worker -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Two focused worker tests pass bounded event delivery, complete summary, stale-generation supersession, and clean shutdown.

- [x] D3: Unified native UI exposes truthful Search Contents controls/results
    and exact-path action reuse.
  CHECK: cargo test -p floe-app phase_13d_content_search_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused UI test passes three plain-language modes and explicit local-read/skip help. Browser integration compiles with line/snippet/folder result rows, exact-entry selection, Reveal, context, navigation, refresh, cancel, and result-status lifecycle.

- [x] D4: Existing Phase 13 behavior and strict workspace gates remain green.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: Formatting, workspace check, strict all-target/all-feature Clippy with warnings denied, 377 app tests (376 passed plus one intentional graphical ignore), 122 core tests, native app build, and diff hygiene pass.

- [x] D5: Real GTK and isolated native Wayland smoke verify accessible controls,
    action/mode lifecycle, D-Bus health, and clean Quit.
  EVIDENCE: Opt-in real GTK widget/accessibility contract passes. Isolated Plasma Wayland launch exports shared search actions, accepts open/clear lifecycle activation, answers Peer.Ping before and after, and Quit exits 0. Standalone AT-SPI bus unavailable is recorded without claiming semantic E2E.

- [x] D6: Persistent documents mark only verified Phase 13D complete and set
    exactly Phase 13E NEXT without claiming saved search or indexing.
  CHECK: rg -n '\| .*NEXT' docs/ROADMAP.md
  EXPECT: 13E — Saved searches | NEXT
  EVIDENCE: AGENTS, DESIGN, ROADMAP, FEATURE_MATRIX, PRIVACY_SECURITY, DEVELOPMENT, PLAN, and this ledger record the verified boundary; saved searches and indexing remain excluded and exactly Phase 13E is NEXT.

---

# Archived gates: Floe Phase 13C — Advanced Filters

Scope: Combined, bounded advanced predicates in both existing search modes with
lazy worker-owned metadata and exact Linux path identity.

- [x] C1: Core predicates are bounded, composable, case-aware, and path-safe.
  CHECK: cargo test -p floe-core phase_13c -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Five focused core tests pass combined
  type/extension/size/date/hidden, lazy MIME/owner facts with unknown exclusion,
  invalid ranges/MIME, raw non-UTF-8 extension identity, case-aware glob search,
  predicate-only hidden search, and cheap-before-MIME resolution. Existing raw-
  name matcher regression also passes.

- [x] C2: Both application workers remain capacity-one and generation-safe.
  CHECK: cargo test -p floe-app phase_13c -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Three focused application/UI tests pass combined lazy owner
  filtering, predicate-only hidden traversal, generation-tagged result delivery,
  and bounded plain-language control policy. Existing queue-pressure/latest-
  generation tests remain green in the full workspace suite.

- [x] C3: Unified native UI exposes accessible, truthful advanced controls.
  CHECK: cargo test -p floe-app advanced_filter -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Deterministic control contract passes visible bounded presets and
  Match Case help. The opt-in graphical GTK component gate passes real roles and
  labels for Filters, five dropdowns, extension/MIME entries, Match Case, Apply,
  and Clear Filters.

- [x] C4: Existing Phase 13 behavior and strict workspace gates remain green.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: `cargo fmt --all -- --check`, workspace check, strict all-target/all-
  feature Clippy with `-D warnings`, native app build, and `git diff --check`
  pass. Final serial workspace suite passes 374 application tests with one
  intentional graphical ignore, 117 core tests, and zero doctest failures. The
  one split-drop fixture failure from the first full run passed focused and on
  the complete rerun; no Phase 13 regression remained.

- [x] C5: Native Wayland smoke verifies controls, mode/action lifecycle,
    responsive D-Bus health, and clean Quit.
  EVIDENCE: Isolated HOME/XDG launch in the active Plasma Wayland session
  exported and accepted folder-filter, filename-search, start/stop, and close-
  search actions; D-Bus Peer.Ping returned `()`, and the application `quit`
  action exited status 0. The graphical GTK gate independently constructed and
  verified the shared advanced controls. Host Python lacks `dogtail`, so AT-SPI
  E2E is not claimed.

- [x] C6: Persistent documents mark only verified Phase 13C complete and set
    exactly Phase 13D NEXT without claiming tags or later phases.
  CHECK: rg -n '\| .*NEXT' docs/ROADMAP.md
  EXPECT: 13D — Content search | NEXT
  EVIDENCE: AGENTS, DESIGN, ROADMAP, FEATURE_MATRIX, PRIVACY_SECURITY,
  DEVELOPMENT, PLAN, and this ledger describe the verified Phase 13C boundary;
  ROADMAP has exactly one NEXT row, Phase 13D. Tags, persistence, content
  search, indexing, remote roots, and result ordering remain excluded.

---

# Archived gates: Floe Phase 13B — Filename Search

* [x] S1: Core traversal is bounded, cancellable, and path-safe.
  CHECK: cargo test -p floe-core phase_13b_filename_search -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Four focused core tests pass for folder/subtree scope, raw
    non-UTF-8 identity, symlink non-descent/root rejection, cancellation,
    128-result batches, caps/truncation, query/root validation, and device
    boundary policy.

* [x] S2: Application streaming worker has bounded generation-safe lifecycle.
  CHECK: cargo test -p floe-app phase_13b_filename_search -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Three focused application tests pass for capacity-1 requests,
    capacity-32 responses, 128/128/44 streaming, generation supersession,
    bounded backpressure, and clean worker shutdown.

* [x] S3: Unified native search UX and exact result actions are verified.
  CHECK: cargo test -p floe-app phase_13b_search_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Three focused UI/registry/feedback tests pass. Native Wayland D-Bus
    described enabled Quick Filter, Search Files, Start, Stop, and Close actions,
    disabled Reveal without selection, activated all non-destructive search
    lifecycle actions, disabled view/zoom actions only while search results owned
    the surface, restored them on Close, retained Peer.Ping liveness, and Quit
    cleanly.

* [x] S4: Supplied Floe application icon is embedded and selected safely.
  CHECK: cargo test -p floe-app phase_13b_application_icon -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused resource test passes the PNG signature under stable
    `io.github.floe.FileManager`; GTK default and application-window icon names
    use that same resource alias.

* [x] S5: Privacy, design, roadmap, matrix, and persistent status are exact.
  CHECK: rg -n '13B.*COMPLETE|13C.*NEXT|Filename search|Search Files' AGENTS.md DESIGN.md docs/ROADMAP.md docs/FEATURE_MATRIX.md docs/PRIVACY_SECURITY.md docs/DEVELOPMENT.md
  EXPECT: Phase 13B
  EVIDENCE: ROADMAP marks Phase 13B COMPLETE and exactly Phase 13C NEXT; matrix,
    privacy, design, development, plan, gates, and AGENTS describe the verified
    memory-only filename boundary and keep Phase 13C/later work excluded.

* [x] S6: Full deterministic and applicable native gates pass.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: Formatting, workspace check, strict all-target Clippy, 371 app plus
    112 core tests, native build, diff hygiene, and the opt-in real GTK
    component/accessibility test pass. Isolated Wayland
    action/health/clean-Quit smoke passes; Dogtail/pyatspi remain unavailable
    and are not claimed.

---

# Archived gates: Permanent Layered Testing Foundation

* [x] T1: Baseline audit is measured and preserved.
  - CHECK: `cargo test --workspace`
  - EXPECT: the pre-change 365 application and 104 core tests remain passing;
    existing test/native-smoke infrastructure is not deleted or rewritten.
  EVIDENCE: Baseline `cargo test --workspace` passed 365 application and 104
    core tests. Final run passes the same 365 application tests plus four new
    core properties; the one new graphical GTK test remains explicitly ignored
    in the headless suite.

* [x] T2: Selective `floe-core` property tests enforce useful invariants.
  - CHECK: `cargo test -p floe-core property_ -- --nocapture`
  - EXPECT: deterministic sort/set preservation, exact arbitrary Linux filename
    identity, bounded filter behavior, and navigation-history invariants pass;
    failures report reproducible proptest seeds.
  EVIDENCE: Four properties pass with 128 generated cases each: arbitrary raw
    Linux filename identity, deterministic sort/multiset preservation,
    non-UTF-8 text filtering, and Back/Forward navigation round trips.

* [x] T3: GTK component/accessibility tests are a real separate graphical gate.
  - CHECK: `cargo test -p floe-app phase_testing_gtk -- --ignored --nocapture`
  - EXPECT: real Floe controls expose stable roles, visible/accessible names or
    descriptions, and action wiring; ordinary headless workspace tests remain
    independent from display availability.
  EVIDENCE: One ignored-by-default real-widget contract passed in the active
    Wayland session. It verifies navigation, hidden, location, folder-filter,
    feedback, progress, cancel, retry, and pause roles/action/help contracts.
    Only the pre-existing host libadwaita GtkSettings warning appeared.

* [x] T4: Native E2E harness is safe, semantic, and explicitly opt-in.
  - CHECK: `python3 -m unittest discover -s e2e -p 'test_*.py'`
  - EXPECT: preflight fails/skips truthfully when Dogtail/AT-SPI/session support
    is unavailable; E2E-01 through E2E-08 are discoverable; every launch uses
    private temporary HOME/XDG/Trash roots, condition waits, and no coordinates
    or fixed timing sleeps.
  EVIDENCE: Python discovery passes three deterministic harness tests covering
    stable E2E-01..08 registration and private temporary HOME/XDG/Trash roots.
    The native workflow class skips truthfully because this host lacks Python
    Dogtail and `pyatspi`; no native E2E execution is claimed.

* [x] T5: Permanent policy and developer instructions cover every layer.
  - CHECK: `rg -n 'Property-based|GTK component|Dogtail|E2E-08|Niri smoke|Plasma smoke|Regression test|Security and privacy testing' AGENTS.md docs/DEVELOPMENT.md`
  - EXPECT: future feature leaves select applicable layers, add failure/regression
    coverage during implementation, run exact gates, preserve isolation, and
    report unavailable native checks without claiming execution.
  EVIDENCE: `AGENTS.md` and `docs/DEVELOPMENT.md` now define unit, tempfile
    filesystem, proptest, GTK/accessibility, Dogtail E2E, isolated Mutter,
    Niri, Plasma, security/privacy, regression, reporting, and CI boundaries.

* [x] T6: Full deterministic repository gates pass.
  - CHECK: `cargo fmt --all -- --check`
  - CHECK: `cargo check --workspace`
  - CHECK: `cargo clippy --workspace --all-targets -- -D warnings`
  - CHECK: `cargo test --workspace`
  - CHECK: `git diff --check`
  - EXPECT: all commands exit zero; no web E2E dependency or unrelated feature
    change is introduced.
  EVIDENCE: Formatting, workspace check, strict all-target Clippy, 473 passing
    Rust tests with one expected ignored graphical test, and diff hygiene all
    pass. Cargo manifests contain no web E2E dependency.

* [x] T7: Final status is truthful and measured.
  - EXPECT: all active T1-T7 gates are checked with concrete evidence, or any
    genuinely unavailable native run is recorded explicitly without a false
    pass claim. `AGENTS.md` names this testing foundation without changing the
    roadmap's Phase 13B recommendation.
  EVIDENCE: Active ledger records seven of seven outcomes with command-backed
    evidence. `AGENTS.md` records the testing foundation and retains Phase 13B
    as the sole roadmap recommendation; native E2E unavailability remains
    explicit rather than being reported as a pass.

---

# Archived gates: Floe Phase 13A — Current-Folder Filter

* [x] G1: Exact-name filter semantics are bounded and path-safe.
  - CHECK: `cargo test -p floe-core phase_13a_filter -- --nocapture`
  - EXPECT: empty/text/glob/regex behavior, invalid patterns, 256-character
    limit, preserved input order, and raw non-UTF-8 matching are deterministic.
  - EVIDENCE: Three focused core tests passed for all modes, invalid syntax,
    256-scalar rejection, Unicode names, and raw non-UTF-8 matching.
* [x] G2: Application filtering is responsive and generation-safe.
  - CHECK: `cargo test -p floe-app phase_13a_filter -- --nocapture`
  - EXPECT: one bounded worker rejects capacity overflow, stale generations do
    not replace newer results, 100,000 loaded entries remain bounded, and
    retained exact-path selection is restored only for visible matches.
  - EVIDENCE: Six focused application tests passed for capacity pressure,
    latest-generation replacement, stale rejection, 100,000 entries, stable
    order, and exact visible-only selection restoration.
* [x] G3: Native UI is discoverable, accessible, and truthful.
  - CHECK: focused UI/action contract tests plus native Wayland smoke.
  - EXPECT: header control and `Ctrl+F` expose the filter; Text/Glob/Regex have
    visible labels; count and invalid-pattern text are non-color-only; Escape
    clears and closes; list/grid/Miller reuse one filtered backing view.
  - EVIDENCE: Registry/UI tests verified Ctrl+F and the three visible modes.
    Native Wayland rendered the row, activated both filter actions, answered
    D-Bus Ping, and Quit cleanly. Post-phase shortcut tests verify bare Space
    is excluded from application accelerators, retained by list/grid/Miller
    file views, and disabled or replaced consistently with the user's effective
    Quick Preview binding. Filter-mode contract tests verify three visible popup
    summaries and mode-specific accessible hover help, including concrete Glob
    wildcard examples.
* [x] G4: Full repository quality gates pass.
  - CHECK: `cargo fmt --all -- --check`
  - CHECK: `cargo check --workspace`
  - CHECK: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - CHECK: `cargo test --workspace`
  - CHECK: `cargo build -p floe-app`
  - EVIDENCE: Formatting, workspace check, strict Clippy, 365 application tests,
    104 core tests, and the native application build passed.
* [x] G5: Documentation and roadmap status are exact.
  - CHECK: `git diff --check`
  - CHECK: `rg -n '\| .*NEXT' docs/ROADMAP.md`
  - EXPECT: Phase 13A is COMPLETE only with G1-G4 evidence; exactly Phase 13B
    is NEXT; Phase 13C and later remain unimplemented.
  - EVIDENCE: Phase documents record the verified boundary with exactly Phase
    13B NEXT.

---

# Archived gates: Floe Phase 12F — Productivity Action Integration

* [x] G1: Context-menu policy is compact, bounded, and archive-aware.
  - CHECK: `cargo test -p floe-app phase_12f_context_menu -- --nocapture`
  - EXPECT: Archives appear by default; fixed essentials cannot disappear;
    optional reviewed groups preserve deterministic order without duplicates.
  - EVIDENCE: Three focused model tests passed; default Archives/Batch rename,
    fixed essentials, compact sections, empty/custom policies, ordering, and
    deduplication were verified.
* [x] G2: Preference migration and customization UI are safe and accessible.
  - CHECK: `cargo test -p floe-app phase_12f_context_preferences -- --nocapture`
  - EXPECT: legacy defaults migrate, unknown/duplicate/over-capacity group IDs are
    rejected or normalized deterministically, and dialog rows expose visible
    names, descriptions, state, reset, apply, and keyboard operation.
  - EVIDENCE: Three focused preference tests passed; legacy/default, explicit
    empty, unknown/duplicate normalization, version-8 round trip, seven labeled
    switch rows, reset/apply, and capacity-one persistence path were verified.
* [x] G3: Command and surface parity remain centralized.
  - CHECK: `cargo test -p floe-app phase_12f_action_integration -- --nocapture`
  - EXPECT: customization is registered for header/palette/shortcuts; file,
    background, list, grid, and Miller surfaces use the same reviewed models and
    existing archive GAction eligibility.
  - EVIDENCE: Two focused registry tests passed; six Phase 12 productivity and
    customization commands are searchable with reviewed header/file/background
    placements, while shared live menu objects serve list/grid/Miller surfaces.
* [x] G4: Full workspace and native Wayland gates pass.
  - CHECK: `cargo fmt --all -- --check`
  - CHECK: `cargo check --workspace`
  - CHECK: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - CHECK: `cargo test --workspace`
  - CHECK: `cargo build -p floe-app`
  - CHECK: `git diff --check`
  - EXPECT: native Wayland smoke opens the customization dialog, exposes archive
    and customization action eligibility, responds to D-Bus Ping, and quits
    cleanly; G1 provides deterministic file/background model verification.
  - EVIDENCE: formatting, workspace check, strict all-target/all-feature Clippy,
    352 app + 101 core tests, native build, and diff hygiene passed. Wayland
    `DescribeAll` exposed customization enabled and archive/batch actions
    selection-disabled, the native seven-row editor rendered aligned/unclipped,
    D-Bus Ping responded, and Quit exited 0. Only documented host libadwaita and
    RADV warnings appeared.
* [x] G5: Persistent phase documents are truthful.
  - CHECK: exactly Phase 12F is COMPLETE and exactly one Phase 13A entry is NEXT
    only after G1-G4 pass.
  - EVIDENCE: `phase-12f-docs-ok`; ROADMAP, FEATURE_MATRIX,
    PRIVACY_SECURITY, DESIGN, README, PLAN, GATES, and AGENTS now describe the
    verified boundary and exactly Phase 13A is NEXT.
