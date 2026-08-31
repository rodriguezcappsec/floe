# Gates: Phase 23 multi-window reliability and edge-case repair

> Reopened 2026-08-31 after the user reproduced the close-one-window freeze
> together with a `GtkEntry` child-popover finalization warning. R10 and R11
> supersede the earlier incomplete native-close evidence until both pass.

- [x] R10: Closing a window explicitly tears down every manually parented
  window transient before its GTK parent is finalized; the location `GtkEntry`
  cannot retain its completion `GtkPopover` child.
  CHECK: cargo test -p floe-app phase_23_reliability_window_transient_teardown -- --ignored --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: PASS. The focused real-GTK component gate confirms all six
  manually parented browser popovers have parents before teardown and none
  afterward; the window then closes without a child-finalization warning.
- [x] R11: A native Wayland run creates two real windows, closes one real window,
  observes no `Finalizing GtkEntry ... children left` warning, and proves the
  survivor answers an application/window action before clean quit.
  CHECK: bash scripts/native-close-survivor-kde.sh
  EXPECT: /PASS close-survivor-responsive=true third-window=true/
  EVIDENCE: PASS. A guarded KWin helper selected only an exact same-process pair
  of Floe windows and closed one. Forty bounded liveness probes, Refresh on the
  survivor, creation of a third real window, clean quit, and log rejection of
  `Finalizing GtkEntry`/`children left` all passed. The helper unloaded cleanly.

Scope: Eliminate the two-window close freeze and all six confirmed Phase 23 audit defects without beginning Phase 23H.

- [x] R1: Dropping every browser-owned read worker is nonblocking even when its worker thread is deliberately stalled; idle window state is not strongly retained until application shutdown.
  CHECK: cargo test -p floe-app phase_23_reliability_worker_drop -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: PASS. Deliberately gate-blocked MetadataWorker Drop returns within 250 ms; browser-owned filesystem, metadata, preview, thumbnail, search/filter, duplicate, integration, chooser, and presentation workers close channels/cancel then detach rather than joining GTK. Application shutdown captures `Weak<ApplicationState>`.

- [x] R2: A window with active mutating operations rejects close with accessible wait/cancel guidance, while an idle window closes and leaves another window responsive.
  CHECK: cargo test -p floe-app phase_23_reliability_close_policy -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: PASS. Job-manager terminal/nonterminal and browser close-policy regressions pass; guidance explicitly names finish, cancel, and closing this window.

- [x] R3: Failed new-window construction cannot receive or misroute its target to an existing controller, leave a broken blank window, or quit healthy windows.
  CHECK: cargo test -p floe-app phase_23_reliability_window_routing -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: PASS. Build returns its exact new controller; failed builds return `None`, do not present a secondary error window or quit normal application, and routing regression proves no old-window fallback.

- [x] R4: Portal responses accept valid nonmatching local URIs because filters are advisory and preserve selected-filter metadata.
  CHECK: cargo test -p floe-app phase_23_reliability_portal_filter -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: PASS. SaveFile `*.pdf` request accepts explicit `file:///tmp/notes.txt` and returns current-filter index 0.

- [x] R5: Properties checksum dispatch captures its presented authoritative path and cannot retarget after selection changes.
  CHECK: cargo test -p floe-app phase_23_reliability_properties_checksum -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: PASS. Presentation-owned `/tmp/presented-a.txt` remains the checksum target when later live selection is `/tmp/later-selected-b.txt`.

- [x] R6: Equal local job IDs from distinct windows generate distinct stable path-free completion-notification IDs.
  CHECK: cargo test -p floe-app phase_23_reliability_notification_identity -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: PASS. Two allocated window namespaces with local job ID 1 produce unequal IDs and neither contains a path separator.

- [x] R7: Full formatting, check, strict all-target/all-feature Clippy, workspace tests, docs/render/package/release/E2E contracts, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: PASS. Format, workspace check, strict all-target/all-feature Clippy, 646 application tests (627 passed, 19 intentional graphical ignores), 21 controller, 174 core, six duplicate workflows, diff hygiene, strict 21-file docs, render, package layout, migrations, deterministic source, release candidate, and five E2E contracts pass; focused real-GTK teardown and guarded KDE Wayland close-survivor gates pass.

- [x] R8: Applicable native Wayland two-window open/close/liveness behavior is verified with private HOME/XDG roots and a guarded compositor close action.
  EVIDENCE: PASS. `scripts/native-close-survivor-kde.sh` created two real KDE Wayland windows, loaded a temporary KWin helper that closes one member of an exact two-window same-process Floe group, proved sustained main-loop and survivor action responsiveness, created a third real window, quit cleanly, rejected the reported GTK warning, and unloaded the helper.

- [x] R9: AGENTS, roadmap, feature matrix, architecture/user/security documents, plan, gates, and deterministic release checksum truthfully reflect the verified repair and retain exactly one bounded NEXT phase.
  CHECK: python3 scripts/check-docs.py --strict && test "$(rg -c '^\| .*\| NEXT \|' docs/ROADMAP.md)" -eq 1
  EXPECT: /phase-21c-docs-ok/
  EVIDENCE: PASS. Persistent documents describe close ownership, detached-read privacy boundary, advisory filters, exact checksum target, notification namespace and verified native close; strict docs reports 21 files and exactly Phase 23H NEXT. Release source/candidate hash is `114030a5d017976ae42f973c9cb332d1c6fd73735b6b29d8b2d857f7dcf92ca1`.
