# Floe Phase 18A Security Decision Record

Status: **COMPLETE architecture baseline**, reviewed `2026-08-27`.

These decisions constrain later Phase 18 implementations. **Accepted for later
implementation** means the architecture or policy is accepted, not that runtime
behavior exists. **Candidate** means implementation-time review is still
required. Phase 18A adds no dependency and selects no final crypto library,
credential backend, vault backend, or sandbox mechanism.

### SEC-18A-01 — Phase 18A changes architecture, not runtime

- **Status:** Accepted for later implementation.
- **Context:** Security claims are unsafe when prose, dependencies, and UI land
  before threat, lifecycle, and failure behavior are bounded.
- **Decision:** Phase 18A contains only threat analysis, decisions, dependency
  rationale, sequencing, and a test plan. Phase 18B and later own code and user
  surfaces.
- **Rationale:** This gives implementation reviews stable IDs without implying
  protection from documentation alone.
- **Rejected/deferred:** Adding a crypto crate, password dialog, vault scaffold,
  helper sandbox, or security badge “for later” is rejected in this phase.
- **Implementation gates:** Manifest/lockfile diff is empty; runtime source diff
  is attributable only to pre-existing work; later status remains `PLANNED`.

### SEC-18A-02 — Portable encryption format and library

- **Status:** `age` v1 is the leading candidate; dependency selection deferred
  to Phase 18B.
- **Context:** Floe needs streaming, authenticated, interoperable passphrase and
  later recipient encryption without inventing cryptography.
- **Decision:** Phase 18B must review the current age specification and at least
  one maintained pure-Rust implementation for authenticated streaming,
  passphrase KDF behavior, recipient interoperability, maintenance, audit
  history, MSRV, licensing, and dependency surface. The output must identify its
  format/version and fail closed.
- **Rationale:** An established external format permits independent recovery and
  fixtures while keeping primitives outside Floe.
- **Rejected/deferred:** Custom framing, KDF/MAC/nonce design, shelling out with
  secrets, opaque Floe-only ciphertext, unauthenticated encryption, and
  plaintext fallback are rejected. A final crate/version is deferred to 18B.
- **Implementation gates:** Cross-tool fixtures; wrong-password,
  malformed/truncated/tampered input; large streaming files; cancellation;
  private staging; atomic no-replace publication; source preservation.

### SEC-18A-03 — Secret wrappers and memory lifetime

- **Status:** Candidate; selection deferred to Phase 18E and any earlier phase
  that first handles secrets.
- **Context:** Ordinary `String`, `Clone`, `Debug`, async captures, and error
  conversion can multiply secret lifetime and leak values.
- **Decision:** Review secrecy-style non-`Debug` wrappers and zeroize-on-drop
  support. Secret-bearing types must make exposure explicit, avoid implicit
  clone/serialization, remain operation- or privacy-session-scoped, and never
  cross argv, environment, logs, notifications, clipboard, filenames, config,
  or D-Bus action parameters.
- **Rationale:** Type pressure and short lifetimes reduce accidental leakage,
  while documentation remains honest about allocator, swap, crash, and
  same-user limitations.
- **Rejected/deferred:** A global master-password `String`, best-effort manual
  buffer overwrite marketed as complete erasure, and secret-bearing `Debug` are
  rejected. Concrete crates and timeout semantics are deferred.
- **Implementation gates:** Compile-time/API review plus log/error/config/crash,
  focus, timeout, cancellation, clone, drop, lock, and shutdown tests.

### SEC-18A-04 — Desktop-neutral credential storage

- **Status:** Candidate; final backend and UX deferred to the phase that first
  persists an opted-in credential.
- **Context:** Plasma, GNOME, and other Wayland sessions expose different
  services. Phase 14 can report Secret Service presence but reads no secret.
- **Decision:** Prefer a desktop-neutral Secret Service abstraction with
  explicit opt-in, scoped record identifiers, locked/unavailable behavior, and
  no secret in normal preferences. Portable passphrase files must always remain
  decryptable through user entry without a machine-local keyring record.
- **Rationale:** Standards-first storage preserves desktop portability and
  avoids making KWallet or one compositor part of core.
- **Rejected/deferred:** Plaintext config, implicit password saving, mandatory
  KDE/GNOME backend, silent fallback, and storing file content are rejected.
  KWallet-specific enhancement stays deferred with Plasma integration.
- **Implementation gates:** Service absent/locked/replaced, denial, migration,
  deletion, duplicate scope, crash, secret-channel, and portable recovery tests.

### SEC-18A-05 — Encrypted vault backend

- **Status:** Deferred to Phase 18F architecture review.
- **Context:** A usable vault requires filename privacy, large-file/random-access
  semantics, crash safety, concurrency, password change, recovery, portability,
  and no persistent plaintext staging.
- **Decision:** Phase 18F must compare established designs such as Cryptomator,
  gocryptfs, and reviewed FUSE or application-owned virtual-filesystem options
  against Floe's exact-path/job architecture. It must select or reject a backend
  before 18G code.
- **Rationale:** A password dialog over a plaintext directory is security theater;
  premature format code creates long-lived compatibility and recovery risk.
- **Rejected/deferred:** A homegrown vault format, plaintext folder branded as a
  vault, absolute machine paths, unsafe forced unmount, and silent format
  downgrade are rejected.
- **Implementation gates:** Independent format recovery where applicable;
  filename/structure leakage analysis; large/random writes; rename/truncate;
  crash/full-disk/disconnect; concurrent handles; password change; recovery.

### SEC-18A-06 — Provider sandbox boundary

- **Status:** Phase 18L selected Bubblewrap for external providers; restricted
  application launch is intentionally outside current scope.
- **Context:** Current in-process parsers and system thumbnail providers run with
  normal user authority. A helper process alone is not a sandbox.
- **Decision:** A restricted action must establish a verifiable deny-by-default
  boundary before untrusted bytes execute: target-only read, isolated writable
  temp, no unrelated home/device/vault access, no network by default, bounded
  resources, process-group termination, and explicit unsupported state. It must
  never silently launch normally.
- **Rationale:** Bubblewrap can assemble Linux namespaces, while portals can
  narrow desktop grants and Landlock may add filesystem defense in depth; actual
  availability and policy must be verified at implementation time.
- **Rejected/deferred:** Renaming a helper “sandboxed,” fake blur-style security
  indicators, silent fallback, and whole-home read access are rejected.
- **Implementation gates:** Policy-inspection tests, escape corpus, missing
  mechanism, setup race/failure, network/filesystem denial, timeout/cancel,
  descendant termination, compatibility, and non-color status UI.

### SEC-18A-07 — Cache, history, and lock invalidation

- **Status:** Accepted for later implementation.
- **Context:** Paths, queries, thumbnails, previews, notifications, indexes,
  sessions, jobs, clipboard, and journals can outlive protected work.
- **Decision:** Every persistent or cross-process surface declares data classes,
  owner, capacity, permissions, version, retention, corruption behavior, and
  Private/Sensitive/vault policy. Lock advances a generation and invalidates
  protected in-memory and Floe-owned persistent state before reporting locked.
- **Rationale:** One central trace policy is testable and prevents ad-hoc
  “private” exceptions.
- **Rejected/deferred:** Scattered booleans, best-effort cleanup followed by a
  success claim, and claims about traces owned by other processes are rejected.
- **Implementation gates:** Inventory-based leak tests across all lifecycle
  paths, stale result rejection, lock ordering, crash/restart, migration, and
  explicit external-retention limitations.

### SEC-18A-08 — Integrity algorithm and portable manifests

- **Status:** Accepted for Phase 18T architecture.
- **Context:** Phase 10E already provides reviewed streaming SHA-256 checksums;
  Floe needs saved fingerprints and portable directory verification.
- **Decision:** Phase 18T reuses that SHA-256 engine and supports a strict,
  path-safe `SHA256SUMS`-compatible manifest profile. Results retain algorithm,
  exact identity context, and verification time/state. Hash equality is byte
  integrity evidence, not sender authenticity or malware safety.
- **Rationale:** SHA-256 is widely interoperable and the existing bounded engine
  minimizes new cryptographic surface.
- **Rejected/deferred:** A new hash crate without need, digest-as-authenticity,
  lossy path reconstruction, shell parsing, following links, silent omission,
  and automatic background monitoring are rejected. Monitoring stays 18U.
- **Implementation gates:** GNU-compatible fixtures where representable;
  non-UTF-8 and hostile names; malformed/duplicate/escaping manifest entries;
  changed/missing/new files; symlinks; races; cancellation; bounded traversal.

### SEC-18A-09 — Privileged access

- **Status:** Accepted architecture; user-facing implementation remains planned.
- **Context:** Some locations require administrator authorization, but elevating
  the entire file manager would also elevate previews, terminals, custom
  actions, and unrelated operations.
- **Decision:** Use typed GFile/GVfs `admin://` targets with polkit-mediated,
  minimum-scope authorization and a persistent visible privileged state. Exact
  target and authority must not be reconstructed from display text or inherited
  by normal actions.
- **Rationale:** The existing GIO application boundary can isolate privileged
  locations without contaminating `floe-core` path models or process authority.
- **Rejected/deferred:** Running Floe via `sudo`/pkexec, shell interpolation,
  privileged terminal/plugin/provider inheritance, and hidden privilege state
  are rejected.
- **Implementation gates:** Denial/cancel/timeout, stale authorization, exact URI,
  no authority inheritance, normal-navigation recovery, accessibility, and
  polkit/GVfs-unavailable native tests.

### SEC-18A-10 — Security dependency admission

- **Status:** Accepted policy.
- **Context:** Security-sensitive dependencies can expand unsafe/native code,
  parser, cryptographic, process, and maintenance risk.
- **Decision:** Admit a dependency only in its implementation phase after
  documenting why standard Rust/GIO/XDG APIs are insufficient, maintenance and
  release cadence, license, audit/advisory history, transitive/native surface,
  unsafe usage, feature minimization, MSRV/build impact, and a removal or
  migration strategy. Pin via `Cargo.lock`; run available advisory/license
  tooling and record limitations.
- **Rationale:** Candidate names in architecture prose are not dependency
  approval.
- **Rejected/deferred:** Adding crates speculatively, enabling broad default
  features, or treating popularity as a security audit is rejected.
- **Implementation gates:** Written dependency note, minimal features, lockfile
  review, tests/fuzzing as applicable, advisory evidence, and final 18AA audit.

### SEC-18A-11 — Staging and publication failure policy

- **Status:** Accepted for later implementation.
- **Context:** Encryption, sanitization, verified copy, and recovery can destroy
  or misrepresent data if output is published before complete validation.
- **Decision:** Write to a private same-destination-filesystem sibling when
  possible, preserve source, authenticate/validate output, synchronize at the
  defined durability boundary, revalidate identities, and publish atomically
  with no-replace semantics. Cleanup occurs only for a proven Floe-owned partial
  and reports uncertainty honestly.
- **Rationale:** This extends existing copy/move/archive safety patterns to
  security-sensitive transforms.
- **Rejected/deferred:** Silent overwrite, source-first deletion, globally
  predictable temp paths, success before validation, and guessing cleanup after
  a crash are rejected.
- **Implementation gates:** Conflict, race, link replacement, full disk,
  cancellation, crash-point, sync failure, publication failure, cleanup failure,
  and source/output permission tests.

### SEC-18A-12 — Security terminology and product semantics

- **Status:** Accepted policy.
- **Context:** Similar UI labels can imply protections the code does not provide.
- **Decision:** **Encrypted Vault** means real encrypted storage; **Sensitive
  Folder** means reduced Floe-owned traces; **Protected Folder** means
  accidental-change guardrails; **Private Mode** means history/cache
  minimization; and **Integrity verified** requires completed verification.
  Ordinary Open/Open With must never be described as sandboxed. State is textual and
  accessible, not color-only.
- **Rationale:** Precise language is part of the security boundary.
- **Rejected/deferred:** Marketing euphemisms, fear scores, “secure erase,”
  antivirus claims, and degraded actions retaining protected names are rejected.
- **Implementation gates:** Central strings/state model, unavailable/failure
  cases, accessibility tests, documentation review, and 18AA claim audit.

### SEC-18A-13 — Privacy-aware recovery journal

- **Status:** Minimum architecture accepted; format deferred to Phase 18Y.
- **Context:** Interrupted cross-filesystem and security-sensitive jobs need
  recovery, but a journal can leak paths, secrets, vault names, and destructive
  intent or authorize unsafe replay.
- **Decision:** Use a versioned, typed, bounded, private, atomic journal that
  stores only the minimum C1/C5 identity and state needed. Never store secrets or
  content. Validate corruption and freshness; uncertain records are review-only
  and never trigger automatic overwrite, source deletion, or authority reuse.
- **Rationale:** Conservative recovery improves data safety without turning
  persisted intent into a confused deputy.
- **Rejected/deferred:** Generic serialized job objects, secret-bearing records,
  automatic destructive replay, and silent deletion of unknown partials are
  rejected.
- **Implementation gates:** Every crash point, truncation/tampering/version,
  permissions, symlink/path escape, stale identity, duplicate replay, privacy
  mode, vault lock, migration, and cleanup tests.

### SEC-18A-14 — Suspicious-file and privacy findings

- **Status:** Accepted evidence model for Phases 18N–18S.
- **Context:** Executability, double extensions, MIME mismatch, Unicode controls,
  origin metadata, personal metadata, permissions, and secret-like patterns are
  useful signals but not proof of malicious intent.
- **Decision:** Findings are typed evidence with source, limits, and explanation.
  Floe reports uncertainty, supports false positives, escapes names, redacts
  secret values, and never labels a file malware or claims exhaustive removal.
- **Rationale:** Calm, explainable evidence helps users act without pretending to
  be an antivirus or data-loss-prevention system.
- **Rejected/deferred:** Fear scores, cloud lookup by default, content uploads,
  secret-value display, automatic quarantine/deletion, and one-extension verdicts
  are rejected.
- **Implementation gates:** Known benign/malicious-shaped corpus, Unicode and
  non-UTF-8 names, malformed metadata, false positives, redaction, local-only
  behavior, cancellation, and accessible explanation.

### SEC-18A-15 — Local-first telemetry and submission policy

- **Status:** Accepted policy.
- **Context:** Security features can reveal filenames, hashes, metadata, secret
  matches, vault state, and user behavior to telemetry or reputation services.
- **Decision:** Floe performs no telemetry, hash/content upload, cloud scanning,
  recipient synchronization, or remote reputation query by default. Any future
  network feature requires a separate explicit scope, consent, data-flow record,
  redaction policy, failure behavior, and off switch.
- **Rationale:** Local-first processing minimizes a major trust boundary and
  preserves offline operation.
- **Rejected/deferred:** Implicit analytics and “anonymous” upload without a
  reviewed data inventory are rejected.
- **Implementation gates:** Network-denial tests, dependency/network endpoint
  inventory, opt-in persistence review, traffic inspection, and documentation.

### SEC-18A-16 — Phase 18 sequencing

- **Status:** Accepted.
- **Context:** Niri, Plasma-specific, remote, and Android/MTP integrations are
  user-deferred. Encryption, vault, and sandbox work require larger dependency
  and lifecycle reviews. The existing SHA-256 engine makes integrity tools a
  bounded high-value next step.
- **Decision:** Mark Phase 18A complete after documentation and repository gates,
  defer Phases 15–17, and select exactly Phase 18T — Integrity Tools as `NEXT`.
  Other Phase 18 leaves remain `PLANNED` and their dependency ordering remains
  authoritative.
- **Rationale:** 18T adds useful local verification while reusing reviewed code
  and avoiding premature secret, vault, or helper authority.
- **Rejected/deferred:** Automatically starting 18T during this phase or marking
  the whole Phase 18 family complete is rejected.
- **Implementation gates:** One roadmap `NEXT`; cross-document agreement; 18T
  must create its own plan/gates and satisfy SEC-18A-08 and the test matrix.
