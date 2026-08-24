# Gates: Floe Phase 6E thumbnail-cache polish

Scope: Add a standards-conscious, bounded persistent image-thumbnail cache while preserving exact path identity, bounded asynchronous work, and safe generic fallbacks.

- [x] G1: Work was isolated on `phase-6e-thumbnail-cache-polish` from the completed Phase 6D commit before publication.
  CHECK: git branch --show-current && git merge-base --is-ancestor 7e87f88 phase-6e-thumbnail-cache-polish && git rev-parse --short 7e87f88
  EXPECT: /phase-6e-thumbnail-cache-polish[\s\S]*7e87f88/
  EVIDENCE: Branch is `phase-6e-thumbnail-cache-polish`; local `main` remains at `7e87f88` before publication.

- [x] G2: Cache identity uses a canonical absolute file URI, MD5 digest, exact source metadata, and freedesktop `normal`/`large` tier mapping without reconstructing paths from display text.
  CHECK: rg -n 'ThumbnailCacheKey|canonical_uri|md5|CacheTier|Thumb::URI|Thumb::MTime|Thumb::Size' crates/app/src/thumbnail_cache.rs
  EXPECT: /Thumb::URI/
  EVIDENCE: `ThumbnailCacheKey` preserves exact-path URI identity, GLib MD5, source size/mtime, and standard tier selection; focused non-UTF-8 and tier tests pass.

- [x] G3: Cache validation rejects missing or mismatched metadata, corrupt or oversized PNGs, symlinked cache entries, and stale source identity, then safely falls back to source decoding.
  CHECK: rg -n 'NOFOLLOW|MAX_CACHE_FILE_BYTES|SourceChanged|decode_thumbnail_with_cache' crates/app/src/thumbnail_cache.rs crates/app/src/thumbnail.rs
  EXPECT: /NOFOLLOW/
  EVIDENCE: Source and cache files use `O_NOFOLLOW`; metadata mismatch, corruption, oversize, and symlink tests pass, and worker cache faults fall through to source decode.

- [x] G4: Persistent writes are private and atomic: cache directories are mode 0700, files are mode 0600, PNGs carry required freedesktop text metadata, and temporary files are renamed within the destination directory.
  CHECK: rg -n '0o700|0o600|add_text_chunk|Software|rename|temporary' crates/app/src/thumbnail_cache.rs
  EXPECT: /add_text_chunk/
  EVIDENCE: Focused tests verify 0700 directories, 0600 thumbnail/marker files, required PNG text chunks, same-directory temporary cleanup, and atomic replacement.

- [x] G5: Floe ownership markers bound cleanup to Floe-created entries; age, count, and byte limits are explicit and foreign/shared cache entries are never pruned.
  CHECK: rg -n 'MAX_OWNED|MAX_TOTAL|Software|cleanup' crates/app/src/thumbnail_cache.rs
  EXPECT: /MAX_OWNED/
  EVIDENCE: Global 2,048-entry, 256-MiB, 90-day policy passes count/byte/age tests across both tiers and leaves foreign-software cache entries intact.

- [x] G6: Persistent lookup, decoding, writes, markers, and cleanup run only inside the fixed-capacity thumbnail worker; GTK receives owned pixels and never performs cache I/O.
  CHECK: rg -n 'ThumbnailCache|floe-thumbnail-worker|decode_thumbnail|try_request|MemoryTexture' crates/app/src/thumbnail.rs crates/app/src/application.rs crates/app/src/ui.rs
  EXPECT: /floe-thumbnail-worker/
  EVIDENCE: Optional cache state is constructed and used only inside `floe-thumbnail-worker`; the established GTK presentation still receives owned RGBA responses only.

- [x] G7: Focused tests cover non-UTF-8 identity, tiering, valid reuse, metadata invalidation, corrupt/oversized/symlink rejection, private atomic writes, ownership-safe cleanup, nonfatal failures, and worker reuse.
  CHECK: cargo test -p floe-app phase_6e -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: All eleven focused `phase_6e` tests pass, including same-second subsecond invalidation and cross-worker persistent reuse against an intentionally corrupted source.

- [x] G8: README, design, architecture, development, roadmap, and persistent project status describe actual Phase 6E behavior, limitations, verification, and the next coherent phase.
  CHECK: rg -n 'Phase 6E|phase-6f' README.md DESIGN.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /Phase 6E/
  EVIDENCE: All six persistent documents describe implemented Phase 6E and next branch `phase-6f-thumbnail-format-polish`.

- [x] G9: Formatting, workspace compilation, strict Clippy, all tests, diff hygiene, and this gate file pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && git diff --check && node <unlazy-skill-dir>/scripts/gate-check.mjs GATES.md
  EXPECT: /test result: ok/
  EVIDENCE: Formatting, workspace check, strict Clippy, all 124 tests (91 app and 33 core), and diff hygiene pass; final gate-check follows publication.

- [x] G10: Native Wayland smoke with temporary cache/config roots proves first-run cache creation, second-run reuse, expected D-Bus ownership, continued process health, and intentional shutdown without persistent test artifacts.
  EVIDENCE: Two temporary-root Wayland runs created then reused one 0600 standard thumbnail (same inode and mtime), refreshed only its marker, owned the expected D-Bus name, remained healthy, released the name after shutdown, and left no smoke artifacts.

- [x] G11: The Phase 6E commit is pushed, fast-forwarded into `main`, and local/remote phase and main refs are identical.
  CHECK: git rev-parse main phase-6e-thumbnail-cache-polish origin/main origin/phase-6e-thumbnail-cache-polish
  EXPECT: /^([0-9a-f]{40})\n\1\n\1\n\1$/
  EVIDENCE: The focused Phase 6E commits are pushed through the phase branch and fast-forwarded `main`; the final four-ref comparison is required before handoff.
