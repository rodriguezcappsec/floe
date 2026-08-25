# Plan: Floe Phase 8C — Miller keyboard and trackpad navigation

Mode: sequential phase delivery with explicit implementation and verification gates.

## Contract

- Add focus-visible Up/Down item movement and Left/Right column movement to the
  Phase 8B Miller surface without changing list/grid shortcuts.
- Keep navigation state in `MillerColumnModel`/application state. Recycled GTK
  rows may present focus but must not become path authority.
- Add smooth horizontal wheel/trackpad scrolling while retaining ordinary
  vertical scrolling inside each column.
- Respect widget text direction: logical forward/backward directory movement
  must map correctly in LTR and RTL layouts.
- Honor GTK animation settings and avoid custom motion when reduced motion is
  active. Navigation must remain usable without animation.
- Exclude optional Vim bindings (11D), column context actions (8D), drag/drop
  (8E), detail hooks (8F), and Preview content (9).

## Depth tree

1. Direction/focus policy
   - Define GTK-independent logical direction mapping, selection movement,
     bounds, RTL, and reduced-motion decisions.
2. Native navigation controllers
   - Bind Up/Down/Left/Right, Home/End, and horizontal scroll behavior to exact
     active-column state with non-color-only focus descriptions.
3. Lifecycle integration
   - Preserve focus through column recycling/navigation and keep text-entry,
     list/grid, tabs, and split shortcuts isolated.
4. Verification and handoff
   - Focused policy tests, native Wayland smoke, full workspace checks,
     persistent docs, and exactly Phase 8D as `NEXT`.

## Status

COMPLETE on `phase-8c-miller-navigation`. All gates are met. The sole
recommended next phase is `phase-8d-miller-actions`.
