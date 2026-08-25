# Gates: Floe Phase 9F — Preview interaction and lifecycle polish

- [x] G1: Correct phase branch.
  CHECK: git branch --show-current
  EXPECT: phase-9f-preview-polish
  EVIDENCE: phase-9f-preview-polish.
- [x] G2: Space toggles Preview without consuming editable text input; live selection follows exact generations and focus restores.
  CHECK: cargo test -p floe-app phase_9f_interaction -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; stable Space accelerator and text-focus policy verified. Existing exact-target lifecycle plus native open/close action/focus behavior passed.
- [x] G3: Image/document/font zoom/reset/fullscreen controls are presentation-only, bounded, accessible, and HiDPI-safe.
  CHECK: cargo test -p floe-app phase_9f_presentation -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; 50–400% clamping and reset policy verified. GTK paintable controls label zoom/monitor scaling and native fullscreen toggle passed.
- [x] G4: Memory-cache purge cancels current work, clears cached payloads, and never touches persistent thumbnail state.
  CHECK: cargo test -p floe-app phase_9f_cache_privacy -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 1/1 passed; cached provider reused once, purge advanced cancellation/cache epoch, next request reloaded. UI closes detail; persistent thumbnail worker is untouched.
- [x] G5: Native Wayland Space/action/fullscreen/close lifecycle smoke passes.
  EVIDENCE: Wayland app accepted Miller, quick-preview, zoom, fullscreen on/off, cache-purge and quit actions, answered Peer.Ping, and exited 0.
- [x] G6: Formatting, check, strict Clippy, and all workspace tests pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace
  EXPECT: test result: ok
  EVIDENCE: fmt/check/strict Clippy passed; 371 tests passed (280 app, 91 core), no failures.
- [x] G7: Patch hygiene is clean.
  CHECK: git diff --check
  EXPECT: /^$/
  EVIDENCE: git diff --check exited 0 with no output.
- [x] G8: Docs mark Phase 9 complete, 9F complete, and exactly 10A next without sandbox claims.
  CHECK: rg -n "9F.*COMPLETE|10A.*NEXT" docs/ROADMAP.md docs/FEATURE_MATRIX.md AGENTS.md
  EXPECT: 10A
  EVIDENCE: Roadmap, matrix, privacy/security, and AGENTS record Phase 9/9F COMPLETE, sole 10A NEXT, and retain Phase 18L sandbox boundary.
