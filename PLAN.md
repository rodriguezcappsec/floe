# Plan: Floe Phase 6L system thumbnailers

Mode: solo, depth 3. The phase has one architectural seam—system provider
discovery/execution feeding the existing thumbnail result/cache boundary—and is
kept separate from future Preview and sandbox phases.

## Contract

- Discover freedesktop `.thumbnailer` providers from standards-based data
  directories with deterministic precedence.
- Resolve content type and execute providers only on the thumbnail worker, never
  in GTK callbacks.
- Parse provider command lines into argv and substitute reviewed thumbnailer
  field codes without a shell or filename interpolation.
- Supervise helpers with exact input identity, private temporary output,
  cancellation, timeout, output limits, source revalidation, and cleanup.
- Validate/decode provider output through existing image/cache limits and retain
  generic icon fallback for unsupported, malformed, failed, stale, or busy work.
- Do not add Quick Preview, active content, provider installation, a sandbox
  dependency, or a claim that Phase 6L providers are sandboxed.

## Tree

1. Provider policy and discovery
   - Inspect freedesktop thumbnailer files, current worker/cache contracts, and
     available GLib/GIO process/content-type APIs.
   - Add GTK-independent parsing, precedence, MIME selection, and argv expansion.
2. Supervised execution and integration
   - Add private temporary output, timeout/cancellation/process termination,
     bounded output decode, source revalidation, cache write, and fallback.
   - Integrate system providers after native raster handling without disturbing
     current list/grid presentation or cache reuse.
3. Verification and handoff
   - Cover hostile definitions, MIME selection, non-UTF-8 paths, malformed and
     oversized output, timeout, cancellation, stale sources, and cleanup.
   - Run formatting, workspace check, strict Clippy, all tests, and native
     Wayland smoke with a controlled provider fixture.
   - Update project status, roadmap, matrix, gates, architecture/security docs;
     mark 6L complete only after every gate passes and make 6M the sole next.

## Status log

- 2026-08-24: Phase 6L identified as the sole NEXT phase; branch, contract, and
  gates established before implementation.
- 2026-08-24: Provider discovery/policy, no-shell supervised process execution,
  process-group cancellation, bounded PNG validation, and worker/cache
  integration completed with eleven focused tests.
- 2026-08-24: Full Rust verification passed with 159 application and 33 core
  tests. Two isolated native Wayland launches proved one provider execution,
  persistent cache reuse while the provider failed, clean D-Bus release, and no
  leftover provider temporary directories.
- 2026-08-24: Persistent status, roadmap, feature matrix, architecture, and
  privacy/security documentation updated; Phase 6M is the sole recommended next
  phase and no Phase 6M implementation was started.
