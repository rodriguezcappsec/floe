# Plan: Floe Phase 9F — Preview interaction and lifecycle polish

## Contract

- Add a Space shortcut for Quick Preview in Miller view while allowing Space to reach editable text controls unchanged.
- Keep an open Preview synchronized with exact selection/navigation generations and restore active-column focus on close.
- Add presentation-only zoom in/out/reset and fullscreen controls for image/document/font surfaces; never modify source content.
- Keep raster rendering monitor-scale aware through GTK paintables and bounded original pixel payloads.
- Add an explicit memory-preview-cache purge hook that cancels the current generation and clears successful cached payloads before the next request.
- Preserve media pause/clear retirement, stale-result rejection, accessibility text, reduced-motion behavior, and native Wayland responsiveness.
- Do not claim sandboxing or implement Inspector metadata (10A).

## Status

COMPLETE on `phase-9f-preview-polish`; all eight gates verified. Phase 10A is the sole recommended next phase.
