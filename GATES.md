# Gates: Floe Phase 12E — Links/Duplicate Polish

- [x] G1: Relative/absolute and broken symbolic-link policy tests pass.
CHECK: cargo test -p floe-core phase_12e_symbolic_link -- --nocapture
EXPECT: test result: ok
EVIDENCE: phase_12e_symbolic_link passed; exact non-UTF-8 relative and absolute targets remained readable after target removal, and relative input rejection was explicit.

- [x] G2: Hard-link eligibility and duplicate naming tests pass.
CHECK: sh -c 'cargo test -p floe-core phase_12e_hard_link -- --nocapture && cargo test -p floe-app phase_12e_duplicate -- --nocapture'
EXPECT: test result: ok
EVIDENCE: phase_12e_hard_link and phase_12e_duplicate passed; regular/symlink/cross-device policy, inode identity, raw extensions, existing suffix advancement, malformed suffixes, and bounds were verified.

- [x] G3: Full workspace gates and native build pass.
CHECK: sh -c 'cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check && echo phase-12e-full-gate-ok'
EXPECT: phase-12e-full-gate-ok
EVIDENCE: phase-12e-full-gate-ok; strict Clippy passed, 342 app and 101 core tests passed, native build passed. Corrected Wayland service smoke Pinged, activated, described create-symbolic-link, Quit, and exited 0.

- [x] G4: Exactly 12F is NEXT.
CHECK: sh -c 'test "$(rg -o "\\| NEXT \\|" docs/ROADMAP.md | wc -l)" -eq 1 && rg -q "12F.*NEXT" docs/ROADMAP.md && echo phase-12e-docs-ok'
EXPECT: phase-12e-docs-ok
EVIDENCE: docs/ROADMAP.md marks Phase 12E COMPLETE and exactly Phase 12F NEXT.
