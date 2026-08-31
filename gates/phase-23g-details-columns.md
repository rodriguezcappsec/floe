# Gates: Floe Phase 23G — Complete details columns

- [x] D1: Owner, Group, Path, Link Target/broken state and existing media columns
  use bounded metadata, exact identity, unknown states and no-follow behavior.
  CHECK: `cargo test -p floe-app phase_23g_details_model -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. UID/GID and present/missing/inaccessible link tests pass.
- [x] D2: New columns join selectable, reorderable, resizable/autosizable,
  persistent virtualized layout with legacy migration.
  CHECK: `cargo test -p floe-core phase_23g_details_columns -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. 14-column legacy migration to 18-column u32 layout passes.
- [x] D3: Unsupported metadata stays unknown and no eager directory media scan,
  shell or privacy claim is introduced.
  EVIDENCE: Existing lazy bound-row metadata requests remain authoritative;
  documentation states unknown values are not guessed.
