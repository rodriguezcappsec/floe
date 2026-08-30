# Gates: Floe Phase 21C — Release Documentation

Scope: Verified release manuals, automated documentation contracts, fresh-user
walkthrough, and package integration without Phase 21D or deferred features.

- [x] D1: Complete reciprocal release-document set and strict checker pass.
  CHECK: python3 scripts/check-docs.py --strict
  EXPECT: /phase-21c-docs-ok/
  EVIDENCE: Strict checker passes all 19 UTF-8 release-facing files with reciprocal links, fragments, duplicate base heading slug rejection, fixed-column tables, identity, limitations, terminology, and exactly one NEXT validated; two focused checker regressions pass.
- [x] D2: All release-facing Markdown renders and known stale/malformed current
  claims are reconciled.
  CHECK: sh scripts/render-docs.sh
  EXPECT: /phase-21c-render-ok/
  EVIDENCE: Dependency-free GFM render sweep passes after roadmap, matrix, privacy, architecture, development, and table reconciliation.
- [x] D3: Isolated installed-artifact fresh-user walkthrough contract and
  truthful semantic-native skip/manual evidence pass.
  CHECK: python3 -m unittest discover -s e2e -p 'test_release_walkthrough.py' -v
  EXPECT: /OK/
  EVIDENCE: Two deterministic installed-artifact/isolation/scope contracts pass; staged native semantic class skips truthfully because Python Dogtail and pyatspi are unavailable. Separate staged binary Ping/Quit lifecycle exits 0.
- [x] D4: Installed public docs, deterministic archive/checksum, package layout,
  migrations and real Arch/staged-native integration pass.
  CHECK: sh packaging/tests/test-package-layout.sh && sh packaging/tests/test-migrations.sh && sh packaging/tests/test-release-source.sh
  EXPECT: /phase-21c-release-source-ok/
  EVIDENCE: Manifest layout/uninstall, migrations, and reproducible source checks pass. Real makepkg clean build passes and staged Arch binary answers Ping, exports/accepts Quit, exits 0.
- [x] D5: Full deterministic and applicable native release gates pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build --frozen --release -p floe-app --bin floe && git diff --check
  EXPECT: /Finished/
  EVIDENCE: Format, workspace check, strict all-target/all-feature Clippy, 554 app tests with 14 graphical ignores, 21 app integration tests, 162 core tests, six duplicate workflows, frozen release build, and diff hygiene pass.
- [x] D6: Status/docs agree on verified limitations and exactly Phase 21D NEXT.
  CHECK: test "$(rg -c '\| NEXT \|' docs/ROADMAP.md)" -eq 1 && rg -n '21C.*COMPLETE|21D.*NEXT' docs/ROADMAP.md
  EXPECT: /21D.*NEXT/
  EVIDENCE: README, roadmap, feature matrix, privacy/security, AGENTS, PLAN, and ledgers record English-only partial RTL, log redaction, native semantic skip, deferred features, Phase 21C COMPLETE, and exactly Phase 21D NEXT.
