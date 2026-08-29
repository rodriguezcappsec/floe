# Gates: Floe Phase 20B2 — Completeness Audit

Scope: integration ledger for the ten selected user-visible completeness
outcomes. The authoritative runnable gates and evidence live at the top of
`GATES.md` as Q1-Q11.

- [x] I1: Q1–Q5 browsing, layout, activation, selection, and column gates pass.
  EVIDENCE: All five focused command filters pass; full core/app workspace suite and session-v3 migration pass.

- [x] I2: Q6–Q9 appearance, focus, accessibility, and scaling gates pass.
  EVIDENCE: All four deterministic filters and two focused real-GTK Phase 20B2 component contracts pass on active Wayland.

- [x] I3: Q10 feedback/localization/RTL gate passes.
  EVIDENCE: Bounded per-toast details, privacy-generic completion notification, message-ID, RTL isolation tests pass.

- [x] I4: Q11 full integration/documentation/native gate passes.
  EVIDENCE: Strict Rust/build/diff gates, documentation/status, three E2E contracts, isolated Wayland Ping/Quit pass; Dogtail/pyatspi absence recorded.
