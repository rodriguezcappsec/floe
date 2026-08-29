# Gates: Floe Phase 20B1A — Advanced Metadata Sort Index

- [x] M1: Core advanced ordering is deterministic with unknown values last.
  CHECK: cargo test -p floe-core phase_20b1a_sort -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: One focused ordering regression passes all 22 indexed fields plus
  Path in both directions with exact-path tie-breakers.
- [x] M2: Extraction is bounded, cancellable, no-follow, and source-revalidated.
  CHECK: cargo test -p floe-app phase_20b1a_extract -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Two focused extraction/worker regressions pass text counts,
  filesystem facts, symlink rejection, cancellation, sorting, and cache reuse.
- [x] M3: The private versioned cache preserves exact paths and rejects unsafe data.
  CHECK: cargo test -p floe-app phase_20b1a_cache -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: One focused cache regression passes raw non-UTF-8 round trip,
  private permissions, invalidation, and corrupt/insecure/symlink rejection.
- [x] M4: Every native advanced Sort By action is real and reachable.
  CHECK: cargo test -p floe-app phase_20b1a_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: One focused UI regression verifies 33 real criteria and no metadata
  placeholder; focused real-GTK header and Settings accessibility gates pass.
- [x] M5: Complete deterministic and applicable native quality gates pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check
  EXPECT: test result: ok
  EVIDENCE: Exit 0; app 532 passed/nine intentional graphical ignores, app
  integration 21 passed, core 158 passed, duplicate workflows six passed. E2E
  harness passes three contracts and skips native Dogtail because dogtail and
  pyatspi are unavailable. Prior isolated native Wayland launch/Ping/Quit exits 0.
