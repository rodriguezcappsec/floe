# Gates: Floe Phase 8B — Virtualized Miller columns

Scope: deliver bounded native Miller columns over the existing exact-path browser pipeline, without Phase 8C–9 work.

- [x] G1: Work is isolated on the prescribed phase branch.
  CHECK: git branch --show-current
  EXPECT: phase-8b-miller-ui
  EVIDENCE: phase-8b-miller-ui

- [x] G2: Miller presentation state has explicit bounded columns, entries, and width policy while preserving exact raw path identity.
  CHECK: cargo test --workspace phase_8b_policy -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Running unittests src/main.rs (target/debug/deps/floe_app-c34335fcae0cf6e5) | Running unittests src/lib.rs (target/debug/deps/floe_core-9689122403c8558b)

- [x] G3: The native Miller surface recycles column/row widgets and exposes non-color-only active-column and width controls.
  CHECK: cargo test -p floe-app phase_8b_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.02s | Running unittests src/main.rs (target/debug/deps/floe_app-c34335fcae0cf6e5)

- [x] G4: Miller mode reuses the existing browser result pipeline and does not add a second directory worker or GTK filesystem enumeration.
  CHECK: cargo test -p floe-app phase_8b_pipeline -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.02s | Running unittests src/main.rs (target/debug/deps/floe_app-c34335fcae0cf6e5)

- [x] G5: List/grid/tabs/split behavior remains available and Miller selection/activation uses authoritative `PathBuf` values.
  CHECK: cargo test -p floe-app phase_8b_integration -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.03s | Running unittests src/main.rs (target/debug/deps/floe_app-c34335fcae0cf6e5)

- [x] G6: Native Wayland smoke verifies Miller activation, width adjustment, application health, and clean shutdown.
  EVIDENCE: Two isolated launches activated `view-miller` and `widen-miller-columns`, answered D-Bus Ping, migrated/restored `miller-column-width=320`, quit with status 0, and released `io.github.floe.FileManager`.

- [x] G7: Rust formatting is clean.
  CHECK: cargo fmt --all -- --check
  EXPECT: /^$/
  EVIDENCE: `cargo fmt --all -- --check` exited 0 with no output after final documentation and code edits.

- [x] G8: The full workspace type-checks.
  CHECK: cargo check --workspace
  EXPECT: Finished `dev` profile
  EVIDENCE: Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.02s

- [x] G9: Strict all-target/all-feature Clippy is warning-free.
  CHECK: cargo clippy --workspace --all-targets --all-features -- -D warnings
  EXPECT: Finished `dev` profile
  EVIDENCE: Checking floe-app v0.1.0 (/run/media/rocappsec/LNX-games-more/app-ideas/floe_file_manager/crates/app) | Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.51s

- [x] G10: All workspace tests pass.
  CHECK: cargo test --workspace
  EXPECT: test result: ok
  EVIDENCE: Running unittests src/lib.rs (target/debug/deps/floe_core-9689122403c8558b) | Doc-tests floe_core

- [x] G11: Patch hygiene is clean.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: `git diff --check` exited 0 with no output after final edits.

- [x] G12: Persistent documentation marks 8B complete, exactly 8C next, and records verified architecture/security boundaries.
  CHECK: rg -n "8B.*COMPLETE|8C.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 8C
  EVIDENCE: docs/ROADMAP.md:157:| 8B — Virtualized columns | COMPLETE | `phase-8b-miller-ui` | Recyclable floating columns and adjustable widths. | Verified one shared active browser model, capacity-16 retained c
