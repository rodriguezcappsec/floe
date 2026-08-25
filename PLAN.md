# Plan: Floe Phase 7A Tab/session model

Mode: sequential solo phase, depth 4.

## Contract

- Implement only a GTK-independent browser-session foundation. No tab widgets,
  tab bar, shortcuts, close/reorder behavior, startup restore, persistence,
  split view, duplicated browser workers, or changes to visible runtime UX.
- Reuse one canonical view-policy type family. Move the existing GTK-independent
  mode/grid/density/column/folder-view policy into `floe-core` rather than create
  duplicate session-only enums.
- A session owns stable ID and bounded complete location states: exact current
  path, back/forward history, exact multi-selection, stable path/index scroll
  anchor, sort/group/folder placement, view mode/grid size/density/columns.
- Navigation transitions move complete location state so Back/Forward can later
  restore selection, scroll, sort, and view without reading widget state.
- Provide a versioned, bounded, in-memory codec that preserves non-UTF-8 Linux
  paths and rejects relative paths, malformed/truncated/oversized data, invalid
  enum values, invalid/zero session IDs, and trailing bytes. Phase 7C owns file
  persistence and privacy policy; Phase 7A performs no I/O.

## Depth tree

1. Canonical view policy
   - Relocate existing GTK-independent view types and focused tests to core.
   - Keep application action mapping as a thin app-owned layer.
2. Session state machine
   - Stable nonzero session ID.
   - Complete current/back/forward location state with explicit bounds.
   - Exact selection and scroll-anchor updates; navigation preserves full state.
3. Versioned serialization boundary
   - Raw Unix path-byte encoding, deterministic view encoding, explicit limits.
   - Round-trip and hostile-input tests without filesystem/config writes.
4. Verification and handoff
   - Focused session/view tests plus full formatting/check/Clippy/tests/diff gates.
   - No native Wayland smoke unless runtime code changes; update persistent docs,
     mark Phase 7A complete, set exactly Phase 7B next, and stop.

## Status

COMPLETE — exactly one recommended next phase: `phase-7b-tabs-interaction`.
