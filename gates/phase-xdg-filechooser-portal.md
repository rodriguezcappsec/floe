# Gates: Floe Phase 22B — Optional XDG FileChooser Portal Backend

- [x] P1: Strict portal-neutral request parsing covers OpenFile, SaveFile, and
  SaveFiles options, raw local paths, parent identifiers, filters, choices,
  counts, sizes, and rejects malformed or unsupported input fail-closed.
  CHECK: `cargo test -p floe-app phase_22b_contract -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Two focused contracts cover OpenFile/SaveFile/SaveFiles,
  multiple and directory policy, raw non-UTF-8 paths, Wayland/X11 parsing,
  filenames, URI normalization, SaveFiles expansion, and fail-closed choices.

- [x] P2: Bounded supervisor launches exact no-shell Selection Mode argv, limits
  processes/stdout/results, handles cancellation and stale completion exactly,
  and shuts down without orphaning chooser processes.
  CHECK: `cargo test -p floe-app phase_22b_supervisor -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Focused exact-argv/result contract passes. Capacity 16,
  2-MiB stdout, capacity-128 results, cancellable per-child ownership, terminal
  removal, and shutdown force-exit are bounded in the application service.

- [x] P3: Optional native D-Bus backend owns the correct name only on explicit
  invocation, registers FileChooser and per-request Close objects, defers one
  terminal reply, and returns exact response/result dictionaries.
  CHECK: `cargo test -p floe-app phase_22b_dbus -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Focused D-Bus name/interface/type contract passes. Isolated
  native service returned response 0 with the exact SaveFile URI and response 1
  for Request.Close; each request object was released with its terminal reply.

- [x] P4: Service/portal packaging is valid, opt-in rather than default, and
  documentation truthfully distinguishes URI selection from sandbox grants.
  CHECK: `bash packaging/tests/test-package-layout.sh && python3 scripts/check-docs.py --strict && test "$(rg -c '^\| .*\| NEXT \|' docs/ROADMAP.md)" -eq 1`
  EXPECT: `/phase-21c-docs-ok/`
  EVIDENCE: PASS. The 24-entry install manifest stages and removes the D-Bus
  service and portal descriptor while preserving user data. Strict docs pass 21
  files and identify Phase 22C as the sole NEXT row; no default selection or
  Floe-created Document Portal grant is claimed.

- [x] P5: Full Rust, strict Clippy, docs/render/release/diff, E2E contracts, and
  isolated session-bus/native Wayland Open/Save/Close/lifecycle gates pass.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Format, workspace check, strict all-target/all-feature Clippy,
  and workspace tests pass: 629 app tests with 18 intentional graphical ignores,
  21 controller, 171 core, and six duplicate workflow tests. Strict docs/render,
  package layout, 206-package/10-license release policy, zero open advisories,
  three-environment matrix, diff hygiene, and five E2E contracts pass; two native
  semantic classes skip exact missing Dogtail/pyatspi and staged artifact inputs.
  Native SaveFile returned `file:///tmp/floe-portal-save-335318.txt` without
  creating it; Close returned response 1; no Floe config/state or transient
  chooser directory remained. Live attachment to an exported foreign Wayland
  parent handle is unverified because none was available.

- [x] P6: Persistent status marks only verified 22B complete, records exact
  external limitations, and names exactly one bounded recommended next phase.
  CHECK: `python3 scripts/check-docs.py --strict && test "$(rg -c '^\| .*\| NEXT \|' docs/ROADMAP.md)" -eq 1`
  EXPECT: `/phase-21c-docs-ok/`
  EVIDENCE: PASS. AGENTS, roadmap, matrix, privacy/security, architecture,
  administration, user guide, README, plan, and gates agree on verified Phase
  22B behavior, explicit unsupported cases, live-parent limitation, and exactly
  Phase 22C Portal option completeness as NEXT.
