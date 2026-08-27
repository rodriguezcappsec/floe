# Floe Phase 18 Security Test Plan

Status: **COMPLETE Phase 18A traceability baseline**, reviewed `2026-08-27`.

This plan does not prove later features exist. A leaf may move to `COMPLETE`
only after its implementation updates this mapping with concrete test names,
commands, results, dependency evidence, unresolved risks, and applicable native
smoke evidence.

## Permanent isolation rules

- Filesystem integration tests operate only below a fresh `tempfile` root and
  injected Trash. They never inspect or mutate real HOME, Trash, XDG roots,
  mounts, credentials, devices, or user data.
- Native GTK/E2E launches use temporary private HOME, XDG config/cache/data/state
  and runtime roots, an isolated Trash, and disposable fixtures. Privileged,
  removable-media, session-lock, and compositor tests require an explicitly
  disposable environment.
- Network is denied unless the specific test verifies an explicitly opted-in
  network boundary. No fixture contains a real password, key, token, personal
  path, user document, or production credential-service record.
- Cryptographic tests use published interoperability vectors plus generated
  throwaway identities and passphrases. Secrets never appear in snapshots,
  assertion messages, process listings, environment, logs, or retained artifacts.
- Provider/sandbox tests use disposable helper binaries and a fake home/device/
  network canary set. A test passes only when forbidden access and descendants
  are denied, not merely when expected output appears.
- Race, interruption, and recovery tests control synchronization points rather
  than relying on sleeps. Power-loss claims require documented fault-injection
  boundaries and must not exceed what the test proves.

## Hostile-input corpus

Maintain bounded checked-in generators or synthetic fixtures for raw non-UTF-8
names, separators and traversal forms, leading dashes, newlines, tabs, bidi and
control characters, confusables, huge declared lengths, sparse files, symlink
and hard-link swaps, recursive trees, malformed/truncated/tampered ciphertext,
manifests, metadata and journals, duplicate records, decompression bombs, hung
providers, wrong credentials, full disk, disconnect, and stale generations.
Property failures retain reproducible seeds or regression cases.

## Failure taxonomy

Security-facing results distinguish at least: unavailable mechanism, unsupported
input, permission denied, authentication failed, malformed or tampered input,
changed/stale source, conflict, policy violation, resource limit, cancellation,
timeout, disconnect, partial but recoverable, published with cleanup failure,
and uncertain recovery. Errors must not reveal secrets or turn authentication
failure into a useful oracle. A protected action never reclassifies one of these
as ordinary success.

## Phase traceability

Test-layer abbreviations: **U** Rust unit/property, **FS** isolated filesystem
integration, **GTK** ignored real-widget/accessibility, **E2E** isolated native
AT-SPI workflow, **W** applicable generic/Niri/Plasma Wayland smoke, **I**
interoperability/fault-injection/dependency audit.

| Phase | Threats | Decisions | Layers | Required success and failure cases | Isolation and evidence required before `COMPLETE` |
| --- | --- | --- | --- | --- | --- |
| 18B | T18-02–06, T18-10–13, T18-15 | 01–03, 10–12, 15 | U, FS, I | Published vectors and independent-tool round trip; huge streaming; wrong password; malformed/truncated/tampered input; link/race/conflict/full-disk/cancel/crash cleanup; original survives | Temp roots and generated secrets; selected crate/version/features/advisory/license rationale; interop fixture provenance; no secret/log/argv/env leak; exact gate results |
| 18C | T18-04–07, T18-10, T18-12–15 | 01–04, 11–12 | U, GTK, E2E, W | Encrypt/decrypt one and many; conflict and progress; mismatch/retry/cancel; output authentication before optional Trash; focus and password visibility; no downgrade | Private XDG/Trash and fake secrets; controller tests plus accessible native prompt/Operations Island evidence; screenshots/logs contain no secret |
| 18D | T18-04, T18-06–07, T18-10, T18-15 | 02–04, 10–12, 15 | U, FS, GTK, I | Recipient interop; fingerprint confirmation; duplicate/mixed recipients; stale/wrong/public/private identity; unavailable credential service; cancellation and source preservation | Generated throwaway identities only; independent-tool vectors and secret-channel audit; no implicit sync or keyring dependency |
| 18E | T18-04, T18-09–10, T18-13, T18-15, T18-18 | 03–04, 07, 12, 15 | U, GTK, E2E, W | Create/use/timeout/manual Lock/Lock All; focus changes; concurrent operations; app exit; stale response; service lock/denial; no Debug/clone/persistence leak | Private session/XDG and fake clock/service; wrapper/API review; memory/lifecycle limitations documented; accessible locked/unlocked state evidence |
| 18F | T18-03–07, T18-09–10, T18-12, T18-15, T18-18 | 02–05, 07, 10–12 | U, FS, I | Backend comparison; key hierarchy; filename/structure leakage; password change; recovery; version/migration/downgrade; corrupt metadata; portability | Disposable vault corpus and generated keys; architecture/dependency review and independent recovery evidence; no implementation before decision approval |
| 18G | T18-02–06, T18-09–13, T18-17–18 | 03, 05, 07, 10–13 | U, FS, I | Create/read/write/rename/move/truncate/large/random files; exact non-UTF-8 names; concurrency; links; full disk; crash; disconnect; corrupt vault; no plaintext temp | Disposable test filesystem/mount only; fault matrix at each commit point; format compatibility and leak inventory; source/vault consistency evidence |
| 18H | T18-04–07, T18-09–10, T18-13, T18-15, T18-18 | 03–05, 07, 12 | U, GTK, E2E, W | Create/Add/Unlock/Lock/Lock All/password change/remove registration/recovery; wrong secret; open handles; unavailable backend; removal is not deletion | Private HOME/XDG and disposable vault; semantic roles/states/focus; no secret in AT-SPI/logs; native lifecycle evidence |
| 18I | T18-05, T18-09, T18-12–13, T18-15, T18-18 | 05, 07, 12 | U, FS, E2E, W | Timeout, app close, reliable session lock, suspend, open handles, delayed lock, drive removal, disconnect and stale generation; no unsafe forced unmount | Fake clock/signal source plus disposable mounts; real supported session-lock/suspend smoke separately; explicit skip/limitation record |
| 18J | T18-04, T18-09, T18-13, T18-15, T18-17–18 | 03, 07, 12–13, 15 | U, FS, GTK, E2E | Inventory thumbnails/previews/Recents/search/index/jobs/session/clipboard/notifications/log/journal; lock invalidation; crash/restart/migration; stale results | Private HOME/XDG with canary scan before/after every lifecycle; external-retention limitations; zero unauthorized Floe-owned trace evidence |
| 18K | T18-05, T18-09, T18-13, T18-15 | 07, 12, 15 | U, FS, GTK, E2E, W | Sensitive Folder and Private Mode suppress each declared trace; nested/moved/renamed roots; mixed windows; restart/crash; clear limitation text; neither claims encryption | Private XDG/canary fixtures; full trace inventory and accessible persistent mode indicator; no claim beyond Floe-owned data |
| 18L | T18-03, T18-05, T18-08, T18-11, T18-13, T18-15 | 06, 10, 12, 15 | U, FS, I, GTK | Target-only read and isolated output; deny unrelated files/devices/vault/network/session bus/write; hostile parser; limits; timeout/cancel; descendants; setup unavailable/fails | Fake helper plus canary namespace and network denial; inspect effective policy; selected-tool rationale; no unsandboxed fallback; fuzz/corpus results |
| 18M | T18-01, T18-03, T18-05, T18-08, T18-14–15 | 06, 09, 12, 14–15 | U, GTK, E2E, W, I | Compatible restricted launch; forbidden filesystem/network/write; persistent active indicator; missing/setup-failed policy; normal Open remains distinct; hostile name | Disposable app/helper and private HOME; policy evidence and accessibility; unsupported apps fail explicitly, never launch normally |
| 18N | T18-01, T18-03, T18-13, T18-15 | 12, 14–15 | U, FS, GTK | Executable/content evidence, double extension, MIME mismatch, origin, bidi/control/confusable; legitimate international and non-UTF-8 names; stale metadata; false positives | Checked-in synthetic corpus only; escaped labels and exact identity; explanations cite evidence and never say malware/antivirus |
| 18O | T18-01, T18-03, T18-09, T18-11, T18-13, T18-15 | 07, 12, 14–15 | U, FS, GTK | Known GPS/EXIF/author/org/app/thumbnail fixtures; absent/malformed/oversized/changing metadata; bounded reads/cancel; exact finding provenance | Synthetic metadata corpus with no personal data; local-only, memory-bounded evidence; no exhaustive-inspection claim |
| 18P | T18-02–06, T18-09, T18-12–13, T18-15 | 07, 11–12, 14–15 | U, FS, GTK, E2E | Format-specific before/after verified removals; batch preview; unsupported fields; cancel/full disk/race/conflict/provider failure; source always preserved | Temp roots and known metadata corpus; atomic no-replace evidence; compare output with format-aware verifier; no exhaustive-removal claim |
| 18Q | T18-04–07, T18-09–10, T18-12–15 | 02–04, 07, 11–12, 14–15 | U, FS, GTK, E2E, I | Compose inspect/sanitize/encrypt/checksum; step ordering; password and recipients; each step fail/cancel/conflict; original unchanged; output independently verified | Temp roots/generated secrets/no network; per-step evidence and interop; Share disabled or explicit when downstream service unavailable |
| 18R | T18-01–03, T18-12–15 | 09, 11–12, 14 | U, FS, GTK | Explain mode/owner/ACL/xattr/capability/immutable evidence; symlinks and unsupported filesystems; conservative fix preview; race/partial edit/rollback; administrator separation | Temp filesystem with capability-based skips; never real home; exact pre/post metadata and accessible explanation evidence |
| 18S | T18-03–05, T18-09, T18-11, T18-13, T18-15 | 07, 12, 14–15 | U, FS, GTK | Explicit local scan; known synthetic keys/tokens/`.env`; redacted findings; binary/huge/inaccessible/linked/changed files; false positives; bounds/cancel; no upload | Synthetic secrets only and network denied; logs/UI never expose full value; no background scan, malware, or exhaustive-detection claim |
| 18T | T18-01–03, T18-11–13, T18-15–16 | 08, 10, 12, 15 | U, FS, GTK, E2E, I | Save/verify SHA-256 fingerprint; strict `SHA256SUMS` generate/verify; empty/large files; raw hostile/non-UTF-8 names; malformed/duplicate/escaping entries; changed/missing/new/link/race/cancel | Temp roots, published GNU fixtures where representable, no shell; exact path-safe manifest and algorithm wording; hash-not-authenticity UI evidence |
| 18U | T18-09, T18-11–13, T18-15–16 | 07–08, 12, 15 | U, FS, GTK, E2E | Explicit baseline; coalesced create/change/delete/rename; event storms; watcher overflow/offline gap; mount loss; stale baseline; pause/disable; no intrusion claim | Temp roots/fake watcher clock; bounded queue and rescan policy evidence; private baseline storage review and accessible uncertainty |
| 18V | T18-02, T18-05–06, T18-11–13, T18-15–16 | 08, 11–12 | U, FS, GTK, E2E | Optional copy then source/destination verification; source changes during copy/hash; injected corruption; conflict/full disk/cancel/retry; sync/cleanup failure; ordinary Copy unchanged | Temp roots and injected I/O/hash faults; exact digest/identity/durability evidence; never claim authenticity or default verification |
| 18W | T18-05–06, T18-11–16 | 08, 11–12 | U, FS, GTK, E2E, W | Copy, Verify, Flush, Eject ordered state machine; each step fail/cancel/disconnect; changed source; device busy; retry; no “safe” before successful eject | Explicit disposable virtual/removable device only; mocked deterministic unit/FS plus separate real-device lab evidence; never user media |
| 18X | T18-01–02, T18-05–06, T18-12–15 | 09, 11–12 | U, FS, GTK, E2E | Protected Folder scope; destructive thresholds; mounted/root/home boundaries; huge and mixed selections; stale selection; confirmation and override; partial failure; no security/encryption claim | Temp roots/fake mounts only; exact target/action summary and accessible confirmation evidence; guardrail bypass remains explicit and logged without sensitive paths |
| 18Y | T18-02–06, T18-09, T18-12–13, T18-15, T18-17 | 07, 11–13 | U, FS, GTK, E2E, I | Crash at every operation state; valid resume/review; truncated/tampered/versioned/duplicate/stale journal; link/path escape; lock/private mode; unknown partial; no automatic destructive replay | Temp roots/private XDG and deterministic crash injection; journal permissions/atomicity/data-class audit; conservative recovery proof and migration fixtures |
| 18Z | T18-04–05, T18-09, T18-13–15 | 07, 12, 14–15 | U, GTK, E2E, W | Aggregate vault/sensitive/session/integrity/finding states; stale/unavailable/partial states; exact actions; no fear score; no secret/path leak; keyboard/screen reader/high contrast | Private fixtures/fake services; state derived from verified owners, not duplicated; semantic accessibility and native lifecycle evidence |
| 18AA | T18-01–18 | 01–16 | U, FS, GTK, E2E, W, I | Combined crypto, dependency, secret, cache, parser, vault, sandbox, privileged, integrity, transfer and recovery audit; regressions; upgrades/removal; every claim and finding closed or recorded | Fresh full corpus and isolated environments; reproducible commands/results/skips; dependency/advisory/license report; external review/fuzz status; no stable Phase 18 claim before pass |

## Dependency and audit evidence

Every implementation phase records the selected component and version, purpose,
alternatives, maintenance and audit history, license, transitive/native/unsafe
surface, enabled features, advisory tool and timestamp, known unresolved
advisories, MSRV/build impact, interoperability source, and replacement/migration
plan. A clean advisory scan is evidence, not proof of security. Phase 18AA repeats
the review across the resolved lockfile and the composed runtime boundaries.

## Evidence template for later leaves

```text
Phase:
Threat IDs / decision IDs:
Implementation commit and dependency review:
Unit/property tests:
Isolated filesystem tests:
GTK/E2E/Wayland gates (or exact N/A reason):
Interoperability/fuzz/fault-injection evidence:
Secret, trace, network, and prohibited-claim audit:
Commands and results:
Environment-dependent skips and limitations:
Unresolved risks / follow-up owner:
```
