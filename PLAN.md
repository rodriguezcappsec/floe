# Plan: Floe Phase 11C — Keybindings

## Contract

- Add a bounded GTK-independent keybinding override model over the Phase 11A command registry.
- Persist versioned overrides through the existing application-owned preference worker; GTK callbacks perform no filesystem I/O.
- Validate and canonicalize GTK accelerator text, reject duplicate bindings, and surface exact command conflicts before applying changes.
- Keep irreversible commands unassignable and confirmation-required commands limited to their reviewed defaults.
- Add a native searchable Keyboard Shortcuts dialog listing every registered command, its effective shortcuts, category, and availability.
- Support editing, individual reset, and reset-all with immediate accelerator reinstallation and accessible status/error feedback.
- Preserve existing defaults and migrate earlier preference files without overrides.
- Exclude Vim mode, terminal integration, new filesystem operations, and future roadmap phases.

## Implementation leaves

1. Implement bounded override parsing, serialization, effective-binding resolution, conflict detection, and tests.
2. Integrate overrides into `ViewPreferences` and the existing asynchronous preference worker.
3. Install effective application accelerators from registry defaults plus validated overrides.
4. Add the registered Keyboard Shortcuts action/dialog and live edit/reset behavior.
5. Run focused, workspace, native Wayland, accessibility, and documentation gates.

## Status

COMPLETE on `phase-11c-keybindings`; verified gates are recorded in `GATES.md`.
