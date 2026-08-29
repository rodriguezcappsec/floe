# Gates: Floe Phase 21A — Performance

Scope: Reproducible measurements and bounded optimization of existing Floe
workloads, including a real 100,000-entry local directory.

- [x] P1: Real release-mode benchmark harness covers all requested existing
  workload families below disposable temporary roots.
  CHECK: cargo test -p floe-app phase_21a_performance --release -- --ignored --nocapture --test-threads=1
  EXPECT: /PHASE21A_RESULT.*status=pass/
  EVIDENCE: Release harness passes all requested production workload families,
  final `entries=100000 temporary_root=true status=pass`, peak 119,876 KiB.
- [x] P2: Performance report records reproduction, host context, workload
  sizes, budgets, baseline/current evidence, memory procedure, and limitations.
  CHECK: test -f docs/PERFORMANCE.md && rg -n '100,000|Baseline|Current|peak memory|Limitations' docs/PERFORMANCE.md
  EXPECT: /Limitations/
  EVIDENCE: `docs/PERFORMANCE.md` records exact host/toolchain/filesystem,
  fixture sizes, budgets, current evidence, VmHWM procedure and limitations.
- [x] P3: A measured bottleneck is improved with before/after and correctness
  evidence rather than speculative architectural rewriting.
  CHECK: rg -n 'Measured optimization|Before|After|Correctness' docs/PERFORMANCE.md
  EXPECT: /Correctness/
  EVIDENCE: Metadata text facts improve 64,108 us to 32,212 us over 16,652,288
  ASCII bytes with exhaustive ASCII/Unicode/line parity regression.
- [ ] P4: Native disposable 100k Wayland launch/liveness/Ping/Quit passes and
  its exact evidence or environmental skip is recorded.
  EVIDENCE: Native 100k Ping/Quit/exit 0, peak 114,296 KiB. AT-SPI refused both
  host and private sockets, preventing the critical-free sub-gate.
ABANDON: P4 critical-free log unavailable due host AT-SPI connection refusal;
lifecycle passed and semantic E2E remains unclaimed.
- [x] P5: Full deterministic quality gates and applicable native gates pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build -p floe-app && git diff --check
  EXPECT: /Finished/
  EVIDENCE: fmt/check/strict Clippy/workspace tests/build/diff pass; 11 GTK
  contracts pass separately; E2E contracts pass and native suite skips missing
  Dogtail/pyatspi with exact reason.
- [x] P6: Roadmap, matrix, README/development docs, AGENTS status, PLAN, and root
  gates agree on verified results and exactly Phase 21B `NEXT`.
  CHECK: test "$(rg -c '\| NEXT \|' docs/ROADMAP.md)" -eq 1 && rg -n '21A.*COMPLETE|21B.*NEXT' docs/ROADMAP.md
  EXPECT: /21B.*NEXT/
  EVIDENCE: Persistent docs/status agree on measured scope and limitations;
  exactly Phase 21B is `NEXT`.
