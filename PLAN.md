# Plan: Floe Phase 10E — Checksums

## Contract

- Add typed exact local regular-file checksum requests for SHA-256, SHA-512, and clearly legacy-labelled MD5.
- Stream bytes on a fixed-capacity application executor with no-follow opens, source identity/change revalidation, determinate byte progress, cancellation, bounded requests/results, and GTK responsiveness.
- Accept an optional strict expected hexadecimal digest only for one selected file; report match/mismatch without implying signature, authorship, freshness, safety, or authenticity.
- Add a selection-aware Calculate Checksums dialog and accessible result surface with selectable/copyable digest text; GTK callbacks submit jobs and never read files.
- Preserve original `PathBuf` identity. Lossy names are display-only and never reconstructed into targets.
- Reuse Operations Island lifecycle and expose specific checksum progress/completion/failure wording.
- Reuse GLib's reviewed checksum implementation already present in the dependency graph; do not invent hash constructions or add an unnecessary crate.
- Exclude saved integrity fingerprints/manifests, duplicate finding, copy verification, signatures, EXIF/media metadata, and Phase 10F work.

## Status

COMPLETE and verified on `phase-10e-checksums`. The sole recommended next phase
is 10F Advanced Metadata; do not begin Phase 11A command-registry work with it.
