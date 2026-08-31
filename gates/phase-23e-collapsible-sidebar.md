# Gates: Floe Phase 23E — Collapsible sidebar

- [x] C1: Collapsed state persists independently from bounded expanded width and
  legacy preferences restore expanded.
  CHECK: `cargo test -p floe-app phase_23e_sidebar_policy -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS.
- [x] C2: Accessible toggle retains destinations as a 56px icon rail, hides only
  visual labels, and resizing collapsed does not overwrite expanded width.
  EVIDENCE: Shared widget tree/CSS keeps accessible button labels and tooltips;
  controller guards Paned position persistence while collapsed.
- [ ] C3: Two-launch semantic collapse/expand/persistence native input passes.
  EVIDENCE: Native launch/action/Ping/Quit passes; no AT-SPI semantic input exists.
ABANDON: C3 Persistence and widget policy are deterministic-tested; native
collapse activation cannot be automated without Dogtail/pyatspi on this host.
