# Plan: Floe Phase 11A — Command Registry

## Contract

- Add one application-layer registry for human-readable command identity, descriptions, categories, search terms, default shortcuts, menu placement, and risk level.
- Reuse existing `app.*` and `win.*` actions as the sole execution path; registry entries contain no filesystem or business logic.
- Resolve current eligibility from the authoritative GAction enabled state rather than duplicating selection, Trash, split, view, or job conditions.
- Centralize existing default window accelerators in registry metadata without changing established shortcuts.
- Cover normal user-invokable commands; explicitly classify parameterized/internal widget plumbing outside the searchable command surface.
- Keep registry order deterministic, labels unique, metadata static and bounded, and action lookup recoverable.
- Add GTK-independent registry validation and native GAction parity/accessibility smoke.
- Exclude the Phase 11B command palette, shortcut customization, Vim mode, terminal launching, and unrelated menu redesign.

## Status

COMPLETE on `phase-11a-command-registry`; verified gates are recorded in `GATES.md`.
