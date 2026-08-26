# Plan: Floe Phase 12F — Productivity Action Integration

## Contract

- Expose the existing selection-aware Extract Here, Extract To, and Compress
  actions in a compact Archives submenu in every normal file context surface.
- Keep a small reviewed essential action set fixed while allowing users to show
  or hide bounded optional productivity groups in file and background context
  menus through an accessible native dialog.
- Persist only stable reviewed group identifiers through the existing bounded
  preference worker; migrate older preference files with safe defaults.
- Rebuild list, grid, Miller, and background menu models from the same policy so
  pointer and keyboard invocation have parity and live GAction eligibility stays
  authoritative.
- Register customization as a human-readable command reachable from the header,
  command palette, and Keyboard Shortcuts dialog.
- Preserve exact paths and existing archive/job architecture; GTK builds menus
  and submits existing actions but performs no filesystem work.
- Exclude arbitrary external commands, plugin actions, shell command templates,
  per-MIME rules, privacy/safe-open actions, and later roadmap phases.

## Status

COMPLETE on `phase-12f-productivity-actions`; verified evidence is recorded in
`GATES.md`. Phase 13A folder filter is the sole recommended next phase.
