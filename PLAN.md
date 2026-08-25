# Plan: Floe Phase 7C Closed tabs and session restore

Mode: sequential solo phase, depth 4.

## Contract

- Extend the bounded live-tab state with recently closed tabs, reopen closed,
  close-left/right/others, and `Ctrl+Shift+T`. Do not add split view, detached
  tabs/windows, optional custom names, or pins.
- Persist only the minimum versioned bounded browser workspace needed for normal
  startup restore. Preserve raw non-UTF-8 paths; reject malformed, oversized,
  duplicate-ID, empty, relative, and unsupported-version data.
- Load and save outside GTK callbacks through a fixed-capacity application
  worker. Use private directories/files and same-directory atomic replacement.
- Centralize an explicit persistence policy. Private or Sensitive state must
  suppress writes and remove Floe's session file; absence/corruption falls back
  safely to one normal initial tab without overstating Private Mode.

## Depth tree

1. Bounded workspace model and codec
   - Recently closed LIFO cap, close variants, reopen with fresh stable ID.
   - Active ordered sessions plus recently closed, version, limits, hostile input.
2. Private atomic persistence worker
   - Startup load, capacity-one clean-shutdown save, shutdown flush, 0700/0600,
     no-follow reads, bounded bytes, atomic rename, explicit suppression.
3. Runtime commands and lifecycle
   - Ctrl+Shift+T and close-left/right/others in tab menus/actions.
   - Restore valid workspace at activation; save final tab/session state on clean
     shutdown without GTK filesystem I/O or background focus theft.
4. Verification and handoff
   - Focused hostile-input/privacy/worker/UI tests, two-launch native Wayland
     restore/action/lifecycle smoke, full gates/docs, and exactly Phase 7D next.

## Status

COMPLETE — exactly one recommended next phase: `phase-7d-split-state`.
