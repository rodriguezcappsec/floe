# Plan: Floe Phase 12B — Archive UX

## Contract

- Add native application-layer workflows for Extract Here, Extract To, and Compress using only the verified Phase 12A executor.
- Resolve exact selected `PathBuf` sources and deterministic proposed destinations without reconstructing identity from lossy labels.
- Present destination, format, conflict, unsupported/password, progress, cancellation, completion, and failure states with native GTK controls and accessible text.
- Keep archive parsing and filesystem mutation off GTK; dialogs only validate bounded names/options and submit typed jobs.
- Preserve no-overwrite semantics. Existing destinations produce explicit conflict feedback and no hidden retry/replace behavior.
- Keep password/encrypted archives unsupported until a reviewed secret-capable backend exists; show truthful unsupported guidance and accept no secret.
- Do not add Phase 12C rename, Phase 12D templates, Phase 12E link polish, or Phase 12F broad context/palette/shortcut integration.

## Implementation leaves

1. Define deterministic archive UX planning for selection eligibility, archive format, Extract Here/To destinations, and collision-safe default compression names.
2. Build native accessible Extract/Compress dialogs and selection-aware application actions that submit exact typed requests.
3. Observe archive jobs through the existing shared Operations lifecycle, support cancellation, refresh affected directories, and show bounded results/failures.
4. Add focused model/UI/state tests for raw paths, destination preview, conflicts, password-required truthfulness, action eligibility, and progress handling.
5. Run formatting, strict Clippy, workspace tests, native Wayland smoke, documentation updates, and sole-next-phase verification.

## Status

IMPLEMENTED on `phase-12b-archive-ui`; verification evidence is recorded in
`GATES.md`. Phase 12C is the sole recommended next phase.
