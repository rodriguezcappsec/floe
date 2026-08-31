# Gates: Floe Phase 23A — Multi-window browsing

- [ ] W1: One application-scoped service set owns jobs, recovery, devices and
  desktop integration for every window.
  EVIDENCE: Not implemented. Secondary windows deliberately use isolated
  transient state to avoid destructive event-consumer and persistence races.
ABANDON: W1 This is the bounded Phase 23H follow-up, not safe supporting work.
- [x] W2: New Window and Open Folder in New Window preserve exact identity,
  never reuse/close another window, and newest-live routing survives closure.
  CHECK: `cargo test -p floe-app phase_23a_multi_window -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Two focused registry/action contracts pass; app action uses
  raw byte-string target and Ctrl+N.
- [ ] W3: Versioned bounded multi-window session restoration with legacy and
  Private/Sensitive migration exists.
  EVIDENCE: Not implemented; only the primary legacy window persists.
ABANDON: W3 Phase 23H owns the session-set model and migration.
- [x] W4: Native Wayland process exports New Window and exact-target actions,
  accepts repeated new-window activation, remains responsive, and quits cleanly.
  EVIDENCE: Isolated launch described `new-window` and `open-new-window(ay)`,
  accepted New Window twice, answered Peer.Ping after each, accepted Quit and
  exited cleanly. Exact visible-window count and close-one semantic input remain
  unverified because Dogtail/pyatspi are unavailable.
