# Gates: Leaf 1.4.1 - Phase 6K documentation and final QA

Scope: Document only verified Phase 6K behavior in the assigned persistent
documentation, update project status, run the full quality suite, and record a
native Wayland smoke without changing Rust source or parent ledgers.

- [x] L1: README and design documentation describe the compact, vertically scrollable, user-resizable Places/Bookmarks/Devices sidebar and all distinct existing XDG folders.
  CHECK: rg -n 'Places, Bookmarks, and Devices|vertically scrollable|user-resizable|Desktop|Documents|Downloads|Music|Pictures|Public Share|Templates|Videos' README.md DESIGN.md
  EXPECT: /Places, Bookmarks, and Devices/
  EVIDENCE: README.md:25-34 and DESIGN.md:38-60 name all eight XDG kinds, exact distinct/existing policy, three sections, vertical scrolling, compact minimum, and current-window resizing.

- [x] L2: Architecture and development documentation accurately describe exact raw-path, private atomic bookmark persistence outside GTK callbacks and GIO VolumeMonitor snapshots/signals/actions.
  CHECK: rg -n 'bookmarks\.bin|raw path|0o700|0o600|atomic|VolumeMonitor|mount|unmount|eject|Busy|busy' docs/ARCHITECTURE.md docs/DEVELOPMENT.md
  EXPECT: /VolumeMonitor/
  EVIDENCE: docs/ARCHITECTURE.md:403-428 records raw Unix bytes, validation boundaries, fixed-capacity worker, 0600 temporary atomic replacement, 0700 parent, VolumeMonitor signals/snapshots, and asynchronous actions; docs/DEVELOPMENT.md:170-183 records runtime/test behavior.

- [x] L3: Roadmap and project status mark Phase 6K complete, identify Phase 6L as next, and explicitly defer remote-root browsing and permanent deletion.
  CHECK: rg -n 'Phase 6K.*[Cc]omplete|Phase 6L|Remote|remote|Permanent|permanent' README.md docs/ROADMAP.md AGENTS.md
  EXPECT: /Phase 6L/
  EVIDENCE: docs/ROADMAP.md:285-304 marks 6K complete and 6L next; AGENTS.md:1132,1289-1294 marks current status/next work; README.md:165-177 and roadmap Phases 6M/17 retain deletion and remote-root deferrals.

- [x] L4: Documentation does not overclaim device behavior: local mounted roots navigate, remote roots remain unavailable, and mount/unmount/eject busy/failure states remain visible and recoverable.
  CHECK: rg -n 'local mounted|local filesystem|Remote.*not supported|remote.*deferred|busy|failure|recover' README.md DESIGN.md docs/ARCHITECTURE.md docs/ROADMAP.md docs/DEVELOPMENT.md AGENTS.md
  EXPECT: /remote/
  EVIDENCE: DESIGN.md:55-61, docs/ARCHITECTURE.md:421-428, docs/DEVELOPMENT.md:178-183, and README.md:165-172 distinguish local navigation, non-local deferral, busy/unavailable row states, and failed-action feedback.

- [x] L5: Four expert passes were completed with the fourth finding no remaining documentation defect.
  EVIDENCE: Pass 1 inspected implementation and drafted docs; pass 2 removed stale Phase 6K-next language and corrected bookmark decode versus submission validation; pass 3 aligned UI architecture and measured 19 focused tests; pass 4 refined transient failure wording, reran all documentation searches/diff check, and found no remaining stale or overstated Phase 6K claim.

- [x] L6: Full formatting, build, strict Clippy, application/core tests, and diff checks pass with measured test counts recorded.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: Combined command exited 0: fmt check, workspace check, strict all-target Clippy, 138 floe-app tests, 33 floe-core tests (171 total), zero failures, doc tests, and git diff check. Focused `phase_6k` run measured 19 passed and zero failed.

- [x] L7: Native Wayland smoke confirms application ownership, startup health, Phase 6K action registration/activation as applicable, clean Quit, and D-Bus name release; only known host warnings are accepted.
  EVIDENCE: Built target/debug/floe-app launched on active Wayland, owned io.github.floe.FileManager, exported 24 window actions, safely activated refresh, answered Peer.Ping afterward, accepted quit, exited 0, and released its D-Bus name; only known host libadwaita/RADV warnings appeared.

- [x] L8: Only assigned documentation and this leaf ledger were edited by this leaf.
  CHECK: git status --short -- README.md DESIGN.md docs/ARCHITECTURE.md docs/ROADMAP.md docs/DEVELOPMENT.md AGENTS.md gates/leaf-1.4.1.md
  EXPECT: /leaf-1.4.1.md/
  EVIDENCE: Scoped status lists only AGENTS.md, DESIGN.md, README.md, docs/ARCHITECTURE.md, docs/DEVELOPMENT.md, docs/ROADMAP.md, and gates/leaf-1.4.1.md; this leaf made no Rust, root GATES, PLAN, or node-ledger edit.
