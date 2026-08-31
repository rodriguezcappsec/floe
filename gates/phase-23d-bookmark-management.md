# Gates: Floe Phase 23D — Bookmark organization

- [x] B1: Version-2 bookmark records preserve raw paths, bounded aliases and
  ordering, migrate v1, fail malformed records and preserve missing targets.
  CHECK: `cargo test -p floe-app phase_23d_bookmark_model -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Codec/model and missing-target worker tests pass.
- [x] B2: Rename, Reset Name, Move Up/Down and Remove are deterministic,
  endpoint-sensitive and never alter path identity.
  CHECK: `cargo test -p floe-app phase_23d_bookmark_actions -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Record action model test passes; GTK uses one accessible options menu.
- [x] B3: Narrow-sidebar/raw-name behavior remains bounded.
  EVIDENCE: One ellipsized exact-path-tooltip row plus one options button replaces
  five inline buttons; raw path remains captured independently from label.
