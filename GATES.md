# Gates: Floe Phase 11E — Terminal Integration

Scope: Deliver safe, preferred-provider Open Terminal Here for exact local directories without shells, password argv, GTK filesystem work, or an embedded terminal.

- [x] G1: The active branch and diff contain only Phase 11E work.
  CHECK: git branch --show-current
  EXPECT: phase-11e-terminal-integration
  EVIDENCE: Confirmed `phase-11e-terminal-integration`; the diff is limited to the reviewed terminal provider/worker/UI integration, preference and command wiring, tests, and Phase 11E ledgers.

- [x] G2: Reviewed providers and target policy are bounded, deterministic, selection-aware, exact-path safe, and reject Trash/remote/stale/non-directory contexts.
  CHECK: cargo test -p floe-app phase_11e_terminal_policy -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Two focused policy tests passed for nine reviewed providers, stable persisted IDs, deterministic fallback, selected-folder/current-folder resolution, absolute/local/bounded targets, and rejected Trash, relative, missing, and non-directory contexts.

- [x] G3: Worker discovery/launch uses direct argv plus exact working directory, is bounded/non-blocking, and handles hostile/non-UTF-8 paths without shell interpolation.
  CHECK: cargo test -p floe-app phase_11e_terminal_worker -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Two focused worker tests passed capacity-4 non-blocking submission and direct process spawning in an exact hostile, metacharacter-bearing, non-UTF-8 working directory; no shell or path argument is used.

- [x] G4: Preferred provider is explicit, migrates safely, round-trips through the existing preference worker, and falls back truthfully when unavailable.
  CHECK: cargo test -p floe-app phase_11e_terminal_preferences -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: The focused preferences test passed version-6 migration to Automatic, version-7 reviewed-provider round-trip, invalid-ID fallback, and preservation of existing settings; only the provider ID is persisted.

- [x] G5: Registered selection-aware action, context/header/palette access, chooser, availability, and accessible feedback are verified.
  CHECK: cargo test -p floe-app phase_11e_terminal_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: The focused UI test passed both registered commands, 63-command registry count, file/background/header/palette placements, chooser bounds and labels, live availability, and accessible status wiring.

- [x] G6: Native Wayland smoke verifies exported action/chooser, safe launch or truthful unavailable result, D-Bus health, clean quit, and name release.
  EVIDENCE: Isolated Wayland launch with `PATH=/nonexistent` exported disabled `open-terminal` and enabled `terminal-preferences`; the chooser exposed `Preferred Terminal` and `Automatic (first available)` through AT-SPI, D-Bus Ping succeeded, Quit exited 0, and application-name ownership was released. Only the known host RADV/Vulkan warning appeared.

- [x] G7: Formatting, workspace check, strict Clippy, all tests, native build, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check
  EXPECT: Finished
  EVIDENCE: All commands exited 0; 423 tests passed (329 application plus 94 core), zero failed, strict Clippy, native application build, formatting, workspace check, and diff hygiene passed.

- [x] G8: Documentation marks 11E complete, selects exactly 12A next, and records no-shell/exact-path/credential limits truthfully.
  CHECK: test "$(rg -o '\| NEXT \|' docs/ROADMAP.md | wc -l)" -eq 1 && rg -n "11E.*COMPLETE|12A.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 12A
  EVIDENCE: ROADMAP has exactly one NEXT at 12A; AGENTS, matrix, privacy/security, plan, and gates record Phase 11E completion, exact raw working-directory use, no shell/credential injection, and no terminal history persistence.

## Follow-up gates: Properties context-menu parity

- [x] F1: Concrete file and Trash menu models expose `win.properties` in a
  separated final section while reusing the existing action.
  CHECK: cargo test -p floe-app properties_context_menu -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: The focused concrete-model test passed for both builders and verifies the final section contains exactly one item with action `win.properties`.

- [x] F2: List, grid, and Miller file views continue to share the corrected
  file menu model; Properties remains selection-aware and keyboard reachable.
  CHECK: cargo test -p floe-app phase_10c_properties_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: The Phase 10C discoverability/selection test passed; code inspection confirms list, grid, and Miller all consume `build_file_context_menu_model`, and keyboard context-menu dispatch opens the active view's same model.

- [x] F3: Formatting, strict lint, tests, build, diff hygiene, documentation,
  and the sole Phase 12A `NEXT` remain verified.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check && test "$(rg -o '\| NEXT \|' docs/ROADMAP.md | wc -l)" -eq 1
  EXPECT: all commands exit 0
  EVIDENCE: All commands exited 0; 424 tests passed (330 application plus 94 core), strict Clippy, native build, formatting, workspace check, diff hygiene, documentation, and exactly one NEXT at Phase 12A passed.
