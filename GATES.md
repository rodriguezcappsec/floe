# Gates: Floe Phase 12D — Create/Templates Polish

- [x] G1: Bounded template policy tests pass on the dedicated branch.
CHECK: cargo test -p floe-app phase_12d_templates -- --nocapture
EXPECT: test result: ok
EVIDENCE: 3 phase_12d_templates tests passed; bounded exact-path discovery, no-follow filtering, capacity/truncation, worker response, and unavailable state verified.

- [x] G2: Template copies are non-executable and exact-name safe.
CHECK: cargo test -p floe-core phase_12d_template_create -- --nocapture
EXPECT: test result: ok
EVIDENCE: phase_12d_template_create_strips_execute_bits_and_rejects_links passed; source mode/payload remained unchanged and destination execute bits were zero.

- [x] G3: Full workspace gates and native build pass.
CHECK: sh -c 'cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check && echo phase-12d-full-gate-ok'
EXPECT: phase-12d-full-gate-ok
EVIDENCE: phase-12d-full-gate-ok; strict Clippy passed, 341 app and 99 core tests passed, native build passed. Active Wayland launch returned 0, D-Bus Peer.Ping and Quit both returned ().

- [x] G4: Exactly 12E is NEXT.
CHECK: sh -c 'test "$(rg -o "NEXT" docs/ROADMAP.md | wc -l)" -eq 1 && rg -q "12E.*NEXT" docs/ROADMAP.md && echo phase-12d-docs-ok'
EXPECT: phase-12d-docs-ok
EVIDENCE: docs/ROADMAP.md marks Phase 12D COMPLETE and exactly Phase 12E NEXT.
