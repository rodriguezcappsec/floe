# Gates: Floe Phase 13F — Optional Search Indexing

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
