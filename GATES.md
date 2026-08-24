# Gates: Floe Phase 6K places, bookmarks, and devices

Scope: A compact resizable sidebar with standards-based places, exact-path bookmarks, and live GIO storage devices with recoverable mount actions.

- [x] G1: The dedicated phase branch starts from finalized Phase 6J on main.
  CHECK: git branch --show-current && git merge-base --is-ancestor a0228e8 HEAD
  EXPECT: /phase-6k-places-and-devices/
  EVIDENCE: Current branch is `phase-6k-places-and-devices`; finalized Phase 6J commit `a0228e8` is its ancestor.

- [x] G2: Default Places include Home and every distinct existing XDG user directory without inventing or reconstructing paths from display text.
  CHECK: cargo test -p floe-app phase_6k_standard_locations -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Two focused tests pass for ordered existing XDG directories, omission, and exact-path deduplication.

- [x] G3: User bookmarks preserve exact Linux paths, persist atomically with private permissions, deduplicate, and reject invalid or missing directories without blocking GTK.
  CHECK: cargo test -p floe-app phase_6k_bookmark -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Seven bookmark tests pass for validation, non-UTF-8 binary round-trip, corrupt input, bounded queues, private atomic writes, structured failures, and shutdown.

- [x] G4: The sidebar is compact and pointer-resizable, separates Places, Bookmarks, and Devices, and exposes add/remove bookmark actions with accessible labels.
  CHECK: cargo test -p floe-app phase_6k_sidebar -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Four sidebar/controller tests pass; GtkPaned permits start-child resize/shrink with a 128-pixel floor and scrollable sections.

- [x] G5: GIO drive, volume, and mount snapshots use stable identities, distinguish mounted/unmounted/removable state, and update on VolumeMonitor signals.
  CHECK: cargo test -p floe-app phase_6k_device -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Ten device tests pass, including stable identity, monitor notification, and hierarchy collapse to one useful sidebar row.

- [x] G6: Mount, unmount, and eject requests go through an application-owned asynchronous device boundary with busy, success, and understandable failure states.
  CHECK: cargo test -p floe-app phase_6k_device_action -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Action tests and the safety scan pass; GIO async APIs reserve one action per device and return structured outcomes without shell or blocking filesystem calls.

- [x] G7: Mounted local devices navigate by authoritative paths; unmounted rows mount first; unavailable/non-local roots never corrupt navigation state.
  CHECK: cargo test -p floe-app phase_6k_sidebar_ -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Four sidebar/controller tests pass for exact non-UTF-8 navigation, mount-first activation, local navigation, and honest remote/busy/unavailable states.

- [x] G8: Formatting, compilation, strict Clippy, all tests, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: All commands passed; 171 tests passed: 138 application and 33 core.

- [x] G9: Native Wayland smoke keeps Floe healthy while sidebar actions and device monitoring are active.
  EVIDENCE: Floe owned `io.github.floe.FileManager`, exported 24 window actions, activated Refresh, answered D-Bus Ping, exited zero through Quit, and released its name; only known host warnings appeared.

- [x] G10: README, DESIGN, architecture, roadmap, development notes, and AGENTS status truthfully record Phase 6K behavior and limitations.
  CHECK: rg -n 'Phase 6K|Places|Bookmarks|Devices|VolumeMonitor' README.md DESIGN.md docs/ARCHITECTURE.md docs/ROADMAP.md docs/DEVELOPMENT.md AGENTS.md
  EXPECT: /Phase 6K/
  EVIDENCE: Documentation records implemented behavior, 171 measured tests, remote-root and sidebar-width limitations, and Phase 6L as next.

- [x] G11: The phase branch and main are committed, pushed, merged, and synchronized at the implementation checkpoint.
  CHECK: git rev-parse main phase-6k-places-and-devices origin/main origin/phase-6k-places-and-devices
  EXPECT: /^([0-9a-f]{40})\n\1\n\1\n\1$/
  EVIDENCE: Before ledger-only finalization, all four refs resolved to `e6a28b440a830ab56d9c676c3c62b24d6e1e43bf`.

- [x] G12: The gate checker reports every Phase 6K gate met.
  CHECK: node <unlazy-skill-dir>/scripts/gate-check.mjs --status GATES.md
  EXPECT: /ALL MET/
  EVIDENCE: The status-only checker reported `ALL MET (12 met)` after final ledger repair.
