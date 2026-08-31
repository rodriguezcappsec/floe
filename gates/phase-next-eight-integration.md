# Gates: Floe next-eight daily-driver integration

- [x] I1: Phase 22C and Phase 23B–23G ledgers are complete; Phase 23A is usable
  but explicitly PARTIAL with W1/W3 deferred intact.
  CHECK: `for gate in gates/phase-22c-portal-options.md gates/phase-23{a-multi-window,b-notifications,c-natural-sort,d-bookmark-management,e-collapsible-sidebar,f-inspector-checksums,g-details-columns}.md; do node <unlazy-skill-dir>/scripts/gate-check.mjs --status "$gate"; done`
  EXPECT: `/ALL MET/`
  EVIDENCE: Per-leaf deterministic gates pass; external semantic gaps use explicit ABANDON lines.
- [x] I2: Format, workspace check, strict all-target/all-feature Clippy, workspace
  tests, docs/render/package/migrations/matrix and diff gates pass.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && git diff --check`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. 620 app plus 18 ignored graphical, 21 controller, 174 core and
  six duplicate workflow tests pass; strict Clippy passes.
- [x] I3: Applicable E2E contracts and isolated native Wayland lifecycle pass;
  exact unavailable graphical layers are recorded.
  EVIDENCE: Five E2E contracts pass with two truthful skips. Native action
  description, repeated New Window, Ping and Quit pass. Aggregate ignored GTK
  command fails because GTK initializes from different Rust test threads;
  Dogtail/pyatspi are unavailable.
- [x] I4: Persistent documents mark verified completion/partial state and exactly
  Phase 23H NEXT.
  CHECK: `python3 scripts/check-docs.py --strict && test "$(rg -c '^\| .*\| NEXT \|' docs/ROADMAP.md)" -eq 1`
  EXPECT: `/phase-21c-docs-ok/`
  EVIDENCE: PASS. Strict docs/render/package layout/migrations/environment matrix pass.
