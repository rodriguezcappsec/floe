# Gates: Floe Phase 18N/18N2 — Suspicious files and local threat scanning

Scope: explainable local filename/type/executable findings plus optional bounded
ClamAV `clamd` scanning; no cloud, execution, deletion, quarantine, or safety claim.

- [x] N1: GTK-independent suspicious analysis handles raw non-UTF-8 names,
  double extensions, MIME mismatch, executability, desktop/AppImage/script types,
  bidi/invisible/control hazards, and conservative false-positive cases.
  CHECK: `cargo test -p floe-core phase_18n_suspicious -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Three core tests cover double extensions, MIME/executable evidence, scripts/AppImages/desktops, bidi/control/raw-byte display, and compound-extension false positives.

- [x] N2: ClamAV protocol/discovery streams bounded no-follow regular files to a
  reviewed Unix socket, parses fragmented/malformed responses, supports
  cancellation/timeouts, revalidates identities, and never links `libclamav` or
  invokes a shell.
  CHECK: `cargo test -p floe-app phase_18n_clamav -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Five focused tests capture exact fragmented `VERSION`/`INSTREAM` bytes in memory, parse OK/finding/error/malformed responses, reject symlinks, preserve cancellation, and capability-gate the real pathname connector. This host denies Unix bind with `Operation not permitted`, which is reported as a skip rather than a false pass.

- [x] N3: Scan setup supports selected files and bounded local folder recursion;
  results separate Detected, No known signature, Not scanned, Limit, Changed,
  Cancelled, and Error, remain memory-only, and offer review/reveal/ordinary Trash
  without automatic mutation.
  CHECK: `cargo test -p floe-app phase_18n_scan_workflow -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Focused workflow scans two regular files, records one signature, retains the symlink as not scanned, and never follows it. Source/work totals, depth, device, and retained-result caps remain deterministic constants covered by the full suite.

- [x] N4: Context/Properties/Inspector actions are selection-aware, accessible,
  and use evidence-based wording rather Clean, Safe, antivirus, or malware-free.
  CHECK: `cargo test -p floe-app phase_18n_ui -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Focused command contract verifies Inspect, local ClamAV, and Cancel commands are human-named/searchable, available in File Context and Header Menu, identify separately installed `clamd`, and state that no-signature is not proof of safety. Native launch exposed the menu without markup warnings after the label fix.
