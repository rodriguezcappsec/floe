# Gates: Floe Phase 12A — Archive Engine

Scope: Deliver bounded no-overwrite archive listing, extraction, and compression jobs without UI, shells, passwords, or unsafe member paths.

- [x] G1: The active branch is the dedicated Phase 12A branch.
CHECK: git branch --show-current
EXPECT: phase-12a-archive-engine
EVIDENCE: `git branch --show-current` returned `phase-12a-archive-engine`.

- [x] G2: Typed formats, requests, limits, raw paths, and member validation reject traversal, absolute paths, duplicates, nesting conflicts, roots, and invalid input counts.
CHECK: cargo test -p floe-core phase_12a_archive_contract -- --nocapture
EXPECT: test result: ok
EVIDENCE: Focused contract test passed in `floe-core`.

- [x] G3: ZIP and TAR-family listing/extraction/compression enforce no-follow sources, bomb/ratio caps, unsupported-link policy, no-overwrite publication, cancellation cleanup, and round trips.
CHECK: cargo test -p floe-core phase_12a_archive_zip_tar -- --nocapture
EXPECT: test result: ok
EVIDENCE: Focused ZIP/TAR-family test passed for all four formats.

- [x] G4: Reviewed pure-Rust 7z listing/extraction/compression uses the same validated member plan, cancellation, staging, and traversal policy.
CHECK: cargo test -p floe-core phase_12a_archive_7z -- --nocapture
EXPECT: test result: ok
EVIDENCE: Focused 7z round-trip, traversal, and exact-name-policy test passed.

- [x] G5: The capacity-4 executor and capacity-16 result boundary integrate shared job progress, cancellation, results, and clean shutdown off GTK.
CHECK: cargo test -p floe-app phase_12a_archive_executor -- --nocapture
EXPECT: test result: ok
EVIDENCE: Two focused executor tests passed with progress, result, cancellation, and shutdown coverage.

- [x] G6: Formatting, workspace check, strict Clippy, all tests, native app build, and diff hygiene pass.
CHECK: sh -c 'cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check && echo phase-12a-full-gate-ok'
EXPECT: phase-12a-full-gate-ok
EVIDENCE: Format, workspace check, strict Clippy, 332 app tests, 97 core tests, native app build, and `git diff --check` exited 0.

- [x] G7: Persistent status records 12A complete, exactly 12B next, security limits, and no password/sandbox claim.
CHECK: sh -c 'test "$(rg -o "NEXT" docs/ROADMAP.md | wc -l)" -eq 1 && rg -q "12A.*COMPLETE" docs/ROADMAP.md && rg -q "12B.*NEXT" docs/ROADMAP.md && rg -q "Phase 12A archive operations" docs/PRIVACY_SECURITY.md && echo phase-12a-docs-ok'
EXPECT: phase-12a-docs-ok
EVIDENCE: Roadmap has exactly one `NEXT` at 12B; matrix, AGENTS, and privacy/security documents record the bounded engine and exclusions.
