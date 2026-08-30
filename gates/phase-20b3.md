# Gates: Floe Phase 20B3 — Compact Tabs

- [x] T1: Tabs have bounded compact adaptive width, height, spacing, label
  ellipsization, and restrained non-color-dependent active treatment. Device
  rows retain the device name above a concise free-space line; both labels stay
  single-line and tooltip-backed when the sidebar narrows.
  EVIDENCE: CSS and presentation tests verify 72px minimum width, 24px height,
  18-character centered end-ellipsized title budget, hover/focus 20px close
  control, flat nested target, one underline-only active marker, and distinct
  device-name/free-space label contracts.
- [x] T2: Full path tooltip, accessible tab/close names, keyboard activation,
  middle-click close, context menu, drag/drop, and horizontal overflow remain.
  EVIDENCE: Existing stable-ID actions and gestures remain unchanged; the title
  remains display-only and exact `PathBuf` state remains authoritative.
- [x] T3: Focused deterministic and real-GTK tab contracts pass without GTK or
  libadwaita criticals.
  EVIDENCE: Two focused deterministic tests and one ignored real-GTK property
  contract pass. Isolated native Wayland launch parsed CSS without criticals,
  answered `Peer.Ping`, accepted Quit, and exited 0; PID 473815.
- [x] T4: Formatting, workspace check, strict Clippy, workspace tests, strict
  docs/render/package checks, diff hygiene, and exactly one next phase pass.
  EVIDENCE: Workspace format/check/strict all-target/all-feature Clippy/tests,
  strict docs and rendering, package layout/migrations, E2E contracts, and diff
  hygiene pass. Exactly Phase 18Y2 is `NEXT`.
