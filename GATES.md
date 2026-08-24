# Gates: Floe Phase 6I Open With fallback

Scope: Make normal Open fall back to Floe's asynchronous application chooser when GIO has no default handler, without changing associations implicitly.

- [x] G1: Work is isolated on `phase-6i-open-with-fallback` from completed Phase 6H commit `7bd6ab0`.
  CHECK: git branch --show-current && git merge-base --is-ancestor 7bd6ab0 HEAD && git rev-parse --short 7bd6ab0
  EXPECT: /phase-6i-open-with-fallback[\s\S]*7bd6ab0/
  EVIDENCE: Branch and ancestor check identify `phase-6i-open-with-fallback` from `7bd6ab0`.

- [x] G2: Default launch resolution returns a distinct no-default outcome rather than flattening it into an opaque launch error.
  CHECK: rg -n 'DefaultLaunch|NoDefault|launch_default|default_for_type' crates/app/src/launcher.rs
  EXPECT: /NoDefault/
  EVIDENCE: `DefaultLaunch::{Launched, NoDefault(OpenWithOptions)}` preserves the chooser-ready outcome.

- [x] G3: Normal Open launches the registered default when present and automatically opens the existing chooser when no default exists.
  CHECK: rg -n 'launch_file|show_open_with|NoDefault|Open With' crates/app/src/browser.rs
  EXPECT: /NoDefault/
  EVIDENCE: `launch_file` handles `NoDefault` with `present_or_report_open_with` and keeps normal launch silent on success.

- [x] G4: Chooser discovery and launch remain asynchronous GIO operations retaining the original `PathBuf`; GTK callbacks do not perform MIME or filesystem work.
  CHECK: rg -n 'spawn_future_local|discover_open_with|PathBuf|gio::File|launch_uris' crates/app/src/launcher.rs crates/app/src/browser.rs
  EXPECT: /discover_open_with/
  EVIDENCE: GIO query runs in a GLib local future and async launch receives the original path through `gio::File` URI conversion.

- [x] G5: One-time Open and explicit Set as Default remain separate actions; absence of a default never changes associations automatically.
  CHECK: rg -n 'set_as_default_for_type|set_default_button|open_button|is_default' crates/app/src/launcher.rs crates/app/src/ui.rs crates/app/src/browser.rs
  EXPECT: /set_as_default_for_type/
  EVIDENCE: Only the chooser's explicit Set as Default button calls `set_as_default_for_type`; fallback Open never does.

- [x] G6: Focused Phase 6I tests cover registered-default and no-default routing with exact path retention.
  CHECK: cargo test -p floe-app phase_6i -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Two focused tests passed for registered-default and chooser routes.

- [x] G7: Persistent documentation describes Phase 6I and names `phase-6j-places-and-devices` next.
  CHECK: rg -n 'Phase 6I|phase-6j-places-and-devices|no default' README.md DESIGN.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /phase-6j-places-and-devices/
  EVIDENCE: README, design, architecture, development, roadmap, and AGENTS contain Phase 6I and next-branch guidance.

- [x] G8: Formatting, workspace compilation, strict Clippy, all tests, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: Formatting, workspace check, strict Clippy, 108 app tests, 33 core tests, doc tests, and diff hygiene passed.

- [x] G9: Native Wayland smoke confirms Floe owns its D-Bus name, remains healthy, exposes Open/Open With actions, and quits cleanly; no-default routing is covered deterministically by focused tests.
  EVIDENCE: Isolated native run returned D-Bus ownership true, described `open` and `open-with`, remained healthy, and exited through Quit without warnings beyond the known RADV notice.

- [ ] G10: Phase 6I is committed, pushed, fast-forwarded into `main`, and local/remote phase/main refs all match.
  CHECK: git rev-parse main phase-6i-open-with-fallback origin/main origin/phase-6i-open-with-fallback
  EXPECT: /^([0-9a-f]{40})\n\1\n\1\n\1$/
  EVIDENCE: pending

- [ ] G11: The gate checker reports every Phase 6I acceptance gate met.
  CHECK: node /home/rocappsec/.codex/skills/unlazy/scripts/gate-check.mjs GATES.md
  EXPECT: /ALL MET/
  EVIDENCE: pending
