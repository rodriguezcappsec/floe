# Plan: Floe Phase 9C — PDF and document Preview

## Contract

- Register one deterministic document provider after built-in raster/text providers.
- Reuse the reviewed freedesktop thumbnailer registry and supervised argv-only process boundary from Phase 6L.
- Limit eligibility to PDF and common office/document formats; never execute macros or active document content.
- Open the exact source no-follow before provider dispatch and revalidate it afterward.
- Accept only bounded passive PNG output, decode it to owned RGBA, and label it as a first-page/document rendition.
- Preserve cancellation, timeout, output limits, stale-generation rejection, honest unsupported fallback, and main-thread GTK object creation.
- State truthfully that installed external providers run with normal user authority and are not sandboxed until Phase 18L.
- Exclude audio/video (9D), font/archive (9E), and interaction polish (9F).

## Status

COMPLETE on `phase-9c-preview-documents`; all eight gates verified. Phase 9D is the sole recommended next phase.
