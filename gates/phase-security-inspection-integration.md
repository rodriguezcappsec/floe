# Gates: Floe multi-window and local security-inspection integration

Scope: integrate Phase 23H, 18L, 18N/18N2, 18O, and 18P without regressions or
overstated security/privacy claims.

- [x] I1: Every leaf ledger is fully evidenced with no silent scope removal.
  CHECK: `for gate in gates/phase-23h-multi-window-runtime.md gates/phase-18l-sandboxed-providers.md gates/phase-18n-local-threat-analysis.md gates/phase-18o-privacy-inspector.md gates/phase-18p-metadata-sanitization.md; do node <unlazy-skill-dir>/scripts/gate-check.mjs --status "$gate"; done`
  EXPECT: `/ALL MET/`
  EVIDENCE: PASS. The Unlazy status checker reports ALL MET for 3 Phase 23H, 3 Phase 18L, 4 Phase 18N/18N2, 3 Phase 18O, and 3 Phase 18P gates: 16 of 16 leaf gates.

- [x] I2: Formatting, workspace check, strict all-target/all-feature Clippy, and
  complete workspace tests pass.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Format, workspace check, strict all-target/all-feature Clippy, and sequential workspace tests pass. Measured suites: 653 app passed with 19 intentional graphical ignores, 21 controller passed, 177 core passed, six duplicate workflows passed.

- [x] I3: Strict documentation, rendered docs, dependency/advisory, packaging,
  migration, release-source/candidate, E2E contracts, and diff hygiene pass.
  CHECK: `python3 scripts/check-docs.py --strict && git diff --check`
  EXPECT: `/phase-21c-docs-ok/`
  EVIDENCE: PASS. Strict docs reports 21 files; render, five E2E contracts with two truthful native-dependency skips, package layout, three migrations, deterministic release-source/candidate, and diff hygiene pass. The release candidate contains 261 files and its generated checksum matches the Arch package recipe.

- [x] I4: Native Wayland proves multi-window restore/close-survivor, real
  Bubblewrap provider isolation, fake-clamd scan, sanitized output reveal,
  responsiveness, clean Quit, and no GTK/libadwaita criticals where host
  dependencies permit; unavailable semantic dependencies are recorded exactly.
  EVIDENCE: PASS with explicit host limitations. KDE Wayland smoke reports `close-survivor-responsive=true third-window=true restored-windows=true`, clean Quit, and two-window restart restoration from the same private roots; the privacy label markup warning is gone. AT-SPI refuses connections, so semantic action/reveal E2E is skipped. Bubblewrap fails required namespace creation with `NETLINK_ROUTE ... Operation not permitted`; no direct fallback runs. `clamd` is installed but no reviewed socket is running, so no live signature scan is claimed. Deterministic exact protocol and sanitizer publication tests pass.

- [x] I5: AGENTS, README, User Guide, Architecture, Roadmap, Feature Matrix,
  Privacy/Security, Plan, Gates, packaging, and release checksum match verified
  behavior; exactly one bounded later phase is `NEXT`.
  CHECK: `test "$(rg -c '^\| .*\| NEXT \|' docs/ROADMAP.md)" -eq 1`
  EXPECT: `/^$/`
  EVIDENCE: PASS. Persistent docs describe boundaries, rationale, use, multi-window generation routing, host limitations, and one recommended next phase. Strict docs pass, the roadmap has exactly one `NEXT` row (Phase 18M), and the Arch checksum matches the final deterministic source archive.
