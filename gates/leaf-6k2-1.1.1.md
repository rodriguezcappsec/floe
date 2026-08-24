# Gates: Phase 6K2 sidebar preference foundation

Scope: Backward-compatible, clamped, bounded persistence for sidebar density and divider width.

- [x] L1: SidebarDensity has compact, balanced, and comfortable stable persisted values with compact default.
  CHECK: cargo test -p floe-app phase_6k2_preference_density -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: One focused test passed; all three exact names round-trip, Compact is the default, and an invalid density falls back to Compact.

- [x] L2: Sidebar width parses, clamps, serializes, and round-trips while old view-only files remain compatible.
  CHECK: cargo test -p floe-app phase_6k2_preference_width -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: One focused test passed; legacy view/grid input remains intact, absent/malformed widths fall back to None, numeric bounds clamp to 128/480, and complete state round-trips.

- [x] L3: Existing bounded worker coalescing and shutdown preserve the newest complete preference set.
  CHECK: cargo test -p floe-app phase_6k2_preference_worker -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: One focused test passed; a capacity-one queued default was superseded at shutdown by complete non-default view, grid, density, and width state.
