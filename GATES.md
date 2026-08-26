# Gates: Floe Phase 12B — Archive UX

Scope: Deliver native accessible Extract Here/To and Compress workflows over the Phase 12A bounded engine.

- [x] G1: The active branch is the dedicated Phase 12B branch.
CHECK: git branch --show-current
EXPECT: phase-12b-archive-ui
EVIDENCE: `git branch --show-current` returned `phase-12b-archive-ui`.

- [x] G2: Archive UX planning resolves exact sources, reviewed formats, deterministic destinations, raw names, conflicts, and bounded selection eligibility.
CHECK: cargo test -p floe-app phase_12b_archive_ui_contract -- --nocapture
EXPECT: test result: ok
EVIDENCE: Focused raw-name and exact-destination contract test passed.

- [x] G3: Native archive dialogs/actions expose Extract Here, Extract To, and Compress with accessible labels, destination preview, live eligibility, and no GTK filesystem mutation.
CHECK: cargo test -p floe-app phase_12b_archive_ui_actions -- --nocapture
EXPECT: test result: ok
EVIDENCE: Focused action/presentation test passed; live Wayland window exported all three actions and Extract Here was disabled with no selection.

- [x] G4: Archive UI jobs reuse shared progress/cancellation/results, refresh affected directories, and report conflict/password/unsupported states truthfully without accepting secrets or overwriting.
CHECK: cargo test -p floe-app phase_12b_archive_ui_jobs -- --nocapture
EXPECT: test result: ok
EVIDENCE: Focused state test passed completion, conflict preservation, cancellation, affected-directory, and bounded-result assertions; password/conflict guidance test passed.

- [x] G5: Formatting, workspace check, strict Clippy, all tests, native build/smoke, and diff hygiene pass.
CHECK: sh -c 'cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check && echo phase-12b-full-gate-ok'
EXPECT: phase-12b-full-gate-ok
EVIDENCE: Format, workspace check, strict Clippy, 335 app tests, 97 core tests, native build, diff hygiene, D-Bus Ping, action export/state, Quit, and exit 0 passed.

- [x] G6: Persistent status records 12B complete and exactly 12C next without password, overwrite, or sandbox claims.
CHECK: sh -c 'test "$(rg -o "NEXT" docs/ROADMAP.md | wc -l)" -eq 1 && rg -q "12B.*COMPLETE" docs/ROADMAP.md && rg -q "12C.*NEXT" docs/ROADMAP.md && echo phase-12b-docs-ok'
EXPECT: phase-12b-docs-ok
EVIDENCE: Roadmap has exactly one `NEXT` at 12C; matrix, AGENTS, development, and privacy/security records match implementation and exclusions.
