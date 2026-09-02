# Gates: adversarial core copy remediation

Scope: Fix two confirmed ordinary-copy identity/ownership defects in
`floe-core` using only isolated temporary filesystem tests.

- [x] C1: Ordinary copy opens planned regular files without following a
  substituted symbolic link and rejects kind or identity changes before bytes
  are accepted.
  CHECK: `cargo test -p floe-core adversarial_copy_source_identity -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS; 2 focused tests passed, including same-size regular-file
  inode substitution and a substituted symlink rejected before destination
  creation.

- [x] C2: Cancellation/error rollback removes only the exact file or directory
  object Floe created; a replacement inode remains intact and ownership loss is
  reported explicitly.
  CHECK: `cargo test -p floe-core adversarial_copy_cleanup_identity -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS; 3 focused tests passed. Replacement file and directory
  inodes remained at their exact paths, including a replacement injected in
  the former check/remove window; ownership loss returned `CleanupFailed`.

- [x] C3: Existing copy, move, verified-copy, non-UTF-8, symlink,
  cancellation, metadata, and no-overwrite semantics remain passing.
  CHECK: `cargo test -p floe-core -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS; 182 `floe-core` unit/property tests and 6 isolated duplicate
  workflow integration tests passed; doc tests also passed.

- [x] C4: Core remains GTK-independent, adds no dependency, preserves exact
  `PathBuf` identity, and documents conservative cleanup partials.
  EVIDENCE: `copy.rs` alone adds rustix no-follow descriptor checks, raw
  `PathBuf`/device/inode/kind tracking, and atomic no-overwrite cleanup
  quarantine. Its documentation records Linux's lack of conditional
  unlink-by-inode and conservative partial failure. Core strict Clippy,
  package formatting, and diff hygiene pass.
