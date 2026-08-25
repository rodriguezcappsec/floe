# Plan: Floe Phase 6Q Create, duplicate, and links

Mode: sequential solo phase, depth 4.

## Contract

- `floe-core` owns exact-path, no-overwrite creation requests and execution for directories, empty files, template copies, symbolic links, and hard links.
- The application layer owns bounded execution, retries, duplicate batching/naming, native dialogs, clipboard presentation, and asynchronous link-target resolution.
- GTK callbacks submit typed application commands or asynchronous GIO requests only. No filesystem mutation or blocking metadata work runs on the GTK main loop.
- Names remain `OsString`; original targets and sources remain `PathBuf`. UI text is authoritative only when the user explicitly enters a new UTF-8 filename.
- Creation never uses a shell, never elevates Floe, never overwrites, and never follows a source symlink for hard-link eligibility.
- Template selection uses the native asynchronous file dialog, initially rooted at the XDG Templates directory when available; actual creation runs through the bounded executor.
- Duplicate uses deterministic bounded sibling naming and preserves symlinks rather than following them.
- Symbolic links may deliberately be broken. Hard links are limited to regular non-symlink files and kernel-enforced same-filesystem semantics.
- Reveal Link Target reads the stored target asynchronously, preserves raw target identity, resolves relative targets lexically, and navigates without executing the target.
- Copy Name/Path/Relative Path rejects values that cannot be represented losslessly as UTF-8 text. Copy URI uses exact percent-encoded local-file identity.
- Phase 6R drag-and-drop remains out of scope until Phase 6Q is verified and merged.

## Depth tree

1. Core creation semantics
   - Add validated request/kind/outcome/error models.
   - Implement no-overwrite directory/file/template/symbolic/hard-link execution.
   - Cover collision, broken link, same-filesystem, symlink, and raw non-UTF-8 behavior.
2. Application execution and state
   - Add fixed-capacity creation executor and shared job lifecycle integration.
   - Add batch duplicate submission, retry/conflict destination revision, and bounded outcome tracking.
3. Desktop actions and presentation
   - Add New Folder/File/From Template dialogs and selection/background menu actions.
   - Add Duplicate, symbolic/hard link, Reveal Link Target, and copy name/path/relative/URI actions with truthful sensitivity.
4. Verification and documentation
   - Add focused core/application/UI tests and native Wayland action smoke.
   - Run full formatting, build, strict Clippy, tests, and diff gates.
   - Update persistent documentation and mark exactly Phase 6R `NEXT`.

## Status log

- 2026-08-24: Created `phase-6q-create-duplicate-links` from verified Phase 6P main at `de3cdc9`.
- 2026-08-24: Read project instructions and inspected current core copy/move models, bounded executors, application state, browser actions, menus, clipboard, navigation, and GTK dependencies.
- 2026-08-24: Defined Phase 6Q contract and executable gates before coding.
- 2026-08-24: Implemented and focused-tested core creation, bounded executor/state integration, duplicate batching/conflicts, native actions/dialogs, asynchronous reveal, and exact clipboard text/URI policy.
- 2026-08-24: Full formatting, workspace check, strict Clippy, 265 tests, diff hygiene, and isolated native Wayland 42-action/dialog/health smoke passed.
- 2026-08-24: Updated persistent documentation and set exactly Phase 6R drag and drop as `NEXT`; Phase 6R code was not started.

## Status

COMPLETE — next: `phase-6r-drag-drop`
