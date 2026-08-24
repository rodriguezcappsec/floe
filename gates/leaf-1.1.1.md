# Gates: XDG places and bookmark persistence

Scope: standards-based local places plus bounded, private, exact-path user-bookmark persistence.

- [x] G1: Home and every deliberate existing distinct XDG user directory are returned in the documented order from authoritative `PathBuf` values.
  CHECK: cargo test -p floe-app phase_6k_standard_locations -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: 2 focused location tests passed; 0 failed.

- [x] G2: Bookmark validation accepts only existing directories, deduplicates exact paths, and round-trips raw non-UTF-8 Unix path bytes through a versioned private binary format.
  CHECK: cargo test -p floe-app phase_6k_bookmark -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: 3 focused validation and binary-format tests passed; 0 failed.

- [x] G3: Bookmark persistence uses a bounded nonblocking queue, atomic same-directory writes, 0700 parent and 0600 file modes, structured errors, and bounded clean shutdown.
  CHECK: cargo test -p floe-app bookmark_worker -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: 4 focused bookmark-worker tests passed; 0 failed.

- [x] G4: The two owned modules are formatted and pass strict Clippy in the application target.
  CHECK: cargo fmt --all -- --check && cargo clippy -p floe-app --all-targets -- -D warnings
  EXPECT: /Finished/
  EVIDENCE: cargo fmt --all -- --check exited 0; strict floe-app all-target Clippy finished successfully.

- [x] G5: Scope remains confined to `locations.rs`, `bookmarks.rs`, and this leaf ledger.
  CHECK: git status --short -- crates/app/src/locations.rs crates/app/src/bookmarks.rs gates/leaf-1.1.1.md
  EXPECT: /crates\/app\/src\/locations\.rs/
  EVIDENCE: scoped git status listed only crates/app/src/locations.rs, crates/app/src/bookmarks.rs, and gates/leaf-1.1.1.md.
