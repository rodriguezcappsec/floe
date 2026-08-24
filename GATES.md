# Gates: Floe Phase 6G iconography polish

Scope: Replace weak generic folder/file presentation with one cohesive, accessible, theme-aware icon system shared by Floe's list and grid while retaining thumbnail and exact-path behavior.

- [x] G1: Work is isolated on `phase-6g-iconography-polish` from completed Phase 6F commit `a70933d`.
  CHECK: git branch --show-current && git merge-base --is-ancestor a70933d phase-6g-iconography-polish && git rev-parse --short a70933d
  EXPECT: /phase-6g-iconography-polish[\s\S]*a70933d/
  EVIDENCE: Branch check prints `phase-6g-iconography-polish`; `a70933d` is its verified Phase 6F ancestor.

- [x] G2: A GTK-independent semantic icon policy covers directories, regular files, executables, symlinks, and reviewed extension families without deriving paths from display text.
  CHECK: rg -n 'EntryIcon|IconFamily|icon_for_entry|Directory|Executable|Symlink|Archive|Audio|Code|Document|Image|Pdf|Video' crates/app/src
  EXPECT: /EntryIcon/
  EVIDENCE: `EntryIcon` covers fourteen families; enumerated kind/executable metadata precedes extension classification on original paths.

- [x] G3: Floe owns one coherent scalable vector icon family with deliberate folder/file silhouettes, consistent view boxes/strokes, and no emoji, raster, or theme-dependent generic icons.
  CHECK: find crates/app -type f -path '*icons*' -name '*.svg' -print | sort && rg -n '<svg|#[0-9a-fA-F]{6}|alias=.*floe' crates/app -g '*.svg' -g '*.xml'
  EXPECT: /#78c7f2/
  EVIDENCE: Fourteen 48x48 SVG sources compile to non-symbolic resource aliases with shared folder/page geometry and one restrained palette.

- [x] G4: List and grid factories use the same semantic policy, stable optical-size tokens, and shared fallback behavior; real thumbnails still replace generic icons only when available.
  CHECK: rg -n 'entry_icon|LIST_ICON|GRID_ICON|set_from_icon_name|apply_thumbnail|clear_thumbnail' crates/app/src/ui.rs crates/app/src/browser.rs crates/app/src/appearance.rs
  EXPECT: /entry_icon/
  EVIDENCE: Both factories call `apply_entry_icon`; list uses 28 pixels, grid 48-88, and `apply_thumbnail` preserves independent requested edges.

- [x] G5: Folder/file meaning and selection/focus remain understandable without color alone, decorative glyphs do not duplicate accessible file names, and theme state uses semantic GTK colors.
  CHECK: rg -n 'accessible|Presentation|selected|focus|opacity|accent|floe-entry-icon' crates/app/src crates/app -g '*.svg'
  EXPECT: /Presentation/
  EVIDENCE: Distinct silhouettes accompany visible names/types; images use Presentation role and semantic GTK selected/focus styling with opacity parity.

- [x] G6: Unclassified filenames and unavailable thumbnails retain a stable embedded generic file fallback without filesystem I/O, decoding, or unbounded per-row work in GTK callbacks.
  CHECK: rg -n 'Fallback|Generic|icon_for_entry|from_entry|set_icon_name|ThumbnailKey' crates/app/src
  EXPECT: /Generic/
  EVIDENCE: Unclassified/invalid extensions return `EntryIcon::Generic`; embedded resources and pre-enumerated metadata keep bind work bounded and I/O-free.

- [x] G7: Focused Phase 6G tests cover semantic families, case-insensitive extensions, non-UTF-8 names, directory/symlink/executable precedence, fallback, list/grid size tokens, and resource registration.
  CHECK: cargo test -p floe-app phase_6g -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: Focused command passes five Phase 6G tests covering all listed behaviors and fourteen resource aliases.

- [x] G8: README, design, architecture, development, roadmap, and AGENTS status describe implemented iconography and identify `phase-7a-tabs-foundation` as the next branch.
  CHECK: rg -n 'Phase 6G|phase-7a-tabs-foundation|iconography' README.md DESIGN.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /phase-7a-tabs-foundation/
  EVIDENCE: All six persistent documents describe Phase 6G; roadmap and AGENTS name `phase-7a-tabs-foundation` next.

- [x] G9: Formatting, workspace compilation, strict Clippy, all tests, and diff hygiene pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: Formatting, workspace check, strict Clippy, 101 application plus 33 core tests, and diff hygiene all pass.

- [x] G10: Native Wayland list/grid smoke shows the new folder and representative file-family icons, preserves thumbnail replacement, owns the expected D-Bus name, and exits cleanly from isolated temporary roots.
  EVIDENCE: Settled list/grid captures showed all representative vectors and WebP pixels; Floe owned D-Bus, stayed healthy, cached normal/large thumbnails, exited 0, released its name, and all temporary artifacts were removed.

- [ ] G11: Phase 6G is pushed, fast-forwarded into `main`, and local/remote phase and main refs are identical.
  CHECK: git rev-parse main phase-6g-iconography-polish origin/main origin/phase-6g-iconography-polish
  EXPECT: /^([0-9a-f]{40})\n\1\n\1\n\1$/
  EVIDENCE: Pending publication.

- [ ] G12: The unlazy gate checker reports all Phase 6G gates met after publication.
  CHECK: node /home/rocappsec/.codex/skills/unlazy/scripts/gate-check.mjs GATES.md
  EXPECT: /ALL MET/
  EVIDENCE: Pending.
