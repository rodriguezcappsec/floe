# Gates: Floe Phase 18P — Metadata sanitization

Scope: preview-first, no-overwrite sanitized copies for explicitly supported
formats; the original is never modified or removed.

- [x] P1: Format-specific sanitizer removes only documented JPEG/PNG/WebP privacy
  metadata, preserves decodable content, rejects malformed/unsupported/symlinked
  sources, and verifies the selected metadata is absent from output.
  CHECK: `cargo test -p floe-app phase_18p_formats -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Focused format tests remove reviewed JPEG/PNG/WebP metadata, verify no reviewed finding remains, reject malformed data, and clear retained WebP VP8X EXIF/XMP feature bits.

- [x] P2: Application worker uses exact identity, private staging, cancellation,
  output limits, source revalidation, atomic no-replace publication, and explicit
  cleanup/partial outcomes; no failure changes source bytes.
  CHECK: `cargo test -p floe-app phase_18p_worker -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Focused worker tests cover mixed batch results/cancellation and failed-revalidation stage cleanup; sibling-copy tests preserve source bytes, skip occupied names, use private create-new stages, and publish with `RENAME_NOREPLACE`.

- [x] P3: Selection-aware accessible preview/confirmation lists removable
  categories, destination, unsupported items, and truthful after-result evidence;
  completed output is revealed without selecting a substitute.
  CHECK: `cargo test -p floe-app phase_18p_ui -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Focused UI command contract requires source-preserving, no-overwrite, JPEG/PNG/WebP wording and a cancellation description that retains already verified copies. Browser integration confirms before submission and reveals the first exact successful destination.
