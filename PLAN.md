# Plan: Floe Phase 12A — Archive Engine

## Contract

- Add GTK-independent typed list, extract, and compress requests for ZIP, TAR,
  TAR.GZ, TAR.XZ, and reviewed pure-Rust 7z backends.
- Preserve exact source and destination `PathBuf` values; archive member names
  are validated component-by-component and never reconstructed from display
  text.
- Preflight entry count, path size/depth, per-entry bytes, total expanded bytes,
  compression ratio, duplicate/file-directory collisions, source identity, and
  unsupported links before writing output.
- Extract into a private hidden sibling staging directory, publish with Linux
  no-replace semantics, and clean staging after cancellation or failure.
- Compress from a bounded, no-follow, identity-revalidated source plan into a
  hidden staging file and publish without replacing an existing archive.
- Run archive work through a fixed-capacity application executor with structured
  progress, cancellation, bounded listing results, job lifecycle, and no GTK
  filesystem work.
- Do not add Phase 12B context actions/dialogs, passwords, shell commands,
  external helpers, automatic overwrite, or persistent archive history.

## Implementation leaves

1. Define archive formats, limits, member/request/outcome models, and path-safe
   validation in `floe-core`.
2. Implement bounded listing/extraction/compression for ZIP and TAR family with
   staging, cancellation, conflict, bomb, traversal, and link defenses.
3. Add reviewed pure-Rust 7z listing/extraction/compression under the same
   validation and limits.
4. Add a capacity-4 application archive executor integrated with shared job
   progress, cancellation, terminal failures, and capacity-16 list results.
5. Run focused hostile-archive tests, workspace gates, documentation updates,
   native build, and sole-next-phase verification.

## Status

IMPLEMENTED on `phase-12a-archive-engine`; verification evidence is recorded in
`GATES.md`. Phase 12B is the sole recommended next phase.
