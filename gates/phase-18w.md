# Leaf Gates: Phase 18W — Verified USB Transfer

Scope: Live Copy → Verify → Flush → Eject/Unmount workflow for one explicitly selected local item and one currently mounted removable destination. No real-device automation and no Phase 18X work.

- [x] W1: Explicit workflow state machine orders Copy, Verify, Flush, Eject/Unmount and records truthful success, failure, cancellation, retry boundary, and partial destination states.
  CHECK: cargo test --workspace phase_18w_workflow -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 4 focused tests passed, including claimed child JobId result ownership exactly once and invalid or terminal transition rejection.

- [x] W2: Flush targets the revalidated verified destination/device through a one-worker one-slot off-GTK executor; safe-removal language appears only after matching successful GIO removal completion.
  CHECK: cargo test --workspace phase_18w_safety -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 4 focused tests passed for deepest mount selection, outside-mount rejection, replaced-destination symlink rejection, and out-of-order safety. Fresh DeviceId, mount root, removable flag, destination relationship, and action availability are revalidated before removal.

- [x] W3: Native command, header/file menu placement, selection sensitivity, Operations Island stages/cancellation, and accessible terminal feedback remain distinct from ordinary Copy and Copy and Verify.
  CHECK: cargo test -p floe-app phase_18w_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 3 focused UI and command tests passed. The real GTK component accessibility gate also passed on the active display with one pre-existing libadwaita host warning.

- [x] W4: Existing mount/unmount/eject and ordinary copy behaviors remain unchanged when verified removable transfer is not selected.
  CHECK: cargo test --workspace phase_18w -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: 11 aggregate Phase 18W tests passed. Real removable-media automation was intentionally skipped because no explicit disposable test device was provided; no real user device or data was touched.
