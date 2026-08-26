# Gates: Permanent Layered Testing Foundation

- [x] T1: Baseline audit is measured and preserved.
  - CHECK: `cargo test --workspace`
  - EXPECT: the pre-change 365 application and 104 core tests remain passing;
    existing test/native-smoke infrastructure is not deleted or rewritten.
  EVIDENCE: Baseline `cargo test --workspace` passed 365 application and 104
    core tests. Final run passes the same 365 application tests plus four new
    core properties; the one new graphical GTK test remains explicitly ignored
    in the headless suite.

- [x] T2: Selective `floe-core` property tests enforce useful invariants.
  - CHECK: `cargo test -p floe-core property_ -- --nocapture`
  - EXPECT: deterministic sort/set preservation, exact arbitrary Linux filename
    identity, bounded filter behavior, and navigation-history invariants pass;
    failures report reproducible proptest seeds.
  EVIDENCE: Four properties pass with 128 generated cases each: arbitrary raw
    Linux filename identity, deterministic sort/multiset preservation,
    non-UTF-8 text filtering, and Back/Forward navigation round trips.

- [x] T3: GTK component/accessibility tests are a real separate graphical gate.
  - CHECK: `cargo test -p floe-app phase_testing_gtk -- --ignored --nocapture`
  - EXPECT: real Floe controls expose stable roles, visible/accessible names or
    descriptions, and action wiring; ordinary headless workspace tests remain
    independent from display availability.
  EVIDENCE: One ignored-by-default real-widget contract passed in the active
    Wayland session. It verifies navigation, hidden, location, folder-filter,
    feedback, progress, cancel, retry, and pause roles/action/help contracts.
    Only the pre-existing host libadwaita GtkSettings warning appeared.

- [x] T4: Native E2E harness is safe, semantic, and explicitly opt-in.
  - CHECK: `python3 -m unittest discover -s e2e -p 'test_*.py'`
  - EXPECT: preflight fails/skips truthfully when Dogtail/AT-SPI/session support
    is unavailable; E2E-01 through E2E-08 are discoverable; every launch uses
    private temporary HOME/XDG/Trash roots, condition waits, and no coordinates
    or fixed timing sleeps.
  EVIDENCE: Python discovery passes three deterministic harness tests covering
    stable E2E-01..08 registration and private temporary HOME/XDG/Trash roots.
    The native workflow class skips truthfully because this host lacks Python
    Dogtail and `pyatspi`; no native E2E execution is claimed.

- [x] T5: Permanent policy and developer instructions cover every layer.
  - CHECK: `rg -n 'Property-based|GTK component|Dogtail|E2E-08|Niri smoke|Plasma smoke|Regression test|Security and privacy testing' AGENTS.md docs/DEVELOPMENT.md`
  - EXPECT: future feature leaves select applicable layers, add failure/regression
    coverage during implementation, run exact gates, preserve isolation, and
    report unavailable native checks without claiming execution.
  EVIDENCE: `AGENTS.md` and `docs/DEVELOPMENT.md` now define unit, tempfile
    filesystem, proptest, GTK/accessibility, Dogtail E2E, isolated Mutter,
    Niri, Plasma, security/privacy, regression, reporting, and CI boundaries.

- [x] T6: Full deterministic repository gates pass.
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

- [x] T7: Final status is truthful and measured.
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
