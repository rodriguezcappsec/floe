# Gates: Phase 6K2 documentation and native QA

Scope: Truthful daily-driver customization, security, thumbnail roadmap, full tests, and native verification.

- [x] L1: Persistent docs record implemented density/width/operation/auth behavior without claiming privileged browsing is complete.
  CHECK: rg -n 'sidebar density|sidebar width|GtkMountOperation|Open as Administrator|admin://' README.md DESIGN.md docs/ARCHITECTURE.md docs/ROADMAP.md docs/DEVELOPMENT.md AGENTS.md
  EXPECT: /Open as Administrator/
  EVIDENCE: README.md:29-36,172-195; DESIGN.md:44-72,183-194,325-336; docs/ARCHITECTURE.md:251-264,431-453; docs/ROADMAP.md:301-315; docs/DEVELOPMENT.md:192-225; AGENTS.md:1257-1274 document density, persistence/reset, aligned operation rows, credential-opaque native mount prompts, and intentionally unexposed administrator design.

- [x] L2: System-thumbnailer scope explicitly names video, PDF, office/DOCX, fonts, audio art, text/code, and archives.
  CHECK: rg -n 'video|PDF|DOCX|font|audio|text|archive' README.md DESIGN.md docs/ROADMAP.md AGENTS.md
  EXPECT: /DOCX/
  EVIDENCE: README.md:187-191, DESIGN.md:320-324, docs/ROADMAP.md:318-326, and AGENTS.md:1303-1308 name video frames, PDF pages, office/DOCX, fonts, text/code, embedded audio artwork, and archive previews.

- [x] L3: Full quality suite and measured test counts pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: 2026-08-24 full gate passed formatting, workspace check, strict Clippy, git diff check, 148 application tests, 33 core tests, and 0 doc tests; 181 non-doc tests total with zero failures. Ten focused Phase 6K2 tests also passed.

- [x] L4: Native Wayland and operation-layout audit evidence is recorded.
  EVIDENCE: Niri action smoke exported 26 actions including sidebar-density/reset-sidebar-width, applied Balanced/Comfortable/Compact, answered Peer.Ping, quit 0, and released D-Bus ownership. Isolated two-launch smoke held Comfortable width 333 through allocation; Reset removed only width; a second no-width launch kept it absent through shutdown. Both isolated runs answered Peer.Ping, quit 0, and released ownership. Screenshot tools were unavailable; deterministic operation-layout tests prove 340px/12px row geometry and native health/persistence evidence is recorded in docs/DEVELOPMENT.md:211-233 and AGENTS.md. Only documented host warnings appeared.
