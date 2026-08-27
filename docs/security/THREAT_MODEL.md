# Floe Phase 18A Threat Model

Status: **COMPLETE architecture baseline**, reviewed `2026-08-27` against the
Phase 14 runtime and the verified post-Phase-14 icon corrections.

This document defines what later Phase 18 work must defend, what it explicitly
cannot defend, and where authority changes. It does not claim that encryption,
vaults, sandboxing, Private Mode, sensitive scanning, verified transfer, or
operation recovery are implemented. [PRIVACY_SECURITY.md](../PRIVACY_SECURITY.md)
remains the product-wide security architecture and terminology authority; this
document is the Phase 18A threat-analysis record.

## Scope and security objectives

Floe is a desktop file manager running with the authority of the logged-in
user. Later security features should:

- preserve user data against accidental overwrite, confused-deputy behavior,
  partial publication, unsafe recovery, and misleading destructive UI;
- protect application-owned secrets and decrypted content from offline storage
  disclosure while avoiding unnecessary same-user exposure;
- constrain attacker-controlled files and helper processes to explicitly
  reviewed inputs, outputs, resources, and authority;
- minimize Floe-owned path, filename, content, metadata, query, thumbnail,
  clipboard, notification, session, and journal traces where a named policy
  requires it;
- report protection, verification, cancellation, partial success, and failure
  truthfully, with no silent downgrade; and
- preserve exact `PathBuf`/`OsString` identity and never derive an operation
  target from lossy presentation text.

## Protected assets

| Asset | Required property | Examples |
| --- | --- | --- |
| User source data | Integrity and availability until an explicit commit | Files selected for encryption, sanitization, transfer, rename, Trash, or delete |
| Protected output | Confidentiality, authenticity, and version compatibility | Portable ciphertext, vault objects, private recovery material |
| Plaintext and decoded content | Minimum authority and lifetime | Decrypted streams, previews, extracted metadata, temporary provider input |
| Filesystem identity | Exactness and no path escape | Raw filenames, paths, URIs, device/inode identity, vault entry names |
| Security evidence | Accuracy, freshness, and provenance | Hash records, verification results, recovery records, sandbox status |
| User intent | Explicit scope and authority | Recipients, overwrite decisions, selected roots, administrator targets |
| Floe-owned traces | Policy-bounded retention | Sessions, thumbnails, indexes, saved searches, notifications, logs |
| Availability | Bounded work and recoverable failure | GTK responsiveness, cancellation, device disconnect, hostile files |

## Adversaries

The model includes:

- attacker-controlled filenames, paths, links, archives, documents, media,
  metadata, MIME claims, ciphertext, vault metadata, manifests, and provider
  output;
- an offline thief or storage operator with copied encrypted data or a lost
  powered-off device;
- a malicious or compromised thumbnailer, previewer, metadata provider, archive
  parser process, external application, custom action, or future plugin;
- a malicious remote endpoint, removable device, filesystem image, mount, or
  desktop-service response;
- another normal same-user process able to observe the ordinary clipboard,
  D-Bus, notifications, world-readable files, or unlocked plaintext;
- accidental user action, deceptive UI, stale selection, wrong destination,
  interrupted work, full storage, disconnect, crash, and power loss;
- resource-exhaustion input intended to consume CPU, memory, disk, processes,
  descriptors, recursion depth, output bytes, or GTK event capacity; and
- a confused-deputy attempt to reuse mount, portal, privileged, or previously
  granted authority for a different target.

## Trusted components and assumptions

Floe relies on the Linux kernel, the active user session, Rust's safe-language
guarantees, reviewed dependency behavior, and correctly configured desktop
services following their documented contracts. A successful D-Bus ownership
check, portal response, Secret Service presence check, mount prompt, or polkit
prompt proves only that a service or authorization exchange exists; it does not
prove that a file, helper, service, or result is safe.

The model assumes Floe's process and loaded native dependencies have not already
been compromised. It does not treat GTK, GIO, GVfs, portals, Secret Service,
polkit, filesystem drivers, or sandbox tools as infallible. Later phases must
minimize the authority and data supplied to each component and test failure and
unavailability explicitly.

Memory zeroization can reduce lifetime and accidental copies, but cannot promise
protection from a compromised kernel, debugger with equivalent authority,
keylogger, screen capture, arbitrary same-user process-memory access,
swap/hibernation capture, or forensic memory recovery.

## Trust and authority boundaries

1. **GTK presentation to application command.** Filenames and metadata are
   untrusted display values. Commands carry separately retained exact identity.
   GTK callbacks submit bounded work and never perform filesystem, parsing,
   cryptographic, or privileged operations.
2. **Application controller to workers/core.** Requests require typed scope,
   limits, cancellation identity, and generation. Stale responses cannot replace
   current state. Queues and retained results remain bounded.
3. **Floe to kernel/filesystem.** Paths, links, mounts, permissions, identities,
   and free-space results may change after preflight. Commit paths use no-follow,
   identity revalidation, no silent overwrite, staging, and explicit partial
   outcomes as applicable.
4. **Floe to desktop services.** GIO, GVfs, portals, mount helpers,
   notifications, Secret Service, clipboard, and polkit are external
   authorities. Floe passes the minimum target and never treats service
   availability as a security claim.
5. **Floe to helper/provider process.** Input bytes, process output, exit state,
   and claimed MIME support are untrusted. A future sandbox must start before
   processing and must not silently fall back to normal authority.
6. **Floe to cryptographic format/library.** The reviewed format/library owns
   primitives, framing, authentication, nonces, recipient wrapping, and KDF
   behavior. Floe owns exact-path I/O, secret lifetime, jobs, conflicts,
   cancellation, publication, cleanup, interoperability tests, and claims.
7. **Floe to credential storage.** A desktop-neutral credential service may
   store only explicitly opted-in secrets. Portable passphrase ciphertext must
   remain usable without a machine-local keyring entry.
8. **Normal to administrator authority.** Administrator locations use typed
   GFile/GVfs authority with persistent visible state. Normal paths, providers,
   terminals, custom actions, previews, and future plugins must not inherit it.
9. **Persistent to private/sensitive state.** Every cache, index, history,
   clipboard, notification, and journal write crosses a declared data-class
   boundary. Lock and policy changes advance generations and invalidate data
   before protected state is reported.

### Authority matrix

| Component | May receive | Must not receive or infer |
| --- | --- | --- |
| GTK UI | Escaped labels, bounded presentation, operation-local secret input | Reconstructed target paths, long-lived keys, unrestricted helper handles |
| Filesystem worker | Exact scoped paths and typed operation policy | GTK objects, unrelated roots, shell strings, broader authority on retry |
| Preview/metadata provider | One explicitly requested read-only target and isolated output | Home, network, devices, vault roots, session bus, or write authority unless policy grants it |
| Crypto worker | Operation secret wrapper, exact source/destination, reviewed format policy | Secret in argv/env/log, silent plaintext fallback, overwrite permission |
| Credential service | Explicitly opted-in scoped credential record | File content, implicit vault registration, portable-ciphertext dependency |
| Notification/clipboard | Explicitly approved redacted message or transfer value | Passwords, keys, scanner matches, or decrypted vault names by default |
| Privileged provider | Explicit typed administrator URI operation | Normal `PathBuf` masquerading as authority, plugin/provider inheritance |

## Abuse cases

| ID | Abuse or failure | Required control family |
| --- | --- | --- |
| T18-01 | Deceptive Unicode, whitespace, double extension, or lossy label targets another file | Exact identity, escaped display, suspicious-file evidence |
| T18-02 | Symlink, hard-link, or path replacement occurs between preflight and commit | No-follow open, device/inode revalidation, descriptor-relative traversal |
| T18-03 | Malformed or oversized file, ciphertext, manifest, journal, or provider output exploits a parser or allocation | Fixed limits, isolated parser/provider, hostile corpus, fail closed |
| T18-04 | A secret enters logs, `Debug`, config, argv, environment, notification, clipboard, or crash text | Secret wrappers, redaction, channel audits, lifecycle tests |
| T18-05 | A sandboxed, safe, encrypted, or verified action silently degrades to an ordinary action | Distinct actions, setup before use, explicit unavailable state, no downgrade |
| T18-06 | Failure overwrites a source or publishes a successful-looking partial output | Private staging, authenticated completion, atomic no-replace publication, cleanup proof |
| T18-07 | A wrong recipient, stale identity, duplicate recipient, or machine-local credential dependency makes protected data unavailable or misdirected | Fingerprints, confirmation, interoperability, explicit credential ownership |
| T18-08 | A helper escapes intended filesystem or network policy or survives cancellation | Deny-by-default policy, resource limits, process-group termination, policy verification |
| T18-09 | A thumbnail, index, history, notification, clipboard, or journal leaks sensitive identity or content | Data classification, policy partitioning, invalidation, explicit retention UX |
| T18-10 | Wrong-password and corrupt-data responses become a useful credential oracle | Conservative error taxonomy, uniform authentication handling, rate/lifetime review |
| T18-11 | A recursive scan, watcher storm, archive bomb, huge file, or provider hangs | Bounded queues/work, deadlines, cancellation, process-group termination |
| T18-12 | Device disconnect, full disk, crash, or power loss occurs at publication or verification boundary | Defined durability points, staged recovery, explicit uncertain/partial result |
| T18-13 | A delayed worker/provider result applies to a new selection or unlocked/locked generation | Generation identity, source revalidation, state invalidation |
| T18-14 | A normal target, custom action, or future plugin reuses administrator, mount, or portal authority | Typed authority, visible badge, minimum target, no inheritance |
| T18-15 | UI language overstates encryption, sandboxing, sanitization, integrity, deletion, or recovery | Prohibited-claim tests and evidence-linked status strings |
| T18-16 | A hash or verified-copy result is stale, raced, or presented as authenticity | Identity context, source/destination re-read policy, algorithm label, limited claim |
| T18-17 | A corrupt or attacker-edited recovery journal causes overwrite, unsafe resume, or deletion | Validated versioned minimum journal, integrity context, review-only recovery when uncertain |
| T18-18 | Same-user malware reads plaintext while a vault or encrypted file is unlocked | Explicit non-protection and minimized Floe-owned lifetime; no impossible claim |

## Explicit non-protections

Floe is not an antivirus, endpoint detector, intrusion detector, password
manager, backup system, anonymity system, digital-rights-management system, or
forensic-erasure tool. It does not protect against a compromised kernel,
keylogger, screen capture, hostile firmware, arbitrary same-user process-memory
access, or another application that receives plaintext while a vault is
unlocked. It cannot remove copies retained by clipboard managers,
notifications, desktop search, backups, snapshots, CoW history, flash
controllers, remote services, or other processes.

Portable encryption protects encrypted bytes according to the selected format;
it cannot revoke a recipient's existing key or copy. Sensitive Folder and
Private Mode reduce only named Floe-owned traces. Protected Folder is an
accidental-change guardrail. Hashes detect byte differences under a stated
algorithm and identity context; they do not establish authorship or
trustworthiness. Permanent deletion remains ordinary filesystem removal, not
guaranteed secure erase.

## Security data classification

| Class | Examples | Default persistence, log, and notification policy |
| --- | --- | --- |
| C0 — Public product data | Static command IDs, format versions, generic capability names | May persist/log normally; do not combine with sensitive values unnecessarily |
| C1 — User identity metadata | Paths, filenames, URIs, selections, queries, device labels, public recipient fingerprints | Persist only for a named feature; redact normal logs; omit notifications by default |
| C2 — User content | File bytes, decoded text/media, metadata values, archive listings, scanner findings | Memory/operation-local by default; never normal logs or notifications |
| C3 — Secret | Passphrases, private identities, recovery secrets, vault/file keys | Non-`Debug` wrapper; never ordinary persistence, argv/env, D-Bus, clipboard, notification, or log |
| C4 — Unlocked protected state | Decrypted names/content, vault search/index/thumbnail state, authorized handles | Memory or protected cache only; generation-bound; invalidate on lock |
| C5 — Security evidence | Hash manifests, verification outcome, sandbox-policy result, recovery journal | Versioned minimum necessary data with integrity/freshness context and explicit retention |

Every persistent record must declare its classes, owner, path, permissions,
capacity, version, corruption behavior, cleanup, Private/Sensitive/vault policy,
and whether another process may retain a copy.

## Security invariants

- No homemade cipher, KDF, MAC, nonce scheme, signature system, key exchange,
  credential protocol, or vault format is introduced for convenience.
- Protected actions fail closed: no plaintext, unsandboxed, broader-authority,
  overwrite, follow-link, weaker-format, or normal-launch fallback.
- Originals survive failed encryption and sanitization. Unknown partial output
  is never silently deleted unless Floe proves its ownership and incomplete
  state.
- Secrets never enter normal logs, `Debug`, config, command-line arguments,
  environment variables, D-Bus action parameters, notifications, or filenames.
- UI text is not path identity. All security-sensitive work retains and
  revalidates exact original identities.
- Security-critical work is bounded and cancellable without abandoning an
  ambiguous published state.
- Claims such as “encrypted,” “sandboxed,” “sanitized,” “integrity verified,”
  “locked,” and “safe to remove” require evidence that the named mechanism
  completed.

## Prohibited claims

Floe must not claim “military-grade,” “unhackable,” antivirus protection,
malware detection, secure erase, complete metadata removal, anonymity, sender
authenticity from a checksum, sandboxing when restriction setup failed, vault
protection while plaintext is exposed to another application, or guaranteed
recovery before the corresponding verified mechanism exists. “Open Safely,”
“Encrypted Vault,” “Sensitive Folder,” “Protected Folder,” “Private Mode,” and
“Integrity verified” retain the exact meanings in
[PRIVACY_SECURITY.md](../PRIVACY_SECURITY.md).

## Review and change control

Each later Phase 18 leaf must cite threat IDs and decision IDs, update the test
traceability matrix, and record unresolved risks. Changing a cryptographic
format, secret backend, sandbox boundary, vault backend, privileged authority,
or recovery-journal design requires a new or superseding decision entry with a
compatibility and migration plan. Phase 18AA must audit the combined system;
completing individual leaves does not make the whole security family stable.
