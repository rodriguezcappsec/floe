# Plan: Floe Phase 9D — Audio and video Preview

## Contract

- Add a deterministic local media provider for reviewed audio/video extensions and GIO MIME identity.
- Preflight exact sources no-follow and return only typed path/MIME/poster metadata from the worker.
- Create GTK MediaFile, Video, and MediaControls objects only on the main thread; never shell out or install codecs.
- Use the existing supervised freedesktop thumbnailer boundary for an optional bounded passive video poster frame; audio has a truthful icon fallback.
- Expose native play/pause, seeking, duration/error state through GTK controls.
- Explicitly pause and release the active stream whenever selection, directory, view, or detail state retires it.
- Preserve cancellation, stale-generation rejection, main-loop responsiveness, and honest unsupported/decoder error feedback.
- Exclude font/archive preview (9E) and global preview interaction polish (9F).

## Status

COMPLETE on `phase-9d-preview-media`; all eight gates verified. Phase 9E is the sole recommended next phase.
