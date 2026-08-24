# Gates: Floe Phase 6K2 daily-driver polish

Scope: Compact customizable sidebar spacing, remembered divider width, native authentication UX, and a structurally aligned Operations Island.

- [x] G1: The dedicated branch starts from completed Phase 6K on main.
  CHECK: git branch --show-current && git merge-base --is-ancestor c98b862 HEAD
  EXPECT: /phase-6k2-daily-driver-polish/
  EVIDENCE: Current branch is `phase-6k2-daily-driver-polish`; completed Phase 6K commit `c98b862` is its ancestor.

- [x] G2: Preferences persist a clamped sidebar width and explicit compact, balanced, or comfortable sidebar density without regressing view/grid preferences.
  CHECK: cargo test -p floe-app phase_6k2_preference -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Four focused tests passed for stable density names, legacy parsing, clamped width, complete-state merge, and bounded latest-state shutdown persistence.

- [x] G3: The sidebar defaults to a denser daily-driver rhythm and exposes accessible live density choices plus reset-width control.
  CHECK: cargo test -p floe-app phase_6k2_sidebar -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Three focused tests passed; native D-Bus exported the stateful density action and reset action, and all three density choices mapped to exact accessible menu labels.

- [x] G4: Pointer divider changes are restored on next launch and persisted through debounced application state rather than synchronous GTK filesystem I/O.
  CHECK: cargo test -p floe-app phase_6k2_sidebar_width -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Two focused tests passed. Persistence is armed after initial allocation, divider changes debounce 320 ms to the bounded worker, the sidebar remains fixed during window resize, and shutdown flushes the newest complete state.

- [x] G5: Copy/move/rename/trash progress and terminal feedback use an aligned Operations Island with bounded metrics, clear hierarchy, and reachable Retry, Resolve Conflict, and Cancel controls.
  CHECK: cargo test -p floe-app phase_6k2_operation_island -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Two geometry tests passed; the 340-pixel island has 12-pixel insets and separate title/cancel, detail, flexible progress, and end-aligned recovery rows.

- [x] G6: Password-protected device mounting uses a window-parented native GtkMountOperation, permits system password/polkit interaction, and never stores or logs credentials.
  CHECK: cargo test -p floe-app phase_6k2_mount_auth -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: One focused policy test passed; runtime delegates mount authentication to `gtk::MountOperation::new(Some(&window))` and Floe owns no password field or credential value.

- [x] G7: Privileged browsing is not faked by running Floe as root; the roadmap defines a GFile-native administrator location boundary before exposing Open as Administrator.
  CHECK: rg -n 'Open as Administrator|GFile|admin://|whole.*root|privileged' README.md DESIGN.md docs/ARCHITECTURE.md docs/ROADMAP.md docs/DEVELOPMENT.md AGENTS.md
  EXPECT: /Open as Administrator/
  EVIDENCE: Persistent docs and `docs/PRIVILEGED_ACCESS.md` define GFile/GVfs `admin://`, polkit delegation, authority-preserving provider/jobs, visible administrator state, and test/rollout gates; the action remains unexposed.

- [x] G8: Video, PDF, office-document, font, text/code, audio-art, and archive thumbnail coverage remains explicitly next on the bounded freedesktop system-thumbnailer branch.
  CHECK: rg -n 'video|PDF|office|DOCX|font|audio|archive|system thumbnail' README.md DESIGN.md docs/ROADMAP.md AGENTS.md
  EXPECT: /system thumbnail/
  EVIDENCE: README, design, roadmap, development, and project status name Phase 6L coverage for video frames, PDF pages, office/DOCX, fonts, text/code, embedded audio artwork, and archive previews.

- [x] G9: Formatting, compilation, strict Clippy, all tests, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: Parent final suite passed formatting, workspace check, strict Clippy, 148 application tests, 33 core tests, and diff hygiene; 181 tests passed with zero failures.

- [x] G10: Native Wayland smoke verifies settings actions, divider restoration, device-monitor health, and clean D-Bus release; operation layout receives explicit structural audit evidence.
  EVIDENCE: Niri smoke exported 26 actions, exercised Balanced/Comfortable/Compact, and remained healthy. Isolated two-launch QA preserved 333 pixels, Reset removed only width, and the reset survived restart/shutdown. Floe exited 0 and released its D-Bus name; screenshot tools were unavailable, so Operations Island evidence is deterministic geometry plus live health rather than a claimed screenshot.

- [x] G11: The phase branch and main are committed, pushed, merged, and synchronized.
  CHECK: git rev-parse main phase-6k2-daily-driver-polish origin/main origin/phase-6k2-daily-driver-polish
  EXPECT: /^([0-9a-f]{40})\n\1\n\1\n\1$/
  EVIDENCE: Feature commit `f2c75cc` was pushed to the phase branch and fast-forwarded to main; the final ledger commit is synchronized to both refs before handoff.

- [x] G12: The status-only gate checker reports every Phase 6K2 gate met.
  CHECK: node /home/rocappsec/.codex/skills/unlazy/scripts/gate-check.mjs --status GATES.md
  EXPECT: /ALL MET/
  EVIDENCE: Final read-only status check reports all 12 root gates met after feature, native QA, documentation, and repository synchronization.
