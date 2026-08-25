# Plan: Floe Phase 6T Browser completeness

Mode: sequential solo phase, depth 5.

## Contract

- Preserve the shared virtualized list/grid model, exact `PathBuf`/`OsString`
  identity, 256-entry insertion batches, watcher reconciliation, selection,
  scroll anchors, context menus, and drag/drop.
- Add extension sorting, independent type/extension grouping, configurable
  folder placement, Compact/Comfortable/Spacious density, optional list columns,
  and clamped pointer plus keyboard/menu column resizing.
- Request MIME, Created, Accessed, and Permissions only for bound rows through a
  fixed-capacity worker and bounded presentation cache. Metadata arrival must
  never reorder the directory.
- Migrate legacy view preferences; persist complete global policy plus opt-in,
  capped exact raw-path per-folder overrides outside GTK callbacks.
- Report only known non-recursive bytes. Query current-location and mounted-local
  device size/free/read-only facts asynchronously through bounded GIO work with
  generation and identity rejection.
- Exclude tabs, tab widgets, session restore, Inspector, recursive folder sizes,
  owner-name lookup, media metadata, search, and future settings UI.

## Depth tree

1. Core view policy
   - Extension sort and raw identity.
   - Directory first/last independent from sort direction.
   - None/Type/Extension grouping independent from sorting.
2. Bounded enrichment and presentation
   - Capacity-64 metadata worker, 512-result cache, exact source identity.
   - Nine stable columns, optional visibility, clamped widths, accessible sort
     state, recycled-row-safe bindings.
   - Shared list/grid density classes without density-only factory replacement.
3. Preference scope
   - Version-2 migration preserving legacy view/grid/sidebar values.
   - Global policy plus opt-in 256-entry raw-path folder overrides.
4. Status and storage
   - Known selected/visible bytes without recursive directory size claims.
   - Capacity-32 GIO storage worker for current location and mounted devices.
5. Verification and handoff
   - Focused and full Rust gates, diff audit, native two-launch Wayland smoke.
   - Update architecture, design, privacy/security, roadmap, matrix, and status.

## Evidence

- Focused Phase 6T suites: 3 core sorting/grouping tests and 17 application
  metadata/column/density/preference/status tests passed.
- Full workspace: 228 application plus 66 core tests passed (294 total).
- `cargo fmt --all -- --check`, workspace check, strict all-target/all-feature
  Clippy, and `git diff --check` passed.
- Native Niri/Wayland smoke exported 73 actions, exercised density, grouping,
  directory placement, optional columns, and width adjustment, restored their
  state on a second isolated launch, answered D-Bus `Peer.Ping`, quit cleanly,
  and released the application name twice.
- Spectacle produced no capture on this host. Runtime action/persistence/health
  evidence is recorded in `docs/DEVELOPMENT.md`.

## Status

COMPLETE — exactly one recommended next phase: `phase-7a-tabs-foundation`.
