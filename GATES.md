# Gates: Floe Phase 5D Open With and file associations

Scope: Add GIO-backed Open With interaction on branch `phase-5d-open-with`. Eligible selected files can asynchronously resolve content type, current default, and capable applications; a native accessible dialog launches the chosen app and can explicitly set it as default. Original paths remain intact. Direct shell execution, custom external tools, directory associations, and filesystem mutation callbacks remain out of scope.

- [x] G1: Branch is `phase-5d-open-with` and `main` remains at Phase 5C commit.
  CHECK: git branch --show-current && git rev-parse main
  EXPECT: /phase-5d-open-with[\s\S]*440aa0c/
  EVIDENCE: branch is `phase-5d-open-with`; local main remains `440aa0c`.

- [x] G2: Association discovery is asynchronous, GIO-backed, preserves the original path, and returns structured current-default/application data.
  CHECK: rg -n "query_info_future|standard::content-type|recommended_for_type|default_for_type|OpenWith" crates/app/src
  EXPECT: /query_info_future/
  EVIDENCE: `discover_open_with` takes `PathBuf`, awaits `query_info_future`, resolves content type/default/recommended/all URI-capable apps, and returns `OpenWithOptions`.

- [x] G3: The context menu exposes selection-sensitive Open With without altering default Open or adding shell commands.
  CHECK: rg -n "Open With|win\.open-with|open_with|Command::new|sh -c" crates/app/src
  EXPECT: /win\.open-with/
  EVIDENCE: `win.open-with` is enabled only for regular files and non-directory links; default Open remains unchanged and repository search finds no shell execution.

- [x] G4: The chooser has clear loading, empty, error, default, launch, cancel, and explicit set-default states with native keyboard focus.
  CHECK: rg -n "Loading applications|No compatible applications|Set as Default|Current default|Could not|Cancel|Open" crates/app/src/ui.rs crates/app/src/browser.rs
  EXPECT: /Set as Default/
  EVIDENCE: immediate loading status precedes a focused native dialog with current-default text, selectable boxed list, Cancel/Open/Set as Default, empty-result toast, and specific recovery errors.

- [x] G5: Launch and default-association changes use GIO `AppInfo` APIs, launch asynchronously, and surface failures.
  CHECK: rg -n "launch|set_as_default_for_type|spawn_local|add_toast" crates/app/src/launcher.rs crates/app/src/browser.rs
  EXPECT: /set_as_default_for_type/
  EVIDENCE: chosen apps launch through `launch_uris_async`; explicit association changes call `set_as_default_for_type`; both error paths log and toast.

- [x] G6: Focused Phase 5D tests cover eligibility, application ordering/deduplication, and selection/default state rules.
  CHECK: cargo test -p floe-app phase_5d -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: 3 focused tests pass for launchable entry kinds, default-first deduplicated application ordering, and chooser/default button sensitivity.

- [x] G7: Documentation and project status describe behavior, limits, verification, and the next coherent branch.
  CHECK: rg -n "Phase 5D|Open With|file association|external tools|phase-5e-conflict" README.md DESIGN.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /phase-5e-conflict/
  EVIDENCE: README, DESIGN, architecture, development, roadmap, and AGENTS status describe Phase 5D, defer external tools, and name `phase-5e-conflict-foundation`.

- [x] G8: Formatting, workspace compilation, strict Clippy, all tests, diff hygiene, and native Wayland smoke pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: fmt, workspace check, strict Clippy, all 75 tests (28 core, 47 app), and diff hygiene passed; native launch logged startup and stayed healthy until timeout.
