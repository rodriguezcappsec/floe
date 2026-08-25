# Gates: Floe Phase 7B Tab interaction

Status: COMPLETE

- [x] G1: Work is isolated on `phase-7b-tabs-interaction`; no closed-tab store,
  startup persistence, split view, detached window, or future phase code exists.
  CHECK: git branch --show-current
  EXPECT: phase-7b-tabs-interaction
  EVIDENCE: Branch command returned `phase-7b-tabs-interaction`.

- [x] G2: A bounded GTK-independent tab collection provides stable-ID new,
  activate, duplicate, close, and deterministic reorder behavior without
  duplicating widget or worker ownership.
  CHECK: cargo test -p floe-core phase_7b_tabs -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Three focused core tab tests passed.

- [x] G3: Switching, history navigation, and active-tab close capture and restore
  exact path, multi-selection, path/index scroll anchor, and complete view policy.
  CHECK: cargo test -p floe-app phase_7b_session -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Session snapshot and core transitions preserve raw paths, complete
  view state, selection, and anchor.

- [x] G4: The native tab strip exposes labelled active tabs, visible close/new
  controls, pointer reorder, middle-click close, and keyboard switch/reorder
  alternatives with stable focus and non-color-only active semantics.
  CHECK: cargo test -p floe-app phase_7b_tab_ui -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Focused UI contract passed; native actions exercised switch, reorder,
  duplicate, and close while the application remained D-Bus responsive.

- [x] G5: Folders can open in foreground or background tabs from list/grid;
  middle click opens in background and never launches files or steals focus.
  CHECK: cargo test -p floe-app phase_7b_folder_tabs -- --nocapture
  EXPECT: test result: ok
  EVIDENCE: Folder-only/trash/file/multi-selection policy passed and both
  virtualized factories install middle-click background activation.

- [x] G6: GTK callbacks submit bounded state commands only; exact `PathBuf`
  identity remains authoritative and the existing shared worker/model/watcher/job
  architecture remains single-owned and responsive.
  EVIDENCE: Dormant tabs contain only `BrowserSession` values; code review found
  one controller-owned worker/model/watcher set and no display-text path parsing.

- [x] G7: Formatting, workspace check, strict all-target/all-feature Clippy,
  workspace tests, diff hygiene, and native Wayland action/focus/lifecycle smoke pass.
  CHECK: cargo fmt --all -- --check
  EXPECT: /^$/
  EVIDENCE: Formatting, check, strict Clippy, 304 tests, and diff hygiene passed.
  Native Wayland actions stayed healthy, quit cleanly, and released the name;
  only the documented RADV swapchain warning appeared.

- [x] G8: Project docs mark verified Phase 7B complete and exactly Phase 7C as
  `NEXT`, without claiming persistence or split view.
  CHECK: rg -n '7B — Tab interaction.*COMPLETE|7C — Closed tabs/restore.*NEXT' docs/ROADMAP.md
  EXPECT: 7C — Closed tabs/restore
  EVIDENCE: Roadmap, matrix, design, architecture, development, privacy/security,
  plan, gates, and AGENTS mark 7B complete and exactly 7C next.

Recommended next phase: `phase-7c-tab-session-restore`.
