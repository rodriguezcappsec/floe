# Plan: Floe Phase 7B Tab interaction

Mode: sequential solo phase, depth 4.

## Contract

- Implement only live tab interaction over the Phase 7A `BrowserSession` model.
  Do not add closed-tab retention, startup persistence, split view, detached
  windows, or per-tab filesystem workers.
- Keep one shared virtualized directory model, browser worker, thumbnail worker,
  metadata worker, watcher, job manager, and operation surface. Switching tabs
  swaps exact bounded session state and supersedes stale browser responses.
- Provide accessible native tab controls for new, close, switch, duplicate,
  reorder, foreground/background folder open, middle-click open/close, and
  keyboard alternatives. Background opening must not steal focus.
- Preserve exact `PathBuf` selection/history/scroll identity. Lossy path text is
  display-only and never reconstructed into a path.

## Depth tree

1. GTK-independent live tab state
   - Bounded stable-ID collection over `BrowserSession`.
   - New, activate, duplicate, close, and deterministic reorder transitions.
2. Shared-browser session wiring
   - Capture active location/view/selection/scroll before transitions.
   - Restore complete destination state through the existing one-worker load.
3. Native tab interaction
   - Compact labelled tab strip with active semantics, close controls, tooltips,
     pointer reorder, middle click, context commands, and keyboard alternatives.
   - Folder foreground/background tab actions in list and grid.
4. Verification and handoff
   - Focused core/application tests, formatting/check/strict Clippy/workspace tests,
     native Wayland action/focus/lifecycle smoke, docs, and exactly Phase 7C next.

## Status

COMPLETE — exactly one recommended next phase: `phase-7c-tab-session-restore`.
