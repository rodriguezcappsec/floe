# Gates: Floe Phase 5C context menu

Scope: Add a native selection-aware context menu on branch `phase-5c-context-menu`. Secondary-click selects the exact pointer-targeted row before showing Open, Copy, Cut, Rename, and Move to Trash. The menu reuses existing `win.*` actions, preserves original `DirectoryEntry` paths, and has a list-focused keyboard route. Open With, file-association management, external tools, and new filesystem execution paths remain deferred.

- [x] G1: Branch is `phase-5c-context-menu` and `main` remains at Phase 5B commit.
  CHECK: git branch --show-current && git rev-parse main
  EXPECT: /phase-5c-context-menu[\s\S]*6ceb27b/
  EVIDENCE: branch is `phase-5c-context-menu`; local main remains `6ceb27b`.

- [x] G2: Secondary-click selects the pointer-targeted virtualized list item before opening a native GTK popover menu.
  CHECK: rg -n "BUTTON_SECONDARY|set_selected|PopoverMenu|set_pointing_to|popup" crates/app/src/ui.rs
  EXPECT: /BUTTON_SECONDARY/
  EVIDENCE: row setup uses a secondary-button `GestureClick`, validates `ListItem::position`, calls `SingleSelection::set_selected`, anchors the popover to the event point, then calls `popup`.

- [x] G3: Context items reuse the existing `win.open`, `win.copy`, `win.cut`, `win.rename`, and `win.trash` actions without filesystem work in UI callbacks.
  CHECK: rg -n "win\.open|win\.copy|win\.cut|win\.rename|win\.trash|FILE_CONTEXT" crates/app/src/ui.rs
  EXPECT: /FILE_CONTEXT/
  EVIDENCE: `FILE_CONTEXT_ACTIONS` contains exactly the five existing window actions; the row callback only selects and presents the popover.

- [x] G4: The context menu has a list-focused keyboard route and native action sensitivity/focus behavior.
  CHECK: rg -n "context_menu|Shift|F10|Menu|key_pressed" crates/app/src/browser.rs crates/app/src/ui.rs
  EXPECT: /F10/
  EVIDENCE: the list-owned key controller handles Menu and Shift+F10, ignores lock state, rejects extra command modifiers, and existing `GSimpleAction` sensitivity drives native menu state.

- [x] G5: Focused Phase 5C tests verify the complete human-readable action mapping and selection-safe position validation.
  CHECK: cargo test -p floe-app phase_5c -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: all 3 `phase_5c` tests passed: action mapping, invalid virtualized position, and keyboard modifier handling.

- [x] G6: Documentation and project status describe Phase 5C behavior, limits, verification, and the next coherent phase.
  CHECK: rg -n "Phase 5C|context menu|Open With|external tools|Recommended next" README.md DESIGN.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /Phase 5C/
  EVIDENCE: README, DESIGN, architecture, development, roadmap, and AGENTS status document Phase 5C and name `phase-5d-open-with`; Open With and external-tool boundaries are explicit.

- [x] G7: Formatting, workspace compilation, strict Clippy, complete tests, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: fmt, workspace check, strict Clippy, all 72 tests (28 core, 44 app), and `git diff --check` passed.

- [x] G8: Native Wayland launch emits startup and remains healthy until timeout.
  CHECK: RUST_LOG=floe_app=info timeout 8s cargo run -p floe-app
  EXPECT: /Floe application started/
  EVIDENCE: clean current-binary Wayland launch logged `Floe application started` and remained healthy until timeout exit 124; only known host libadwaita/RADV warnings appeared.
