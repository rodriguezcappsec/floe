# Plan: Floe Phase 9B — Image and passive text Preview

Mode: sequential phase delivery with explicit implementation and verification gates.

## Contract

- Register deterministic first-party raster-image and passive-text providers on the Phase 9A worker.
- Use exact no-follow source identity and existing limits.
- Decode one bounded raster frame to owned RGBA; decode bounded UTF-8/UTF-16 source for plain text, code, Markdown, JSON, and XML.
- Never render HTML, Markdown, SVG, scripts, macros, remote resources, or shell content.
- Create GTK textures/text widgets only on the main thread and release retired payloads on selection/navigation/cancellation.
- Exclude PDF/documents (9C), media (9D), fonts/archives (9E), and preview polish (9F).

## Status

COMPLETE on `phase-9b-preview-images-text`; all eight gates verified. Phase 9C is the sole recommended next phase.
