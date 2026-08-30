# Gates: user-facing feature rationale and Floe philosophy

- [x] R1: A user-facing philosophy explains Floe's product principles and maps
  important feature behavior to explicit reasons and tradeoffs.
  CHECK: rg -n 'The principles|Why key features behave this way|What is the tradeoff' docs/PHILOSOPHY.md
  EXPECT: /Why key features behave this way/
  EVIDENCE: `docs/PHILOSOPHY.md` defines nine product principles, a concrete
  feature/reason table, and the four-question rationale contract.

- [x] R2: README, Getting Started, User Guide, and Administration link the
  philosophy, and administrator UI/user documentation explains why it is
  read-only rather than merely stating the limitation.
  CHECK: rg -n 'PHILOSOPHY.md|Why read-only|elevated access stays isolated' README.md docs/GETTING_STARTED.md docs/USER_GUIDE.md docs/ADMINISTRATION.md crates/app/src/privileged_access.rs
  EXPECT: /Why read-only/
  EVIDENCE: All four user entry points link the philosophy; User Guide and the
  administrator status text explain narrow elevated authority in plain language.

- [x] R3: The philosophy is a validated, rendered, installed, release-source
  document rather than an untracked side note.
  CHECK: python3 scripts/check-docs.py --strict && sh scripts/render-docs.sh && sh packaging/tests/test-package-layout.sh && sh packaging/tests/test-release-source.sh
  EXPECT: /phase-21c-release-source-ok/
  EVIDENCE: Strict checker passes 20 release documents; render, staged package
  layout, and deterministic source checks pass with SHA-256
  `81dcd69448f32c403325d2bc4b2875dca50af66b1cf179db71227381afdd497c`.

- [x] R4: Formatting, focused administrator tests, strict documentation tests,
  and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo test -p floe-app phase_14b -- --nocapture && python3 -m unittest scripts/test_check_docs.py -v && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: Full formatting/check/strict-Clippy/workspace tests pass, focused
  Phase 14B and real-GTK accessibility tests pass, checker regressions and diff
  hygiene pass.
