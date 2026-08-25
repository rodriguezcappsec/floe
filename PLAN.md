# Plan: Floe Phase 7F — Tab/split drag

Mode: sequential solo phase, depth 3.

## Contract

- Accept standard local-file drops on the inactive split pane and resolve the
  authoritative opposite `BrowserSession` path at interaction time.
- Reuse Phase 6R file-list decoding, copy/move/link modifier negotiation,
  no-overwrite FIFO jobs, self-nesting rejection, and accessible feedback.
- Preserve exact `PathBuf` identity. GTK callbacks may submit typed requests but
  may not enumerate or mutate the filesystem.
- Keep explicit Open/Copy/Move and add Link to Other Pane as complete keyboard and
  menu alternatives. Existing stable-ID tab reorder remains intact. Do not add
  tab detachment, Miller-column drag, a second browser pipeline, or hover-open of
  the inactive pane.

## Depth tree

1. Destination contract
   - Exact opposite-side resolver with split/trash gating and non-UTF-8 coverage.
2. Native interaction
   - Inactive-pane drop target using the shared dispatcher and accessible state.
   - Link-to-opposite action plus keyboard/menu alternative.
3. Verification and handoff
   - Focused tests, full gates, native Wayland action/drop-target lifecycle,
     persistent docs, exactly Phase 8A next.

## Status

COMPLETE — all Phase 7F gates verified. Exactly one recommended next phase:
`phase-8a-miller-model`.
