# Gates: Floe Phase 10B — Metadata providers

- [x] G1: Work is isolated on the correct phase branch.
  CHECK: git branch --show-current
  EXPECT: phase-10b-metadata-providers
  EVIDENCE: `phase-10b-metadata-providers`.

- [x] G2: Inspector metadata work is bounded, GTK-independent, exact-path, no-follow, and generation-stale safe.
  CHECK: cargo test -p floe-app phase_10b_metadata_worker -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; capacity-16 requests/results, exact raw identity, no-follow descriptors, source revalidation, superseding generation, and clean worker shutdown are exercised.

- [x] G3: Lazy facts cover MIME, exact timestamps, Unix owner/group/mode, symlink target/status, safe image dimensions, and disappearing or changed sources.
  CHECK: cargo test -p floe-app phase_10b_metadata_facts -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; real PNG dimensions, MIME, timestamps, UID/GID/mode, raw non-UTF-8 link target/status, and vanished-source result verified.

- [x] G4: Explicit folder aggregation is bounded, non-recursive, cancellable/superseding, and truthful about unknown/overflow state.
  CHECK: cargo test -p floe-app phase_10b_folder_aggregate -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; descriptor-relative no-follow immediate enumeration ignores nested bytes, enforces shared 16,384-entry budget, reports truncation, and saturates overflow explicitly.

- [x] G5: The read-only Inspector presents single-entry provider facts and truthful multi-selection aggregates without property edits or eager directory enrichment.
  CHECK: cargo test -p floe-app phase_10b_inspector_metadata -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; single MIME/time/Unix/image facts and multi-folder immediate aggregates are accessible, read-only, and explicitly non-recursive.

- [x] G6: Native Wayland smoke verifies Inspector open, metadata loading, selection change, close/focus lifecycle, D-Bus health, and clean quit.
  EVIDENCE: Live Wayland app exported its window actions; D-Bus activated Miller, Select All, Inspector open/close, answered Peer.Ping, quit through the app action, exited 0, and released its name. Spectacle/Flameshot produced no capture on this host, so no visual screenshot claim is made; only known libadwaita/RADV/Vulkan warnings appeared.

- [x] G7: Repository formatting, workspace checks, strict Clippy, full tests, native app build, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check
  EXPECT: test result: ok
  EVIDENCE: fmt/check/strict Clippy/build/diff all exited 0; 379 tests passed (288 application, 91 core).

- [x] G8: Persistent documentation marks 10B complete, marks exactly 10C next, and retains truthful privacy/no-edit/no-recursion claims.
  CHECK: test "$(rg -o '\| NEXT \|' docs/ROADMAP.md | wc -l)" -eq 1 && rg -n "10B.*COMPLETE|10C.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 10C
  EVIDENCE: ROADMAP has one NEXT at 10C; matrix, AGENTS, privacy/security, plan, and gates record read-only bounded non-recursive scope and future exclusions.
