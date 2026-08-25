# Plan: Floe Phase 10D — Permissions

## Contract

- Add explicit Properties-driven editing of executable state and Unix mode bits on exact selected local entries.
- Add deliberate owner/group editing using numeric UID/GID and validated local-name resolution; never elevate the whole Floe process.
- Represent direct and recursive changes as typed application jobs with whole-request no-follow preflight, fixed capacity, progress, cancellation before commit, and explicit partial-failure evidence after commit.
- Preserve exact `PathBuf` identity and refuse roots, mount crossings, replaced entries, and implicit symbolic-link following.
- Recursive policy must be opt-in, visibly scoped, bounded, and apply mode semantics separately to files and directories.
- Reuse the Operations Island lifecycle and refresh affected parents; GTK callbacks submit commands and never mutate the filesystem.
- Present current Phase 10B UID/GID/mode before editing, validate octal mode and owner/group input inline, and require explicit confirmation for recursive or ownership changes.
- Exclude ACL/xattr/capability/immutable editing, polkit/admin browsing, whole-process root, checksums, and future metadata work.

## Status

COMPLETE and verified on `phase-10d-permissions`. The sole recommended next phase is 10E Checksums; do not begin Phase 10F advanced metadata with it.
