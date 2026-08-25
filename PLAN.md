# Plan: Floe Phase 11D — Optional Vim Mode

## Contract

- Add one explicit persisted Vim-navigation preference layered on the Phase 11C keybinding architecture; default remains off.
- Handle unmodified `h`, `j`, `k`, `l`, `g`, `G`, and reviewed selection/open keys only while a browser list, grid, or Miller file view owns focus.
- Preserve ordinary text input: entries, search entries, spin buttons, text views, dialogs, location editing, command palette, shortcut editor, and other editable controls keep native keys.
- Reuse existing navigation, selection, activation, parent/child, and focus commands; do not duplicate filesystem or navigation business logic.
- Expose a registered toggle action in the command palette/header and a visible non-color-only mode indicator.
- Keep custom shortcut conflict behavior stable and do not add terminal integration or future phases.

## Implementation leaves

1. Add GTK-independent Vim key policy/state with bounds and focus-context tests.
2. Persist the opt-in preference through versioned settings migration.
3. Wire one capture controller at the browser boundary to existing actions and view-specific movement.
4. Add registered toggle/discoverability, visible mode state, and accessibility semantics.
5. Run focused, workspace, native Wayland, documentation, and gate verification.

## Status

COMPLETE on `phase-11d-vim-mode`; verified gates are recorded in `GATES.md`.
