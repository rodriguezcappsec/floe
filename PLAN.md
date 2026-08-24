# Plan: Floe Phase 6K places, bookmarks, and devices

Depth: tree 4

Mode: orchestrated

Budget note: This sidebar subsystem spans exact-path persistence, GIO lifecycle integration, GTK presentation, navigation wiring, and native verification.

## Contract

- Interfaces: `locations` owns standards-based local places; `bookmarks` owns exact-path persistence; `devices` owns GIO objects and asynchronous storage actions; `ui` presents rows; `browser` translates user intent into application-owned services and navigation.
- Data ownership: place/bookmark, device, UI integration, and documentation leaves own disjoint files.
- Paths: `PathBuf` remains authoritative. Display labels may be lossy, but no path is reconstructed from a label. Bookmark persistence round-trips raw Unix `OsStr` bytes.
- Responsiveness: bookmark I/O runs on a bounded worker; GIO mount operations use asynchronous APIs and report structured completion.
- Device scope: local mounted roots are navigable. Non-local roots remain visible but unavailable because remote browsing is a later phase.

## Tree

- 1 Phase 6K places, bookmarks, and devices ........ `GATES.md`
  - 1.1 Local navigation sources .................... `gates/node-1.1.md`
    - 1.1.1 XDG places and bookmark persistence ..... `gates/leaf-1.1.1.md`
  - 1.2 Storage integration ......................... `gates/node-1.2.md`
    - 1.2.1 GIO device monitor and actions .......... `gates/leaf-1.2.1.md`
  - 1.3 Sidebar integration ......................... `gates/node-1.3.md`
    - 1.3.1 GTK presentation and browser wiring ..... `gates/leaf-1.3.1.md`
  - 1.4 Product integration ......................... `gates/node-1.4.md`
    - 1.4.1 Documentation, native smoke, final QA .... `gates/leaf-1.4.1.md`

## Status log

- 2026-08-24 plan written; contracts fixed before leaf implementation.
- 2026-08-24 leaves 1.1.1 and 1.2.1 started in parallel.
- 2026-08-24 leaf 1.3.1 integrated stable backend interfaces.
- 2026-08-24 parent review collapsed duplicate GIO hierarchy rows.
- 2026-08-24 leaves 1.1.1, 1.2.1, and 1.3.1 parent-verified.
- 2026-08-24 native Wayland smoke passed ownership, Refresh, liveness, clean Quit, and name release.
- 2026-08-24 leaf 1.4.1 documented and verified Phase 6K at 8/8 gates.
