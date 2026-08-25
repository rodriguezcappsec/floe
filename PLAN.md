# Plan: Floe Phase 6O Transfer semantics

Mode: solo, depth 4.

## Contract

- `floe-core` owns destination-space preflight, basic metadata preservation,
  source-tree identity checks, and no-overwrite cross-filesystem move semantics.
- The application owns bounded executor dispatch, structured job outcomes, and
  interoperable desktop clipboard parsing/publishing. GTK callbacks only submit
  application commands and asynchronous clipboard requests.
- Original `PathBuf`/`OsString` values remain authoritative. URI clipboard text
  is decoded through filename-URI APIs and never reconstructed from labels.
- Copy preserves supported POSIX mode plus access/modification timestamps.
  Ownership, ACLs, extended attributes, sparse extents, reflinks, capabilities,
  and security labels are not claimed as preserved.
- Cross-filesystem move keeps the source until a fully copied staging tree is
  synchronized, revalidated, and atomically finalized without overwrite. A
  post-commit delete failure is an explicit non-retryable partial outcome.
- Queue controls, richer progress/history, overwrite/apply-to-all, verified-copy
  hashing, drag and drop, remote transfers, and later roadmap phases stay out of
  scope.

## Depth tree

1. Copy contract
   - Capture no-follow source identity, POSIX mode, access time, and modified time.
   - Check destination filesystem availability before creating output.
   - Preserve supported metadata after content and synchronize regular files.
   - Keep cleanup exact and report unsupported/insufficient-space cases truthfully.
2. Cross-filesystem move
   - Retain atomic `RENAME_NOREPLACE` fast path.
   - On `EXDEV`, copy to a collision-safe hidden sibling staging path.
   - Revalidate the complete source tree before finalization; never delete a
     source that changed while copying.
   - Atomically publish staging, then remove only identity-matching source nodes.
     Classify cancellation/failure after publication as partial completion.
3. External clipboard
   - Publish bounded `text/uri-list`, `x-special/gnome-copied-files`, and KDE cut
     marker formats alongside the exact in-process transfer buffer.
   - Read supported formats asynchronously, validate local file URIs and limits,
     deduplicate exact paths, and stage them through `ApplicationState`.
   - Prefer the richer GNOME copy/cut format, then URI list plus KDE cut marker;
     reject remote/invalid clipboard targets without filesystem work in GTK.
4. Verification and records
   - Add focused core, executor, state, and clipboard tests covering space,
     timestamps/mode, cross-device fallback injection, cancellation/partials,
     symlinks, conflicts, and non-UTF-8 paths.
   - Run formatting, workspace check, strict Clippy, all tests, diff hygiene, and
     isolated native Wayland clipboard/transfer smoke where practical.
   - Update persistent architecture, roadmap, feature, privacy/security, gates,
     and project status; select exactly one recommended next phase.

## Status log

- 2026-08-24: Created `phase-6o-transfer-semantics` from synchronized `main` at
  `ed966e1`; inspected copy/move executors, application transfer state, browser
  actions, dependencies, roadmap, architecture, and privacy/security boundaries.
- 2026-08-24: Phase contract and executable gates defined before coding.
- 2026-08-24: Implemented destination-space preflight, supported POSIX
  metadata preservation, staged no-overwrite `EXDEV` move fallback with exact
  source-tree revalidation, and bounded standard desktop clipboard exchange.
- 2026-08-24: Added eight focused core and seven focused application tests;
  the complete workspace now passes 235 tests (56 core, 179 application).
- 2026-08-24: Isolated native Wayland smoke verified application ownership,
  30 exported window actions, Select All/Copy activation, Paste enablement from
  Floe's provider, D-Bus health, and clean shutdown. The compositor input-serial
  rule prevented automated external-client ownership, so interoperability is
  supported by deterministic provider/parser tests rather than a native
  cross-client claim.
- 2026-08-24: Updated persistent architecture, design, development,
  privacy/security, roadmap, feature matrix, gates, and project status. Exactly
  one next phase is recommended: `phase-6p-operation-control`.

## Status

COMPLETE
