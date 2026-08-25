# Gates: Floe Phase 11C — Keybindings

Scope: Deliver bounded, persistent, conflict-safe shortcut customization and complete native shortcut discovery without adding Vim or terminal behavior.

- [x] G1: The active branch and diff contain only Phase 11C work.
  CHECK: git branch --show-current
  EXPECT: phase-11c-keybindings
  EVIDENCE: Confirmed `phase-11c-keybindings`; implementation and documentation are limited to shortcut model, preferences, native dialog, action/menu wiring, and phase ledgers.

- [x] G2: Override parsing, canonicalization, bounds, migration, conflict detection, reset-one/reset-all, and conservative risk guardrails are verified.
  CHECK: cargo test -p floe-app phase_11c_keybinding_model -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Two focused model tests passed, including malformed/hostile records, four-binding and 64-override bounds, exact conflicts, typing-key rejection, protected destructive actions, and resets.

- [x] G3: Preferences round-trip validated effective shortcuts through the existing bounded worker while legacy files retain defaults.
  CHECK: cargo test -p floe-app phase_11c_keybinding_preferences -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused migration/round-trip test passed for version-4 defaults, version-5 custom and disabled bindings; full existing capacity-one worker persistence tests also passed.

- [x] G4: The native Keyboard Shortcuts surface discovers every registered command and supports edit, conflict feedback, individual reset, and reset-all through existing GActions.
  CHECK: cargo test -p floe-app phase_11c_keybinding_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: UI contract test passed for all 60 bounded registry entries, 128-character search, registered action, native editor controls, accessible labels/status, custom/default and unavailable context.

- [x] G5: Existing default accelerators remain unchanged when no override exists, and effective accelerators update without duplicating command eligibility logic.
  CHECK: cargo test -p floe-app phase_11c_effective_accelerators -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused test preserved Back and Refresh defaults; runtime installer derives only effective registry metadata while existing live GActions remain execution/eligibility authority.

- [x] G6: Native Wayland smoke verifies dialog activation, searchable shortcut discovery, accessibility, accelerator update, D-Bus health, and clean quit.
  EVIDENCE: Isolated Niri/Wayland run exported `keyboard-shortcuts`, D-Bus activation opened the native dialog, AT-SPI exposed `Keyboard Shortcuts`, Peer.Ping stayed healthy, Quit exited 0, and application-name ownership became false; only known RADV/Vulkan warnings appeared. Unit/persistence tests verify effective accelerator updates.

- [x] G7: Formatting, workspace check, strict Clippy, all tests, native build, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check
  EXPECT: Finished
  EVIDENCE: All commands exited 0; 412 tests passed (318 application plus 94 core), zero failed, native application build and diff hygiene passed.

- [x] G8: Persistent documentation marks 11C complete, selects exactly 11D next, and records shortcut persistence and risk limits truthfully.
  CHECK: test "$(rg -o '\| NEXT \|' docs/ROADMAP.md | wc -l)" -eq 1 && rg -n "11C.*COMPLETE|11D.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 11D
  EVIDENCE: ROADMAP has exactly one NEXT at 11D; AGENTS, matrix, privacy/security, plan, and gates record bounded persistence, no usage/path history, and protected destructive bindings.
