# Plan: Floe Phase 10C — Properties

## Contract

- Add a native, accessible, read-only Properties surface for exact current selection.
- Present General facts from Phase 10B: exact display path identity, kind, MIME, known size, dates, link state, dimensions, and immediate non-recursive folder facts.
- Present truthful multi-selection common/differing/unknown summaries without inventing shared values.
- Reuse existing asynchronous GIO Open With discovery and explicit set-default action; launching and association changes remain deliberate and separate.
- Add bounded asynchronous filesystem/mount facts for the exact local selection/common parent: filesystem type, total/free space, read-only state, and mount root when available.
- Calculate explicitly requested recursive selected-folder item/known-byte totals on the same cancellable bounded worker, never following symbolic links or crossing the entry cap silently.
- Keep GTK responsive; callbacks submit application work and render returned owned facts only.
- Preserve raw `PathBuf`/`OsString` identity, stale-generation rejection, no-follow source rules, and memory-only property state.
- Exclude Phase 10D mode/owner/group edits, root-process elevation, ACL/xattr changes, checksums, EXIF/media tags, and persistent property history.

## Status

COMPLETE on `phase-10c-properties`.

Implemented native read-only Properties over one capacity-8 stale-safe worker,
shared Phase 10B facts, containing GIO filesystem/mount facts, and capped
descriptor-relative recursive folder totals. Focused tests, strict checks, 382
tests, native build, diff hygiene, and live Wayland action/dialog/health/clean
quit passed. Phase 10D is the sole recommended next phase.
