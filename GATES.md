# Gates: Floe Phase 10A — Inspector foundation

- [x] G1: Correct phase branch.
  CHECK: git branch --show-current
  EXPECT: phase-10a-inspector-foundation
  EVIDENCE: `phase-10a-inspector-foundation`.

- [x] G2: Bounded GTK-independent worker aggregates exact raw selection facts and rejects invalid/oversized work.
  CHECK: cargo test -p floe-app phase_10a_inspector_worker -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; fixed 16-request queue and 4,096-entry cap preserve raw non-UTF-8 paths and reject zero-generation/oversized requests.

- [x] G3: Inspector lifecycle and presentation handle multi-selection, stale generations, read-only accessibility, and focus-safe action contract.
  CHECK: cargo test -p floe-app phase_10a_inspector_lifecycle -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 2/2 passed; matching generations present bounded aggregate facts, stale results remain loading, exact raw targets survive, Ctrl+I/action contract and close-focus path are covered.

- [x] G4: Independent clamped Inspector width persists and migrates asynchronously.
  CHECK: cargo test -p floe-app phase_10a_inspector_preferences -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; 180–520 pixel clamping, version-4 round trip, and version-3 default migration verified. Native actions expose labelled Narrower/Wider controls.

- [x] G5: Native Wayland Inspector selection/action/width/close lifecycle smoke passes.
  EVIDENCE: Live app switched to Miller, selected the directory, opened/closed Inspector, adjusted width across launches, answered D-Bus Peer.Ping, persisted version-4 280-pixel width, and exited 0. Only documented host libadwaita/RADV/Vulkan warnings appeared.

- [x] G6: Formatting, check, strict Clippy, build, and all workspace tests pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app
  EXPECT: test result: ok
  EVIDENCE: All commands exited 0; 375 tests passed (284 application, 91 core), with zero failures.

- [x] G7: Patch hygiene is clean.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: exited 0 with no output.

- [x] G8: Docs mark 10A complete and exactly one 10B next; security docs retain read-only/no-provider/no-edit boundaries.
  CHECK: rg -n "10A.*COMPLETE|10B.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 10B
  EVIDENCE: Roadmap and matrix mark 10A COMPLETE and only 10B NEXT; AGENTS recommends only 10B; privacy/security documents memory-only listing facts, width-only persistence, no filesystem reads/providers/edits.
