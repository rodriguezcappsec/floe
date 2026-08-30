# Floe Recovery and Data Safety

Floe prevents silent overwrite and provides bounded recovery mechanisms. It is
not a backup program or transactional filesystem.

## Trash, permanent deletion, and Undo

Move to Trash is intended to be recoverable. Restore refuses silent overwrite.
Permanent deletion is ordinary filesystem removal, not secure erase; storage
firmware, copy-on-write history, snapshots, backups, and other copies may retain
data.

Operation History is bounded and memory-only. Completed move and rename can be
undone only after identity and destination checks. Completed Create can be
undone through recoverable Trash only while unchanged; created directories must
remain empty. Copy, Trash, and permanent delete have no general Undo.

## Interrupted operations

Before copy, move, rename, and create mutations, Floe writes a private bounded
record to `$XDG_STATE_HOME/floe/operation-recovery-v1.bin` or its XDG fallback.
**Operation Recovery…** shows current source/destination state. Retry is offered
only for a prior-process copy, move, or rename with an intact source and absent
destination. Floe never deletes uncertain partial output automatically.

A corrupt, symlinked, insecure, oversized, or unreadable journal blocks new
journaled mutations while browsing remains available. **Reset Recovery Store**
removes only the unreadable journal, never recorded user files.

## Guardrails and integrity

Protected Folder is an accidental-change guardrail, not encryption, access
control, immutability, or attacker protection. Hashes and integrity monitoring
provide byte-comparison evidence, not authenticity, malware safety, or intrusion
detection. “Safe to remove” applies only after verified copy, flush, and GIO
eject/unmount succeeds; physical media evidence remains unclaimed without a
disposable lab device.

Keep independent backups. Package removal retains user data. Follow
[Migrations](./MIGRATIONS.md) before rollback or manual cleanup.

Phase 21D re-runs the isolated recovery journal suite as a release blocker. A
candidate cannot ship if corrupt, symlinked, insecure, stale, changed, or
uncertain records are accepted as safe retry instructions. See the
[release environment matrix](./RELEASE_MATRIX.md) for native host evidence.
