# Gates: Floe Phase 12C — Batch Rename

Scope: Deliver previewed, collision-safe, bounded batch rename with exact in-session undo.

- [x] G1: Dedicated Phase 12C branch is active.
CHECK: git branch --show-current
EXPECT: phase-12c-batch-rename
EVIDENCE: Active branch is `phase-12c-batch-rename`.

- [x] G2: Transform and preview model covers literal/regex/prefix/suffix/number/case/date, extension policy, raw-name rejection, capacity, and deterministic collisions.
CHECK: cargo test -p floe-app phase_12c_batch_rename_model -- --nocapture
EXPECT: test result: ok
EVIDENCE: Focused model test passed regex/templates, numbering, case, collision, and raw-name policy.

- [x] G3: Whole-batch apply and undo validate every exact mapping before no-overwrite mutation, revalidate identities, and report cancellation/partial failure truthfully.
CHECK: cargo test -p floe-app phase_12c_batch_rename_jobs -- --nocapture
EXPECT: test result: ok
EVIDENCE: Core and app job tests passed cycle-safe apply, conflict preservation, cancellation, shared progress, result, and exact inverse mapping.

- [x] G4: Native preview UI is accessible, selection-aware, deterministic, and delegates all mutation to application jobs.
CHECK: cargo test -p floe-app phase_12c_batch_rename_ui -- --nocapture
EXPECT: test result: ok
EVIDENCE: Bounded preview test passed; native Wayland described selection-aware Batch Rename and Undo actions as disabled without eligible state.

- [x] G5: Formatting, workspace check, strict Clippy, all tests, native build/smoke, and diff hygiene pass.
CHECK: sh -c 'cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check && echo phase-12c-full-gate-ok'
EXPECT: phase-12c-full-gate-ok
EVIDENCE: Strict Clippy, 338 app tests, 98 core tests, native build, diff hygiene, Wayland D-Bus Ping, Quit, and exit 0 passed.

- [x] G6: Persistent status records 12C complete and exactly 12D next.
CHECK: sh -c 'test "$(rg -o "NEXT" docs/ROADMAP.md | wc -l)" -eq 1 && rg -q "12C.*COMPLETE" docs/ROADMAP.md && rg -q "12D.*NEXT" docs/ROADMAP.md && echo phase-12c-docs-ok'
EXPECT: phase-12c-docs-ok
EVIDENCE: Roadmap has exactly one NEXT at 12D; AGENTS, matrix, privacy/security, plan, and gates match verified scope.
