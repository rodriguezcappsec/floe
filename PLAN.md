# Plan: Floe Phase 11E — Terminal Integration

## Contract

- Add a bounded application-owned terminal provider registry with deterministic availability and an explicit preferred provider.
- Launch providers with native process APIs and reviewed argv templates only; never invoke a shell, interpolate a path into command text, or place credentials in argv/environment.
- Resolve “Open Terminal Here” from one selected navigable local folder, otherwise the active local directory; reject Trash, remote, missing, non-directory, and stale targets truthfully.
- Open the terminal with the exact directory as child working directory so spaces, metacharacters, and non-UTF-8 Unix paths are not reconstructed from display text.
- Keep process discovery and launch off GTK callbacks through one bounded application worker and return generation-bound results.
- Add registered command/context/header/palette access plus a native preferred-terminal chooser and accessible status.
- Defer embedded terminals, shell sessions owned by Floe, command execution, repository detection, and future phases.

## Implementation leaves

1. Implement reviewed terminal providers, exact target policy, bounded requests/results, and tests.
2. Add an application-owned capacity-limited worker for executable discovery and direct no-shell launch.
3. Persist preferred provider through versioned settings migration without storing paths or command history.
4. Wire selection-aware action, chooser, menus/palette, feedback, and availability state.
5. Run hostile-path, focused, workspace, native Wayland, documentation, and gate verification.

## Status

COMPLETE on `phase-11e-terminal-integration`; all focused, workspace, native
Wayland, documentation, and completion gates are verified in `GATES.md`.
