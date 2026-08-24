# Plan: Floe Phase 6K2 daily-driver polish

Depth: tree 4

Mode: orchestrated

Budget note: This phase crosses preference persistence, GTK layout/state, operation feedback, authentication UX, documentation, and native runtime verification.

## Contract

- Interfaces: `preferences` owns parsed, clamped, asynchronously persisted sidebar density and width; `ui` owns presentation; `browser` owns debounced user-intent wiring; `operations` owns feedback state only.
- Data ownership: preference leaf owns `preferences.rs`; integration leaf owns `appearance.rs`, `application.rs`, `browser.rs`, `operations.rs`, and `ui.rs`; documentation leaf owns persistent Markdown files and AGENTS status. No concurrent leaf edits overlap.
- Density: three explicit choices use a 4/8-based rhythm. Compact is the daily-driver default; Balanced and Comfortable remain available.
- Width: authoritative values are clamped to a safe desktop range and persisted off the GTK thread. Divider drags coalesce rather than writing every pixel.
- Authentication: encrypted/password-protected storage continues through window-parented `GtkMountOperation`. Floe never handles, stores, or logs the password itself.
- Privilege: never run the whole GTK application as root. A future Open as Administrator action requires GFile-native `admin://` browsing and operation boundaries before it is exposed.
- Thumbnails: system thumbnailer work remains the immediately following dedicated branch for video, PDF, office, font, audio, text/code, and archive formats.

## Tree

- 1 Phase 6K2 daily-driver polish ................. `GATES.md`
  - 1.1 Preference foundation ...................... `gates/node-6k2-1.1.md`
    - 1.1.1 Sidebar preference model/persistence ... `gates/leaf-6k2-1.1.1.md`
    - 1.1.2 Privileged browsing security design .... `gates/leaf-6k2-1.1.2.md`
  - 1.2 GTK and operation integration .............. `gates/node-6k2-1.2.md`
    - 1.2.1 Sidebar/settings/auth/feedback UI ....... `gates/leaf-6k2-1.2.1.md`
  - 1.3 Product integration ........................ `gates/node-6k2-1.3.md`
    - 1.3.1 Documentation/native QA ................ `gates/leaf-6k2-1.3.1.md`

## Status log

- 2026-08-24 plan written and contracts fixed before implementation.
- 2026-08-24 preference and privileged-browsing design leaves started in parallel.
- 2026-08-24 parent verification accepted preference, privileged-access design, and GTK integration leaves; node 1.1 and 1.2 gates are complete.
- 2026-08-24 documentation/native QA completed after an isolated Niri smoke found and fixed startup allocation rewriting sidebar width; node 1.3 is complete.
