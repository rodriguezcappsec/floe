# Gates: Floe Phase 10D — Permissions

- [x] G1: Correct phase branch.
  CHECK: git branch --show-current
  EXPECT: phase-10d-permissions
  EVIDENCE: `phase-10d-permissions` confirmed before implementation and verification.

- [x] G2: Typed permission requests validate exact paths, octal modes, UID/GID identities, roots, symlink policy, and recursive scope before mutation.
  CHECK: cargo test -p floe-core phase_10d_permission_request -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused core test passed; exact targets, root/nesting rejection, octal bounds, ambiguity, numeric IDs, validated local names, and confirmation policy are covered.

- [x] G3: Bounded permission executor applies direct executable/mode/owner/group changes without following symlinks and reports cancellation, stale identities, mount refusal, and committed partial failure truthfully.
  CHECK: cargo test -p floe-app phase_10d_permission_executor -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Two focused executor tests passed, including worker lifecycle, pre-cancel, direct chmod, bounded local-name resolution, selected-symlink rejection, deterministic stale-identity refusal, cancellation after one of two commits reported as partial with the second file unchanged, and mount-boundary failure mapping.

- [x] G4: Recursive preflight is descriptor-relative, bounded, cancellable, no-follow, deterministic, and distinguishes file/directory mode semantics.
  CHECK: cargo test -p floe-app phase_10d_recursive_permissions -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused recursive test passed with separate file/directory modes and an internal symlink skipped without changing its external target; code caps 250,000 entries and depth 1,024 and refuses mount crossings.

- [x] G5: Native Properties permission editor validates inputs, confirms risky scope, remains accessible, and submits only application jobs without root-process elevation.
  CHECK: cargo test -p floe-app phase_10d_permissions_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused editor-policy test passed for exact targets, octal/identity validation, recursive acknowledgement, no-change and ambiguous-input rejection; GTK callback only builds and submits the typed request.

- [x] G6: Native Wayland smoke verifies editor open/validation/cancel, a safe temporary-fixture change, Operations feedback, D-Bus health, focus, and clean quit.
  EVIDENCE: Isolated `/tmp/floe-phase10d-smoke.sc15Uq` smoke opened Properties and Edit Permissions through D-Bus/AT-SPI, exposed labelled controls, returned `select at least one permission change`, applied only fixture mode `0644` to `0600`, exercised Cancel, answered Peer.Ping, returned accessible window state, quit cleanly, and released `io.github.floe.FileManager`; only documented RADV/Vulkan warnings appeared.

- [x] G7: Formatting, workspace check, strict Clippy, full tests, build, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check
  EXPECT: test result: ok
  EVIDENCE: Full command passed: 388 tests total (296 application, 92 core), strict Clippy, native build, and diff hygiene all clean.

- [x] G8: Documentation marks 10D complete, exactly 10E next, and retains the no-elevation/no-ACL/no-symlink-follow claims.
  CHECK: test "$(rg -o '\| NEXT \|' docs/ROADMAP.md | wc -l)" -eq 1 && rg -n "10D.*COMPLETE|10E.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 10E is the sole next phase
  EVIDENCE: Roadmap has one `NEXT` row at 10E; roadmap, matrix, AGENTS, and privacy/security documentation describe 10D truthfully and keep excluded security claims explicit.
