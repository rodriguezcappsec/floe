# Floe Privacy, Security, and Data-Integrity Architecture

Status: authoritative design and threat model. Most capabilities in this document are **PLANNED**, not implemented. This document does not select a cryptographic format, cryptographic library, vault backend, or sandbox mechanism.

Last reviewed against the repository: `2026-08-25`, after Phase 7A.

## Status vocabulary and claim discipline

- **IMPLEMENTED** means the behavior exists in current code and has focused tests or repository verification.
- **PARTIAL** means a useful safety property exists, but it does not satisfy the complete privacy or security capability.
- **PLANNED** means this document defines required behavior for a future phase. It is not a claim that the feature exists.
- **DEFERRED** means implementation must wait for a named dependency or design decision.
- **NOT APPLICABLE** means a concept is deliberately outside Floe's claim or responsibility.

Security state must use text and accessible semantics, never color alone. A failed or unavailable protection must not silently fall back to an unprotected path under the same action name.

## Current security and privacy baseline

- Phase 7C/7D normally persist bounded ordered live tabs, active tab, recently
  closed tabs, up to two pane sessions per tab, exact paths, history, selection,
  scroll anchors, active split side, bounded ratio, and view policy
  across clean shutdown in `$XDG_CONFIG_HOME/floe/browser-session-v1.bin`.
  The application-owned worker uses a 0700 directory, 0600 same-directory
  atomic file, bounded no-follow reads, version and hostile-input validation,
  and directory synchronization. It stores no file contents or credentials.
- Explicit `Private` and `Sensitive` session trace policy loads no workspace,
  removes Floe's owned session file, and suppresses shutdown recreation. The
  current environment integration is not a complete user-facing Private Mode:
  it does not clear view preferences, bookmarks, thumbnails, clipboard/desktop
  history, backups, storage history, or traces owned by another process, and
  private file permissions do not defend against the same user.

### IMPLEMENTED

- Phase 8E adds no drag history or new persistent path channel. Standard local
  file-list payloads and typed hover destinations remain interaction-lifetime
  state. Timers are single, cancellable, and revalidate live tab/Miller
  ownership; normal logs receive aggregate failures rather than source lists.
- Phase 8D column action contexts remain memory-only and bounded to 4,096
  selected identities. Floe revalidates the exact retained depth, directory,
  and direct-child entry before dispatch; stale and overflowed contexts are
  rejected. Aggregate ownership failures may be logged, but paths and
  selections are not added to normal logs or persistence.

- Phase 7A's per-session codec and Phase 7C/7D's workspace envelope preserve exact
  raw path bytes and therefore contain sensitive navigation, selection, and
  history data. Both are bounded and malformed-input checked; corruption,
  unsupported versions, relative paths, duplicate IDs, and oversized data fall
  back to one normal tab rather than partial restore.

- Phase 8A's Miller model and Phase 8B presentation retain at most 16 exact
  directory/selection paths in memory. Historical columns retain at most 4,096
  shared entry identities each and are discarded with the application; the
  active column shares the existing browser model, worker, and watcher. Only
  the non-sensitive global column width and view-mode policy are added to the
  private preference file. No column path, item snapshot, or selection is
  logged or newly persisted by Phase 8B.
- Phase 8C keyboard and trackpad navigation produces no new history, cache,
  log, or preference format. It operates only on exact identities already held
  by the active/retained Miller state. Horizontal deltas and focus transitions
  are not logged or persisted.

- Floe runs as the calling desktop user. It does not run its GTK process as root and does not expose `Open as Administrator...` today.
- The Cargo workspace forbids Rust `unsafe` code. The core crate is GTK-independent, and filesystem work stays out of GTK callbacks.
- Filesystem identities retain `PathBuf` and `OsString`. Lossy display labels are not reconstructed into operation targets. GIO launches receive a URI created from the exact local path.
- Directory enumeration uses `symlink_metadata`. Copy has an explicit preserve-or-reject symlink policy. Same-filesystem move and rename use `RENAME_NOREPLACE`, so a conflict cannot overwrite an existing target. Phase 6O cross-filesystem move copies to a hidden sibling, revalidates exact no-follow source identity, atomically publishes without overwrite, synchronizes the destination parent, and classifies post-commit source-cleanup failure as non-retryable partial completion.
- Copy, move, rename, Trash, restore, and permanent deletion use bounded application-owned executors, structured job state, cooperative cancellation, and explicit terminal failures. Copy tracks newly created destinations for best-effort cleanup after failure. Trash delegates to GIO rather than a shell command. Permanent deletion uses a validated exact-path batch, full no-follow preflight, root/mount refusal, postorder removal, and device/inode/kind revalidation without shelling out.
- Permanent deletion requires an explicit safe-focus confirmation showing escaped exact target labels. Cancellation is confirmed only before the first removal; after commit, completion or exact partial failure is reported. Partial deletion is non-retryable and no undo or secure-erasure claim is made.
- Phase 6N local Trash metadata is treated as untrusted input. Floe bounds
  `.trashinfo` reads to 64 KiB, opens metadata with `O_NOFOLLOW`, rejects NUL,
  malformed percent encoding, relative traversal, symlinked roots, and
  non-sticky shared roots. Exact decoded path bytes are retained separately from
  lossy display. Restore accepts only matching `files/name` and
  `info/name.trashinfo` pairs, uses `RENAME_NOREPLACE`, and removes metadata only
  after payload commit. Orphan payloads remain visible but are not restorable.
  Empty Trash and per-item permanent deletion reuse Phase 6M; companion metadata
  is included to avoid retaining original-path history. Cleanup preferences are
  not claimed or implemented.
- Raster thumbnails have an explicit PNG, JPEG, WebP, GIF, BMP, TIFF, and ICO policy. The bounded worker opens regular files with `O_NOFOLLOW`, enforces 32 MiB encoded and 128 MiB decoded limits plus dimension limits, revalidates source metadata, and returns a generic icon on failure.
- The freedesktop thumbnail cache validates URI, modification time, size, and Floe's nanosecond marker. Floe uses private cache directories, `0600` files, atomic writes, and ownership markers. MD5 is only the freedesktop cache filename convention; it is not an integrity mechanism.
- Bookmarks retain exact path bytes and use bounded asynchronous atomic persistence with `0700` directories and `0600` files. View preferences use asynchronous atomic `0600` file writes.
- Phase 6T's optional per-folder view memory persists exact local path bytes as hex-encoded records in that same private preference file, capped at 256 entries. This can reveal viewed folder names and locations to same-user processes, backups, or copied configuration. Disabling the option clears Floe's saved overrides; it cannot erase external copies and is not Private Mode.
- Password-protected device mounting uses a window-parented `GtkMountOperation`. The desktop and GIO own the prompt and credential exchange. Floe code does not receive, persist, log, or pass the password on a command line.
- Floe's exact-path transfer buffer and terminal operation history are bounded and memory-only. Phase 6O also publishes selected local file URIs to the desktop clipboard using standard GNOME/KDE-compatible formats. Clipboard managers, desktop services, and other applications may retain those paths after Floe changes or exits; this is interoperability, not a privacy feature or audit journal.

- Phase 6P operation history remains bounded and memory-only. It captures typed outcomes and exact operation identities; Clear Completed preserves failures, conflicts, cancellations, and partial evidence. Safe Undo exists only for completed move/rename records after no-follow destination identity revalidation and no-overwrite original-path checks. It is not rollback, crash recovery, persistent audit history, or a claim that irreversible work is reversible.

- Phase 6Q creation uses a bounded application-owned executor and never shells out, elevates Floe, or silently overwrites. Directory and empty-file requests use create-new semantics; template duplication reuses the no-follow copy engine; symbolic links preserve the exact stored target and may intentionally remain broken; hard links accept only regular non-symlink sources and report same-filesystem limitations. User-entered destination names are validated as one component before worker submission.
- Phase 6Q Copy Name, Path, Relative Path, and URI are explicit user actions. Text forms reject any selected value that cannot be represented losslessly as UTF-8; local file URIs retain raw path bytes through percent encoding. These clipboard values reveal filenames and locations and may outlive Floe in desktop clipboard history.
- Phases 6R and 7F drag sources disclose explicitly selected local paths to the chosen desktop drop recipient using standard GDK file-list format. Incoming drops accept local `gio::File` paths only and reject empty, malformed, unavailable, or non-local identities. Exact paths, including live opposite-pane destinations, route through existing no-overwrite copy/move/link/Trash jobs; drag callbacks never shell out, elevate Floe, or mutate the filesystem directly. Drag-and-drop is interoperability, not a privacy boundary or audit trail.
- Phase 6S observes only the active local directory through one non-recursive GIO monitor. Exact changed paths and rename pairs remain bounded, memory-only, and are discarded after a 140 ms coalesced reconciliation or on navigation/shutdown. Normal logs record only aggregate event/path/rename counts and overflow state, never watched paths. Live updates are not integrity monitoring, attribution, audit history, malware detection, or evidence of changes made while Floe was not watching.

### PARTIAL or absent protections

- Phase 6O preserves regular-file/directory mode and access/modification times,
  but does not claim ownership, ACL, xattr, capability, security-label, sparse,
  reflink, or symlink-metadata preservation. Destination-space preflight is a
  point-in-time `statvfs` comparison, not reserved capacity. Staged
  cross-filesystem publication reduces data-loss risk but is not persistent
  interrupted-operation recovery, content verification, or a durable
  transaction; Phase 18V owns Copy Verify and Phase 18Y owns recovery journals.

- Persistent thumbnails contain derived pixels and standard `Thumb::URI` source metadata. File permissions reduce cross-account exposure, but another process running as the same user, a backup, or a copied cache can read them. Sensitive Folder, vault, and Private Mode cache policies do not exist yet.
- Structured tracing exists, but there is no comprehensive path-redaction or private-session logging policy. Some structured errors may include paths. Floe cannot currently claim privacy-safe logging.
- Normal Open and Open With give an external application the selected file URI under the user's normal authority. The launch is not sandboxed. That application or the desktop may record recents, logs, thumbnails, or network activity outside Floe's control.
- Raster decoding is bounded and non-executing, but it occurs in Floe's thumbnail worker process. It is not a parser sandbox; a decoder vulnerability can affect Floe's process.
- Phase 6L system thumbnailers run as supervised external process groups on the
  capacity-64 thumbnail worker. Floe parses reviewed field codes into argv
  without a shell, passes one exact input URI/path plus a private temporary
  output, enforces timeout/cancellation, rejects symlink/non-regular/oversized
  output, decodes only bounded PNG pixels, revalidates source metadata, and
  cleans temporary output. These helpers are **not sandboxed**: they inherit the
  user's normal filesystem, environment, session, and network authority, so a
  vulnerable or malicious installed provider can access data beyond the input.
- Conflict refusal, atomic no-replace rename, source revalidation, and cleanup improve data safety. They are not content-integrity verification, authenticity, crash recovery, or durable transaction guarantees.
- Current toasts and desktop integration have no complete sensitive-name notification policy. Search indexing, privacy-aware session history, secret clipboard handling, and persistent operation recovery are not implemented.
- Permanent deletion cannot remove copies retained by snapshots, backups, journal or CoW history, remote services, storage firmware, or another process. It is ordinary filesystem unlink/removal, not forensic erasure.

### PLANNED and unavailable today

Portable encryption, recipient encryption, encrypted vaults, recovery keys, privacy sessions, Sensitive Folders, Private Mode, Privacy Lock, Protected Folders, privacy-safe caches and history, sandboxed providers, Open Safely, suspicious-file analysis, metadata sanitization, permission auditing, sensitive-content scanning, integrity manifests, verified transfer, and operation recovery are all planned. No current Cargo dependency implements cryptography or a sandbox.

## Security objectives and intended protections

After their planned phases are complete and audited, Floe intends to:

- protect encrypted-file and locked-vault plaintext against offline access to copied ciphertext or a powered-off or lost storage device, subject to credential strength and documented metadata leakage;
- keep passwords, private identities, recovery material, and decrypted keys out of configuration, logs, process arguments, notifications, and ordinary persistence;
- prevent untrusted providers and Open Safely targets from receiving unrelated filesystem, vault, network, device, or write access beyond a documented sandbox policy;
- reduce Floe-owned traces for Sensitive Folders and Private Mode without calling those modes encryption;
- report evidence-based suspicious file traits, permission exposure, and privacy metadata without declaring a file malicious;
- preserve originals and report exact partial outcomes when encryption, sanitization, transfer, verification, recovery, or destructive work fails;
- provide hashes and manifests as integrity evidence, and claim authenticity only after a separately reviewed signature system verifies it.

## Explicit non-protections

Floe does not intend to protect against:

- a compromised kernel, compositor, display server, firmware, filesystem implementation, or privileged system service;
- a keylogger, screen capture, hardware implant, or memory-forensics adversary beyond a realistic desktop-application threat model;
- malware or another process running as the same user while plaintext, a vault, or a sensitive file is accessible;
- an external application retaining plaintext, recents, logs, temporary files, backups, or network copies after Floe grants access;
- plaintext exported, printed, synchronized, or backed up outside an encrypted boundary;
- weak or disclosed passphrases, lost private identities, malicious recipients, or unverified recipient substitution;
- denial of service from storage failure, device removal, exhausted space, or hostile inputs beyond implemented bounds;
- rollback to older valid ciphertext or manifests unless a future design explicitly provides trusted freshness;
- recovery when the password and all recovery material are lost;
- secure erasure on SSD, flash, CoW, journaled, snapshotted, deduplicated, cached, backed-up, or remote storage.

Floe is not antivirus, endpoint detection, intrusion detection, a password manager, a backup system, a forensic erasure tool, or an anonymity system.

## Assets, adversaries, and trust boundaries

### Assets

Protected assets include file contents; names, directory structure, paths, URIs, metadata, thumbnails, previews, searches, histories, notifications, and operation records; future passwords, keys, private identities, and recovery material; integrity baselines and trust roots; and the user's authority to modify or destroy files.

### Adversaries and failure sources

- an offline thief or storage operator possessing encrypted data;
- another local account or a same-user process in the desktop session;
- attacker-controlled files, filenames, metadata, URIs, mounts, and provider responses;
- a malicious or compromised helper, external application, custom action, plugin, desktop service, or remote endpoint;
- accidental user action, misleading UI, confused-deputy routing, stale results, path substitution, symlink races, disconnects, crashes, partial writes, and corrupt storage.

### Trust boundaries

1. **GTK presentation to application commands.** Labels are untrusted presentation. Commands carry validated exact identities. GTK callbacks do not perform filesystem or cryptographic work.
2. **Application state to bounded workers.** Requests, generations, operation IDs, cancellation, conflicts, and outcomes are typed. Stale responses are rejected. Cancellation is not completion or rollback.
3. **Floe to Linux and desktop services.** The kernel, GIO, GVfs, portals, polkit agent, secret service, and mount helpers are external authorities. Their errors and metadata are untrusted.
4. **Floe to persistent state.** Configuration, caches, indexes, histories, journals, and vault ciphertext need permissions, atomicity, retention, migration, and corruption rules. Private permissions do not stop same-user processes.
5. **Floe to parsers and providers.** File bytes and provider output are untrusted. Sandboxing limits impact; it does not prove semantic correctness.
6. **Floe to external applications and extensions.** Normal Open transfers access to another process. Plugins receive no vault keys or decrypted-vault access by default.
7. **Cryptographic boundary.** A reviewed format and library own primitives, framing, authentication, nonces, and KDF behavior. Floe owns UX, exact-path I/O, jobs, secret lifetime, conflict policy, and truthful claims.

The normal desktop user's kernel and session are trusted while sensitive plaintext is in use. Offline-storage attackers are in the planned encryption threat model; a compromised live user session is not.

## Non-negotiable cryptographic rules

- No homemade cipher, KDF, MAC, signature scheme, hash construction, nonce scheme, casual container format, or proprietary crypto for branding.
- Use an established, versioned, interoperable format and a maintained reviewed implementation after explicit dependency review.
- Use authenticated encryption and the format's established password KDF. Never use unauthenticated encryption, nonce reuse, silent downgrade, or plaintext fallback.
- Passwords, private identities, recovery material, and keys must not enter configuration, logs, crash text, telemetry, notifications, command-line arguments, environment variables, shell interpolation, filenames, or D-Bus activation parameters.
- Encryption and decryption must stream with bounded memory, support cancellation at defined boundaries, authenticate before success, use safe output permissions, and never overwrite silently.
- Failure preserves the original. A known partial output is removed only when ownership is certain; uncertain data is reported, not silently deleted.
- Cipher and KDF controls are not normal preference knobs. Format versions and security parameters need migration and compatibility policy.
- Constant-time behavior, zeroization, locked memory, dumps, swap, and hibernation must be evaluated honestly. Application cleanup cannot promise erasure of every runtime, kernel, or hardware copy.
- Security dependencies need rationale covering maintenance, audit history, interoperability, MSRV, unsafe surface, transitive code, secret handling, malformed input, and update response.

No library or format is selected by this document.

## Secret ownership and lifetime

### IMPLEMENTED mount credentials

Mount credentials belong to `GtkMountOperation` and the desktop or GIO authentication stack. Floe owns only the request and result. Desktop credential storage or authorization caching is outside Floe and must not be described as Floe memory hygiene.

### PLANNED application-owned secrets

- A password widget passes a secret through the shortest reviewed path to one cryptographic operation. Raw secret types must not implement `Debug` or casual `Clone`.
- Secrets are scoped to one operation or an explicitly unlocked privacy session. They are cleared from widgets, released on terminal outcome, and zeroized where the chosen representation makes that meaningful.
- No secret is persisted for convenience. Future OS credential-store integration is opt-in, separately threat-modeled, and never required for portable passphrase files.
- A privacy session may temporarily authorize selected vault credentials. It is not a universal vault password and must not silently alter per-vault encryption.
- Public recipient identities are not secret. Import, a label, a QR code, or a fingerprint does not prove real-world ownership.
- Private recipient identities require stricter import, export, storage, backup, permission, and lifetime rules than preferences. Floe must not upload, synchronize, or expose them.
- Recovery material is exported only after explicit action. It is never auto-copied, logged, or silently stored beside the ciphertext it recovers.

## Portable file encryption

Status: **PLANNED**. A standard `age`-compatible workflow is the leading candidate, subject to implementation-time format and dependency review.

- Ciphertext is an ordinary transferable file that compatible independent tools can decrypt. Passphrase portability must not depend on Floe configuration, a local database, a keyring entry, or a hidden device secret.
- The first engine supports explicit passphrase encryption and decryption. Huge files use streaming jobs, bounded memory, progress, and cancellation.
- Output name, destination, conflicts, permissions, partial cleanup, and original disposition are explicit. `Encrypt and Trash Original` occurs only after authenticated durable finalization and is never called secure erase.
- Portable encryption protects content as defined by the format. A file such as `report.pdf.age` still leaks existence, approximate size, timestamps, filesystem metadata, and often a meaningful name.
- Decryption never overwrites silently. Wrong credentials, malformed or truncated ciphertext, unsupported versions, authentication failure, full storage, disconnect, and cancellation produce conservative outcomes without exposing successful partial plaintext.
- Remote-path encryption and silent plaintext staging are **DEFERRED** until remote streaming and temporary-file trust are designed.

## Recipient identities

Status: **PLANNED**, dependent on portable-format review.

- The selected format library parses and canonicalizes recipient strings. Unknown types, malformed identities, ambiguous encodings, and duplicates fail explicitly.
- A saved recipient contains a user label, canonical public identity, type, optional source note, and comparison or fingerprint data supported by the format.
- A fingerprint helps detect substitution during comparison; it does not establish ownership by itself. Confirmation displays every complete canonical recipient identity.
- Multiple-recipient semantics follow the selected standard. Removing a saved recipient does not revoke access to ciphertext already made for that recipient.
- Key generation, private identity storage, hardware identities, QR workflows, revocation, and trust establishment are separate reviewed scopes. Floe will not invent a key exchange, certificate system, identity server, escrow service, or revocation guarantee.

## Encrypted vault architecture

Status: **PLANNED** and dependent on a dedicated backend review. A plaintext directory plus a Floe password dialog is prohibited.

### Required storage semantics

While locked, an Encrypted Vault exposes ciphertext only and conceals filenames and meaningful directory structure to the documented extent of the selected design. While unlocked, a reviewed provider exposes a decrypted filesystem view. Same-user applications that can access that view can read it.

The format must be versioned and portable without absolute paths, a particular desktop, or an undisclosed machine-local secret. The review should prefer an established architecture and study systems such as Cryptomator and gocryptfs without assuming either is suitable. FUSE or another mechanism is selected only after lifecycle, crash, permission, packaging, and desktop review.

The backend must define exact-name handling, huge-file and random access, create, read, write, truncate, rename, move, directory operations, concurrent handles, durability, interrupted writes, clean unmount, stale mounts, device disconnect, symlinks, and hard links. It must not rely on persistent plaintext staging. Unsupported filesystem semantics must be rejected or documented.

### Conceptual key hierarchy

```text
password or recovery credential
        -> format-defined derived or wrapping key
        -> authenticated wrapping of a random vault root key
        -> format-defined file and directory keys
        -> authenticated encrypted metadata and content
```

The selected format and library define KDFs, domain separation, key sizes, nonces, framing, and metadata authentication. Password change should rewrap a stable random vault root key rather than rewrite every file. Full key rotation is a different operation. Every wrapper and format record is versioned and authenticated.

Each vault has an independent credential policy. A privacy session may temporarily authorize a vault, but changing a global session password or preference must not silently alter that vault's encryption. Future password-based and key-based vaults require explicit, separately reviewed credential types.

### Recovery and portability

Recovery material is **DEFERRED** until review proves a design that wraps the vault root key without weakening password protection. Creation must warn that loss of both credentials and recovery material can make data unrecoverable. The user must verify exported recovery material. Floe provides no backdoor, vendor recovery, or guaranteed recovery.

A locked vault should be portable to another disk or computer with compatible software and the intended credentials. Portability does not promise safe concurrent use, conflict-free cloud sync, rollback detection, or recovery from corrupt ciphertext unless the chosen format provides those properties.

### Vault lifecycle

Application state, not GTK widgets, owns `Locked`, `Unlocking`, `Unlocked`, `Locking`, and recoverable error states. Unlock and lock are visible and generation-bound. A failed unlock reveals neither partial plaintext nor an unnecessary credential oracle.

Locking clears Floe-owned selections, previews, decrypted metadata, search results, cached credentials, and sensitive clipboard state. It requests clean provider closure and reports open handles, active jobs, stale mounts, external-drive removal, and partial writes. Floe must not force-unmount when corruption is possible or call a vault locked while decrypted access remains.

Auto-lock on timeout, app exit, session lock, suspend, or device removal ships only after each signal and failure path is verified. Closing a view must not hide active vault or administrator jobs.

## Sensitive Folder, Private Mode, and Protected Folder

These terms are deliberately distinct and all are **PLANNED**.

### Sensitive Folder

An ordinary folder marked Sensitive receives reduced Floe-owned traces. Persistent thumbnails, preview caches, recents, search indexing, session restoration, navigation history, operation history, and names in notifications may be suppressed by one visible policy.

Required wording: “Sensitive mode reduces traces inside Floe. It does not encrypt this folder.”

### Private Mode

A Private Floe window minimizes new Floe-owned persistent navigation, search, closed-tab, session, preview, thumbnail, operation, and command history. It has a persistent non-color-only indicator and explicit exit behavior.

Private Mode is not encryption, anonymity, sandboxing, screen protection, or control over records made by the desktop, filesystem, external applications, remote services, backups, or the operating system.

### Protected Folder

A Protected Folder adds accidental-operation guardrails for rename, move, mass delete, permanent delete, or other configured destructive changes. It is mistake prevention, not access control, immutability, encryption, or attacker resistance. Other tools can modify it.

`Privacy Lock` may later lock vaults where safe, close sensitive surfaces, clear Floe-owned sensitive memory and clipboard state, and end the privacy session. It never deletes user data and must report anything it could not lock or clear.

A future Privacy and Security Center may summarize real vault, Sensitive Folder, privacy-session, cache, permission, and integrity state. It must not create a fear-based security score or mark protection active from configuration alone.

## Privacy-safe state and trace policy

### Thumbnails and previews

- **Current:** eligible raster thumbnails use the ordinary freedesktop cache and may reveal pixels, URI identity, size, and timestamps.
- **Planned vault policy:** never write decrypted vault thumbnails to the ordinary cache. Use disabled persistence or a reviewed memory-only or encrypted per-vault cache. Lock invalidates in-memory pixels, metadata, provider results, and GTK textures.
- **Planned Sensitive and Private policy:** default to memory-only generation. Marking an existing folder Sensitive offers cleanup of Floe-owned prior entries, but cannot promise removal of copies owned by other applications or backups.

### History, search, and indexes

Policy covers navigation, recent locations, closed tabs, session restore, operation and recovery history, search history, command recents, previews, and saved searches. Locked vault names and content never appear in ordinary indexes or results. A future private vault index must be memory-only or encrypted under vault-derived keys and unavailable while locked.

**Current session policy:** normal clean shutdown retains at most 64 live tabs,
32 recently closed tabs, up to two pane sessions per tab, and each session's
bounded history and selection.
Private or Sensitive policy removes and suppresses only Floe's workspace file.
No claim is made that shutdown writes survive crashes before atomic publication
or that deleting the file erases backups, snapshots, journals, or prior storage
blocks.

### Logging and diagnostics

Passwords, keys, recovery material, plaintext content, decrypted vault names, administrator URIs, and scanner matches are prohibited in logs. Normal logs prefer operation IDs and classified outcomes with redacted paths. Debug diagnostics that include paths require explicit, time-limited consent and remain incompatible with Private Mode unless separately authorized. Secret wrappers do not derive `Debug`.

### Notifications

Sensitive, vault, and Private operations default to generic notifications without filenames or paths. Notification bodies, actions, lock-screen visibility, and history are disclosure surfaces. Desktop-controlled retention cannot be guaranteed.

### Clipboard and transfer buffers

Floe never auto-copies passwords or private key material. Copying recovery material requires an explicit action and warning. Replacing Floe-owned secret clipboard data after a timeout is best effort; clipboard managers and other applications may already retain it. “Erased from clipboard” is therefore prohibited.

The internal file transfer buffer contains exact paths, not file contents.
Phase 6O's interoperable desktop clipboard contains local file URIs, which still
reveal names and locations and may be retained by clipboard managers or other
applications. Floe clears a cut clipboard after it queues the move as a
best-effort lifecycle action, never as an erasure claim. Future Sensitive and
Private policy must clear or visibly scope both internal and desktop state.

## Sandboxed providers and Open Safely

Status: **PARTIAL**. Phase 6L supervises installed system thumbnailers, but no
current previewer, thumbnailer, or Open action is sandboxed. Phase 18L owns the
restricted provider boundary described below.

### Provider execution boundary

The following are Phase 18L requirements, not claims about the current Phase 6L
helpers. External thumbnail, preview, metadata, archive, office, font, and media
providers process attacker-controlled bytes in a supervised process. The target
capabilities are:

- read-only access to one exact input;
- isolated temporary storage;
- no unrelated home, configuration, cache, vault, network, or device access;
- no inherited secrets and a sanitized environment;
- bounded protocol input and output;
- timeout, cancellation, and practical resource limits.

Provider identity, version, MIME claims, exit status, crashes, malformed output, and sandbox setup failures are explicit. Unsupported or failed generation yields a generic icon or unavailable preview. It never retries by running the provider unsandboxed.

Active content, macros, scripts, archive entries, links, embedded objects, and network references do not execute or resolve merely for preview. Provider output remains untrusted even inside a sandbox.

The implementation phase must compare portals, Bubblewrap, Landlock, namespaces, resource controls, and packaging constraints. Merely invoking one mechanism is not a security design. Unavailable support produces a truthful unavailable result.

### Open Safely

Open Safely is a distinct action that launches a compatible external application only after an actual restriction policy is active. Its UI states the granted input, write locations, network and device policy, and known limits.

Sandbox setup failure stops Open Safely. A user may separately choose normal Open after a warning, but Floe must never silently convert the safe action into a normal launch or retain a sandbox indicator.

Child processes, D-Bus and session-bus access, portals, file chooser grants, downloads, save and export behavior, and cleanup require tests. An unsupported application is reported unsupported, not sandboxed.

## Suspicious files and metadata privacy

Status: **PLANNED**.

- Suspicious-file analysis combines reliable origin metadata, executable bits and content type, MIME and extension mismatch, desktop or script traits, double extensions, Unicode bidi or invisible controls, deceptive whitespace, and safe escaped filename display.
- A warning states evidence and uncertainty. “Type does not match extension” or “potentially executable” is not “malware.” Legitimate international names are not rejected merely for containing Unicode.
- A future quarantine is a Floe-managed restricted location with exact original identity, restore, delete, and Open Safely-only behavior. It is not antivirus quarantine.
- The Privacy Inspector reports format-specific evidence such as GPS, camera or device data, timestamps, author, organization, creator, comments, revisions, embedded thumbnails, and media tags.
- Absence of a finding does not prove that a file has no identifying metadata.
- Create Sanitized Copy preserves the original, lists targeted fields, uses a reviewed format-specific writer, finalizes safely, and verifies supported fields. “All metadata removed” is prohibited without exhaustive format-specific verification.
- Share-time warnings are evidence-based and restrained. Secure Share sequences inspect, optionally sanitize, optionally encrypt, optionally hash, and create a new output. It never silently modifies the original.

## Permission auditor and sensitive-content scanner

Status: **PLANNED**.

The permission auditor explains Unix modes, owner and group, POSIX ACLs, xattrs, capabilities, immutable attributes, and mount context only when queried successfully. Findings such as world-writable or broadly readable private keys include exact evidence and scope.

Mode bits alone can be incomplete because of ACLs, mount semantics, remote backends, namespaces, or application-level sharing. Automatic repair is out of scope until every transformation has explicit semantics, rollback, symlink policy, and tests.

The sensitive-content scanner is local, opt-in, bounded, cancellable, and heuristic. It may identify possible private keys, `.env` secrets, API-like tokens, cloud or SSH credentials, password exports, and database dumps without displaying or logging full values.

False positives and false negatives are expected. The scanner is not malware detection, data-loss prevention, proof of exposure, or proof that a file is safe. Cloud submission is off unless a separately named integration receives informed consent.

## Integrity, verified transfer, and recovery

Status: **PLANNED**, except for the data-safety primitives listed in the current baseline.

- Integrity fingerprints store a named standard hash algorithm, expected digest, and exact identity context. A matching hash means bytes match that record; it does not establish authorship, freshness, permission safety, or absence of malicious content.
- Folder manifests use a documented portable format such as `SHA256SUMS`, path-safe encoding, cancellation, and clear changed, missing, new, and unreadable results.
- Signed manifests wait for a reviewed established signature and trust model. Floe does not invent a signature scheme.
- Integrity monitoring is opt-in baseline comparison with bounded and coalesced watchers. It is not intrusion detection, cannot identify who changed data, and cannot observe while Floe or the watcher is unavailable.
- Copy and Verify performs a safe copy, flushes at the defined durability boundary, re-reads according to a documented race policy, and reports its algorithm and result. “Verified” appears only after verification completes.
- Copy, Verify and Eject has separate copy, flush, verify, unmount, and eject states. Partial success stays visible. Floe never says “safe to remove” until the relevant desktop operation succeeds.
- The operation journal stores only the minimum typed data needed to explain or resume supported work. Sensitive policy redacts or suppresses paths. It never stores passwords, keys, decrypted content, or scanner matches.
- After restart, uncertain partial destinations are shown with conservative actions. Floe never silently resumes a destructive step or deletes uncertain user data.
- Destructive preflight reports operation, item counts, known bytes, target authority, reversibility, and affected Protected, vault, or mounted roots. It never calls permanent deletion secure erase.
- Snapshot integration is capability-driven and optional. Floe does not assume all Btrfs or ZFS systems share one snapshot policy or make Floe a filesystem administrator.

## Hostile input and failure behavior

All filenames, paths, URI components, links, metadata, device fields, cache records, ciphertext, vault metadata, manifests, archives, provider messages, desktop errors, and external state are untrusted.

- Preserve exact binary-safe identity. Reject invalid conversions and traversal. Never reconstruct a target from lossy display text or interpolate it into a shell.
- Use no-follow opens and revalidation where policy requires. Recursive operations state symlink and hard-link behavior. A preflight check is not assumed valid at commit time.
- Bound encoded input, decoded allocation, recursion, output, queues, concurrency, time, and retained state. Cancellation is cooperative and is not rollback.
- Unknown format versions, algorithms, providers, identities, permission semantics, and URI authorities fail closed for the protected action.
- There is no plaintext, unsandboxed, overwrite, follow-link, or weaker-format fallback.
- Originals survive failed encryption and sanitization by default. Remove known temporary output only when ownership is certain; identify uncertain or committed partial output for review.
- Authentication failure, malformed data, unsupported input, permission denial, full storage, disconnect, timeout, cancellation requested, cancellation confirmed, partial success, and internal failure remain distinguishable where that does not create a credential oracle.
- Retry uses a fresh attempt identity and revalidates state. It does not broaden authority, overwrite policy, recipients, sandbox policy, or secret lifetime.
- Security phases require malformed, truncated, oversized, corrupt, non-UTF-8, deceptive-name, symlink, race, disk-full, permission, disconnect, cancellation, and concurrent-operation tests. Fuzzing is used where practical.

## Privileged access boundary

Status: **PLANNED**. `docs/PRIVILEGED_ACCESS.md` is the authoritative detailed design.

Floe must never elevate its whole process, interpolate a path into `sudo` or `pkexec`, collect the polkit password, or disguise a normal `PathBuf` as an `admin://` URI.

Privileged identity remains a typed GFile and GVfs authority routed to a separate provider, visibly Administrator-badged and reversible. External applications, terminals, plugins, thumbnailers, previews, and context tools do not inherit that authority. The action remains unavailable until the resource model and security gates pass.

Encrypted-volume mount authentication is separate. The desktop-owned `GtkMountOperation` flow is **IMPLEMENTED**, but a successful mount grants only the permissions supplied by that filesystem and desktop service.

## Prohibited security and privacy claims

Floe must not claim:

- “military-grade,” “unbreakable,” “zero knowledge,” “anonymous,” or impossible-to-access protection;
- secure erase, shredding, or guaranteed unrecoverability on SSD, flash, CoW, journaled, snapshotted, deduplicated, remote, or backed-up storage;
- expiration or revocation that a holder of ciphertext and a valid key cannot bypass;
- vault protection from same-user malware, external applications, screen capture, or key capture while unlocked;
- encryption when data is merely hidden, renamed, obfuscated, permission-gated, Sensitive, Private, or Protected;
- that Sensitive Folder is encryption, Private Mode is cryptographic privacy, or Protected Folder resists attackers;
- sandbox security, Open Safely, or sandboxed preview when the actual restriction did not start or silently degraded;
- antivirus, malware detection, intrusion detection, compromise detection, or quarantine scanning without a real implementation;
- that MIME mismatch, executable state, Unicode, origin metadata, a scanner heuristic, or a permission finding proves malicious intent;
- exhaustive metadata removal or privacy merely because known fields were removed;
- that a hash alone proves signature, authenticity, freshness, authorship, or safety;
- “Integrity verified” before verification completes, or “safe to remove” before successful flush, unmount, and eject;
- guaranteed clipboard erasure, memory erasure, desktop-history erasure, or deletion of copies owned by other programs;
- guaranteed recovery, rollback, atomicity, cancellation, or partial-data deletion beyond the exact completed mechanism;
- that private file permissions defend against a process running as the same user;
- that successful polkit or mount authentication gives Floe ownership of desktop-cached credentials;
- that using `age`, Bubblewrap, Landlock, portals, FUSE, a password dialog, or any named dependency alone establishes protection.

Language is part of security correctness. Use **Encrypted Vault** only for real encrypted storage, **Sensitive Folder** only for reduced Floe-owned traces, **Protected Folder** only for accidental-change guardrails, **Private Mode** only for trace minimization, **Open Safely** only for an active restriction policy, and **Integrity verified** only after real verification.

## Dependency, lifecycle, and audit gates

No privacy or security capability moves from **PLANNED** to **IMPLEMENTED** until its phase records:

1. protected assets, adversaries, non-protections, authority boundaries, metadata leakage, and exact user claims;
2. reviewed format, backend, and dependency rationale plus interoperability fixtures where relevant;
3. secret creation, ownership, copying, persistence, timeout, cancellation, lock, crash, shutdown, and destruction behavior;
4. cache, index, journal, and configuration permissions, atomicity, retention, migration, corruption, backup, and downgrade behavior;
5. startup, normal work, cancellation, timeout, retry, app exit, session lock, suspend, disconnect, crash, upgrade, and removal lifecycle;
6. hostile-input, link, race, stale-response, partial-output, resource-exhaustion, wrong-credential, malformed-version, and concurrency tests;
7. accessible non-color-only state, conservative errors, password and recovery UX, and no silent protection downgrade;
8. formatting, check, strict Clippy, workspace tests, and native Wayland smoke when UI or desktop integration changes;
9. dependency audit and security review before stable claims, with fuzzing and external-review opportunities recorded for crypto, vault, parser, and sandbox boundaries.

Phase 18A may refine this architecture and select candidates, but selection alone does not make a feature implemented. The Phase 18 family is not stable until its final audit verifies cryptography, secrets, caches, metadata, sandbox assumptions, hostile inputs, vault lifecycle, recovery, and claims together.
