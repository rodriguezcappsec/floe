# Gates: Phase 6K GIO device monitor boundary

Scope: Application-owned live GIO storage snapshots and asynchronous mount, unmount, and eject requests.

- [x] L1: Pure snapshot policy distinguishes drive, volume, and mount rows; collapses GIO parent/child duplication; covers mounted and unmounted state, removable media, local and non-local roots; and exposes mount, unmount, eject, unavailable, and busy action states.
  CHECK: cargo test -p floe-app phase_6k_device_policy -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: 3 policy tests passed; 0 failed, covering row-kind action matrices, supported-only busy state, volume-over-drive preference, empty-drive visibility, and standalone-mount visibility.

- [x] L2: Stable opaque identities do not depend on display labels and local navigation preserves exact non-UTF-8 `PathBuf` values while non-local roots expose no path.
  CHECK: cargo test -p floe-app phase_6k_device_identity -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: 2 identity tests passed; 0 failed, including opaque kind-scoped IDs and exact non-UTF-8 local paths.

- [x] L3: The application-owned GIO boundary privately retains drive, volume, and mount objects, observes every topology-changing VolumeMonitor signal, rebuilds snapshots, and gives clients refresh notifications.
  CHECK: cargo test -p floe-app phase_6k_device_monitor -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: 2 monitor tests passed; 0 failed. Private DeviceObject retains GIO handles; ten drive, volume, and mount signals call refresh_shared.

- [x] L4: Mount, unmount, and eject use GIO asynchronous APIs, prevent simultaneous work for one device, surface busy state immediately, and report structured, understandable completion outcomes.
  CHECK: cargo test -p floe-app phase_6k_device_action -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: 3 action tests passed; 0 failed, covering duplicate reservation, success/failure outcomes, GIO error classification, and recovery wording.

- [x] L5: The device module adds no shell execution, blocking filesystem calls, unsafe code, or dependency changes.
  CHECK: if rg -n "Command::|std::fs|unsafe|\.wait\(|thread::|tokio|async_std" crates/app/src/devices.rs; then exit 1; else echo "device boundary is asynchronous and shell-free"; fi
  EXPECT: /device boundary is asynchronous and shell-free/
  EVIDENCE: device boundary is asynchronous and shell-free; Cargo manifests were not changed by this leaf.

- [x] L6: The leaf is formatted and warning-free under strict Clippy.
  CHECK: cargo fmt --all -- --check && cargo clippy -p floe-app --all-targets -- -D warnings
  EXPECT: /Finished/
  EVIDENCE: cargo fmt --all -- --check exited 0; strict all-target Clippy finished successfully in 0.76s.
