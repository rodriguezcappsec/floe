# Gates: Floe Phase 11A — Command Registry

- [x] G1: Correct phase branch and bounded Phase 11A-only scope.
  CHECK: git branch --show-current
  EXPECT: phase-11a-command-registry
  EVIDENCE: Branch confirmed; no palette, shortcut customization, Vim mode, terminal launching, or unrelated menu redesign was added.

- [x] G2: Registry metadata has unique stable IDs/actions, non-empty human names/descriptions, deterministic categories/order, and bounded search terms.
  CHECK: cargo test -p floe-app phase_11a_registry_contract -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Passed for 59 unique `win.*` commands, unique names, non-empty descriptions/categories, deterministic static order, and at most eight non-empty search terms.

- [x] G3: Registry invokes no business logic; it resolves eligibility solely from authoritative GAction presence/enabled state and reports unavailable actions explicitly.
  CHECK: cargo test -p floe-app phase_11a_registry_eligibility -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Passed enabled, disabled, and missing live-action transitions through SimpleActionGroup; registry definitions contain metadata only.

- [x] G4: Existing default window shortcuts are sourced from registry metadata without changing bindings or making irreversible actions easier to trigger.
  CHECK: cargo test -p floe-app phase_11a_registry_shortcuts -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Passed existing navigation, refresh, preview, split, file, tab, and view bindings; permanent delete retains Shift+Delete plus confirmation and Empty Trash gains no accelerator.

- [x] G5: Menu/context actions have registry placement metadata and public registered-action parity; parameterized/internal plumbing is explicitly excluded.
  CHECK: cargo test -p floe-app phase_11a_registry_parity -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Passed every file, Trash, and background context action against its registry placement; parameterized tab IDs, stateful column/density controls, and widget plumbing remain excluded.

- [x] G6: Native Wayland smoke verifies registry/action parity, enabled-state changes, shortcut metadata, accessibility, D-Bus responsiveness, clean quit, and name release.
  EVIDENCE: Niri/Wayland startup logged `commands=59 disabled=31` with zero missing actions; D-Bus exposed the existing window action set with Open disabled and Refresh enabled in no-selection context, Ping returned, AT-SPI exposed accessible ID `io.github.floe.FileManager`, Quit exited cleanly, and the bus name was released.

- [x] G7: Formatting, workspace check, strict Clippy, full tests, native build, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check
  EXPECT: all commands exit 0
  EVIDENCE: Passed; 403 tests total (309 app, 94 core), zero failures, native app build succeeded, and diff hygiene is clean.

- [x] G8: Documentation marks 11A complete, sets exactly 11B next, and excludes palette/custom-keybinding/terminal work.
  CHECK: test "$(rg -o '\| NEXT \|' docs/ROADMAP.md | wc -l)" -eq 1 && rg -n "11A.*COMPLETE|11B.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 11B is the sole next phase.
  EVIDENCE: Roadmap has exactly one NEXT row at 11B; AGENTS, feature matrix, plan, and gates document the bounded 11A implementation and exclusions.
