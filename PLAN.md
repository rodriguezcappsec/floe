# Plan: Floe Phase 8E — Miller cross-column drag/drop

Mode: sequential phase delivery with explicit implementation and verification gates.

## Contract

- Make active and retained Miller columns exact local-file drag sources and
  directory drop destinations without adding a second browser pipeline.
- Reuse the existing GDK file-list payload, copy/move/link negotiation,
  `DropRequest` validation, bounded FIFO jobs, and no-overwrite behavior.
- Resolve tab, split-pane, sidebar, device, and Miller destinations from
  authoritative application state at interaction time; never from labels.
- Add one cancellable bounded hover-open path for directory, tab, opposite-pane,
  and Miller child targets. Leaving, dropping, navigation, and shutdown cancel it.
- Preserve vertical edge autoscroll and add clamped horizontal edge autoscroll
  for the Miller strip. Feedback must name action and destination in text.
- Reject non-local, empty, same-destination, self-nesting, stale-column, and
  unavailable destination drops without direct GTK filesystem mutation.
- Exclude Phase 8F detail hooks and all Preview providers/content.

## Depth tree

1. Typed hover/drop policy
   - Replace boolean hover ownership with typed exact targets and bounded
     cancellation; extend clamped edge scrolling to both axes.
2. Miller sources and destinations
   - Bind recycled active/retained selection models to file-list drag sources,
     folder/background destinations, exact logical depth, and hover activation.
3. Cross-surface integration
   - Add live tab destinations and hover activation; preserve sidebar, mounted
     device, and split-pane destination semantics with explicit ownership.
4. Verification and handoff
   - Focused hostile-path/stale/modifier/autoscroll tests, native Wayland action
     lifecycle smoke, full checks, docs, and exactly Phase 8F as `NEXT`.

## Status

COMPLETE on `phase-8e-miller-drag-drop`. All gates are met. The sole
recommended next phase is `phase-8f-miller-detail-hooks`.
