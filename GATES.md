# Gates: Floe Phase 13C — Advanced Filters

Scope: Combined, bounded advanced predicates in both existing search modes with
lazy worker-owned metadata and exact Linux path identity.

- [ ] G1: Core predicates are bounded, composable, case-aware, and path-safe.
  CHECK: `cargo test -p floe-core phase_13c -- --nocapture`
  EXPECT: `test result: ok`
  EVIDENCE: pending

- [ ] G2: Quick Filter and Search Files workers resolve only required metadata and
  remain capacity-one, cancellable, generation-safe, and bounded.
  CHECK: `cargo test -p floe-app phase_13c -- --nocapture`
  EXPECT: `test result: ok`
  EVIDENCE: pending

- [ ] G3: The unified native UI exposes accessible advanced controls with truthful
  validation, hidden policy, Apply/Clear behavior, and empty-query support only
  when predicates are active.
  CHECK: `cargo test -p floe-app advanced_filter -- --nocapture`
  EXPECT: `test result: ok`
  EVIDENCE: pending

- [ ] G4: Existing Phase 13 behavior and full workspace quality gates pass.
  CHECK: `cargo fmt --all -- --check`
  CHECK: `cargo check --workspace`
  CHECK: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  CHECK: `cargo test --workspace -- --test-threads=1`
  CHECK: `cargo build -p floe-app`
  CHECK: `git diff --check`
  EXPECT: every command exits 0
  EVIDENCE: pending

- [ ] G5: Native Wayland smoke verifies visible controls, mode switching, action
  lifecycle, responsive D-Bus health, and clean Quit.
  EVIDENCE: pending

- [ ] G6: Persistent documents truthfully mark only verified Phase 13C complete
  and set exactly Phase 13D as `NEXT` without claiming tags or later phases.
  CHECK: `rg -n '\| .*NEXT' docs/ROADMAP.md`
  EXPECT: `13D — Content search | NEXT`
  EVIDENCE: pending

---

# Archived gates: Floe Post-13B — Unified Search Surface

- [x] U1: One search surface owns both discoverable modes.
  - CHECK: `cargo test -p floe-app unified_search -- --nocapture`
  - EXPECT: one header search control, visible Quick Filter/Search Files selector, and
    shared query field with mode-appropriate controls and plain-language help pass.
  - EVIDENCE: Focused unified-search test passed. Native Wayland captures verified one aligned two-row surface and one header button: Quick Filter showed Text/Glob/Regex and count; Search Files showed scope/Search/Stop/status, both with one query and Close.
- [x] U2: Keyboard/action behavior is compatible and unambiguous.
  - CHECK: `cargo test -p floe-app phase_13_search_shortcuts -- --nocapture`
  - EXPECT: Ctrl+F opens Quick Filter; Ctrl+Shift+F opens Search Files in the same
    surface; searchable commands explain the direct-mode behavior; close/Escape and
    mode switching cancel incompatible work without stale presentation.
  - EVIDENCE: Two shortcut/registry tests passed. Live D-Bus activated folder-filter, filename-search, repeated cross-mode transitions, and close-search-surface. Deferred entry focus eliminated the synchronous GtkText focus warning.
- [x] U3: Existing Phase 13A/13B engine safety and exact-path behavior are preserved.
  - CHECK: `cargo test -p floe-core phase_13 -- --nocapture`
  - CHECK: `cargo test -p floe-app phase_13 -- --nocapture`
  - EXPECT: bounded filtering/traversal, cancellation, raw non-UTF-8 identity, exact
    reveal, result caps, and generation rejection remain green.
  - EVIDENCE: Seven focused core Phase 13 tests and twelve focused application Phase 13 tests passed, covering bounded workers, cancellation, raw names, caps, stale rejection, feedback, shortcuts, and Reveal contracts.
- [x] U4: Full quality and native lifecycle gates pass.
  - CHECK: `cargo fmt --all -- --check`
  - CHECK: `cargo check --workspace`
  - CHECK: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - CHECK: `cargo test --workspace -- --test-threads=1`
  - CHECK: `cargo build -p floe-app`
  - CHECK: `git diff --check`
  - EXPECT: native Wayland smoke exposes and activates both entry actions through one
    surface, responds to D-Bus health, and exits cleanly.
  - EVIDENCE: Formatting, workspace check, strict all-target/all-feature Clippy, 372 application tests, 108 core tests, doc tests, native build, and diff hygiene passed. Two isolated native Wayland runs rendered both modes, exported/activated search actions, answered Peer.Ping, and Quit exited 0; only host RADV and unavailable accessibility-bus warnings remained.
- [x] U5: Persistent documentation describes the unified model truthfully.
  - CHECK: `rg -n '\| .*NEXT' docs/ROADMAP.md`
  - EXPECT: AGENTS, DESIGN, README, roadmap, feature matrix, plan, and gates describe
    one search surface; exactly Phase 13C remains NEXT and no future phase is claimed.
  - EVIDENCE: AGENTS, DESIGN, README, ROADMAP, FEATURE_MATRIX, PRIVACY_SECURITY, DEVELOPMENT, PLAN, and GATES describe the unified surface and unchanged privacy/engine boundary. Exactly Phase 13C remains NEXT.

---

# Archived gates: Floe Phase 13B — Filename Search

- [x] G1: Core filename traversal is bounded, cancellable, and path-safe.
  - CHECK: `cargo test -p floe-core phase_13b_filename_search -- --nocapture`
  - EXPECT: current/subtree scopes, Unicode/raw non-UTF-8 matching, cancellation,
    inaccessible children, no symlink traversal, no mount crossing, query/depth/
    entry/directory/result caps, explicit truncation, and exact paths pass.
  - EVIDENCE: Four focused core tests passed for current-folder/subtree scopes,
    raw non-UTF-8 names, no symlink descent, cancellation, root/query/limit
    validation, explicit caps, and streamed summaries.
- [x] G2: Application streaming remains bounded and generation-safe.
  - CHECK: `cargo test -p floe-app phase_13b_filename_search -- --nocapture`
  - EXPECT: capacity-one requests, bounded streamed responses, superseding
    generations, stale rejection, cancellation, final summaries, and worker
    shutdown pass without GTK or unbounded queues.
  - EVIDENCE: Three worker tests passed for fixed queue capacities, batches,
    terminal summaries, and stale generation supersession. Browser/registry
    tests passed for status semantics, shortcut discovery, and Reveal mapping.
- [x] G3: Search UX is native, accessible, and exact-path action capable.
  - CHECK: focused Phase 13B UI/action tests plus native Wayland smoke
  - EXPECT: `Ctrl+Shift+F`, visible scope/Search/Stop/Close, filename and
    containing-folder rows, streamed count/skipped/truncated feedback, normal
    actions, exact Reveal in Folder, filter/search separation, and clean
    cancellation/location exit are verified.
  - EVIDENCE: Focused UI/action contracts and the full 371-test application
    suite passed. Isolated native Wayland D-Bus exposed enabled Search/Start/
    Stop/Close actions and selection-disabled Reveal; all lifecycle actions,
    Peer.Ping, and clean Quit succeeded. The host lacked a Wayland input
    injector and its screenshot tool captured another compositor workspace, so
    automated typed-query/pixel claims are intentionally excluded.
- [x] G4: Floe uses the supplied application icon safely.
  - CHECK: focused icon resource contract test plus native Wayland smoke
  - EXPECT: the RGBA PNG is embedded under the stable application icon name,
    GTK resolves it from Floe's resource path, and the default/window icon name
    is `io.github.floe.FileManager` without changing entry icons.
  - EVIDENCE: The 1254×1254 RGBA source was visually inspected, embedded under
    the stable application-ID resource path, resolved by the focused GResource
    PNG-signature contract, registered with GTK's icon theme, selected on the
    native window, and launched successfully in the isolated Wayland smoke.
- [x] G5: Full repository gates pass.
  - CHECK: `cargo fmt --all -- --check`
  - CHECK: `cargo check --workspace`
  - CHECK: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - CHECK: `cargo test --workspace -- --test-threads=1`
  - CHECK: `cargo build -p floe-app`
  - CHECK: `git diff --check`
  - EVIDENCE: Formatting, workspace check, strict all-target/all-feature Clippy,
    371 application tests, 108 core tests, doc tests, native application build,
    and `git diff --check` all passed.
- [x] G6: Documentation and roadmap status are truthful.
  - CHECK: `rg -n '\| .*NEXT' docs/ROADMAP.md`
  - EXPECT: Phase 13B becomes COMPLETE only after G1-G5 pass; exactly Phase 13C
    is NEXT; Phases 13C-13F remain unimplemented.
  - EVIDENCE: AGENTS, plan/gates, roadmap, feature matrix, design, development,
    and privacy/security documents record the verified Phase 13B boundary.
    Exactly Phase 13C is NEXT; Phases 13C-13F remain unimplemented.

---

# Archived gates: Floe Phase 13A — Current-Folder Filter

- [x] G1: Exact-name filter semantics are bounded and path-safe.
  - CHECK: `cargo test -p floe-core phase_13a_filter -- --nocapture`
  - EXPECT: empty/text/glob/regex behavior, invalid patterns, 256-character
    limit, preserved input order, and raw non-UTF-8 matching are deterministic.
  - EVIDENCE: Three focused core tests passed for all modes, invalid syntax,
    256-scalar rejection, Unicode names, and raw non-UTF-8 matching.
- [x] G2: Application filtering is responsive and generation-safe.
  - CHECK: `cargo test -p floe-app phase_13a_filter -- --nocapture`
  - EXPECT: one bounded worker rejects capacity overflow, stale generations do
    not replace newer results, 100,000 loaded entries remain bounded, and
    retained exact-path selection is restored only for visible matches.
  - EVIDENCE: Six focused application tests passed for capacity pressure,
    latest-generation replacement, stale rejection, 100,000 entries, stable
    order, and exact visible-only selection restoration.
- [x] G3: Native UI is discoverable, accessible, and truthful.
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
- [x] G4: Full repository quality gates pass.
  - CHECK: `cargo fmt --all -- --check`
  - CHECK: `cargo check --workspace`
  - CHECK: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - CHECK: `cargo test --workspace`
  - CHECK: `cargo build -p floe-app`
  - EVIDENCE: Formatting, workspace check, strict Clippy, 365 application tests,
    104 core tests, and the native application build passed.
- [x] G5: Documentation and roadmap status are exact.
  - CHECK: `git diff --check`
  - CHECK: `rg -n '\| .*NEXT' docs/ROADMAP.md`
  - EXPECT: Phase 13A is COMPLETE only with G1-G4 evidence; exactly Phase 13B
    is NEXT; Phase 13C and later remain unimplemented.
  - EVIDENCE: Phase documents record the verified boundary with exactly Phase
    13B NEXT.

---

# Archived gates: Floe Phase 12F — Productivity Action Integration

- [x] G1: Context-menu policy is compact, bounded, and archive-aware.
  - CHECK: `cargo test -p floe-app phase_12f_context_menu -- --nocapture`
  - EXPECT: Archives appear by default; fixed essentials cannot disappear;
    optional reviewed groups preserve deterministic order without duplicates.
  - EVIDENCE: Three focused model tests passed; default Archives/Batch rename,
    fixed essentials, compact sections, empty/custom policies, ordering, and
    deduplication were verified.
- [x] G2: Preference migration and customization UI are safe and accessible.
  - CHECK: `cargo test -p floe-app phase_12f_context_preferences -- --nocapture`
  - EXPECT: legacy defaults migrate, unknown/duplicate/over-capacity group IDs are
    rejected or normalized deterministically, and dialog rows expose visible
    names, descriptions, state, reset, apply, and keyboard operation.
  - EVIDENCE: Three focused preference tests passed; legacy/default, explicit
    empty, unknown/duplicate normalization, version-8 round trip, seven labeled
    switch rows, reset/apply, and capacity-one persistence path were verified.
- [x] G3: Command and surface parity remain centralized.
  - CHECK: `cargo test -p floe-app phase_12f_action_integration -- --nocapture`
  - EXPECT: customization is registered for header/palette/shortcuts; file,
    background, list, grid, and Miller surfaces use the same reviewed models and
    existing archive GAction eligibility.
  - EVIDENCE: Two focused registry tests passed; six Phase 12 productivity and
    customization commands are searchable with reviewed header/file/background
    placements, while shared live menu objects serve list/grid/Miller surfaces.
- [x] G4: Full workspace and native Wayland gates pass.
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
- [x] G5: Persistent phase documents are truthful.
  - CHECK: exactly Phase 12F is COMPLETE and exactly one Phase 13A entry is NEXT
    only after G1-G4 pass.
  - EVIDENCE: `phase-12f-docs-ok`; ROADMAP, FEATURE_MATRIX,
    PRIVACY_SECURITY, DESIGN, README, PLAN, GATES, and AGENTS now describe the
    verified boundary and exactly Phase 13A is NEXT.
