# Plan: Floe Phase 9A — Preview provider architecture

Mode: sequential phase delivery with explicit implementation and verification gates.

## Contract

- Add a GTK-independent typed provider registry and fixed-capacity Preview
  worker with exact paths, stable request generations, and deterministic order.
- Define explicit source/output/time limits, cooperative cancellation, stale
  result rejection, queue-full/disconnected behavior, and memory-only cache
  policy. Persistent preview cache remains disabled by default and unimplemented.
- Providers receive only the selected target and limits. No shell, network,
  unrelated-file access, active content, or sandbox claim is introduced.
- Connect the Phase 8F Preview handoff to truthful loading, unsupported, failed,
  and cancelled lifecycle states. With no Phase 9B provider, normal requests
  resolve to Provider unavailable rather than fabricated content.
- Keep GTK responsive through the existing bounded worker-drain loop and cancel
  on selection/navigation/mode changes or superseding requests.
- Exclude every format renderer, Space shortcut, fullscreen/polish, and Phase 10.

## Depth tree

1. Provider contract and policy
   - Typed request/outcome/errors, registry ordering, limits, cache policy, and
     exact no-lossy identity.
2. Bounded worker lifecycle
   - Fixed queue, cooperative generation token, stale suppression, clean drop,
     panic/failure containment, and fake-provider tests.
3. Detail-hook integration
   - Submit Preview-ready targets, show truthful lifecycle, drain responses on
     GTK timer, and cancel on selection/navigation/mode exit.
4. Verification and handoff
   - Focused hostile/stale/cancel/limits tests, native Wayland smoke, full
     checks, docs, and exactly Phase 9B as `NEXT`.

## Status

COMPLETE on `phase-9a-preview-providers`. All gates are met. The sole
recommended next phase is `phase-9b-preview-images-text`.
