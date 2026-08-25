# Plan: Floe Phase 7E — Split interaction

Mode: sequential solo phase, depth 4.

## Contract

- Expose Phase 7D per-tab split state through one native horizontal `GtkPaned`:
  toggle, activate opposite side, close, swap, pointer and keyboard ratio changes.
- Keep one active virtualized list/grid model, browser worker, thumbnail worker,
  metadata worker, watcher, job manager, and Operations Island. The inactive pane
  shows a bounded read-only snapshot and exact path; activation restores its
  session through the existing shared pipeline.
- Provide explicit Open in Other Pane, Copy to Other Pane, and Move to Other Pane
  commands using exact paths and existing no-overwrite jobs. Do not add
  inter-pane drag-and-drop, detached windows, or Miller columns.
- Make active/inactive ownership textual and accessible, with pointer, menu, and
  keyboard alternatives. Preserve per-tab split state through clean restore.

## Depth tree

1. Split presentation
   - Native paned surface, active shell, inactive snapshot, bounded ratio.
   - Reparent one active virtualized presentation without cloning workers.
2. State interaction
   - Toggle, side switch, close, swap, and ratio capture/restore.
   - Per-tab restoration through existing generation-safe browser ownership.
3. Opposite-pane workflows
   - Folder open without focus theft; copy/move through existing bounded jobs.
   - Selection-sensitive actions, menus, shortcuts, and truthful feedback.
4. Verification and handoff
   - Focused tests, full gates, two-launch native Wayland smoke, persistent docs.

## Status

COMPLETE — all Phase 7E gates verified. Exactly one recommended next phase:
`phase-7f-tab-split-drag`.
