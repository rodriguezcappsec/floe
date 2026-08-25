# Plan: Floe Phase 11B — Command Palette

## Contract

- Add Ctrl+Shift+P native palette over the Phase 11A command registry and existing GActions.
- Provide bounded deterministic metadata-only search across command names, categories, descriptions, and search terms; never search file paths or contents.
- Show live enabled/disabled state from the window action map and never duplicate eligibility or business logic.
- Support keyboard-first search, Down/Up selection, Enter activation, Escape close, visible focus, and screen-reader names/descriptions.
- Retain at most 16 recent command action IDs in memory only for the current process; do not persist command history.
- Bound query length, results, rows, and recent history; rebuild only small GTK models on the main loop.
- Exclude Phase 11C shortcut customization, Vim mode, terminal integration, and filesystem changes.

## Status

COMPLETE on `phase-11b-command-palette`; verified gates are recorded in `GATES.md`.
