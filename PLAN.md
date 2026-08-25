# Plan: Floe Phase 10F — Advanced Metadata

## Contract

- Extend the existing Phase 10B Inspector provider with bounded, lazy, read-only advanced image and media/audio metadata for exact selected local regular files.
- Parse reviewed EXIF fields without executing content, following links, using network access, or turning metadata into Phase 18O privacy/security findings.
- Parse reviewed audio/media duration and common descriptive tags with explicit unavailable, unsupported, malformed, changed, and limit states.
- Add optional Dimensions, Duration, Artist, Album, and Track list columns that request enrichment only for bound visible rows; initial directory enumeration remains cheap.
- Preserve exact `PathBuf` identity and revalidate device/inode/size/mtime/ctime after parsing. Lossy strings are presentation-only.
- Keep requests, source reads, strings, tag counts, queues, results, and caches bounded; malformed metadata must fail recoverably and never freeze GTK.
- Persist the expanded optional-column layout through the existing versioned preferences path with migration and stable sorting/grouping behavior during asynchronous enrichment.
- Use maintained parser crates only after dependency/MSRV review; no shell helpers, network providers, persistent metadata cache, metadata editing, privacy verdicts, or Phase 11 command work.

## Status

COMPLETE on `phase-10f-advanced-metadata`; verified gates are recorded in `GATES.md`.
