# Plan: Floe Phase 8D — Miller column actions

Mode: sequential phase delivery with explicit implementation and verification gates.

## Contract

- Give every active and retained Miller column a native selection-aware item
  and background context menu with pointer and Shift+F10/Menu-key access.
- Carry exact logical depth and exact `Arc<DirectoryEntry>` identities to
  `BrowserController`; never reconstruct targets from labels or stale indices.
- Reuse existing Open, Open With, Copy, Cut, Rename, Trash, permanent-delete,
  create, duplicate, link, reveal, copy-text, and tab/split action semantics.
- Before dispatch, make the originating column the explicit action owner and
  synchronize its exact selection into application state. Unsupported actions
  remain disabled with truthful parity rather than silently targeting another
  column.
- Keep GTK callbacks command-only and preserve existing job/no-overwrite paths.
- Exclude cross-column drag/drop (8E), detail hooks (8F), and Preview (9).

## Depth tree

1. Exact action context
   - Define bounded depth/selection context, action eligibility, background/item
     ownership, and stale-column rejection.
2. Native menus
   - Add recycled-row secondary click, keyboard menu access, and background
     menus to every column with accessible ownership descriptions.
3. Controller routing
   - Activate the source column/session, synchronize exact selections, and
     delegate to existing actions/jobs without filesystem work in GTK.
4. Verification and handoff
   - Focused parity/staleness/non-UTF-8 tests, native Wayland smoke, full checks,
     docs, and exactly Phase 8E as `NEXT`.

## Status

COMPLETE on `phase-8d-miller-actions`. All gates are met. The sole recommended
next phase is `phase-8e-miller-drag-drop`.
