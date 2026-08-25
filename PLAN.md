# Plan: Floe Phase 6P Operation control

Mode: sequential solo phase, depth 4.

## Contract

- `floe-core` owns explicit progress units and identity validation needed by reversible move/rename attempts. Existing legal job transitions remain authoritative.
- The application layer owns batch boundaries, queue state, rate/ETA sampling, in-session history, scoped conflict decisions, and Undo dispatch.
- Pause is truthful: Floe pauses serial multi-item batches only between items. It never labels an actively executing syscall or GIO operation paused.
- Undo is offered only for completed move/rename attempts whose exact resulting object identity was captured by the worker. Reverse moves revalidate identity and use no-overwrite semantics.
- Keep Both uses bounded deterministic raw sibling naming and atomic no-replace attempts. Skip All is scoped to one stable batch. Replace and Replace All remain unavailable until backup/rollback semantics exist.
- History is bounded and memory-only. Clear Completed never removes failures, conflicts, cancellations, or partial outcomes.
- GTK callbacks submit application commands only; no filesystem mutation or metadata inspection runs on the GTK main loop.
- Phase 6Q create/duplicate/link functionality remains out of Phase 6P scope.

## Implemented depth tree

1. Core operation semantics
   - Added `ProgressUnit::{Unknown, Bytes, Items}` and unit-aware constructors.
   - Captured no-follow destination identity after successful move/rename.
   - Revalidated expected source identity before a reverse move can commit.
2. Batch and telemetry policy
   - Added bounded stable batch IDs, FIFO pending items, counts, pause/resume/cancel, and terminal snapshots.
   - Added deterministic smoothed byte telemetry with suppression for invalid samples.
3. Conflict, history, and Undo state
   - Added Keep Both, batch-scoped Skip All, bounded terminal history presentation, evidence-preserving clearing, and one-shot safe Undo.
4. GTK integration and verification
   - Added pause/history controls, unit-aware detail, batch summaries, conflict actions, and Undo controls through `ApplicationState` commands.
   - Added focused core/application tests, full workspace gates, native Wayland health smoke, and persistent documentation updates.

## Status log

- 2026-08-24: Created `phase-6p-operation-control` from verified Phase 6O main at `45f1fba` and defined gates before coding.
- 2026-08-24: Completed implementation and focused defect-hunt pass.
- 2026-08-24: Verified formatting, workspace check, strict Clippy, all 251 tests, diff hygiene, and isolated native Wayland/D-Bus startup, history action, and shutdown.
- 2026-08-24: Marked exactly Phase 6Q `NEXT`; Phase 6P does not implement Phase 6Q features.

## Status

COMPLETE
