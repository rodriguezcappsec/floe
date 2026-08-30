# Gates: Floe USB Device Discovery Bug Fix and Logical Edge Review

Scope: repair empty-name removable-volume presentation first, then perform a
read-only adversarial logical review. No other finding is implemented.

- [x] D1: Every GIO snapshot has a bounded nonempty presentation name without
  changing authoritative identity, exact roots, or action objects.
  CHECK: cargo test -p floe-app usb_device_display_name -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: PASS. Focused resolver and live host GIO snapshot tests pass; every
  returned name is nonempty, control-free, and at most 160 characters.

- [x] D2: Empty/whitespace/control-heavy names and sibling partitions receive
  deterministic label/drive/device/generic fallbacks.
  CHECK: cargo test -p floe-app usb_device_snapshot -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: PASS. Empty siblings resolve distinctly through the associated
  drive plus `sdc1`/`sdc2`; label, device-hint, and generic fallbacks are
  covered.

- [x] D3: Existing topology, action, deduplication, and accessibility contracts
  remain intact.
  CHECK: cargo test -p floe-app devices::tests -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: PASS. All 13 device tests pass, including live monitor,
  hierarchy-collapse, opaque identity, raw paths, actions, and verified
  removable-target resolution.

- [x] D4: Full Rust, docs/diff, and applicable native GIO/Wayland gates pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace
  EXPECT: test result: ok
  EVIDENCE: PASS. Format/check/strict Clippy/workspace tests pass: 589 app plus
  18 graphical ignores, 21 controller, 169 core, and six duplicate workflows.
  Strict docs/render/dependencies/matrix, E2E contracts, and diff hygiene pass;
  semantic E2E records two external-dependency skips. Isolated Wayland
  `Peer.Ping`/Quit exits 0.

- [x] D5: Adversarial review findings survive the mandatory skeptic pass and
  remain read-only outside the explicitly authorized USB repair.
  EVIDENCE: PASS. Four confirmed findings survive caller and existing-test
  review; duplicate-UUID device ordering remains needs-verification. All remain
  read-only and are reported with minimum regression tests.

- [x] D6: Status documents the verified repair and exactly one roadmap phase
  remains NEXT.
  CHECK: python3 scripts/check-docs.py --strict
  EXPECT: phase-21c-docs-ok
  EVIDENCE: PASS. Persistent docs cover the fallback and exact-identity
  boundary; strict docs pass and Phase 6W is the sole `NEXT` phase.
