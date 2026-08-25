# Plan: Floe Phase 6R Drag and drop

Mode: sequential solo phase, depth 4.

## Contract

- `floe-core` owns GTK-independent exact-path drag payload, destination, and requested-action policy where domain validation is useful.
- The application layer maps accepted drops onto existing bounded copy, move, symbolic-link, and Trash jobs. GTK callbacks never mutate or inspect the filesystem.
- Internal drags preserve every selected original `PathBuf`; external drops accept standards-based local file lists without reconstructing paths from display text.
- List/grid folder rows, directory background, eligible Places/bookmarks/devices, and Trash are explicit targets. Unsupported or unavailable targets reject the drop.
- Copy, move, and link actions are explicit; the negotiated action is surfaced before commit and no path may be overwritten implicitly.
- Hover-open and edge autoscroll use one bounded cancellable timer per active drag and are cancelled on leave/drop/navigation.
- Drop eligibility and destination feedback use native accessible labels/status text plus visual styling; color is never the only signal.
- Existing Copy/Cut/Paste, context-menu, and keyboard commands remain complete non-drag alternatives.
- Phase 6S file watching remains out of scope until Phase 6R is verified, pushed, and merged.

## Depth tree

1. Drag domain policy
   - Model exact internal/external sources, destination kinds, and requested action.
   - Validate duplicate/self/descendant/Trash/link cases without lossy path reconstruction.
2. Application operation routing
   - Add FIFO batch submission for copy, move, link, and Trash drops.
   - Reuse existing conflict, progress, pause/cancel, retry, and refresh semantics.
3. GTK interaction
   - Install drag sources on virtualized list/grid views.
   - Install directory-row/background, sidebar, bookmark, device, and Trash targets.
   - Add hover-open, autoscroll, action negotiation, and accessible drop feedback.
4. Verification and records
   - Add focused domain, state, and UI-policy tests.
   - Run formatting, check, strict Clippy, workspace tests, diff hygiene, and native Wayland drag-action smoke.
   - Update persistent docs to mark 6R complete and exactly 6S next.

## Status log

- 2026-08-24: Created `phase-6r-drag-drop` from verified Phase 6Q `main` at `1693c1a`.
- 2026-08-24: Read project instructions and inspected the roadmap/matrix, operation state, directory worker, list/grid virtualization, sidebar/device rendering, GTK dependencies, and accessibility guidance.
- 2026-08-24: Defined the Phase 6R contract and executable gates before implementation.
- 2026-08-24: Implemented exact GDK file-list sources/destinations, FIFO copy/move/link/Trash routing, recycled row and sidebar targets, hover-open, edge autoscroll, and accessible drop feedback.
- 2026-08-24: Formatting, workspace check, strict Clippy, 271 tests, diff hygiene, and native Wayland 42-action/health/clean-quit smoke passed.
- 2026-08-24: Updated persistent records to mark Phase 6R complete and exactly Phase 6S next.

## Status

COMPLETE — next: `phase-6s-file-watching`
