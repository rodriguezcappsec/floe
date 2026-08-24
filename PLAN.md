# Plan: Floe Phase 6M permanent deletion

Mode: solo, depth 3.

## Contract

- `floe-core` owns exact multi-target validation, whole-batch no-follow
  preflight, mount/root refusal, deterministic postorder deletion, identity
  revalidation, progress, and the irreversible cancellation boundary.
- The application owns a fixed-capacity worker, job lifecycle mapping,
  request tracking, cancellation, and conservative retry policy.
- GTK gathers authoritative selected `PathBuf` values, presents one explicit
  irreversible confirmation, and submits an application command only.
- User-visible language is “Delete Permanently.” Floe does not claim secure
  erase, offer undo, or call committed partial deletion cancelled.
- Phase 6N Trash browsing, restore, Empty Trash, retention, and later roadmap
  work remain out of scope.

## Depth tree

1. Core destructive boundary
   - Validate raw paths and reject empty, relative, root, unnormalized,
     duplicate, and nested targets.
   - Preflight every selected tree without following symlinks or crossing
     mounts; revalidate device/inode/kind before removal.
   - Allow cancellation only before the first removal; afterward finish or
     report exact non-retryable partial failure.
2. Application boundary
   - Run requests on a named fixed-capacity worker.
   - Map queued/running/progress/completed/cancelled/partial/failed states into
     the shared job model.
   - Track affected directories and reject generic retry after partial commit.
3. GTK interaction and verification
   - Add selection-aware “Delete Permanently…” and `Shift+Delete`.
   - Make Cancel the initial/default focus; show target count and escaped exact
     paths in a scrollable copyable surface.
   - Verify focused tests, full Rust gates, isolated native Wayland smoke, and
     update the persistent project documentation.

## Status log

- 2026-08-24: Created `phase-6m-permanent-delete` from synchronized `main` and
  defined the phase contract and gates before coding.
- 2026-08-24: Core planner/executor, application tracking, confirmation UI,
  truthful feedback, and focused tests implemented.
- 2026-08-24: Formatting, workspace check, strict Clippy, all 205 tests, and
  diff hygiene pass. Two isolated native Wayland action smokes proved the
  selection-enabled action, confirmation activation without deletion, D-Bus
  health, fixture preservation, clean exit, and name release.
- 2026-08-24: Persistent documentation now records Phase 6M COMPLETE and Phase
  6N Trash lifecycle as the sole NEXT phase. Phase 6M is complete.
