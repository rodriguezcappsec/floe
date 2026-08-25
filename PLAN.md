# Plan: Floe Phase 10B — Metadata providers

## Contract

- Add a bounded GTK-independent lazy metadata-provider worker for exact selected local entries.
- Provide no-follow MIME, exact timestamps, Unix owner/group/mode, symlink target/status, and safe image-dimension facts.
- Provide bounded non-recursive folder child counts and known immediate-child bytes only on explicit Inspector demand; do not claim recursive folder size.
- Key requests and responses by exact path identity plus selection generation; stale, changed, missing, unreadable, and oversized work must remain explicit.
- Merge provider facts into the existing read-only Inspector without eager whole-directory enrichment or GTK filesystem work.
- Preserve raw `PathBuf`/`OsString` identity, bounded queues/state, cancellation/supersession, privacy boundaries, and Preview behavior.
- Exclude Phase 10C properties pages, metadata/permission edits, recursive traversal, checksums, EXIF/media tags, and persistent metadata caches.

## Status

COMPLETE on `phase-10b-metadata-providers`.

Implemented one fixed-capacity, generation-superseding Inspector provider that
keeps exact path identity, performs no-follow/revalidated read-only metadata
work off GTK, and exposes truthful bounded single/multi-selection facts. Focused
tests, strict workspace checks, 379 tests, native build, diff hygiene, and live
Wayland action/health/clean-quit lifecycle all passed. Phase 10C is the sole
recommended next phase.
