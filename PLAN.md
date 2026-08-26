# Plan: Floe Phase 12C — Batch Rename

## Contract

- Add a preview-first batch rename model for prefix, suffix, literal find/replace, bounded regex, numbering/padding, case, and stable metadata/date templates.
- Preserve exact source `PathBuf` and raw `OsString` names. Lossy names are display-only; transforms that require Unicode reject non-UTF-8 names explicitly.
- Validate the whole batch before mutation: same-parent destinations, safe names, unique outputs, existing-item conflicts, source identity, capacity, and directory cycles.
- Apply through the application job boundary without overwrite; cancellation is allowed before commit and partial failures are explicit.
- Record the exact completed old/new mapping as one bounded in-session undo unit and revalidate before undo.
- Add a native accessible preview dialog with extension policy, validation feedback, deterministic ordering, and no per-keystroke filesystem work.
- Do not implement Phase 12D templates, Phase 12E links, or Phase 12F broad action integration.

## Implementation leaves

1. Add transform/template/preview/collision models and hostile/raw-name tests.
2. Add bounded no-overwrite batch apply/undo execution over existing move semantics and shared jobs.
3. Add native preview dialog, selection-aware action, job feedback, and refresh behavior.
4. Verify full-batch validation, cancellation, conflicts, non-UTF-8 policy, deterministic numbering, and undo revalidation.
5. Run full gates, native smoke, update persistent status, set only 12D next.

## Status

IMPLEMENTED on `phase-12c-batch-rename`; verification evidence is recorded in
`GATES.md`. Phase 12D is the sole recommended next phase.
