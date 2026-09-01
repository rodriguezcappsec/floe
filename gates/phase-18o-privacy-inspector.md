# Gates: Floe Phase 18O — Privacy Inspector

Scope: bounded read-only format-specific privacy findings integrated with
Inspector and Properties without exhaustive-removal or safety claims.

- [x] O1: Worker extracts reviewed GPS/location, device/camera, author,
  organization, creator/application, timestamp, comment, and embedded-thumbnail
  evidence from supported formats under explicit byte/item limits.
  CHECK: `cargo test -p floe-app phase_18o_provider -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Focused provider tests parse a real minimal TIFF containing GPS latitude, camera maker, and date/time fields and verify PNG text evidence plus supported/unsupported/malformed outcomes.

- [x] O2: Missing, malformed, unsupported, inaccessible, symlink, oversized, and
  changed sources return explicit bounded outcomes without logging finding values.
  CHECK: `cargo test -p floe-app phase_18o_failures -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Focused failure test proves missing is inaccessible, symlink is not regular, sparse over-64-MiB input is too large, malformed/unsupported containers stay explicit, and exact metadata identity changes after mutation.

- [x] O3: Accessible Inspector/Properties presentation explains each evidence
  category and states that no findings is not an exhaustive privacy guarantee.
  CHECK: `cargo test -p floe-app phase_18o_ui -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Focused UI command contract requires evidence wording and the explicit `without declaring a file safe` limitation in both File Context and Header Menu placements; native menu launch is warning-free apart from the host AT-SPI refusal.
