# Gates: Floe Phase 6T Browser completeness

Status: COMPLETE

- [x] G1: Work stayed on `phase-6t-browser-completeness`; no Phase 7 code was
  introduced. `git branch --show-current` and diff audit confirmed scope.
- [x] G2: Extension sorting, directory placement, and independent Type/Extension
  grouping are GTK-independent, deterministic, and exact-path-safe. Three
  `phase_6t_sort_group` core tests passed, including non-UTF-8 identity.
- [x] G3: Extended metadata is requested only by bound rows through a capacity-64
  worker and 512-entry cache. Stale identity, saturation, deduplication, eviction,
  and safe fallback tests passed; recycled row bindings reject delayed prior work.
- [x] G4: Nine list columns have stable semantics, optional visibility, clamped
  pointer widths, keyboard/menu parity, and accessible Extension sorting. Three
  `phase_6t_columns` tests passed; the shared model and selection remain intact.
- [x] G5: Compact, Comfortable, and Spacious apply to shared list/grid widgets;
  density-only changes do not replace the factory and focus remains visible. Two
  `phase_6t_density` tests passed.
- [x] G6: Legacy preferences migrate; complete global state and opt-in exact
  raw-path folder overrides persist atomically with a 256-record cap. Three
  `phase_6t_preferences` tests passed.
- [x] G7: Status text reports known non-recursive bytes honestly. A capacity-32
  GIO worker supplies current-location and mounted-device total/free/read-only
  facts with generation, device ID, and exact path checks. Five
  six `phase_6t_status` tests passed.
- [x] G8: Lazy metadata callbacks update presentation only. Deliberate sort/group
  changes use the existing browser worker and preserve exact selection/anchors.
- [x] G9: Source/dependency audit found no new shell, privilege, recursive walk,
  eager directory enrichment, lossy path reconstruction, unbounded worker/cache,
  filesystem mutation in GTK callbacks, or dependency addition.
- [x] G10: `cargo fmt --all -- --check`, `cargo check --workspace`, strict Clippy,
  `cargo test --workspace`, and `git diff --check` passed. Totals: 228 application
  and 66 core tests, 294 overall.
- [x] G11: Native Niri/Wayland smoke used isolated HOME/XDG roots, exported 73
  actions, exercised/restored density, grouping, directory placement, columns,
  and width across two launches, answered D-Bus health checks, exited cleanly,
  and released `io.github.floe.FileManager` twice. Spectacle produced no image;
  action state and lifecycle are the native evidence.
- [x] G12: Roadmap, matrix, design, architecture, development, privacy/security,
  plan, gates, and persistent status record verified Phase 6T and mark exactly
  Phase 7A as `NEXT`.

Recommended next phase: `phase-7a-tabs-foundation`. Stop before implementing it
on this branch.
