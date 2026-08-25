# Plan: Floe Phase 6S File watching

Mode: sequential solo phase, depth 4.

## Contract

- The application layer owns one GIO directory monitor for the active local browser location; the core filesystem crate remains GTK/GIO independent.
- Monitor callbacks only normalize exact local paths into a bounded coalescer. They never enumerate, inspect, or mutate filesystem content on the GTK thread.
- One cancellable coalescing timer collapses duplicate/burst events into a typed batch with capped changed paths and rename pairs; overflow requests one conservative reconciliation.
- Each accepted batch submits at most one superseding enumeration through the existing `BrowserWorker`, never one model rebuild per low-level event.
- A browser snapshot preserves exact selected paths and a stable scroll-anchor identity/index. Rename pairs translate identities; deleted items disappear cleanly; new items do not disturb surviving state.
- Reconciliation remains linear and bounded for 100k-entry locations, ignores stale generations/old directories, and keeps inaccessible/deleted-directory failures recoverable.
- Trash multi-root watching, recursive integrity monitoring, persistent baselines, and Phase 6T metadata/browser-completeness work remain out of scope.

## Depth tree

1. Monitor and coalescer
   - Add one active GIO monitor with explicit lifecycle and structured start failures.
   - Normalize event kinds, exact paths, rename pairs, caps, and one-shot debounce.
2. Reconciliation policy
   - Snapshot selection and scroll anchor before watcher refresh.
   - Reconcile exact identities and rename mappings against the new listing in O(n).
3. Browser integration
   - Stop stale monitors on navigation and restart only after successful local listings.
   - Submit one existing worker enumeration per coalesced batch and restore view state after batched model insertion.
4. Verification and records
   - Cover storms, duplicate events, create/delete/rename, stale generations, and 100k paths.
   - Run formatting, check, strict Clippy, workspace tests, diff hygiene, and native Wayland external-change smoke.
   - Update persistent docs to mark 6S complete and exactly 6T next.

## Status log

- 2026-08-24: Fast-forwarded verified Phase 6R commit `b6cc228` into `main` and pushed both branch and main.
- 2026-08-24: Created `phase-6s-file-watching` from clean synchronized `main`.
- 2026-08-24: Read the Phase 6S roadmap/matrix and inspected BrowserWorker supersession, listing/model batching, exact selection state, GTK scroll APIs, and existing GIO dependency.
- 2026-08-24: Selected GIO directory monitoring over a new crate under the project's standards-first integration priority and defined executable gates before coding.
- 2026-08-24: Implemented one active monitor, capped one-shot coalescing, exact event/rename mapping, stale-generation rejection, and one worker reload per accepted batch.
- 2026-08-24: Added exact selection plus stable path/index scroll-anchor reconciliation for watcher, manual, and operation refresh, including bounded rename chains and a 100k-path test.
- 2026-08-24: Formatting, workspace check, strict Clippy, 277 tests, diff hygiene, and isolated native Wayland create/rename/delete smoke passed.
- 2026-08-24: Updated persistent records to mark Phase 6S complete and exactly Phase 6T next.

## Status

COMPLETE — next: `phase-6t-browser-completeness`
