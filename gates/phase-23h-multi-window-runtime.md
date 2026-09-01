# Gates: Floe Phase 23H — Multi-window runtime and session hardening

Scope: one application-owned mutation/event/persistence boundary plus bounded
multi-window restore without weakening window-local navigation and focus.

- [x] H1: Normal windows share one mutation/recovery state and terminal event
  coordinator; no window starts a competing operation/recovery writer or steals
  another window's event/conflict.
  CHECK: `cargo test -p floe-app phase_23h_runtime -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Three focused runtime tests enforce 16-window capacity, one active presentation owner with close transfer, and bounded generation routing so another window cannot steal privacy/ClamAV results.

- [x] H2: A versioned session envelope round-trips at most 16 raw-path window
  workspaces, migrates the legacy one-window file, and safely rejects corrupt,
  duplicate, oversized, Private, and Sensitive state.
  CHECK: `cargo test -p floe-app phase_23h_session -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Two focused session tests round-trip multiple raw-path workspaces, migrate legacy one-window data, and reject count, trailing, corrupt, oversized, duplicate, Private, and Sensitive restore state.

- [x] H3: Closing one idle window does not stop shared jobs or freeze survivors;
  active work remains observable and close-one/new-window/clean-Quit lifecycle is
  regression-covered.
  CHECK: `cargo test -p floe-app phase_23h_close -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Deterministic close test proves the application state survives one unregister and a third window can register. KDE Wayland native smoke reports `close-survivor-responsive=true third-window=true restored-windows=true`, clean Quit, and a two-window restart from the same private roots; the only remaining critical is the host's refused AT-SPI bus.
