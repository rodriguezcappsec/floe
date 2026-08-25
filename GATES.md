# Gates: Floe Phase 11D — Optional Vim Mode

Scope: Deliver an off-by-default, focus-safe Vim navigation layer for list/grid/Miller views without intercepting text entry or adding terminal behavior.

- [x] G1: The active branch and diff contain only Phase 11D work.
  CHECK: git branch --show-current
  EXPECT: phase-11d-vim-mode
  EVIDENCE: Confirmed `phase-11d-vim-mode`; changes are limited to Vim policy, preference/state/action/view wiring, indicator/menu, tests, and phase ledgers.

- [x] G2: Vim key policy maps reviewed navigation keys, preserves modifiers/native fallthrough, and rejects every editable or dialog focus context.
  CHECK: cargo test -p floe-app phase_11d_vim_policy -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Two focused policy tests passed for h/j/k/l/g/G/o, disabled mode, modifier fallthrough, and non-file-view focus; runtime controllers exist only on list/grid/Miller file views.

- [x] G3: Vim mode is disabled by default and versioned preference migration/round-trip preserve explicit opt-in only.
  CHECK: cargo test -p floe-app phase_11d_vim_preferences -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused test passed version-5 migration to disabled, explicit version-6 true round-trip, and invalid-value fallback; native two-launch restoration retained true in a 0600 preference file.

- [x] G4: Existing list/grid/Miller selection, activation, and parent/child action paths own execution; GTK key handling adds no filesystem work.
  CHECK: cargo test -p floe-app phase_11d_vim_dispatch -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused dispatch test passed selection clamping/empty handling and existing parent/open/Miller-child paths; list/grid select shared model and Miller uses its established dispatchers.

- [x] G5: Registered toggle, header/palette discoverability, visible text indicator, and accessible enabled/disabled state are verified.
  CHECK: cargo test -p floe-app phase_11d_vim_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: UI test passed registered searchable `win.vim-mode`, no forced shortcut, hjkl search metadata, explicit `Vim On`/`Vim Off` text, tooltip, header menu and accessible label wiring.

- [x] G6: Native Wayland smoke verifies opt-in toggle/action state, indicator accessibility, input-safe health, D-Bus ping, clean quit, and name release.
  EVIDENCE: Isolated Niri/Wayland action changed exported boolean state false to true; 0600 version-6 preference recorded true; a second launch restored true. Indicator has code/tested accessible On/Off text. Both launches answered Ping, exited 0, and released the application name; only known RADV warning appeared.

- [x] G7: Formatting, workspace check, strict Clippy, all tests, native build, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check
  EXPECT: Finished
  EVIDENCE: All commands exited 0; 417 tests passed (323 application plus 94 core), zero failed, native application build and diff hygiene passed.

- [x] G8: Documentation marks 11D complete, selects exactly 11E next, and truthfully records opt-in/focus exclusions.
  CHECK: test "$(rg -o '\| NEXT \|' docs/ROADMAP.md | wc -l)" -eq 1 && rg -n "11D.*COMPLETE|11E.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 11E
  EVIDENCE: ROADMAP has exactly one NEXT at 11E; AGENTS, matrix, privacy/security, plan, and gates record default-off state, file-view capture boundary, native input fallthrough, and no key history.
