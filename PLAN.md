# Active plan: persistent background-operation feedback regression

## User-visible contract

1. An accepted long-running action must immediately expose persistent, accessible
   running feedback. Losing focus, navigating Back/Forward, switching folders,
   changing selection, tabs, panes, or windows must not erase the state.
2. Running feedback owns the applicable Cancel action and cannot be displaced by an
   unrelated task. Repeated activation is rejected truthfully.
3. Completion, partial completion, cancellation, and failure replace only their own
   running state with durable dismissible feedback. Reports and created outputs stay
   reachable through explicit View Results or Reveal actions after returning to Floe.
4. Background work remains application-owned and bounded; GTK callbacks only submit,
   cancel, and present state. Exact paths and sensitive findings remain memory-only.

## Depth tree

1. Trace ClamAV and privacy/sanitization request, application routing, per-window
   generation, GTK presentation, cancellation, and completion paths.
2. Add one reusable per-window background-feedback coordinator supporting concurrent
   task IDs, non-expiring running/outcome toasts, exact action ownership, and bounded
   retained report/output state.
3. Wire the three security workflows, then perform the adjacent background feedback audit
   across Operations Island, search/indexing, duplicate scans, properties/checksums,
   preview/thumbnail work, storage actions, and close/navigation/focus transitions.
4. Add lowest-layer lifecycle/routing tests plus applicable real-GTK contracts; run
   focused, workspace, documentation, and native gates before updating status.

## Status log

- 2026-08-31: Regression reproduced from code path: accepted ClamAV work emits only a
  three-second toast, so focus loss or a longer scan leaves no visible running state.
  Gates F1-F5 defined before implementation. Edge-case-hunter optional Rust/Linux/UI
  reference files are absent from this checkout; its complete core workflow is in use.
- 2026-08-31: The background feedback audit traced file jobs to the persistent
  Operations Island/history, filename/content search and search indexing to their
  visible status plus Stop controls, duplicate scans to their persistent progress and
  results dialogs, metadata sorting to spinner/status/cancellation, preview/thumbnail
  work to local loading/error presentation, and device actions to row busy/outcome
  feedback. The concrete transient-only gaps were ClamAV, Privacy inspection,
  sanitization, and Properties. The audit also found and fixed selection-driven silent
  Properties supersession and Privacy cancellation paths that emitted no terminal
  result. Concurrent security activities now use separate panel rows rather than
  mutually blocking persistent toast queues.

---

# Archived active plan: Floe Phase 23H and local security-inspection program

## User-authorized scope

Implement these six bounded capabilities in dependency order:

1. Phase 23H — one application-owned multi-window mutation/event coordinator
   and bounded multi-window session restoration.
2. Phase 18L — enforceable sandboxing for external thumbnail/Preview providers.
3. Phase 18N — explainable suspicious-file analysis without a malware verdict.
4. Phase 18N2 — optional local ClamAV scanning through a separately installed
   `clamd` Unix socket, with no upload or direct `libclamav` linkage.
5. Phase 18O — format-specific, read-only Privacy Inspector findings.
6. Phase 18P — no-overwrite sanitized copies for the formats Floe can verify.

This program excludes encryption, vaults, Open Safely, cloud reputation,
automatic deletion/quarantine, behavioral execution, YARA, remote/MTP scanning,
and Niri/KDE-specific integration.

## Cross-feature contracts

- Exact `PathBuf`/`OsString` identity remains authoritative. Security labels and
  lossy display text never reconstruct a path.
- GTK callbacks submit typed requests only. Traversal, MIME/content inspection,
  ClamAV streaming, provider launch policy, and sanitization run on bounded
  application/core workers.
- One normal Floe process owns exactly one mutation/recovery state and one
  terminal event drain. Windows subscribe to coordinated outcomes; they do not
  compete for destructive events or start additional persistence writers.
- The session store persists at most 16 normal windows, each with an existing
  bounded `BrowserTabs` workspace. Version 1 single-window data migrates; corrupt,
  oversized, duplicate, Private, and Sensitive state fails safely.
- External providers run only when a verified Bubblewrap policy is active:
  private namespaces, no network/session bus, target-only read, output-only
  private writable directory, resource/deadline limits, process-group teardown.
  Missing or failed policy means an explicit unavailable provider, never an
  unsandboxed fallback.
- Suspicious-file findings are evidence: executable state, extension/MIME
  mismatch, double extension, and Unicode/control hazards. They are not a
  malware diagnosis.
- ClamAV is optional and local. Floe discovers reviewed Unix socket locations,
  streams bounded regular-file bytes with `INSTREAM`, revalidates source identity,
  reports engine/signature/limit/cancellation/error states, and never calls an
  unscanned or no-signature result safe. Process-wide generations route results
  to the submitting window. Scanner findings remain memory-only and path-free in
  logs/notifications.
- Privacy findings are format-specific and bounded. Unsupported or malformed
  content remains explicit; absence of a finding is not an exhaustive privacy
  guarantee.
- Sanitization never modifies or replaces a source. It writes a private staged
  sibling, cleans failed staging, verifies selected removals and WebP feature
  flags, atomically publishes with no-overwrite semantics, and truthfully reports
  unsupported formats or partial cleanup.

## Depth tree and ledgers

- Phase 23H: `gates/phase-23h-multi-window-runtime.md`
- Phase 18L: `gates/phase-18l-sandboxed-providers.md`
- Phase 18N/18N2: `gates/phase-18n-local-threat-analysis.md`
- Phase 18O: `gates/phase-18o-privacy-inspector.md`
- Phase 18P: `gates/phase-18p-metadata-sanitization.md`
- Root integration: `gates/phase-security-inspection-integration.md`

## Applicable verification layers

- Deterministic unit/property tests for codecs, bounds, raw names, classification,
  protocol parsing, sanitization, and claim wording.
- Tempfile-only filesystem integration for no-follow identity races, provider
  policy, fake `clamd`, malformed media, cancellation, staging, and no-overwrite.
- Focused ignored real-GTK tests for actions, accessible dialogs, and multi-window
  lifecycle where the host display permits.
- Isolated native Wayland smoke for restore, close-one/survivor, sandbox helper,
  fake-scanner action, sanitization publication, Ping, and clean Quit.
- Full format/check/strict-Clippy/workspace tests plus docs/render/package/release
  consistency gates before any phase becomes `COMPLETE`.

## Status log

- 2026-08-31: Program started on `phase-23h-security-inspection-suite` from
  verified commit `07e14e8`; contracts and leaf gates written before code.
- 2026-08-31: Adversarial completion replaced path-bound fake-ClamAV dependence with exact in-memory protocol capture plus a capability-gated real connector, added process-wide generation routing for multi-window security results, preserved handshake/stream cancellation, cleaned failed sanitizer stages, repaired WebP VP8X metadata flags, and removed a GTK markup warning. Full verification evidence is recorded in the active ledgers.

---

# Plan: Floe Phase 22A — Selection Mode

## Contract

Implement a native Floe-owned local chooser mode independently of any portal:
Open File (single), Open Files (multiple), Select Folder (single), and Save File.
Dedicated chooser invocations use a non-unique application process, reuse the
existing bounded browser and exact `PathBuf` selection, present a compact
mode-specific action surface, and emit percent-encoded local file URIs only
after worker-side revalidation. Cancellation emits no path. Save validates one
UTF-8 filename component, never silently accepts an occupied destination, and
requires an explicit replace confirmation. Normal Floe startup, browsing,
sessions, operations, and GApplication routing remain unchanged. XDG portal
service names, request handles, grants, and responses are Phase 22B and excluded.

## Depth tree

1. Stable chooser domain and invocation contract
   - strict mutually-exclusive raw-argument parser and local initial directory;
   - mode titles, accept labels, cardinality/type/name policy;
   - exact URI output and deterministic cancel/config-error exit behavior.
2. Responsive validation and native presentation
   - fixed-capacity worker revalidates exact local paths off GTK;
   - compact visible chooser footer, accessible status/name/accept/cancel controls;
   - single/multiple selection policy, folder-current-location fallback, save
     conflict confirmation, Enter/Escape behavior.
3. Application isolation and regressions
   - dedicated `NON_UNIQUE` chooser application leaves ordinary app unchanged;
   - exact result handoff quits only chooser process and never persists chooser
     session/history traces;
   - parser, raw-path, race, symlink, conflict, cancellation, UI contract tests.
4. Verification and handoff
   - focused and full Rust, strict docs/render/release, E2E, native Wayland gates;
   - mark 22A complete only with evidence and set exactly Phase 22B NEXT.

## Status log

- 2026-08-30: Started on `phase-selection-mode` from verified Phase 6W commit
  `7f5663b`. Gates are in `gates/phase-selection-mode.md`.
- 2026-08-30: COMPLETE. All six gates pass, including final isolated native
  Wayland accept/cancel, no-config/no-state cleanup, normal Floe Ping/Quit, full
  workspace tests, strict Clippy, docs/render/release, and E2E contracts.

---

# Plan: Floe Phase 6W — Undo Trash

## Contract

Implement durable Undo/Redo only for Floe-owned moves into a standards-correct
local freedesktop Trash whose exact payload and `.trashinfo` metadata Floe can
identify and revalidate. The Trash executor, not GTK, captures a typed receipt
after successful GIO Trash. Undo restores without overwrite to the recorded
original path and removes metadata only after payload commit; Redo moves the
same revalidated item back to Trash and refreshes the receipt. Records are
bounded, expiring, restart-safe, and become review-required after uncertain or
changed state.

Unsupported Trash backends, orphan/malformed entries, permanent deletion,
administrator Trash, remote resources, Restore Elsewhere, and general Trash
history remain explicitly outside this phase. Failure to capture a complete
receipt must not relabel an otherwise successful Trash operation as safely
undoable.

## Depth tree

1. Standards-correct receipt and durable model
   - exact original, payload, metadata paths and no-follow identities;
   - bounded post-GIO discovery limited to reviewed local Trash roots;
   - versioned raw-path codec, expiry, restart review, and capacity behavior.
2. Execution and lifecycle
   - ordinary Trash completion records only a fully revalidated receipt;
   - Undo uses existing no-overwrite Restore semantics;
   - Redo revalidates restored identity, obtains a fresh complete receipt, and
     never overwrites or silently loses metadata.
3. Application and presentation integration
   - Operation History and Recovery Center expose truthful Trash Undo/Redo;
   - refresh/reveal uses exact typed outcomes and ordinary conflict handling;
   - unsupported backends remain successful Trash but explicitly non-undoable.
4. Verification and handoff
   - tempfile hostile/race/restart/raw-name tests plus focused controller/UI tests;
   - full workspace, docs, release, E2E, and applicable native Wayland gates;
   - mark 6W complete only with evidence and select exactly one later phase.

## Status log

- 2026-08-30: Started on `phase-6w-undo-trash` from verified reliability commit
  `1358652`. Gates are in `gates/phase-6w.md`.

---

# Plan: Floe Reliability Hardening Before Undo Trash and File Chooser

## Contract

Repair the four confirmed adversarial-review defects before adding new Undo or
chooser behavior. Preserve existing job/core/UI boundaries: core errors expose
truthful classification, executors map them to structured job failures, the UI
uses operation-specific terminal wording, Undo capacity transitions remain
durable, and advanced-metadata worker results remain bounded without blocking
shutdown. Include nested copy/move cancellation discovered while repairing the
Replace classifier. Do not implement Phase 6W or chooser/portal behavior in
this branch.

## Depth tree

1. Failure truthfulness
   - classify Replace cancellation, partial, conflict, permission, unsupported,
     and ordinary I/O through nested Copy/Move errors;
   - map partial terminal titles by actual tracked operation;
   - add lowest-layer regression coverage for every prior conflation.
2. Durable and responsive lifecycle
   - persist `NeedsReview` transitions before returning Undo capacity failure;
   - replace blocking metadata-index result sends with a bounded coalescing
     queue that preserves terminal results and cannot deadlock Drop;
   - test restart durability, queue pressure, terminal delivery, and shutdown.
3. Integration and handoff
   - run focused and full Rust/native documentation gates;
   - update persistent status without claiming Phase 6W or chooser work;
   - retain exactly Phase 6W as the next roadmap phase.

## Status log

- 2026-08-30: Started on `phase-reliability-hardening` after locally
  fast-forwarding verified USB repair commit `2ecf048` to main. Gates are in
  `gates/phase-reliability-hardening.md`.
- 2026-08-30: Reliability hardening is complete. Focused regressions, workspace
  format/check/strict Clippy/tests, docs/render/release/diff, and E2E contract
  gates pass. Semantic native E2E remains truthfully skipped for the recorded
  missing external dependencies. Phase 6W remains the sole NEXT phase.

---

# Plan: Floe USB Device Discovery Bug Fix and Logical Edge Review

## Contract

Fix the confirmed removable-volume presentation failure first. A GIO volume
whose reported name is empty or only whitespace must still produce a visible,
meaningful, accessible Devices row without changing its authoritative
`DeviceId`, mount object, exact local root, or action routing. Prefer a real
nonempty volume name, then a nonempty filesystem-label identifier, then the
associated drive name plus a bounded partition/device hint, and finally a calm
generic storage label. Display-only fallbacks must not become path identity.

After the fix is implemented and verified, perform a read-only adversarial
logical review of Floe's highest-risk current subsystems. Confirm findings
through callers, tests, runtime evidence, and a skeptic pass; report only
defensible bugs. Do not implement additional findings in this task.

## Depth tree

1. Device presentation repair
   - extract a deterministic bounded display-name policy;
   - preserve exact opaque device identity and GIO action object;
   - keep empty-name volumes visible and distinguish sibling partitions;
   - retain drive/volume/mount deduplication and live signal refresh.
2. Regression and native verification
   - cover empty, whitespace, control-heavy, label, drive, device-hint, and
     ultimate fallback cases;
   - cover duplicate sibling names and non-UTF-8/path-display separation;
   - run focused, workspace, strict Clippy, docs, diff, and native GIO smoke.
3. Adversarial logical review
   - inspect architecture, state/executor boundaries, cancellation/recovery,
     device disappearance, virtualized selection, parsers/providers, and
     persistence/migration invariants;
   - attempt to disprove every candidate before reporting it;
   - leave non-primary findings read-only with exact reproduction and tests.

## Status log

- 2026-08-30: Started on `fix-usb-device-discovery` from clean main commit
  `497c3db`. Live evidence confirms `/dev/sdc` and its mountable/ejectable GIO
  volume exist while GIO supplies an empty volume name; Floe hides the parent
  drive and renders that empty name directly. Gates are in
  `gates/fix-usb-device-discovery.md`.
- 2026-08-30: Repair verified. Empty GIO volume names now resolve through
  bounded display-only label/drive/partition/generic fallbacks, with opaque
  identity and live action objects unchanged. Focused/live device tests, full
  workspace gates, strict docs/release contracts, and isolated Wayland
  Ping/Quit pass. The read-only edge hunt confirmed four unrelated findings
  and retained one duplicate-UUID identity lead as needs-verification; none was
  implemented. Phase 6W remains the sole roadmap `NEXT` phase.

---

# Plan: Floe Phase 6V — Selection and Operation Reveal Polish

## Contract

Implement only Phase 6V. Selection in List, Grid, Search, Trash, and Miller
views must be unmistakable with redundant non-color treatment while preserving
GTK's authoritative multi-selection, pointer, keyboard, context-menu, focus,
and accessibility semantics.

After a successful local Copy, Move, Rename, Create, Duplicate, or Replace,
Floe must derive the exact resulting `PathBuf` from the typed operation result,
refresh the owning visible directory when appropriate, select and scroll that
exact path into view, and apply a brief non-color emphasis without stealing
focus. Reveal requests must be generation- and directory-bound, bounded,
single-use, and safe when the result is hidden, filtered, sorted, grouped,
collapsed, off-screen, in an inactive tab/pane, or no longer exists. Floe must
never reconstruct a path from display text.

Undo Trash, multi-window support, sidebar/location changes, sandboxed preview
providers, notification policy, or unrelated view redesign is outside this
phase.

## Depth tree

1. Exact reveal policy and lifecycle
   - typed exact result paths for every in-scope successful operation;
   - bounded directory/generation-bound pending reveal intent;
   - stale, changed, hidden, filtered, inactive, or missing results fail safely;
   - existing multi-selection is preserved except for the deliberate result
     selection in the owning active view.
2. Shared selection presentation
   - List, Grid/Search/Trash, and Miller rows or tiles expose clear selected and
     keyboard-focus treatment beyond color alone;
   - context selection and native Ctrl/Shift/rubber-band semantics remain one
     authoritative `GtkMultiSelection`;
   - accessible selected/focused state and exact identity remain unchanged.
3. Refresh, scroll, and transient emphasis
   - successful operations enqueue refresh/reveal from application results, not
     GTK filesystem callbacks;
   - post-refresh exact-path resolution selects and scrolls without moving
     keyboard focus;
   - transient emphasis is bounded, cancellable, recycled-widget safe, and
     removed automatically or on generation change.
4. Verification and status
   - deterministic policy/state tests and operation-result regressions;
   - focused real-GTK list/grid/Miller accessibility and reveal contracts;
   - full workspace, docs, package/release, E2E, and isolated Wayland gates;
   - mark complete only with evidence, then select exactly one later phase.

## Applicable testing layers

1. Rust unit tests for reveal admission, exact-path matching, multi-selection,
   generation/directory mismatch, hidden/filtered/missing outcomes, and expiry.
2. Tempfile operation integration tests proving Copy/Move/Rename/Create/
   Duplicate/Replace success yields the exact committed result path and failures
   or partial outcomes do not reveal an unproven result.
3. Focused real-GTK component/accessibility tests for selected styling,
   scroll-to-result behavior, focus preservation, and recycled row/tile cleanup.
4. Native Wayland action/lifecycle smoke plus the ordinary workspace, docs,
   package, release-source, and E2E contract gates.

## Status log

- 2026-08-30: Phase 6V started on the current dirty
  `phase-18y2-complete-undo-recovery` worktree after verified Phase 6U. Gates
  are in `gates/phase-6v.md`; no other requested feature is included.
- 2026-08-30: Phase 6V implementation and all deterministic, real-GTK,
  workspace, documentation, packaging, release-source, E2E-contract, frozen
  release-build, diff-hygiene, and isolated Wayland lifecycle gates pass.
  Phase 6V is complete; Phase 6W Undo Trash is the sole recommended next phase
  and remains unimplemented.

---

# Plan: Floe Phase 6U — Replace Conflict Safety

## Contract

Implement only explicit local Replace and batch-scoped Replace All for Copy,
Move, and Rename conflicts. Replacement must never become a permissive
overwrite flag. Floe must fingerprint both exact no-follow endpoints, prepare a
private bounded backup before mutation, revalidate immediately before commit,
publish without overwriting an unreviewed third-party occupant, and retain
enough durable state for rollback, restart review, and operation-specific
Undo/Redo. Cancellation is accepted only at defined reversible boundaries;
partial or uncertain outcomes remain visible and are never cleaned
automatically.

No administrator, Trash restore, remote, archive, batch rename, multi-window,
sidebar/location, provider-sandbox, ownership, ACL, xattr, snapshot, or general
transaction work belongs to this phase.

## Depth tree

1. Replace engine and private state
   - exact raw-path source/destination identities and reviewed operation kind;
   - owner-only bounded backup root and versioned no-follow atomic manifest;
   - prepare, revalidate, commit, rollback, restart-review state machine;
   - no shell, symlink following, silent overwrite, or unbounded backup.
2. Application execution and durable inverse
   - capacity-bounded worker outside GTK for Copy/Move/Rename replacement;
   - structured progress, cancellation boundaries, partial outcome evidence;
   - durable Undo/Redo swaps reviewed replacement and backup identities safely;
   - cleanup only after expiry or explicit resolution with owned identity proof.
3. Batch conflict policy and UI
   - Replace affects one conflict; Replace All is scoped to one stable batch;
   - source/existing metadata comparison is descriptive, never equality proof;
   - explicit destructive confirmation, accessible buttons, honest limits;
   - Keep Existing, Keep Both, Skip All, and Retry With Name remain intact.
4. Verification and status
   - tempfile hostile-race, rollback, cancellation, batch-scope, restart tests;
   - focused GTK-independent UI contracts and real-GTK accessibility where available;
   - full workspace, docs, package, release-source, E2E, native Wayland gates;
   - mark complete only after evidence and select exactly one later phase.

## Status log

- 2026-08-30: Implemented exact-identity local Replace, atomic retained-version
  Undo/Redo, identity-owned backup lifecycle, accessible second-confirmed
  conflict comparison, and stable-batch Replace All with fresh per-conflict
  capture and Protected Folder pause. Focused engine, batch, recovery, and UI
  contracts pass. Phase 6V selection and operation reveal polish is recorded as
  the sole later recommended phase and remains unimplemented.

- 2026-08-30: Phase 6U started on the current dirty Phase 18Y2 worktree because
  the completed Phase 18Y2 changes were not committed by user request. Gates
  are in `gates/phase-6u.md`; no later requested phase is included.

---

# Plan: Floe Phase 18Y2 — Complete Undo and Recovery

## Contract

Complete only the authoritative Phase 18Y2 data-safety leaf. Successful local
copy, move, rename, and create operations become durable, expiring,
operation-specific Undo/Redo records. Administrator operations participate only
where Floe can persist and revalidate a complete inverse without broadening
authority. Every inverse is no-overwrite, exact-path, no-follow, identity
revalidated, asynchronous, and conservatively reviewable after interruption.

Trash restore is used only when Floe owns exact standards-correct restore
metadata. Permanent delete, unsupported Trash backends, recursive administrator
copy, ownership, ACL/xattr changes, and any operation lacking a proven inverse
remain explicitly irreversible or unsupported. No snapshot, rollback,
transaction, secure-erasure, or automatic-cleanup claim is added.

## Depth tree

1. Durable recovery/history model
   - a versioned private store that coexists with and preserves the Phase 18Y interruption journal;
   - bounded exact raw-path recipes, identities, timestamps, expiry, action state;
   - private no-follow atomic storage with fail-closed corruption handling.
2. Local operation execution
   - pre-mutation journal for copy/move/rename/create;
   - successful completion becomes an Applied history record;
   - capacity-bounded Undo/Redo worker with no-overwrite inverse actions;
   - cancellation/failure/interruption becomes explicit review, never deletion.
3. Administrator boundary
   - request-scoped history only for inverses proven from typed GIO identities;
   - fresh desktop authorization for every administrator Undo/Redo;
   - unsupported irreversible cases remain labelled and unavailable.
4. Native interaction and verification
   - Operation History exposes persistent Undo/Redo and expiry plainly;
   - Recovery Center distinguishes interrupted, uncertain, applied, and undone;
   - deterministic, filesystem, GTK, E2E-preflight, native Wayland, docs, and
     release gates run before COMPLETE.

## Applicable testing layers

1. Rust codec/state-machine tests including the v1 codec, non-UTF-8 paths,
   expiry, capacity, corruption, insecure permissions, and interruption states.
2. Tempfile filesystem integration tests for copy/move/rename/create Undo/Redo,
   identity replacement, destination conflicts, changed directories,
   cancellation, partial outcomes, and unsupported destructive operations.
3. Focused real-GTK accessibility tests for persistent history/recovery actions.
4. Isolated native Wayland Ping/action/Quit plus full workspace, docs, package,
   migration, release-source, and E2E contract gates.

## Status log

- 2026-08-30: Started on `phase-18y2-complete-undo-recovery`; gates are in
  `gates/phase-18y2.md`. No later conflict, multi-window, sandbox, or sidebar
  phase is implemented in this leaf.
- 2026-08-30: Local Copy/Move/Rename/Create durable Undo/Redo, conservative
  interruption review, Operation History/Recovery Center UI, and explicit
  administrator exclusions implemented and verified. Phase 6U Replace conflict
  safety is the sole recommended next phase.

---

# Plan: Floe Phase 20B2 — Visual, Accessibility, and QoL Completeness Audit

## Contract

Close the ten remaining daily-driver gaps selected by the user without entering
Phase 21: date/size grouping with collapsible groups, split-ratio persistence,
single/double-click activation policy, column reorder/autosize, invert
selection, complete appearance controls, predictable focus/Escape behavior,
screen-reader/high-contrast/non-color semantics, HiDPI/fractional-scale-safe
presentation, and detailed errors/completion feedback with localization/RTL
readiness. Preserve exact `PathBuf` identity, bounded versioned preferences,
application-owned state, and responsive GTK. This phase does not add Niri,
Plasma, MTP, remote browsing, packaging, performance rewrites, or translation
catalogs presented as completed translations.

## Applicable testing layers

1. Deterministic core tests for grouping, ordering, invert-selection, and
   direction-independent/raw-path-safe policies.
2. Deterministic app tests for preference migration, split/click/column/group
   persistence, appearance validation, focus/Escape routing, notification and
   localized message models.
3. Focused real-GTK component/accessibility tests for the new settings and
   controls where a display is available.
4. Full format/check/strict Clippy/workspace tests, native build, E2E preflight,
   and isolated native Wayland lifecycle smoke.

## Implementation tree

1. Browsing organization
   - Implement date and size buckets using already-loaded metadata.
   - Make group headers keyboard-accessible and collapsible with bounded
     persisted state.
   - Add deterministic invert selection.
2. Layout and activation
   - Persist the per-tab split divider ratio after real user resizing.
   - Add configurable single-click versus double-click activation.
   - Complete column reordering and content-aware autosizing with bounds.
3. Appearance and scaling
   - Add System/Light/Dark color scheme, font family/scale, reduced-motion, and
     appearance reset controls using centralized tokens.
   - Audit CSS/icons/thumbnails/focus for GTK scale-factor and fractional-scale
     safe behavior without fake blur.
4. Accessibility and daily-driver feedback
   - Define one Escape/focus ownership hierarchy across transient surfaces.
   - Complete stable screen-reader names/descriptions/states and high-contrast,
     non-color cues.
   - Add reusable detailed-error disclosure and bounded completion
     notifications; introduce translation-ready message/direction boundaries
     and verify RTL-safe ordering/path presentation.
5. Integration and verification
   - Run focused and full gates, fix regressions, update README/User Guide,
     roadmap, feature matrix, security documentation if applicable, and
     `AGENTS.md` status. Mark 20B2 complete only with evidence and leave exactly
     Phase 21A recommended next.

## Status

**COMPLETE** on the current worktree. Q1–Q11 pass: all ten focused filters,
strict workspace gates, focused real-GTK Phase 20B2 contracts, E2E harness
preflight, and isolated native Wayland Ping/Quit. The post-completion Q1
regression replaces per-tile Grid View group labels with full-width sections
backed by bounded selection slices over the one authoritative model.
Dogtail/pyatspi remains unavailable and no translation catalogs or physical
multi-monitor fractional-scale matrix are claimed. Phase 21A is the sole
recommended next phase. Phase 20B2A and 20B2 are ready for publication and
merge.

---

# Plan: Floe Phase 21D — Release Candidate

## Contract

Turn the verified Phase 21A–21C tree into a reproducible release candidate. The
phase inventories every shipped Rust dependency and declared license, resolves
or explicitly blocks on current advisories, exercises interrupted-operation
recovery in disposable roots, and publishes a truthful generic/Niri/Plasma
Wayland environment matrix from native evidence. Release artifacts must be
reproducible from a clean source tree and accompanied by checksums and a
machine-readable manifest. A security-critical or data-loss finding blocks
completion.

This phase does not add privileged mutations, broaden Undo, implement a sandbox,
or change filesystem copy semantics. Those remain separate follow-on branches.

## Depth tree

1. Dependency and supply-chain release gate
   1. Resolve current supported dependency advisories and freeze an auditable
      dependency/license inventory.
   2. Add deterministic policy checks that reject unknown/missing licenses,
      forbidden sources, and unresolved recorded advisories.
2. Recovery and environment evidence
   1. Re-run corrupt/insecure/interrupted-operation recovery against isolated
      roots and record the supported recovery contract.
   2. Exercise generic Wayland and available compositor-native launch,
      Ping/Quit, and clean-exit contracts without inventing unavailable claims.
3. Reproducible candidate
   1. Build the release archive twice and verify byte identity, contents,
      release binary, manifest, and SHA-256 checksum.
   2. Run deterministic, GTK/E2E where available, documentation, status, and
      diff-hygiene gates.

## Status log

- 2026-08-29: Phase 21D started on `phase-21d-release-candidate`; gates are in
  `gates/phase-21d.md`. Dependabot reports three medium `tar` advisories in the
  pinned 0.4.44 release; resolving them is a release gate.
- 2026-08-29 pass 1: updated `tar` to 0.4.46; the 204-package offline
  source/license policy and all three recorded patched advisory floors pass.
- 2026-08-29 pass 2: seven recovery tests, twice-built 245-file archive, package,
  docs, E2E contract, and full Rust gates pass. Isolated KDE Wayland release
  Ping/Quit exited 0; Niri and Dogtail/pyatspi remain explicitly unverified.
  Phase 21D is complete and exactly Phase 14C is `NEXT`.

---

# Plan: Floe Phase 14C — Safe Administrator Operations

## Contract

Add an explicit, separately typed GIO/GVfs administrator mutation boundary to
the existing experimental administrator view. The GTK process remains UID
stable and never receives passwords. Every request is built from private
`PrivilegedResourceId` values, requires a fresh visible confirmation and
request-scoped desktop mount operation, defaults to no overwrite/no-follow,
reports structured progress/cancellation/partial failure, and refreshes only the
active administrator provider after terminal completion.

The first bounded surface covers new folder, rename, copy/move to an explicitly
typed absolute local destination converted privately into administrator
identity, Trash with no delete fallback, permanent delete with irreversible
confirmation, and Unix mode changes. Recursive copy, ownership, ACL/xattr,
external tools, previews, archives, ordinary local-job reuse, arbitrary URIs,
and background persistence of administrator paths are excluded.

## Depth tree

1. Typed operation policy
   1. Define immutable requests, operation IDs, destination/name/mode bounds,
      same-authority checks, no-follow fingerprints, and retry policy.
   2. Add a capacity-one GIO service with cancellation-requested versus terminal
      cancellation, progress, no-overwrite flags, and redacted failures.
2. Administrator-view integration
   1. Add accessible operation controls and explicit confirmations using current
      exact selection/current resource identities only.
   2. Keep navigation/Return disabled only while a mutation is active, preserve
      visible Administrator state, and refresh after terminal results.
3. Verification
   1. Fake/policy tests cover raw paths, invalid names/modes, mixed authority,
      conflicts, links, partial failure, cancellation, and no local-job reuse.
   2. Run focused real-GTK/native administrator gates and all release gates.

## Status log

- 2026-08-29: Phase 14C started on `phase-14c-privileged-operations` from
  verified Phase 21D commit `dc70468`; gates are in `gates/phase-14c.md`.
- 2026-08-29: Phase 14C implementation and verification complete. Typed
  administrator New Folder, Rename, single-file Copy/Move, Trash, permanent
  delete, and Unix-mode changes pass focused policy/service/UI tests, strict
  workspace gates, documentation/package/release gates, a real-GTK
  accessibility gate, and isolated KDE Wayland Ping/Quit. The process remained
  UID 1000; a disposable root-owned mutation fixture was unavailable and is
  not claimed. Phase 18Y2 is the sole roadmap `NEXT` phase.

## Focused regression: fresh-session GVfs administrator mount

Reproduced on KDE Wayland with the GVfs daemon and Plasma polkit agent active:
`gio info admin:///boot` returns `G_IO_ERROR_NOT_MOUNTED`. Floe currently
enumerates the administrator `GFile` directly and maps that result to an
unavailable service, so authorization is never requested.

The fix remains inside the application-owned privileged provider. On the first
`NotMounted` result it starts one cancellable, time-bounded
`g_file_mount_enclosing_volume` request with a window-parented
`GtkMountOperation`; success or `AlreadyMounted` retries enumeration once.
Denial, cancellation, timeout, missing-agent/backend errors, stale generations,
and a second `NotMounted` fail explicitly. The ordinary process remains
UID-stable and the administrator view remains read-only with no capability
inheritance into jobs, previews, launchers, archives, terminals, or custom
actions.

Gates are recorded in `gates/fix-admin-gvfs-mount.md`. This regression fix does
not begin Phase 21D or any future privileged mutation work.

Status: implementation, focused Phase 14B tests, full workspace gates, strict
documentation/release-source checks, and real-GTK accessibility contract pass.
The native KDE/GVfs mount request reached desktop authorization, but successful
administrator enumeration remains a truthful manual gate because this run did
not request or receive the user's password.

## Focused documentation: features and their reasons

Floe now has an explicit user-facing philosophy rather than leaving product
rationale only in architecture and security documents. `docs/PHILOSOPHY.md`
defines the voice and decision principles, maps major behaviors to their reasons
and tradeoffs, and requires surprising or safety/privacy-sensitive features to
explain what they do, why, the tradeoff, and what they do not claim.

The philosophy is linked from README, Getting Started, User Guide, and
Administration. The administrator view itself explains why elevated access is
isolated. The document is included in strict link/table/claim validation,
rendering, installed manuals, package-layout checks, and deterministic release
sources. Gates are in `gates/feature-rationale-docs.md`; this does not change the
sole roadmap `NEXT`, Phase 21D.

# Plan: Floe Phase 20B2A — Window Size Persistence

## Contract

Remember the last normal top-level Floe window size and restore it before the
next window is presented. Persist one validated width/height pair through the
existing private preference worker. Observe actual GDK surface configurations
with a main-loop debounce; do not poll, perform filesystem work in GTK, or let
maximized/fullscreen allocations overwrite the normal size.

Wayland does not provide portable application-controlled window placement, so
this leaf does not store position. It also does not restore maximized,
fullscreen, minimized, monitor, workspace, or compositor-specific state and
does not begin the rest of the Phase 20B2 audit.

## Applicable testing layers

1. Deterministic Rust preference tests for version migration, tuple parsing,
   corruption rejection, clamping, and round trip.
2. Deterministic controller-policy tests for normal versus maximized/fullscreen
   allocations and unchanged-size suppression.
3. One focused real-GTK component contract proving the restored default size.
4. Full formatting, check, strict Clippy, workspace tests, native build, E2E
   preflight, and applicable native Wayland lifecycle smoke.

## Implementation order

1. Add a bounded `WindowSize` preference value and migrate v16 to v17.
2. Apply the restored size while constructing the application window.
3. Debounce GDK surface width/height changes, update only normal geometry, and
   synchronously capture normal geometry before final preference submission.
4. Add regressions and run all applicable gates.
5. Update the user guide, README/status/matrix/roadmap and recommend exactly one
   next phase without implementing it.

## Status

**COMPLETE** on `phase-20b2a-window-size-persistence`. All W1–W5 gates pass:
version-17 migration/policy/tracking tests, strict workspace quality gates,
focused real-GTK restoration, E2E preflight, and isolated native Wayland
launch/Ping/Quit with a persisted window-size record. Phase 20B2 remains the
sole recommended next phase.

---

# Plan: Floe Phase 20B1A — Advanced Metadata Sort Index

## Contract

Replace the Phase 20B1 placeholder rows with truthful local-directory sorting
backed by a bounded, cancellable, application-owned metadata index. Reuse the
reviewed Phase 10F image/EXIF/audio providers and no-follow source identity;
add bounded text word/line facts and cheap Unix/link facts. Preserve exact raw
paths and keep all filesystem/content work off GTK.

Indexing is explicit when an advanced criterion is chosen. The cache is a
private, versioned, size-bounded derived-data cache under Floe's XDG cache root,
valid only for an exact dev/inode/size/mtime/ctime fingerprint. File-watcher
changes invalidate exact paths/subtrees. Users can disable persistent reuse and
clear the cache. Unsupported providers yield unknown-last values and truthful
feedback; Floe never invents video/document facts or calls an unavailable
criterion complete.

## Implementation order

1. Extend GTK-independent sort identity and ordering for document, image,
   audio/video, path/link, permission, owner, and group criteria.
2. Add a bounded extractor/index/cache worker with cancellation, progress,
   no-follow validation, global/per-file limits, private atomic persistence,
   corruption/insecure-storage rejection, and watcher invalidation.
3. Route advanced sorts through the index worker, expose real menu actions,
   cancellation and Settings cache controls, and preserve existing selection,
   tabs/split/session, fallback, and stale-generation behavior.
4. Add deterministic core/filesystem/codec/application/GTK regressions and run
   the complete phase gates.
5. Update the roadmap, matrix, privacy/security, architecture, design, README,
   User Guide, and AGENTS status; mark complete only after verification and
   leave exactly one recommended next phase.

## Status

**COMPLETE** on `phase-20b1a-metadata-index`. All M1–M5 gates pass, including
focused extraction/cache/UI contracts, full workspace quality gates, focused
real-GTK accessibility checks, E2E harness preflight, and isolated native
Wayland launch/Ping/Quit. Phase 20B2 is the sole recommended next phase.

---

# Plan: Floe Phase 20B1 — Sort By Completeness

## Contract

Add a compact, keyboard-accessible **Sort By** popover matching the requested
Dolphin-style information hierarchy. Name, Size, Modified, Created, Accessed,
Type, Rating, Tags, and Comment must be real sort policies with deterministic
exact-path tie-breakers and unknown values last in both directions. Direction,
Folders First, and Hidden Files Last are independent, visible stateful choices.

Created/accessed timestamps come from metadata already obtained by the bounded
directory worker. Rating/tags/comment are loaded only after the user explicitly
selects one of those policies, from KDE-compatible no-follow xattrs
`user.baloo.rating`, `user.xdg.tags`, and `user.xdg.comment`; unsupported,
missing, malformed, or oversized values remain unknown. No GTK callback reads
the filesystem, no path is reconstructed from UI text, and no tag/comment data
is logged or persisted by Floe.

Document, Image, Audio, Video, and Other category submenus are present for
discoverability but must clearly expose their currently unavailable indexed
metadata fields as disabled. This leaf does not silently scan file contents,
add a metadata index, create or edit ratings/tags/comments, or claim the later
advanced metadata sort phase complete.

## Implementation order

1. Extend the core entry/sort policy with created/accessed, bounded lazy user
   metadata, hidden-last ordering, cancellation, deterministic missing-value
   behavior, and property/regression tests.
2. Add stateful sort-column, direction, folders-first, and hidden-last actions;
   preserve existing clickable list headings and worker supersession.
3. Add one accessible native Sort By menu beside view controls and reuse its
   model in Browser View options without creating a menu wall.
4. Persist the complete policy through versioned global/per-folder preferences
   and backward-compatible session migration.
5. Run focused/full Rust, real-GTK, E2E, and native Wayland gates; update README,
   User Guide, matrix, roadmap, architecture/security/status ledgers, and stop.

## Status

**COMPLETE.** This bounded Phase 20B quality-of-life leaf owns branch
`phase-20b1-sort-by-menu`. Every gate below passes; advanced indexed metadata
fields remain explicitly disabled and Phase 20B2 is the sole recommended next
leaf.

---

# Plan: Floe Phase 14B — Privileged Local Browsing

## Contract

Add an experimental, read-only **Open as Administrator…** view for one exact
absolute local directory using a typed application-owned resource/provider
boundary and GIO/GVfs `admin://` access. GVfs and the desktop polkit agent own
authentication; Floe remains the caller's unprivileged process and never sees a
password. Administrator identity must never enter the existing local `PathBuf`
browser worker, mutation jobs, previews, thumbnails, terminals, launchers,
custom actions, archives, plugins, clipboard, or ordinary navigation history.

This phase implements bounded read-only navigation, explicit authority state,
cancellation/timeout/stale-response handling, graceful unsupported/denied
fallback, and an opt-in preference. It does not implement privileged mutation,
remote browsing, encrypted-volume unlocking, arbitrary administrator URIs, a
helper process, `sudo`, `pkexec`, or shell execution.

## Implementation order

1. Freeze private validated local/administrator resource identities, provider
   routing, access state, error classification, generation, history, and
   bounded-page contracts.
2. Implement a GIO-main-context privileged service that constructs `admin` URIs
   from canonical GFile URIs, enumerates with `NOFOLLOW_SYMLINKS`, pages results,
   supports cancellation/timeouts, and returns typed events only.
3. Add an experimental setting and explicit current-folder action that opens a
   persistent Administrator-labelled read-only view with Back/Forward/Parent,
   Cancel, Retry, Return to Standard Access, and no local operation affordances.
4. Add hostile URI/raw-path, fake-service state-machine, stale generation,
   cancellation/timeout, page-bound, routing, and real-GTK accessibility tests.
5. Run full workspace/native gates, update README/User Guide/security/roadmap/
   matrix/status ledgers, set exactly one next phase, publish/merge, and stop.

## Status

**COMPLETE.** Phase 14B passes exact-path/URI identity, bounded provider/state,
failure rollback, read-only routing, real-GTK accessibility, native Wayland
UID/liveness, full workspace, E2E harness, documentation, and diff-hygiene
gates. Exactly Phase 20B — Visual, Accessibility, and QoL Audit is `NEXT`; stop
before implementing it on this branch.

---

# Plan: Floe Phase 19B — Associations and Custom Actions

## Contract

Complete local XDG MIME association management and application-owned external
tools without a shell boundary. Reuse GIO `AppInfo` for discovery, launch,
set-default, and reset operations. Store only bounded validated action
definitions; expand reviewed placeholders directly into `OsString` argv and
spawn the selected executable off GTK. Preserve exact selected `PathBuf`
identity, explicit user intent, truthful failure feedback, and existing
selection/context/command-palette ownership.

This phase does not implement plugins, scripts downloaded from the network,
environment-variable expansion, shell syntax, privileged access, vault keys,
remote files, or administrator access.

## Implementation order

1. Freeze bounded association request/result and custom-action definition,
   eligibility, persistence, placeholder, argv, and launch contracts.
2. Move GIO association mutation and external process launch to bounded
   application workers; validate every target and executable without lossy path
   reconstruction or shell interpolation.
3. Add one Applications & Tools editor, complete association inspect/set/reset
   flow, and selection-aware context-menu/command-palette actions.
4. Add hostile raw-path, malformed-store, missing-app, reset/default, argv,
   capacity, accessibility, and failure regression tests.
5. Run full workspace/native gates, update README/User Guide/security/status
   ledgers, mark exactly one next phase, publish/merge, and stop before
   administrator access.

## Status

**COMPLETE.** Phase 19B passes bounded association/custom-action, persistence,
raw-path/argv, real-GTK accessibility, native Wayland/D-Bus lifecycle, full
workspace, E2E harness, documentation, and diff-hygiene gates. The native smoke
also found and fixed an unescaped Adwaita settings-row ampersand, now covered by
a regression test. Exactly Phase 14B — Privileged Local Browsing is `NEXT`;
stop before implementing it on this branch.

---

# Plan: Floe Phase 7G — Navigation Upgrades

## Contract

Upgrade local navigation with keyboard-accessible exact-path breadcrumbs,
bounded asynchronous location completion, a bounded recent-location surface
over application-owned history, restored back/forward history, and validated
GApplication command-line file/folder routing. Preserve `NavigationState` and
`BrowserSession` as authoritative; never reconstruct a path from lossy labels.

Directory probing and completion enumeration stay off GTK. History persistence
continues to obey existing Private/Sensitive session policy. This phase adds no
remote browsing, file associations, external tools, or administrator access.

## Implementation order

1. Freeze breadcrumb, completion, recent-location, and CLI routing models with
   raw-path, capacity, privacy, and file-versus-folder contracts.
2. Extend core navigation/session accessors and existing private workspace
   restoration without adding another history store.
3. Add native breadcrumb/recent controls and a bounded superseding completion
   worker around the existing location editor.
4. Route GApplication file arguments to exact folder navigation or parent plus
   exact reveal after the browser controller exists.
5. Add deterministic/raw-path, worker, CLI, GTK accessibility, full workspace,
   README/user/status, and exactly-one-next-phase gates.

## Status

**COMPLETE.** Phase 7G passes deterministic exact-path, bounded worker,
GApplication routing, real-GTK accessibility, full workspace, isolated native
Wayland/D-Bus, E2E harness, documentation, and diff-hygiene gates. The sole
recommended next phase is 19B — Associations and Custom Actions. Stop before
implementing it on this branch.

---

# Archived plan: Floe Phase 20A — Settings Center

## Contract

Create one native, searchable Settings Center that organizes Floe's existing
preferences and specialized editors without duplicating preference state. The
center uses `ViewPreferences` and existing application actions as the
authoritative boundary, applies safe options live, persists through the
existing bounded writer, and exposes meaningful GTK accessibility metadata.

This phase does not implement navigation upgrades, MIME association editing,
custom actions, administrator browsing, or any later roadmap item. Filesystem
work remains outside GTK callbacks and irreversible confirmations are
unchanged.

## Depth tree

```text
Phase 20A Settings Center
├── Information architecture
│   ├── searchable plain-language categories
│   ├── progressive disclosure for dense controls
│   └── links to existing specialized editors
├── Authoritative preferences
│   ├── live appearance, browsing, view and search controls
│   ├── existing queue-based persistence
│   └── bounded backward-compatible preference parsing
├── Native interaction
│   ├── one win.settings action and discoverable menu entry
│   ├── stable labels, descriptions, roles and keyboard access
│   └── focused model and ignored real-GTK contracts
└── Verification and handoff
    ├── focused/full Rust and native build gates
    ├── README, user guide, matrix, roadmap and status updates
    └── exactly one next phase
```

## Implementation order

1. Inventory the preference, controller, action, menu, and specialized-editor
   boundaries.
2. Implement a focused settings model and native settings surface.
3. Wire safe live controls and links through existing authoritative actions.
4. Add deterministic search/action tests and a real-GTK accessibility gate.
5. Run all available gates and update README plus persistent project docs.

## Status

**COMPLETE.** One searchable native Settings Center now reuses authoritative
preferences and specialized actions. Focused/full deterministic tests, strict
Clippy, native build, clean real-GTK accessibility gate, E2E preflight, diff
hygiene, README/user/project documentation, and sole Phase 7G `NEXT` pass.

---

# Archived plan: Daily-Driver Priority Program

## Contract

Implement the five user-approved priority families as coherent native Floe
features while preserving the existing GTK/core boundary and all completed
behavior:

1. privacy-aware interrupted-operation recovery plus safe reversible actions;
2. a dedicated, organized Settings Center;
3. breadcrumb/completion/recent/CLI navigation upgrades;
4. complete file-association management plus safe user-defined external tools;
5. narrowly scoped administrator browsing through reviewed GIO/polkit
   integration without running Floe itself as root.

Exact Linux identities must never be reconstructed from display text. Slow
filesystem, metadata, configuration, executable discovery, journal, and GIO
work stays off GTK. No shell interpolation, silent overwrite, uncertain-data
cleanup, captured password, or whole-process elevation is permitted.

## Depth tree

```text
Daily-driver priorities
├── Data safety
│   ├── recovery journal and restart review
│   ├── operation-specific safe Undo
│   └── explicit conflict decisions
├── Daily interaction
│   ├── Settings Center information architecture
│   └── breadcrumbs, completion, recents, CLI routing
├── Application integration
│   ├── MIME association management
│   └── safe external custom actions
├── Privileged locations
│   ├── admin URI capability and authentication boundary
│   └── honest unsupported/failure fallback
└── Integration verification
    ├── migrations, hostile inputs, path identity, accessibility
    └── full Rust, GTK, E2E, docs, and roadmap gates
```

## Implementation order

1. Inspect existing job, preference, navigation, launcher, action, and GIO
   boundaries; freeze typed contracts and persistence limits.
2. Implement and verify recovery/Undo/conflict work first.
3. Implement the Settings Center over central preference/application actions.
4. Implement navigation upgrades without replacing exact path state.
5. Implement association/custom-action models, persistence, eligibility, and
   native management UI.
6. Implement administrator-location capability behind GIO application-layer
   integration with explicit authority state and safe fallback.
7. Perform adversarial review, native verification, documentation/status
   updates, and set exactly one roadmap `NEXT` only after verified phases move.

## Status

**IN PROGRESS.** Phase 18Y operation recovery and safe Create Undo are
implemented and pass deterministic/full Rust gates. Native GTK execution was
attempted but skipped because this shell has no usable display; E2E contract
preflight passes while Dogtail/AT-SPI are unavailable. README, user guide,
roadmap, matrix, security model, gates, and project status are updated. Phase
20A Settings Center is the sole next active leaf.

---

# Archived plan: Header Options Information Architecture Follow-up

## Contract

Reorganize the accumulated three-dot header options into a small, predictable
task hierarchy without removing commands, changing shortcuts, or altering
filesystem behavior. Keep frequent categories one level deep, retain existing
specialized submenus where they prevent a menu wall, preserve live GAction
eligibility, and give the menu button an explicit accessible name and
description. Context menus remain selection-aware and customizable.

## Implementation steps

1. Extract the header menu model into one focused builder owned by the GTK UI.
2. Group every existing action under Create, Open & Inspect, File Operations,
   View & Layout, or Tools & Safety, followed by a compact utility section.
3. Add deterministic model tests for category order, complete action
   preservation, uniqueness, root size, and bounded hierarchy depth.
4. Run the focused model test, full Rust gates, native build, and applicable
   real-GTK component contract; update user/design/status documentation.

## Status

**COMPLETE.** The Main menu now exposes five task categories and one compact
utility section. File Operations is subdivided rather than hidden behind a new
flat menu wall. The exact old/new `win.*` action diff is empty; focused model,
full workspace, strict Clippy, native build, real GTK, E2E harness, diff, and
documentation gates pass. Phase 18Y remains the sole roadmap `NEXT`; this
bounded post-Phase-18X correction does not implement operation recovery.

---

# Archived plan: Floe Phase 13G3 — Duplicate Finder Performance

## Contract

Accelerate first and repeated exact-duplicate scans without changing what
“duplicate” means. The final decision remains reviewed streaming SHA-256 plus
byte-for-byte confirmation and mutation revalidation. Preserve bounded local
same-device/no-follow traversal, exact raw paths, cancellation, GTK
responsiveness, hard-link accounting, memory-only results, and explicit Trash.

## Depth tree

```text
13G3 duplicate performance
├── Core staged scan pipeline
│   ├── first/last-chunk quick signature
│   ├── bounded per-device hashing concurrency
│   └── deterministic cancellation and byte confirmation
├── Reusable derived state
│   ├── exact fingerprint-keyed SHA-256 cache
│   ├── private bounded atomic binary persistence
│   └── watcher plus scan-time invalidation
├── Native UX
│   ├── discovery/filter/hash/confirm phase feedback
│   ├── cache-hit and actual hashed-work counters
│   └── truthful exact-versus-similar wording
└── Verification and documentation
    ├── cold/warm/change/corrupt/raw-path tests
    ├── strict workspace and real GTK gates
    └── roadmap/matrix/security-neutral status updates
```

## Status

**COMPLETE.** All six Phase 13G3 gates pass. The adversarial pass corrected
pre/post-hash cache mutation, imbalanced per-device scheduling, explicit watcher
overflow invalidation, and batch eviction at cache capacity. Formatting,
workspace check, strict all-target/all-feature Clippy, complete workspace tests,
native build, focused real GTK contract, diff hygiene, documentation, and the
ledger checker pass. Phase 18Y remains the sole roadmap `NEXT` phase and was not
implemented here.

---

# Archived plan: Floe Phase 13G2 — Duplicate Finder Workflows

## Contract

- Expose full exact-duplicate discovery for one chosen local folder and every
  subfolder.
- Expose “copies of this selected file” inside one chosen folder tree.
- Preserve the existing selected-files/folders recursive workflow.
- Keep exact byte duplicates separate from visually similar media.
- Reuse Phase 13G bounds, same-device/no-follow traversal, reviewed SHA-256,
  byte confirmation, mutation revalidation, cancellation, hard-link accounting,
  memory-only review, and explicit recoverable Trash handoff.
- Preserve exact `PathBuf` identity; lossy UI labels must never reconstruct a
  scan path. Add no index, persistence, remote traversal, automatic deletion,
  perceptual hash, or Phase 18Y work.

## Gates

1. Folder-tree mode finds exact duplicates across nested subfolders.
2. Reference-file mode finds nested copies, hashes only its size class, and
   excludes unrelated duplicate groups.
3. Reference paths already below the chosen root are not counted twice;
   non-UTF-8 identity and non-regular rejection remain exact.
4. Native setup defaults correctly for no selection, one file, one folder, and
   multiple supported selections; unsupported mixed selections are not silently
   truncated.
5. Focused, workspace, strict Clippy, native build, GTK component, and diff
   hygiene gates pass; user, roadmap, matrix, gates, and status docs agree.

## Status

**COMPLETE.** All five gates pass. Formatting, workspace check, strict
all-target/all-feature Clippy, the complete workspace test suite, native build,
focused real GTK component test, and diff hygiene pass. Phase 18Y remains the
sole roadmap `NEXT` phase and was not implemented here.

---

# Archived plan: Floe Phases 18T–18X — Integrity and Data-Loss Safety

## Contract

Implement exactly these five dependency-ordered roadmap leaves:

1. **18T Integrity Tools:** saved SHA-256 fingerprints plus path-safe portable
   `SHA256SUMS` generation and verification, reusing Phase 10E streaming hashes.
2. **18U Integrity Monitoring:** explicit local baselines and bounded/coalesced
   changed, missing, and new-file reporting. This is not intrusion detection.
3. **18V Verified Copy:** an optional Copy and Verify job that compares
   revalidated source and destination bytes through SHA-256 after publication.
4. **18W Verified USB Transfer:** explicit Copy → Verify → Flush → Eject workflow
   with truthful partial states and no safe-removal claim before successful eject.
5. **18X Data-Loss Guardrails:** persisted exact-path Protected Folders and
   thresholded destructive-operation preflight. Protection prevents mistakes;
   it is not encryption or access control.

Preserve exact `PathBuf`/`OsString` identity, no-follow/no-shell/no-silent-
overwrite policy, bounded workers and retained state, GTK responsiveness,
existing operation semantics, and Phase 18A security terminology. Add no Niri,
Plasma-specific, remote, MTP, encryption, vault, sandbox, recovery-journal, or
Security Center work. Do not claim authenticity, malware detection, secure
erase, universal monitoring, or guaranteed safe removal.

## Depth tree and dependency order

```text
18T–18X root integration
├── 18T checksum/fingerprint/manifest engine + native UI
├── 18U baseline/monitor engine + native UI         (depends on 18T)
├── 18V verified-copy operation + progress/UI       (depends on 18T)
├── 18W verified-removable workflow + device UI     (depends on 18V)
└── 18X protected-path/preflight policy + settings/UI
```

Leaves run in order because later phases reuse earlier types and shared app
controllers. Each leaf owns `gates/phase-18*.md`; root integration evidence is
recorded at the top of `GATES.md`. Before each leaf, inspect its existing core,
worker, command, preference, GTK, and test seams and update the documented file
ownership if reality differs. Do not overwrite unrelated dirty icon work.

## Permanent testing layers

- Lowest-layer Rust unit/property tests for manifests, raw names, baselines,
  monitoring diffs/coalescing, verification states, workflow transitions,
  protected-root matching, and threshold decisions.
- Filesystem integration tests only below `tempfile` roots for links, races,
  changed/missing/new files, corrupt manifests, copy corruption, full/partial
  workflow outcomes, and protected targets. Never use real HOME, Trash, mounts,
  or removable devices.
- GTK component/accessibility tests for commands, dialogs, state, progress,
  confirmation, and non-color wording; ignored by ordinary headless tests.
- Isolated native Wayland smoke for user-visible actions and clean Quit where
  the environment permits. Real removable-device gates require explicit
  disposable media and must otherwise be reported as skipped, never simulated
  against user storage.
- Every phase runs focused tests while developing. Root completion requires
  formatting, workspace check, strict Clippy, complete tests, native build,
  diff hygiene, gate checker, status documents, and exactly Phase 18Y as NEXT.

## Status log

- `2026-08-27`: Root plan and leaf ledgers created; implementation inspection
  begins with Phase 18T.
- `2026-08-27`: Parent reverified Phase 18T at 4/4 gates with all 18 focused
  tests passing.
- `2026-08-27`: Parent reverified Phase 18U at 4/4 gates with 13 app and 5 core
  focused tests passing; monitoring remains explicit, bounded, local, and does
  not claim intrusion detection.
- `2026-08-27`: Parent reverified Phase 18V at 4/4 gates with 10 app and 2 core
  focused tests passing; copied-but-unverified output remains explicit and
  ordinary Copy semantics are unchanged.
- `2026-08-27`: Parent reverified Phase 18W at 4/4 gates with all 11 aggregate
  focused tests passing; real removable-media validation is explicitly skipped
  because no disposable lab device was provided.
- `2026-08-27`: Parent reverified Phase 18X at 4/4 gates with 20 app, 21
  integration, and 7 core focused tests passing. The complete Phase 18 family
  filter passes 71 app, 21 integration, and 14 core deterministic tests.
- `2026-08-27`: Root integration formatting, workspace check, strict Clippy,
  466 app plus 21 integration plus 146 core tests, native build, separate GTK
  contracts, E2E harness contracts, leaf gate checker, and diff hygiene pass.
  Native semantic E2E is skipped because Dogtail/pyatspi are unavailable; real
  USB is skipped because no disposable lab device was provided. An isolated
  D-Bus lifecycle retry was blocked by portal services closing the disposable
  bus, so no additional native-lifecycle claim is made.

## Status

COMPLETE. Phases 18T–18X satisfy their 20 leaf gates and root integration
evidence. Exactly Phase 18Y — Operation Recovery is recommended next and is not
implemented by this plan.

---

# Plan: Floe Phase 18A — Security Threat Model

## Contract

- Revalidate the implemented Phase 14 baseline and every planned Phase 18
  security/privacy/integrity family against explicit assets, adversaries,
  non-protections, authority boundaries, data classes, and failure behavior.
- Record decisions as **Accepted for later implementation**, **Candidate**,
  **Deferred**, or **Rejected**. Phase 18A selects no Rust dependency, crypto
  library, vault backend, credential backend, or sandbox mechanism.
- Provide dependency rationale and a traceable implementation-time test plan for
  portable encryption, secrets, vaults, caches/history, provider isolation,
  suspicious/privacy inspection, integrity, verified transfer, guardrails,
  recovery, privileged access, and the final security audit.
- Mark Niri, Plasma-specific integration, remote browsing/recovery, and MTP as
  deferred while retaining generic Wayland/Plasma compatibility gates.
- Add no runtime feature, GTK surface, filesystem operation, secret handling,
  cryptographic claim, dependency, or Phase 18B+ implementation.
- Preserve exact-path, no-shell, no-overwrite, no-follow, bounded-worker, and
  truthful-claim rules already implemented.

## Applicable verification layers

- Documentation structure and cross-reference checks for authoritative sources,
  decision IDs, threat IDs, phase/test mappings, and exactly one roadmap NEXT.
- Dependency manifest/lockfile audit proving Phase 18A adds no runtime crate.
- Existing formatting, workspace check, strict Clippy, and complete tests to
  prove a documentation-only phase did not disturb the verified baseline.
- Native GTK/Wayland smoke is not applicable because Phase 18A changes no
  runtime behavior or UI; later user-visible security phases retain those gates.

## Status

COMPLETE. The threat model, 16 stable decisions, and 26-leaf Phase 18 test
traceability plan are cross-linked from the authoritative project documents.
All Phase 18A gates and repository regressions pass; Phase 18T is the sole next
phase and has not started.

---

# Regression plan: post-Phase-14 synthetic execute-bit icon handling

## Contract

- Make recognized extension families authoritative for presentation even when
  exFAT, FAT, NTFS, network, or other mounts synthesize execute permission bits.
- Preserve executable presentation for extensionless or unknown executable
  files, including existing AppImage behavior.
- Keep the change in app-layer icon policy; perform no MIME/content/filesystem
  probe and do not alter operation permissions or execution authorization.
- Add a lowest-layer regression using executable regular-file fixtures.
- Preserve Phase 15 as the sole roadmap NEXT phase.

## Status

COMPLETE. Known file types now outrank synthetic execute bits for icon
presentation, while unknown and extensionless executables retain executable
artwork. Focused/full Rust, strict Clippy, real GTK, native build, diff hygiene,
and documentation gates pass. Exactly Phase 15 remains the sole roadmap NEXT.

---

# Regression plan: post-Phase-14 file-type icon distinction

## Contract

- Reproduce why distinct PDF and plain-text families render identically or
  disappear in one or more of Floe Color, Phosphor Monochrome, and System Theme.
- Keep classification and rendering policy in `floe-app`; preserve exact paths,
  existing thumbnail precedence, responsive GTK behavior, and live switching.
- Make every current semantic family resolve to a usable icon, with a distinct
  PDF/text outcome even when a host icon theme omits or aliases MIME artwork.
- Reset stale `gtk::Image` state when changing representations and rebind every
  already-loaded list/grid/search/Miller presentation.
- Add lowest-layer regression tests plus a real GTK contract and native smoke.
- Do not start Phase 15 or broaden the supported taxonomy speculatively.

## Applicable testing layers

- Rust policy/resources: extension families, distinct style mappings, registered
  app-owned resources, bounded System Theme fallback chains.
- GTK component: resolved paintables, stale-state reset, immediate live rebind.
- Native Wayland: visible PDF/TXT fixture, three-style switching, persistence,
  responsiveness, and clean quit.

## Status

COMPLETE. Plain text is separate from office documents and PDF; all fifteen
families have distinct app-owned fallbacks, stale GTK image storage is cleared,
focused/full Rust tests, real GTK resolved-paintable checks, E2E contracts, and
native Wayland switching/liveness/clean-Quit smoke pass. Exactly Phase 15
remains the sole roadmap NEXT phase.

---

# Plan: Floe post-Phase-14 — Phosphor Icon System

## Contract

- Vendor a reviewed, pinned subset of Phosphor Core 2.1.1 Regular SVGs with its
  MIT license. Icons remain local resources; Floe performs no runtime download.
- Use Phosphor symbolic icons for Floe-owned navigation/action/sidebar chrome
  while preserving stable accessible labels, actions, focus, and keyboard paths.
- Add one persistent live entry-icon style with exactly three choices: Floe
  Color, Phosphor Monochrome, and System Theme. Existing thumbnails continue to
  replace generic icons and exact file paths remain authoritative.
- Keep icon policy in `floe-app`; add no filesystem I/O, worker, core dependency,
  compositor branch, MIME probe, or future Phase 15 integration.
- Rebind the already-loaded virtualized model when style changes. Do not perform
  synchronous enumeration or icon filesystem work on GTK callbacks.
- Preserve Phase 15 as the sole roadmap `NEXT` phase.

## Applicable testing layers

- Rust policy/preferences: stable style IDs/order/labels, invalid and legacy
  fallback, version-12 round trip, semantic icon-family mapping.
- Resource tests: every vendored Phosphor alias resolves, is symbolic SVG using
  `currentColor`, and the original Floe Color resources remain available.
- GTK component/native: menu action state, immediate live rebind, accessible
  toolbar controls, list/grid generic fallback, thumbnail replacement, clean
  Wayland launch/action/Quit lifecycle.

## Status

COMPLETE. Stable style policy, version-12 migration, 42 pinned symbolic
resources, Phosphor interface chrome, live virtualized rebinding, shared toast
escaping, strict workspace tests, real GTK, and two-launch isolated Plasma
Wayland action/state/persistence/lifecycle behavior are verified in `GATES.md`.
Phase 15 remains the sole recommended next phase and has not started.

---

# Plan: Floe Phase 14 — Generic Desktop Integration Framework

## Contract

- Create one application-layer `DesktopIntegration` capability boundary that
  inventories generic Linux desktop services without leaking GIO/GTK, desktop,
  compositor, or environment-specific types into `floe-core`.
- Represent standards-backed capabilities for local/URI launch, GIO mounts and
  volumes, XDG user directories, portals, notifications, Share, theme signals,
  credential service, and reliable session-lock signals. Capabilities must
  distinguish available, unavailable, and degraded/unknown with a human reason.
- Detect asynchronously on one bounded application worker. Missing session bus,
  portal, Secret Service, notification, theme, or lock service is normal and
  must preserve browsing, local paths, GIO launch, and existing device behavior.
  No GTK callback may perform blocking capability probing.
- Route existing generic launcher/device/location/theme facts through the
  boundary where coherent, expose a native **Desktop Integration** status dialog
  and human command, and keep all fallbacks truthful under generic Wayland,
  Plasma, Niri, and no-specialized-backend environments.
- Use standard GLib/GIO/XDG/session-bus signals only. Add no Niri IPC, KDE
  Framework, Plasma type, compositor conditional branches, credential reads,
  notification contents, Share transmission, session-lock control, or remote
  filesystem phase work.
- Keep capability snapshots memory-only and path/content-free. Do not log bus
  names, account identifiers, filenames, secrets, notification bodies, or user
  paths merely to report capabilities.

## Applicable testing layers

- Pure application policy tests: stable capability IDs/order/status/fallback,
  no-specialized-backend behavior, missing/malformed service probes.
- Worker tests: bounded request/result channel, generation supersession, clean
  shutdown, memory-only snapshots, no GTK-thread blocking.
- Integration/UI: native accessible status dialog and command, existing
  launcher/device/location/theme regressions, isolated generic Wayland plus
  first-class Plasma smoke, D-Bus health and clean Quit.

## Status

COMPLETE. Acceptance evidence is recorded in `GATES.md`. Phase 15 is the sole
recommended next phase and has not started.

---

# Archived plan: Floe Phase 13G — Duplicate Finder

## Contract

- Add a GTK-independent duplicate scan over explicitly selected absolute local
  files and/or directory roots. Preserve exact `PathBuf`/`OsStr`, cap roots,
  traversed entries/directories/depth/results/bytes, stay on each root device,
  and never follow symbolic links.
- Discover candidates cheaply by exact regular-file size, then stream the
  reviewed Phase 10E SHA-256 implementation only for same-size candidate sets,
  and finally compare candidate bytes before calling independent files equal.
  A digest match alone is never proof.
- Open inputs no-follow, support cooperative cancellation, and revalidate
  device/inode/size/mtime/ctime before and after hashing and byte comparison.
  Report inaccessible, changed, over-limit, mount, link, and sparse/huge-file
  exclusions truthfully.
- Distinguish hard-link aliases from independent copies by device/inode. Never
  count aliases as reclaimable bytes, follow symbolic links, cross mount roots,
  upload hashes/content, persist results/history, require Phase 13F's index, or
  delete anything automatically.
- Add selection-aware **Check for Duplicates…** to useful list/grid/Miller
  context surfaces and command discovery. Run one bounded application worker;
  present cancellable progress and accessible native review groups with exact
  reveal plus explicit ordinary Trash handoff. No filesystem work in GTK
  callbacks.
- Exclude automated cleanup, permanent deletion, fuzzy/similar matching,
  content-defined chunks, background scans/watchers, remote/Trash roots,
  Phase 14 desktop integration, and unrelated refactors.

## Applicable testing layers

- Core hostile fixtures: size-first pruning, reviewed SHA-256, byte-confirmed
  collision safety, hard links, raw names, links, mutation, permissions,
  cancellation, huge/sparse limits, mount/depth/result/root bounds.
- Application worker: capacity-one generation lifecycle, progress/result
  bounds, cancellation, exact memory-only outcomes, clean shutdown.
- GTK/controller: context/menu/action parity, explicit-root planning, accessible
  review/reveal/Trash policy, native Wayland action/liveness/Quit smoke.

## Status

COMPLETE. Size/hash/byte-confirmed core discovery, reviewed SHA-256 application
worker, cancellation, hard-link accounting, context/command integration, native
review/reveal/Trash handoff, strict workspace suite, real GTK gate, and isolated
Wayland action/lifecycle smoke are verified in `GATES.md`. Exactly Phase 14 is
recommended next.

---

# Archived plan: Floe Phase 13F — Optional Search Indexing

## Contract

- Add a GTK-independent, versioned filename/metadata index for one explicitly
  chosen local root. Preserve exact `PathBuf`/`OsStr` bytes, directory
  fingerprints, no-follow file identity, and all metadata required by Phase
  13C predicates. The index must never contain file contents or snippets.
- Index building is explicit and optional, runs on one bounded application
  worker, stays on the root filesystem, never descends symbolic links, and
  excludes hidden entries/directories conservatively because Phase 18
  Sensitive Folder, Private Mode, and vault classification do not yet exist.
- Persist the optional index only in a private (`0600`) bounded cache file via
  atomic sibling replacement. Reject relative/Trash/remote/symbolic roots,
  malformed records, excessive paths/records/bytes, and unknown versions.
- Accelerate eligible subtree **Search Files** requests only. Before returning
  indexed results, validate the root and every recorded directory fingerprint;
  validate matching entries no-follow before presentation. A missing, corrupt,
  stale, policy-ineligible, or busy index must automatically run the existing
  bounded non-indexed search with truthful status feedback.
- Expose accessible native Enable, Build/Rebuild, and Clear controls with a
  clear capability description: filenames and metadata only, hidden content
  excluded, no content-search acceleration. Persist only the reviewed enabled
  boolean in preferences; explicit saved searches remain independent.
- Exclude content indexing, background filesystem watching/rebuilds, global or
  remote indexes, hidden/sensitive overrides, tags, Phase 13G duplicates,
  Phase 14 desktop integration, Phase 18 privacy claims, and unrelated refactors.

## Applicable testing layers

- Core fixtures: bounded traversal, raw names, symlink/mount/hidden policy,
  codec corruption/version/capacity, directory and entry stale detection,
  predicates, ordering, cancellation, and live-search fallback signal.
- Application worker/persistence: bounded request/event channels, private
  atomic cache, load/build/query/clear lifecycle, generation supersession,
  corrupt/missing/stale fallback, clean shutdown.
- GTK/controller: accessible capability controls, enable/build/clear actions,
  indexed/live status, saved-search compatibility, native Wayland D-Bus health
  and clean Quit.

## Status

COMPLETE. Core index policy/codec/query, bounded application persistence worker,
compact native controls, automatic live fallback, strict workspace suite, real
GTK component gate, and isolated Wayland cache/action/lifecycle smoke are all
verified in `GATES.md`. Exactly Phase 13G is recommended next.

---

# Archived plan: Floe Phase 13E — Saved Searches

## Contract

- Add a GTK-independent validated saved-query model for Search Files and Search
  Contents. Preserve exact absolute `PathBuf` roots, explicit folder/subtree
  scope, Text/Glob/Regex, Match Case, hidden policy, and every Phase 13C
  predicate. Reject relative roots, empty names, invalid queries, malformed
  filters, duplicates, and over-capacity data.
- Persist only searches the user explicitly names and saves. Use a versioned,
  private (`0600`), bounded file written by one capacity-one application worker
  with atomic sibling replacement. Preserve non-UTF-8 root bytes on Unix; never
  reconstruct a path from display text. Corrupt/unknown records are skipped
  independently without discarding valid records.
- Keep recent executed searches bounded, deduplicated, and memory-only for the
  current process. Provide visible Clear Recent control. Do not persist implicit
  history, snippets, results, counters, selection, or usage.
- Add accessible native Save Search and Saved Searches controls to the unified
  search surface. The manager must distinguish saved and recent entries, run a
  selected query against its exact saved root, and delete saved entries only
  after explicit activation. Missing/unavailable roots fail through ordinary
  search feedback; they are not silently rewritten to the current folder.
- Add deterministic result ordering by Name, Modified, or Size without changing
  exact-path identity or performing filesystem work on GTK callbacks. Content
  matches for one file retain stable line order.
- Exclude Phase 13F indexing, Phase 13G duplicate finding, Phase 14 desktop
  integration, tags, global/remote roots, Private Mode/Sensitive Folder claims,
  persistent implicit history, result export, and unrelated refactors.

## Applicable testing layers

- Core model/catalog tests: validation, capacity, deduplication, exact raw roots,
  all filter fields, session history suppression/clear, deterministic ordering.
- Application persistence tests: versioned round trip, migration, corruption,
  `0600` mode, atomic replace, capacity-one latest-save behavior and shutdown.
- UI/controller tests: accessible labels, saved/recent manager policy, exact-root
  replay, delete/clear actions, filename/content mode restoration.
- Strict workspace gates and isolated native Wayland D-Bus/lifecycle smoke.

## Status

COMPLETE. Every Phase 13E gate has measured evidence in `GATES.md`. Exactly
Phase 13F optional indexing is recommended next; later phases remain excluded.

---

# Archived plan: Floe Phase 13D — Content Search

## Contract

- Add an explicit **Search Contents** mode to the existing unified search
  surface. It searches only local regular files rooted at the active folder,
  with This Folder and Include Subfolders scopes; it never searches Trash,
  remote locations, symbolic-link targets, or another filesystem device.
- Add a GTK-independent bounded content-search engine with exact `PathBuf`
  identity, Text/Glob/Regex query modes, explicit Match Case, cancellation,
  stable line-number/snippet results, and explicit result/file/byte/depth caps.
- Read only candidate regular files that pass the existing Phase 13C advanced
  predicates. Use no-follow opens and revalidation. Reject binary/NUL-bearing,
  over-limit, unsupported-encoding, inaccessible, or changed inputs with
  truthful aggregate counters instead of lossy fallback or silent broadening.
- Support bounded UTF-8 plus BOM-declared UTF-16LE/UTF-16BE text. Do not guess
  legacy encodings, execute helpers, inspect archives, upload content, create an
  index, persist queries/results/snippets, or log file contents.
- Run traversal and content reads on one capacity-one application worker with
  bounded response batches and generation cancellation. GTK callbacks only
  submit typed requests and render events.
- Reuse ordinary exact-path Open/Open With/Properties/Reveal actions while the
  result surface visibly presents filename, containing folder, line number, and
  a whitespace-normalized bounded snippet. Closing, navigating, or switching
  modes cancels incompatible work and clears content-derived state.
- Exclude Phase 13E saved searches/history, Phase 13F indexing, Phase 13G
  duplicate discovery, Phase 14 desktop integration, remote roots, regex
  replacement, archive/PDF/document extraction, and search-result sorting.

## Applicable testing layers

- Core fixtures: UTF-8/UTF-16, case/Text/Glob/Regex, raw names, binary and
  malformed inputs, symlink/mount policy, byte/result/depth caps, cancellation,
  changed-file revalidation, and snippets.
- Application worker: capacity-one request/response bounds, generation
  supersession, streaming batches, clean shutdown, no GTK-thread reads.
- UI/accessibility: visible plain-language mode/help, result columns/labels,
  error/stopped/limit feedback, exact Reveal mapping, real GTK component gate.
- Strict workspace gates plus isolated native Wayland D-Bus action/liveness/Quit
  smoke before documentation can mark the phase complete.

## Status

COMPLETE. All Phase 13D gates have measured evidence in `GATES.md`. Exactly
Phase 13E saved searches is recommended next; later phases remain excluded.

---

# Archived plan: Floe Phase 13C — Advanced Filters

## Contract

- Add one optional advanced-filter section inside the existing unified search
  surface. It must serve Quick Filter and Search Files without creating a third
  search pipeline.
- Support combined filename, entry-type, extension, MIME, size, modified-date,
  Unix owner, and hidden predicates. Filename matching retains Text, Glob, and
  Regex and gains one explicit Match Case control.
- Own the predicate model and bounded validation in `floe-core`. Evaluate cheap
  facts first, define missing/unknown metadata semantics explicitly, and leave a
  tag-ready boundary without implementing tags.
- Preserve exact `PathBuf`/`OsStr` identity. Display text must never become a
  filesystem path.
- Keep both existing capacity-one application workers responsive and
  generation-safe. Resolve MIME and owner metadata lazily on workers only when
  active predicates require them; MIME guessing must not read file contents.
- Let Quick Filter temporarily include or isolate hidden loaded entries without
  mutating the global Show Hidden preference. Search Files supports Current
  Setting, Include Hidden, and Hidden Only under its existing traversal bounds.
- Allow an empty filename query only when at least one advanced predicate is
  active. Preserve cancellation, stale-result rejection, exact Reveal,
  memory-only privacy, and all Phase 13A/13B limits.
- Expose compact wrapping native controls with visible labels, plain-language
  descriptions, Apply, and Clear Filters. Exclude tags, content search, saved
  searches/history, indexing, remote roots, persistence, and search-specific
  sorting/grouping.

## Applicable testing layers

- Deterministic core predicate tests for combinations, case, raw names,
  validation, missing metadata, and hidden policy.
- Application worker tests for lazy MIME/owner resolution, fixed capacity,
  generation supersession, and both filter/search engines.
- GTK/accessibility contracts and isolated native Wayland action/liveness/Quit
  smoke for the shared wrapping controls and mode transitions.

## Status

COMPLETE. Structured core predicates, both bounded application workers, the
shared accessible native controls, strict workspace gates, real GTK component
coverage, and isolated Plasma Wayland D-Bus/lifecycle smoke are verified in
`GATES.md`. Exactly Phase 13D content search is next; later phases remain
excluded.

---

# Archived plan: Floe Phase 13B — Filename Search

## Contract

- Add explicit case-insensitive filename-only search rooted at the active local
  folder with fixed `This Folder` and `Include Subfolders` scopes. It must never
  read file contents or search remote/non-local roots.
- Preserve exact `PathBuf`/`OsStr` identity and reuse Phase 13A's reviewed
  Unicode text plus raw-byte non-UTF-8 matching semantics. Cap queries at 256
  Unicode scalar values.
- Run traversal on one capacity-one application worker. Stream batches of at
  most 128 exact results through bounded response state; support generation
  cancellation, explicit Stop, and stale-event rejection.
- Cap one search at 100,000 matches, 1,000,000 examined entries, 100,000
  directories, and depth 128. Report incomplete/skipped states truthfully.
- Traverse with no-follow metadata, never descend symbolic links, and never
  cross the root filesystem device. Continue past inaccessible child entries or
  directories while counting skips; root failures remain explicit.
- Consolidate Quick Filter and Search Files into one native search surface:
  `Ctrl+F` opens Quick Filter, `Ctrl+Shift+F` opens Search Files, and a visible
  mode selector explains their different scope. Preserve typing focus and Space.
- Present streamed results in a dedicated exact-entry multi-selection list with
  filename and containing folder. Reuse ordinary exact-path file actions and add
  selection-aware `Reveal in Folder` that navigates to the exact parent and
  restores exact result selection.
- Clear/cancel on location exit. Keep query, root, results, counters, and usage
  memory-only; do not persist or log them.
- Embed the user-provided `icon_floe.png` under the stable application ID and
  use it as Floe's application/window icon without changing entry iconography.
- Exclude Phase 13C advanced predicates, MIME/owner/date/size filters, Phase 13D
  content search, arbitrary roots, remote search, history, saved searches,
  indexing, tags, duplicate finding, and unrelated refactors.

## Applicable testing layers

- Deterministic core unit and `tempfile` filesystem integration tests for scope,
  raw paths, links, mount boundaries, caps, skips, and cancellation.
- Application worker tests for bounded streaming, cancellation, stale generations,
  and backpressure.
- GTK/action/accessibility contract tests plus an isolated native Wayland
  action/liveness/clean-quit smoke. Full Dogtail E2E remains conditional on its
  external dependencies.
- No new property test is planned: fixed filesystem trees and injected limits
  express these traversal invariants more clearly than generated cases.

## Status

COMPLETE. Core traversal, the bounded streaming worker, unified native search
surface, exact result actions, supplied icon, strict workspace gates, and an
isolated native Wayland action/liveness/clean-Quit smoke are verified in
`GATES.md`. Exactly Phase 13C is next; later phases remain excluded.

---

# Archived plan: Permanent Layered Testing Foundation

## Contract

- Preserve the existing 469-test Rust suite, `tempfile` filesystem isolation,
  and native Wayland smoke practice without reorganizing unrelated tests.
- Define permanent, distinct unit, filesystem-integration, property, GTK
  component/accessibility, native E2E, Niri smoke, and Plasma smoke layers.
- Add `proptest` only to `floe-core` dev dependencies and use it selectively
  for exact Linux filename identity, deterministic sorting, navigation-history,
  and folder-filter invariants that benefit from generated input.
- Add graphical GTK component/accessibility contract tests as an explicit
  opt-in gate. They must exercise real Floe widget/action metadata and must not
  make headless `cargo test --workspace` depend on a display server.
- Add a native Dogtail/AT-SPI E2E layer under `e2e/` with dependency and
  environment preflight, semantic accessible-node interaction, deterministic
  condition waits, and eight named workflow scenarios.
- Every E2E launch must create private temporary HOME/XDG roots and an isolated
  freedesktop Trash. Never touch the developer's real user directories, Trash,
  mounts, preferences, cache, or data.
- Keep compositor-independent E2E separate from Niri and Plasma smoke policy.
  Do not claim graphical execution when Dogtail, AT-SPI, a suitable session, or
  compositor support is unavailable.
- Permanently document regression, future security/privacy, CI, test-layer
  selection, commands, dependencies, limitations, and evidence requirements in
  `AGENTS.md` and `docs/DEVELOPMENT.md`.
- Do not add Playwright, Selenium, Tauri/browser testing, hidden test-only UI,
  timing-only sleeps, or unrelated application features/refactors.

## Audit baseline

- Clean `main` baseline: `ecf2fc2` on isolated branch `testing-foundation`.
- `cargo test --workspace`: 365 `floe-app` + 104 `floe-core` = 469 passing.
- Existing source contains 233 `tempfile` references and extensive exact-path,
  non-UTF-8, bounded-worker, operation, persistence, and lifecycle coverage.
- No `.github` CI workflow, property-test dependency, dedicated GTK test
  target, or `e2e/` directory exists.
- Dogtail and `pyatspi` are unavailable on this host; no isolated Mutter,
  Weston, or Cage runner is installed. Native E2E execution cannot be claimed.

## Status

COMPLETE. All seven active gates contain measured evidence; the repository-wide
gate checker reports all 57 current and historical gates met. This
testing-foundation pass does not advance Phase 13, and Phase 13B remains the
sole recommended next roadmap phase.

---

# Previous completed plan: Floe Phase 13A — Current-Folder Filter

## Contract

- Add an explicit current-folder filter over entries already returned by the
  existing directory worker. It must not enumerate a subtree, read file
  contents, or create a second browsing pipeline.
- Support three fixed modes: case-insensitive text substring, filename glob,
  and regular expression. Cap queries at 256 Unicode scalar values and compile
  glob/regex patterns once per request rather than in GTK row bindings.
- Match valid UTF-8 names with Unicode semantics. For non-UTF-8 Linux names,
  retain exact raw bytes: text performs ASCII-insensitive byte matching, while
  glob and regex use a documented raw-byte fallback. Display text never becomes
  path identity.
- Run filtering on one bounded, generation-superseding application worker so
  large loaded directories and rapid query changes do not block GTK or retain
  unbounded request/result state.
- Expose a discoverable header action and `Ctrl+F`, an accessible mode chooser,
  visible match count, inline non-color-only invalid-pattern feedback, and
  Escape/close clearing. Keep native focus behavior for the editable field.
- Preserve the existing directory order and exact selection only for entries
  that remain visible. Reapply the active filter after refresh, sorting, hidden
  file changes, and watcher reconciliation; clear it when the active location
  changes.
- Keep filter query, mode, results, and usage memory-only. Do not persist or log
  names/queries, add history, indexing, content search, metadata filters, or
  saved searches.

## Status

COMPLETE. Core and application focused tests, strict workspace gates, and native
Wayland action/render/lifecycle smoke verified the bounded current-folder filter.
Phase 13B filename search remains excluded and is the sole recommended next phase.

---

# Previous completed plan: Floe Phase 12F — Productivity Action Integration

## Contract

- Expose the existing selection-aware Extract Here, Extract To, and Compress
  actions in a compact Archives submenu in every normal file context surface.
- Keep a small reviewed essential action set fixed while allowing users to show
  or hide bounded optional productivity groups in file and background context
  menus through an accessible native dialog.
- Persist only stable reviewed group identifiers through the existing bounded
  preference worker; migrate older preference files with safe defaults.
- Rebuild list, grid, Miller, and background menu models from the same policy so
  pointer and keyboard invocation have parity and live GAction eligibility stays
  authoritative.
- Register customization as a human-readable command reachable from the header,
  command palette, and Keyboard Shortcuts dialog.
- Preserve exact paths and existing archive/job architecture; GTK builds menus
  and submits existing actions but performs no filesystem work.
- Exclude arbitrary external commands, plugin actions, shell command templates,
  per-MIME rules, privacy/safe-open actions, and later roadmap phases.

## Status

COMPLETE on `phase-12f-productivity-actions`; verified evidence is recorded in
`GATES.md`. Phase 13A folder filter is the sole recommended next phase.
# Plan: Floe Phase 21A — Performance

## Contract

Measure and improve the performance of Floe functionality that already exists.
The phase must exercise a real 100,000-entry local directory plus bounded
thumbnail, filename/content search, copy, checksum/integrity, duplicate, and
advanced-metadata workloads. Fixtures stay below disposable temporary roots and
must never inspect the user's HOME, Trash, mounts, or data. Measurements must be
reproducible, record host/toolchain context, and distinguish elapsed time,
throughput, memory evidence, and structural capacity bounds. Optimize only a
measured bottleneck without weakening exact-path identity, cancellation,
no-follow policy, bounded concurrency, cache privacy, or GTK responsiveness.

This phase does not benchmark or implement vaults, encryption, remote browsing,
MTP, Niri, Plasma-specific behavior, or any other deferred capability. It does
not add speculative async/runtime or allocator dependencies.

## Depth tree

1. Benchmark contract and fixtures
   1. Add one opt-in release-mode performance harness over real Floe APIs.
   2. Cover 100k browsing/sorting/search and bounded representative expensive
      workloads with machine-readable, human-readable evidence.
2. Measurement and optimization
   1. Record a reproducible baseline on this host.
   2. Profile the slowest applicable path and make the smallest measured
      architectural optimization with correctness regression coverage.
3. Native and release verification
   1. Exercise a real 100k folder through the native Wayland application and
      verify liveness/clean quit without GTK criticals.
   2. Run all deterministic, GTK/E2E where applicable, documentation, status,
      and diff-hygiene gates.

## Status log

- 2026-08-29: Phase 21A started on `phase-21a-performance`; gates are defined in
  `gates/phase-21a.md`. Exactly Phase 21A remains `NEXT` until every gate is
  evidenced.
- 2026-08-29 pass 1: implemented the complete release harness and measured the
  production workload set below one temporary root.
- 2026-08-29 pass 2: replaced the measured two-pass ASCII metadata counter with
  one pass; 16,652,288 bytes improved 64,108 us to 32,212 us with equal facts.
- 2026-08-29 pass 3: found and fixed vertical-tab parity, added exhaustive ASCII
  plus Unicode/line-ending regression coverage, and reran strict Clippy/tests.
- 2026-08-29 pass 4: polished reproduction/limitation documentation, completed
  deterministic, GTK, E2E-preflight and native lifecycle evidence. Phase 21A is
  complete with the critical-free native sub-gate explicitly abandoned because
  host AT-SPI refused connections. Exactly Phase 21B is now `NEXT`.

---
# Plan: Floe Phase 21B — Packaging and Migrations

## Contract

Turn the verified workspace into an installable Linux application without
changing user defaults or inventing legal, sandbox, vault, or remote-support
claims. Freeze `io.github.rodriguezcappsec.Floe` as the application, desktop,
AppStream, resource, and icon identity and `floe` as the installed command.
Select Arch Linux plus a source `DESTDIR` installer as the first verified
packaging path; Flatpak remains deferred because its build tools are absent and
Floe's current broad host-integration contract has not received a truthful
sandbox design.

Package only the binary, desktop file, validated AppStream metadata, reviewed
512px icon derivative, notices, and documentation. Register only
`inode/directory`; installation must never set Floe as the default handler or
write into any user's HOME/XDG roots. Project and icon redistribution remain
explicitly `LicenseRef-proprietary`/all-rights-reserved until the owner chooses
an open-source license; packaging must not imply permission that is absent.

Harden versioned preference persistence and verify supported upgrades,
corrupt/future inputs, rebuildable cache policy, backup/rollback instructions,
and private atomic storage entirely below disposable XDG roots. No vault exists,
so this phase must explicitly test and document that there is no vault migration.

## Depth tree

1. Release identity and metadata
   1. Final application/binary/resource/icon identity and release Cargo profile.
   2. Desktop/AppStream/hicolor assets with native validators.
2. Install and package
   1. Manifest-driven `DESTDIR` install/uninstall with exact ownership.
   2. Arch PKGBUILD/release-source workflow and host-available package checks.
3. Migration safety
   1. No-follow private atomic preferences plus backward/future/corrupt tests.
   2. Disposable clean install, upgrade, cache rebuild, backup/rollback tests.
4. Verification and status
   1. Frozen release build, ELF/runtime/native staged launch, full workspace gates.
   2. Update docs/status and set exactly Phase 21C `NEXT` only after evidence.

## Status log

- 2026-08-29: Phase 21B started on `phase-21b-packaging` from verified Phase 21A
  commit `691098e`; gates are in `gates/phase-21b.md`.
- 2026-08-29 pass 1: froze stable identity, release profile, desktop/AppStream
  metadata, icon, manifest-driven installer, deterministic source workflow,
  and Arch package contract.
- 2026-08-29 pass 2: hardened version-18 preference storage and verified clean,
  legacy, corrupt, oversized, symlink, future-version, residue, cache rebuild,
  backup, and rollback behavior below disposable XDG roots.
- 2026-08-29 pass 3: frozen release, native validators, source/package layout,
  real `makepkg`, staged directory/Ping/Quit lifecycle, and documentation audit
  pass. Phase 21B is complete; exactly Phase 21C is `NEXT`.

---
# Plan: Floe Phase 21C — Release Documentation

## Contract

Create one coherent release documentation set for the verified Phase 21B
artifact. A fresh user must be able to install, launch, learn core workflows,
understand administrator/security/accessibility/recovery behavior, collect and
redact debugging evidence, upgrade/rollback/uninstall, and find limitations
without reading phase history. Current-state documents must agree with code and
tests; historical evidence remains in gate ledgers rather than masquerading as
current architecture.

Add dependency-free link/table/terminology/status validation and render every
release-facing Markdown document. Reconcile stale roadmap/matrix/security/
architecture claims and malformed tables. Document Floe as English-only with
partial RTL foundations: this phase does not invent gettext catalogs or claim a
translated native walkthrough. Diagnostic logs and technical details may expose
sensitive paths, so collection guidance must require review/redaction.

Regenerate the deterministic source archive and Arch checksum after public docs
are added, update the installed documentation manifest, and rerun package and
staged-native verification. Deferred Niri/Plasma-specific, remote, MTP, vault,
encryption, sandbox, Open Safely, Secure Share, and Flatpak functionality remain
clearly unavailable. Dependency/advisory/security audit and release artifacts
remain Phase 21D.

## Depth tree

1. Current-claim reconciliation
   1. Repair roadmap, feature matrix, privacy, architecture and development drift.
   2. Enforce one canonical limitation and security-term vocabulary.
2. Release manuals
   1. Getting started, installation, administration and accessibility.
   2. Recovery, debugging/log policy, localization, security and changelog.
3. Automated documentation contracts
   1. Link, fragment, heading, table, terminology and exactly-one-NEXT checker.
   2. GFM render sweep and bounded fresh-user walkthrough contract/native smoke.
4. Package integration and verification
   1. Install all release docs, regenerate archive/hash and rebuild package.
   2. Run complete workspace/native/doc gates, update status, set 21D NEXT.

## Status log

- 2026-08-29: Phase 21C started on `phase-21c-release-documentation` from
  verified Phase 21B commit `adb3182`; gates are in `gates/phase-21c.md`.
- 2026-08-29 pass 1: added the reciprocal release manual set, strict
  documentation checker, render sweep, and fresh-user walkthrough contracts.
- 2026-08-29 pass 2: adversarially reconciled roadmap, matrix, security,
  architecture, development, and malformed-table claims; strict checks pass.
- 2026-08-29 pass 3: installed public manuals with preserved relative links,
  added deterministic release-source coverage, rebuilt the real Arch package,
  and verified staged D-Bus Ping/Quit exit 0. Native semantic walkthrough skips
  truthfully because Dogtail and pyatspi are unavailable.
- 2026-08-29 pass 4: full format/check/strict-Clippy/workspace-test/frozen-build
  and diff gates pass; Phase 21C is complete and exactly Phase 21D is next.

---
# Plan: Floe Phase 20B3 — Compact Tabs

## Contract

Reduce tab-strip visual noise without changing tab ownership, navigation,
drag/drop, context menus, middle-click closing, keyboard shortcuts, or exact
path identity. Tabs use a compact adaptive width, one restrained active-state
signal, an ellipsized basename label, a full-path tooltip, and a smaller but
still accessible close action. The tab strip must remain horizontally
scrollable and usable under every appearance preset and high-contrast mode.
Device rows retain their name as the primary line and concise available space
as the secondary line; neither label wraps when the sidebar narrows.

## Applicable testing layers

1. Deterministic presentation-policy tests for width, spacing, ellipsization,
   active-state, and accessible close-label contracts.
2. Focused real-GTK component test for compact allocation, active/inactive
   semantics, keyboard focus, close action, and horizontal overflow.
3. Full format, check, strict Clippy, workspace tests, docs, and native Wayland
   lifecycle gates.

## Scope exclusions

- No multi-window lifecycle or application-service refactor.
- No tab/session codec, drag/drop, navigation, or operation behavior change.
- No new appearance preset or unrelated header/sidebar redesign.

## Status log

- 2026-08-29: Started on `phase-20b3-compact-tabs`; gates are recorded in
  `gates/phase-20b3.md`.
- 2026-08-29: Implementation complete. Focused deterministic and real-GTK tests,
  full workspace format/check/strict Clippy/tests, strict docs/render/package
  and E2E contracts, plus isolated native Wayland Ping/Quit pass. Phase 18Y2 is
  restored as the sole roadmap `NEXT` phase.

---
+# Plan: Floe Phase 22B — Optional XDG FileChooser Portal Backend

## Contract

Implement an explicitly opt-in `org.freedesktop.impl.portal.FileChooser` backend
that launches the verified Floe Selection Mode for `OpenFile`, `SaveFile`, and
`SaveFiles`. The backend owns the conventional freedesktop backend bus name only
when invoked with its dedicated service flag. It validates every request handle,
application ID, parent identifier, title, option type, local NUL-terminated byte
path, filter, choice, filename, count, and size before launching UI. Request
objects expose `org.freedesktop.impl.portal.Request.Close`; close/cancel is
idempotent and cannot answer a stale or different request.

The D-Bus method itself remains asynchronous to GTK/GLib: one bounded
application-owned supervisor launches exact argv without a shell, captures only
bounded selector stdout, and delivers one terminal response. Success returns
normalized local `file://` URIs; cancel
returns response 1 and backend/validation failure returns response 2. Floe does
not issue Document Portal grants and never describes a returned URI as new
sandbox authority. The installed portal descriptor is not selected by default;
users/desktops must opt in through `portals.conf`. Ordinary Floe and direct
Selection Mode remain independent when the portal service is absent.

## Depth tree

1. Portal-neutral request model and codec
   - strict method/options/path/filter/choice limits and exact raw path parsing;
   - parent-window identifier parser and sanitized chooser presentation;
   - response codes/results, URI normalization, SaveFiles collision policy.
2. Bounded chooser supervisor
   - exact executable/argv construction with no shell or lossy path conversion;
   - fixed request/process capacity, bounded output, close/cancel, stale-result rejection;
   - deterministic process outcomes and cleanup on shutdown.
3. Optional D-Bus backend and packaging
   - own conventional backend name only in explicit service mode;
   - register FileChooser methods and per-handle Request.Close objects;
   - defer method replies, preserve one terminal reply, release handles;
   - install D-Bus service and portal descriptor without default selection.
4. Verification and handoff
   - isolated session-bus method/options/Close/concurrency native contracts;
   - full Rust, strict docs/render/release/package/E2E and native Wayland gates;
   - mark 22B complete only with evidence and set exactly one bounded NEXT phase.

## Status log

- 2026-08-30: Started on `phase-xdg-filechooser-portal` from verified Phase 22A
  commit `0df44d9`. Gates are in `gates/phase-xdg-filechooser-portal.md`.
- 2026-08-30: COMPLETE. Focused request/supervisor/D-Bus contracts, full workspace
  format/check/strict Clippy/tests, docs/render/package/release/E2E contracts,
  and isolated session-bus native Wayland SaveFile/Request.Close lifecycle pass.
  SaveFile returned its exact URI without creating the destination; Close returned
  response 1; private config/state/transient chooser cleanup passed. A reusable
  exported foreign Wayland parent handle was unavailable, so live parent attachment
  remains explicitly unverified. Phase 22C is the sole recommended next phase.

---

# Plan: Floe next-eight daily-driver maturity program

## User-authorized scope

Implement the eight capabilities requested on 2026-08-31 in dependency order:
22C portal filters/choices; true multi-window support; completion notifications;
natural filename sorting; bookmark reorder/rename; a collapsible sidebar;
on-demand checksums in Inspector/Properties; and complete Owner, Group, Path,
symlink-target, image, and media details columns.

This request explicitly authorizes the broader program, but it does not authorize
Niri/KDE-specific integrations, remote/MTP browsing, tab detachment, encryption,
vaults, arbitrary plugins, unrelated visual redesign, or filesystem work in GTK.

## Shared contracts

- Exact `PathBuf`/`OsString` identity remains authoritative. Display labels,
  localized natural-sort keys, bookmark aliases, and rendered column text never
  reconstruct a path.
- Filesystem, metadata, hashing, notification capability probes, and persistence
  remain behind existing bounded application/core workers. GTK only dispatches
  typed requests and presents results.
- One GApplication may own several windows, but preference/session/job/recovery
  services remain application-scoped. Window-local tabs, panes, selection,
  navigation, dialogs, and focus never leak between windows.
- New preferences use the existing versioned, private, asynchronous codec with
  migration, bounds, and corruption fallback. No new dependency is assumed.
- Portal filters and choices are validated portal-domain values. Unsupported
  patterns/options return response 2; a URI result is not a Document Portal grant.
- Notifications are optional, path-free for sensitive contexts, suppressed for
  the focused window, and never replace in-app terminal evidence.
- Checksums are explicit and on-demand. A digest is not authenticity or malware
  proof. Advanced columns reuse existing bounded metadata providers and unknown
  values remain unknown rather than fabricated.

## Depth tree and gate ownership

1. Portal interoperability — `gates/phase-22c-portal-options.md`.
2. Multi-window application/window ownership — `gates/phase-23a-multi-window.md`.
3. Operation completion notifications — `gates/phase-23b-notifications.md`.
4. Natural filename sorting — `gates/phase-23c-natural-sort.md`.
5. Bookmark reorder/rename — `gates/phase-23d-bookmark-management.md`.
6. Collapsible sidebar — `gates/phase-23e-collapsible-sidebar.md`.
7. Inspector/Properties checksums — `gates/phase-23f-inspector-checksums.md`.
8. Complete details columns — `gates/phase-23g-details-columns.md`.
9. Cross-feature integration and release gates —
   `gates/phase-next-eight-integration.md`.

The work is sequenced where modules overlap. Read-only architecture audits may
run concurrently; edits to shared browser/preferences/application surfaces do not.

## Status log

- 2026-08-31: Started on `phase-next-eight-maturity` from clean verified commit
  `f79c267`. Contracts and per-feature gates were written before implementation.
- 2026-08-31: Implemented Phase 22C and Phase 23B–23G. Phase 23A now provides
  practical independent windows, exact folder-to-new-window routing and
  newest-live controller fallback, but remains deliberately PARTIAL: sharing the
  current destructive event drain would create duplicate or stolen terminal and
  conflict handling, and secondary persistence/session writers would race.
  Phase 23H is the sole next phase for one application-owned coordinator and
  versioned bounded multi-window session restoration.

---

# Plan: Multi-window reliability and edge-case repair

## User-authorized scope

Fix the six concrete defects found by the Phase 23 adversarial audit plus the
reported freeze when one of two Floe windows closes. This is a reliability
repair of the uncommitted Phase 22C / Phase 23A–23G implementation, not the
future Phase 23H application-wide job/session coordinator.

## Contracts

1. Closing a browser window must never synchronously wait for read-only worker
   I/O on GTK's main thread. Window-owned mutating operations remain visible:
   a close with active work is rejected with accessible wait/cancel guidance;
   an idle close releases its state without a strong application-shutdown
   retention cycle.
2. A failed secondary-window construction cannot redirect an existing window,
   leave a broken blank window, or terminate healthy windows. A launch target is
   queued only to the controller returned by that exact successful build.
3. XDG FileChooser filters are advisory as required by the portal contract. A
   valid local URI that does not match the selected filter succeeds while the
   selected-filter result metadata is preserved.
4. A Properties checksum always targets the exact authoritative `PathBuf` whose
   Properties presentation was opened, even if the live selection later changes.
5. Completion notification replacement IDs are namespaced per browser window so
   equal local job IDs from different windows cannot collide; notification text
   remains path-free.
6. Every reproduced defect receives a deterministic regression at the lowest
   practical layer. Full Rust, documentation, packaging, and applicable native
   multi-window gates must pass before the repair is called complete.

## Depth tree

1. Lifecycle safety: operation-aware close policy, weak shutdown ownership, and
   nonblocking cooperative teardown for browser-owned read workers.
2. Routing and identity correctness: exact build-result routing, advisory portal
   filters, exact Properties checksum target, notification namespace.
3. Integration: adversarial regression sweep, full repository gates, native
   Wayland two-window close/liveness smoke where the host permits it, persistent
   status/documentation reconciliation.

## Status log

- 2026-08-31: Reopened close regression is complete. Floe now pops down and
  unparents every manually parented browser popover before GTK parent
  finalization. The focused GTK gate and guarded KDE Wayland
  close/survivor/third-window smoke pass with no `GtkEntry` child warning.
  Phase 23H remains the sole NEXT phase.

- 2026-08-31: Repair started after the user reproduced the two-window close
  freeze. Acceptance gates are in `gates/phase-23-multi-window-reliability.md`.
- 2026-08-31: Implemented all six audit repairs and the close-freeze lifecycle
  fix. Seven focused regressions pass. Full Rust, strict Clippy, docs/render,
  package/migration/release-source/release-candidate/E2E contracts and diff
  hygiene pass. Isolated native Wayland created two windows, pinged the survivor
  and quit cleanly; exact close-button automation is unavailable because this
  host lacks Dogtail and pyatspi/AT-SPI. Phase 23H remains the sole NEXT phase.
- 2026-08-31: Reopened after a user-native close reproduced the freeze and
  `Gtk-WARNING: Finalizing GtkEntry ... but it still has children left` for the
  location-completion popover. The repair is not complete until manually
  parented transient widgets are detached before destruction and an actual
  close-one-window native run proves the surviving window remains responsive.

---

# Plan: User-configurable local ClamAV scan limits

## User-authorized scope

Replace the fixed 1 GiB per-file and 16 GiB per-request ClamAV limits with persisted, user-controlled Privacy & Safety settings. Preserve conservative defaults, bounded validation, cancellability, no-follow path handling, local-only scanning, and `clamd`'s independent policy boundary. Do not change malware-verdict language, add cloud scanning, quarantine, or begin Phase 18M.

## Implementation

1. Add bounded preference fields for the per-file MiB limit and total-request GiB limit, including backward-compatible defaults, versioned persistence, corruption clamping, and round-trip tests.
2. Pass an immutable validated limits value with each typed scan request; enforce it in the application worker and report the exact configured per-file limit for skipped files.
3. Add searchable accessible controls under Settings → Operations & Safety, keep total capacity at least one file capacity, persist changes asynchronously, and show immediate confirmation.
4. Update scan reports and user/security documentation to explain user limits, hard ceilings, and `clamd`'s independent `StreamMaxLength`/engine limits.
5. Run focused and full Rust, GTK, documentation, migration, and hygiene gates. Keep exactly Phase 18M as the sole recommended next phase.

## Completion

- 2026-08-31: Complete. Bounded version-18 preferences, immutable request limits, exact skip/report wording, searchable accessible Settings controls, direct report-to-Settings action, user/security documentation, focused sparse-file/fake-clamd regressions, real-GTK accessibility, full Rust/Clippy/migration/docs/diff gates all pass. Phase 18M remains the sole NEXT phase and was not started.
