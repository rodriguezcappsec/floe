# Plan: Floe Phase 6N Trash lifecycle

Mode: solo, depth 4.

## Contract

- `floe-core` owns exact Trash entry metadata, freedesktop `.trashinfo` parsing,
  no-overwrite restore requests, and isolated local Trash-root discovery.
- The application owns bounded Trash enumeration/restore workers, operation
  lifecycle mapping, permanent-delete reuse, and aggregate Empty Trash state.
- GTK owns only Trash navigation, selection-aware actions, confirmations, and
  status/error presentation. GTK callbacks perform no filesystem work.
- Backing Trash paths and decoded original paths remain exact `PathBuf` values;
  lossy labels are display-only and never reconstruct filesystem targets.
- Restore never overwrites. Destination conflicts remain explicit. Empty Trash
  requires one aggregate irreversible confirmation and uses the verified Phase
  6M permanent-delete engine.
- Floe makes no secure-erasure claim. Cleanup/retention preferences, Trash undo,
  remote Trash, Phase 6O transfer semantics, and later roadmap work stay out of
  scope.

## Depth tree

1. Standards and core model
   - Discover only supported local freedesktop Trash roots without scanning user
     data or following symlinked metadata roots.
   - Parse `[Trash Info]`, percent-encoded `Path`, and `DeletionDate` with
     bounded metadata reads and exact byte preservation where representable.
   - Enumerate backing entries and pair them with safe metadata; malformed or
     orphaned entries remain visible with unavailable restore metadata.
   - Restore through an exact no-replace request, reject unsafe destinations,
     and remove `.trashinfo` only after the payload move succeeds.
2. Application services
   - Add fixed-capacity, cancellable Trash browse/restore worker requests.
   - Map restore jobs into the shared job registry and Operations Island without
     adding GTK filesystem work.
   - Route individual Delete Permanently and confirmed Empty Trash batches to
     the existing Phase 6M executor.
3. Browser and interaction
   - Add a first-class Trash sidebar destination and explicit browser mode.
   - Show deletion date and original location from standards metadata when
     available, retaining normal list/grid virtualization and selection.
   - In Trash mode expose Restore, Delete Permanently, and Empty Trash; suppress
     inapplicable normal-location mutation actions.
   - Refresh Trash after restore/deletion and preserve responsive navigation.
4. Verification and persistence
   - Add core, worker, state, policy, and UI-surface tests using temporary Trash
     roots only.
   - Run formatting, workspace check, strict Clippy, all tests, diff checks, and
     isolated native Wayland smoke without touching the real user Trash.
   - Update persistent architecture, roadmap, feature, design, privacy/security,
     gates, and project-status documentation; set exactly one next phase.

## Status log

- 2026-08-24: Created `phase-6n-trash-lifecycle` from synchronized `main` at
  `942a0c0`; inspected Phase 6M deletion, GIO Trash, application state, browser,
  operations, sidebar, model, worker, roadmap, and privacy/security boundaries.
- 2026-08-24: Phase contract and executable gates defined before coding.
- 2026-08-24: Added local home/mounted Trash discovery, bounded no-follow
  `.trashinfo` parsing, exact metadata-bearing entries, and no-replace restore.
- 2026-08-24: Added fixed-capacity restore execution, shared job/conflict/batch
  state, first-class Trash mode, Restore/Delete Permanently/Empty Trash actions,
  metadata cleanup, and aggregate safe-focus confirmation.
- 2026-08-24: Verified 220 tests, formatting, check, strict Clippy, diff hygiene,
  isolated native Trash/Empty Trash action smoke, and an isolated end-to-end
  restore that removed matching metadata without touching real user Trash.
- 2026-08-24: Updated persistent project documentation and selected only Phase
  6O transfer semantics as NEXT.

## Status

COMPLETE
