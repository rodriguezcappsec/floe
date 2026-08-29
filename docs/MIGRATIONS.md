# Floe settings, cache, and rollback policy

Package installation and removal run as system operations and must never scan
or migrate a user's `HOME`, Trash, or XDG directories. Version migration runs
inside Floe as the signed-in user, with bounded no-follow reads and private
atomic writes.

## Current state inventory

Durable configuration below `$XDG_CONFIG_HOME/floe`:

- `view-preferences.conf` — text schema version 18;
- `bookmarks.bin` — version 1;
- `browser-session-v1.bin` — bounded workspace codec through version 3.

Durable safety and integrity data:

- `$XDG_DATA_HOME/floe/guardrails-v1.bin`;
- `$XDG_STATE_HOME/floe/operation-recovery-v1.bin`;
- `$XDG_DATA_HOME/floe/integrity/fingerprints-v1`;
- `$XDG_DATA_HOME/floe/integrity/baselines-v1/current`.

Rebuildable Floe cache state below `$XDG_CACHE_HOME/floe`:

- `search-index-v1`;
- `duplicate-hashes-v1`;
- `sort-metadata-v1`;
- `thumbnail-ownership` markers.

Normal and large freedesktop thumbnails live in the shared thumbnail cache.
Only entries proven Floe-owned by the existing ownership markers may be
cleaned; a migration must never erase unrelated shared thumbnails.

Floe has no encrypted vault implementation or vault format. Phase 21B performs
and claims no vault migration.

## Preference upgrade behavior

`view-preferences.conf` is limited to 2 MiB and opened with `O_NOFOLLOW`. The
file must be an owned regular file. Future versions, duplicate or invalid
version records, oversized files, and symlinks are refused without overwriting
the original. Invalid UTF-8/NUL-corrupt content is backed up before defaults are
written.

Supported legacy settings are parsed, copied to a private sibling backup named
`view-preferences.conf.pre-v18-legacy`, and rewritten as version 18. Corrupt
input uses the `pre-v18-corrupt` suffix. Backups and current settings are mode
`0600` below a mode-`0700` Floe directory. Writes use unique create-new
temporary files, file sync, atomic rename, and parent-directory sync. Stale
temporary files from an interrupted older write are ignored rather than
treated as current preferences.

Durable safety/integrity formats must fail closed when unsupported or unsafe.
Rebuildable caches may be discarded and rebuilt only through their exact
versioned Floe-owned paths.

## Rollback

Downgrading a package changes the executable but cannot automatically make an
older decoder understand newer user state. To roll back a settings migration:

1. Quit every Floe process.
2. Copy the relevant `pre-v18-*` backup back to
   `$XDG_CONFIG_HOME/floe/view-preferences.conf` while preserving mode `0600`.
3. Remove only an incompatible, versioned Floe-owned cache file if the older
   release cannot rebuild it; do not remove durable safety/integrity data or
   shared thumbnails.
4. Install the older package and launch it with the restored state.

Keep a separate backup before manually changing durable data. Uninstall does
not remove any user state, and Floe does not promise automatic downgrade
compatibility beyond decoders covered by repository tests.
