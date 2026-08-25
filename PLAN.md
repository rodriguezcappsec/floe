# Plan: Floe Phase 9E — Font specimens and archive listings

## Contract

- Add a reviewed installed-provider font specimen path that accepts only bounded passive PNG output and never installs a font.
- Add GTK-independent bounded parsers for ZIP central-directory and uncompressed TAR listings; do not invoke archive commands or extract content.
- Preserve raw archive member bytes separately from lossy display labels and visibly flag absolute/traversal-like member names.
- Enforce exact no-follow source identity, source/output/name/entry caps, cancellation, malformed/truncated input rejection, and stale-generation handling.
- Present font specimens as passive images and archive contents as selectable read-only text on the GTK thread.
- Report compressed TAR, encrypted content details, ZIP64, and unsupported formats honestly without speculative parsing.
- Exclude global Preview keyboard/fullscreen/zoom polish (9F).

## Status

COMPLETE on `phase-9e-preview-fonts-archives`; all eight gates verified. Phase 9F is the sole recommended next phase.
