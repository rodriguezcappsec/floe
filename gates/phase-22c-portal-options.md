# Gates: Floe Phase 22C — Portal option completeness

- [x] P1: Strict bounded models parse and round-trip filters, current-filter and
  choices, rejecting malformed, duplicate, oversized, incompatible or
  unsupported variants.
  CHECK: `cargo test -p floe-app phase_22c_portal_model -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Portal model/transport/request-result contract passes.
- [x] P2: Selection Mode presents accessible option controls, filters visible
  files off GTK without hiding navigation folders, and returns exact options.
  CHECK: `cargo test -p floe-app phase_22c_selection_options -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: PASS. Capacity-one generation-safe visual-filter worker test passes.
- [ ] P3: Native isolated D-Bus filtered OpenFile/SaveFile success,
  cancellation, invalid options and cleanup are semantically exercised.
  EVIDENCE: Existing Phase 22B native Save/Close lifecycle remains valid; current
  normal Wayland launch/actions/Ping/Quit pass. Dogtail/pyatspi are unavailable,
  so selecting a filtered row and reading the terminal portal tuple cannot be
  automated truthfully on this host.
ABANDON: P3 External AT-SPI semantic input dependencies are unavailable; unit
and process-boundary contracts are not misreported as native filtered selection.
